// +----------------------------------------------------------------------+
// | XHCurl 扩展 - PHP Fiber 协程桥接层                                    |
// |                                                                        |
// | 实现"PHP 调用 Rust 协程实现 curl 并行请求"的协程化方案。               |
// |                                                                        |
// | 架构：                                                                 |
// |   PHP 线程                  tokio 工作线程池                           |
// |   ────────                  ────────────────                           |
// |   XHCurl::run(fn() {        XHCurl::await($req):                        |
// |     $r = await($req);  ──┐    1. tokio::spawn(http_task) ──► reqwest   |
// |   });                     │    2. 存 (task_id, fiber) 到 pending 表     |
// |   $fiber->start();        │    3. Fiber::suspend() ── 挂起              |
// |                            │                                            |
// |   事件泵 run() 循环:       │                                            |
// |     recv_timeout(ch) ◄────┴── tokio task 完成时 send (task_id, result)  |
// |     查 pending 表 → $fiber->resume($result)                            |
// |     直到主 fiber 终止                                                  |
// |                                                                        |
// | 关键约束：                                                             |
// | - PHP 执行器非线程安全，tokio 工作线程绝不能触碰 PHP API/zval          |
// | - 结果经 crossbeam-channel（线程安全）回传 PHP 线程                    |
// | - 不使用 block_on（会独占 PHP 线程，与 Fiber 互斥）                    |
// | - 通过 ZendCallable 调用用户态 Fiber::suspend/resume（安全，无 FFI）   |
// +----------------------------------------------------------------------+

use std::cell::RefCell;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use crossbeam_channel::{bounded, Receiver, Sender};
use ext_php_rs::boxed::ZBox;
use ext_php_rs::convert::{FromZval, IntoZvalDyn};
use ext_php_rs::types::{ZendCallable, ZendHashTable, Zval};
use ext_php_rs::zend::ExecutorGlobals;

use crate::multi::RequestResult;
use crate::php_ext::for_each_kv;
use crate::request::XhRequest;

// +----------------------------------------------------------------------+
// | 任务 ID 生成器                                                        |
// +----------------------------------------------------------------------+

/// 全局任务 ID 生成器（线程安全）
static NEXT_TASK_ID: AtomicU64 = AtomicU64::new(1);

fn next_task_id() -> u64 {
    NEXT_TASK_ID.fetch_add(1, Ordering::Relaxed)
}

// +----------------------------------------------------------------------+
// | 线程间通信消息                                                        |
// +----------------------------------------------------------------------+

/// tokio 工作线程 → PHP 线程的消息
/// 仅包含 task_id 和序列化后的结果，不携带任何 PHP zval（线程安全）
struct TaskMessage {
    task_id: u64,
    result: RequestResult,
}

// +----------------------------------------------------------------------+
// | 协程调度器（PHP 线程局部）                                            |
// +----------------------------------------------------------------------+

/// 协程调度器状态。
///
/// ⚠️ 仅在 PHP 线程上访问（通过 thread_local），tokio 工作线程禁止触碰。
/// 跨线程通信仅通过 `result_tx`/`result_rx`（crossbeam-channel 线程安全）。
struct Scheduler {
    /// 待恢复的 Fiber 表：task_id → PHP Fiber 对象的 Zval
    /// await() 时存入，事件泵收到结果时取出并 resume
    pending: std::collections::HashMap<u64, Zval>,

    /// tokio task 完成结果的发送端（克隆后交给 tokio 工作线程）
    result_tx: Sender<TaskMessage>,

    /// 结果接收端（仅 PHP 线程的事件泵使用）
    result_rx: Receiver<TaskMessage>,

    /// 已 spawn 的 tokio 任务句柄（await/gather/each 的 runtime.spawn 返回值）。
    /// drop_scheduler 时全部 abort，避免主 Fiber 异常退出后任务残留全局运行时。
    task_handles: Vec<tokio::task::JoinHandle<()>>,
}

impl Scheduler {
    fn new() -> Self {
        // 有界 channel：缓冲区 256，背压控制
        let (tx, rx) = bounded(256);
        Self {
            pending: std::collections::HashMap::new(),
            result_tx: tx,
            result_rx: rx,
            task_handles: Vec::new(),
        }
    }
}

// PHP 线程局部的调度器。
// 使用 thread_local 而非全局 static，因为：
// 1. PHP 执行器是线程局部的（NTS 构建单线程，ZTS 每线程独立）
// 2. FPM 模式下每个 worker 进程独立（进程级隔离）
// 3. 避免 Send/Sync 约束（Zval 不是 Send）
thread_local! {
    static SCHEDULER: RefCell<Option<Scheduler>> = const { RefCell::new(None) };
}

/// 初始化调度器（在 run() 入口调用）
fn init_scheduler() {
    SCHEDULER.with(|s| {
        if s.borrow().is_none() {
            *s.borrow_mut() = Some(Scheduler::new());
        }
    });
}

/// 清理调度器（在 run() 出口调用）
fn drop_scheduler() {
    SCHEDULER.with(|s| {
        // 先 abort 所有残留的 tokio 任务，避免任务在全局运行时上继续执行
        // （主 Fiber 异常退出时，未完成的 HTTP 任务仍会持有 result_tx clone，
        //  256 容量的有界 channel 填满后任务会永久卡在 send().await 上）
        if let Some(scheduler) = s.borrow_mut().as_mut() {
            for handle in scheduler.task_handles.drain(..) {
                handle.abort();
            }
        }
        // drop Scheduler（释放 channel sender/receiver、pending 表中的 Zval）
        *s.borrow_mut() = None;
    });
}

/// RAII 守卫，确保 fiber_run 在任何返回路径（成功或失败）都清理调度器。
///
/// 在 `init_scheduler()` 后构造，离开作用域时自动调用 `drop_scheduler()`。
/// 避免错误路径（create_fiber/start/take_php_exception 失败、run_event_loop 返回 Err）
/// 跳过清理导致 thread_local 永久残留，使后续 run() 永远误报"不支持嵌套调用"。
struct SchedulerGuard;

impl Drop for SchedulerGuard {
    fn drop(&mut self) {
        drop_scheduler();
    }
}

// +----------------------------------------------------------------------+
// | 协程 API：await（在 Fiber 内调用）                                    |
// +----------------------------------------------------------------------+

/// 在 PHP Fiber 内部等待一个 HTTP 请求完成。
///
/// **PHP 签名**：`public static XHCurl::await(XHRequest $req): array`
///
/// **实现步骤**：
/// 1. 在 tokio 运行时上 spawn 一个 HTTP 任务
/// 2. 获取当前 PHP Fiber 对象（通过 `Fiber::getCurrent()`）
/// 3. 将 (task_id, fiber) 存入 pending 表
/// 4. 调用 `Fiber::suspend()` 挂起当前 Fiber（控制权回到 run() 事件泵）
/// 5. 事件泵收到结果后 `$fiber->resume($result)`，本函数从 suspend 返回
/// 6. 返回 resume 传入的结果（转为 PHP 数组）
pub fn fiber_await(request: &XhRequest) -> Result<ZBox<ZendHashTable>, String> {
    let task_id = next_task_id();

    // 1. 先校验当前是否在 Fiber 上下文中（在 spawn 前校验，
    //    避免校验失败时已 spawn 的 tokio task 变成无对应 pending 表项的孤立任务）
    let get_current =
        ZendCallable::try_from_name("Fiber::getCurrent").map_err(|e| e.to_string())?;
    let current_fiber = get_current.try_call(vec![]).map_err(|e| e.to_string())?;

    // current_fiber 为 null 表示不在 Fiber 上下文中
    if current_fiber.is_null() || !current_fiber.is_object() {
        return Err("XHCurl::await 必须在 Fiber 内部调用（请用 XHCurl::run 包裹）".to_string());
    }

    // 2. 获取 tokio 运行时 handle，spawn HTTP 任务到工作线程
    //    HTTP 请求完全在 tokio 工作线程上执行，不触碰 PHP API
    let runtime = crate::php_ext::global_runtime();
    let client = crate::php_ext::global_client().clone();
    let request_clone = request.clone();
    let result_tx = SCHEDULER.with(|s| {
        s.borrow()
            .as_ref()
            .ok_or_else(|| "调度器未初始化：await 必须在 run() 内调用".to_string())
            .map(|sc| sc.result_tx.clone())
    })?;

    let handle = runtime.spawn(async move {
        // 在 tokio 工作线程上执行 HTTP 请求
        let result = execute_http_task(client, request_clone, task_id).await;
        // 通过线程安全 channel 发送结果回 PHP 线程
        let _ = result_tx.send(TaskMessage { task_id, result });
    });
    // 保存 JoinHandle，主 Fiber 异常退出时 drop_scheduler 会 abort
    SCHEDULER.with(|s| {
        if let Some(scheduler) = s.borrow_mut().as_mut() {
            scheduler.task_handles.push(handle);
        }
    });

    // 3. 存入 pending 表
    SCHEDULER.with(|s| {
        s.borrow_mut()
            .as_mut()
            .expect("调度器未初始化")
            .pending
            .insert(task_id, current_fiber);
    });

    // 4. 调用 Fiber::suspend() 挂起当前 Fiber
    //    控制权回到 $fiber->start() 的调用者（即 run() 事件泵）
    //    Rust 栈帧随 Fiber 一起被冻结，resume 后从下一行继续
    let suspend = ZendCallable::try_from_name("Fiber::suspend").map_err(|e| e.to_string())?;
    let suspended_value = suspend.try_call(vec![]).map_err(|e| e.to_string())?;

    // 5. 事件泵收到结果后调用 $fiber->resume($result)
    //    suspended_value 即 resume 传入的值
    //    将其转为 PHP 数组返回
    let result_array = result_zval_to_array(&suspended_value)?;
    Ok(result_array)
}
// +----------------------------------------------------------------------+
// | 协程 API：gather（并发批量 await）                                    |
// +----------------------------------------------------------------------+

/// 并发发起多个 HTTP 请求，按**完成顺序**返回所有结果。
///
/// **PHP 签名**：`public static XHCurl::gather(array $requests): array`
///
/// 与在循环中串行调用 `await` 不同，`gather` 会：
/// 1. 一次性将所有请求 spawn 到 tokio 工作线程（真正并行）
/// 2. 当前 Fiber 挂起一次
/// 3. 每个请求完成时，事件泵 resume 当前 Fiber，传入该结果
/// 4. Fiber 循环挂起 N 次，收集所有结果
/// 5. 返回按**完成顺序**排列的结果数组（非请求顺序）
///
/// **并行性证明**：返回数组中的结果顺序取决于哪个请求先完成，
/// 而非请求的提交顺序。网络延迟波动会导致顺序打乱。
pub fn fiber_gather(requests: Vec<XhRequest>) -> Result<ZBox<ZendHashTable>, String> {
    let total = requests.len();
    if total == 0 {
        return Ok(ZendHashTable::new());
    }

    // 1. 获取当前 Fiber
    let get_current =
        ZendCallable::try_from_name("Fiber::getCurrent").map_err(|e| e.to_string())?;
    let current_fiber = get_current.try_call(vec![]).map_err(|e| e.to_string())?;
    if current_fiber.is_null() || !current_fiber.is_object() {
        return Err("XHCurl::gather 必须在 Fiber 内部调用（请用 XHCurl::run 包裹）".to_string());
    }

    // 2. spawn 所有 HTTP 请求到 tokio（并行执行）
    //    使用 Semaphore 限制并发数，防止过多请求同时执行耗尽连接池/内存。
    //    并发上限取「请求数」与全局 fiber_max_concurrency 的较小值
    //    （0 表示不限制；默认 64，可通过 setConfig 调整）
    let runtime = crate::php_ext::global_runtime();
    let client = crate::php_ext::global_client().clone();
    let cap = crate::curl::XhCurlManager::global()
        .config()
        .fiber_max_concurrency;
    let max_concurrency = if cap == 0 { total } else { total.min(cap) };
    let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(max_concurrency));

    for request in requests {
        let task_id = next_task_id();
        let result_tx = SCHEDULER.with(|s| {
            s.borrow()
                .as_ref()
                .ok_or_else(|| "调度器未初始化：gather 必须在 run() 内调用".to_string())
                .map(|sc| sc.result_tx.clone())
        })?;

        let client_clone = client.clone();
        let request_clone = request;
        let sem_clone = std::sync::Arc::clone(&semaphore);
        let handle = runtime.spawn(async move {
            // 获取并发许可（_permit 在作用域结束时自动释放）
            // acquire 失败说明信号量已关闭，必须发送错误结果而非继续执行
            let _permit = match sem_clone.acquire().await {
                Ok(p) => p,
                Err(_) => {
                    // 信号量获取失败（已关闭），发送错误结果而非继续执行
                    let result = RequestResult::error(
                        task_id.to_string(),
                        None,
                        "并发信号量获取失败".to_string(),
                        std::time::Duration::from_secs(0),
                    );
                    let _ = result_tx.send(TaskMessage { task_id, result });
                    return;
                }
            };
            let result = execute_http_task(client_clone, request_clone, task_id).await;
            let _ = result_tx.send(TaskMessage { task_id, result });
        });

        // 保存 JoinHandle，主 Fiber 异常退出时 drop_scheduler 会 abort
        SCHEDULER.with(|s| {
            if let Some(scheduler) = s.borrow_mut().as_mut() {
                scheduler.task_handles.push(handle);
            }
        });

        // 为每个 task_id 注册当前 Fiber（同一个 Fiber 对应多个 task_id）
        SCHEDULER.with(|s| {
            s.borrow_mut()
                .as_mut()
                .expect("调度器未初始化")
                .pending
                .insert(task_id, current_fiber.shallow_clone());
        });
    }

    // 3. 循环挂起 N 次，每次收到一个结果
    //    事件泵收到结果 → 查 pending 表 → resume 当前 Fiber → Fiber 从 suspend 返回
    //    Fiber 检查是否收齐，未齐则再次 suspend
    let suspend = ZendCallable::try_from_name("Fiber::suspend").map_err(|e| e.to_string())?;

    let mut results = ZendHashTable::new();
    for i in 0..total {
        let suspended_value = suspend.try_call(vec![]).map_err(|e| e.to_string())?;
        // suspended_value 是事件泵 resume 传入的结果数组
        // 按完成顺序插入（整数键 0, 1, 2, ... 表示第几个完成）
        let _ = results.insert_at_index(i as i64, suspended_value);
    }

    Ok(results)
}

// +----------------------------------------------------------------------+
// | 协程 API：each（流式回调）                                            |
// +----------------------------------------------------------------------+

/// 并发发起多个 HTTP 请求，每完成一个就立即调用回调处理，不累积全部结果。
///
/// **PHP 签名**：`public static XHCurl::each(array $requests, callable $callback): int`
///
/// 与 `gather` 的区别：
/// - gather：等全部完成，累积到数组一次性返回（内存随请求总数增长）
/// - each：每完成一个立即调回调处理，不累积（内存恒定）
///
/// **回调签名**：`function(array $result): void`
///
/// **并行性**：与 gather 相同，请求在 tokio 上并行执行，结果按完成顺序交给回调。
pub fn fiber_each(requests: Vec<XhRequest>, callback: &Zval) -> Result<i64, String> {
    let total = requests.len();
    if total == 0 {
        // 空请求列表：不 spawn 不 suspend，提前返回
        return Ok(0);
    }

    // 1. 获取当前 Fiber
    let get_current =
        ZendCallable::try_from_name("Fiber::getCurrent").map_err(|e| e.to_string())?;
    let current_fiber = get_current.try_call(vec![]).map_err(|e| e.to_string())?;
    if current_fiber.is_null() || !current_fiber.is_object() {
        return Err("XHCurl::each 必须在 Fiber 内部调用（请用 XHCurl::run 包裹）".to_string());
    }

    // 2. 校验用户回调（提前失败，避免 spawn 后才发现回调无效）
    //    ZendCallable::new 借用 callback 的生命周期，在整个函数内可用
    let callback_callable =
        ZendCallable::new(callback).map_err(|e| format!("无效的回调: {}", e))?;

    // 3. spawn 所有 HTTP 请求到 tokio（并行执行）
    //    使用 Semaphore 限制并发数，防止过多请求同时执行耗尽连接池/内存。
    //    并发上限 = 请求数 与 64 的较小值（避免 10000 个请求同时 spawn）
    let runtime = crate::php_ext::global_runtime();
    let client = crate::php_ext::global_client().clone();
    let max_concurrency = total.min(64);
    let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(max_concurrency));

    for request in requests {
        let task_id = next_task_id();
        let result_tx = SCHEDULER.with(|s| {
            s.borrow()
                .as_ref()
                .ok_or_else(|| "调度器未初始化：each 必须在 run() 内调用".to_string())
                .map(|sc| sc.result_tx.clone())
        })?;

        let client_clone = client.clone();
        let request_clone = request;
        let sem_clone = std::sync::Arc::clone(&semaphore);
        let handle = runtime.spawn(async move {
            // 获取并发许可（_permit 在作用域结束时自动释放）
            // acquire 失败说明信号量已关闭，必须发送错误结果而非继续执行
            let _permit = match sem_clone.acquire().await {
                Ok(p) => p,
                Err(_) => {
                    // 信号量获取失败（已关闭），发送错误结果而非继续执行
                    let result = RequestResult::error(
                        task_id.to_string(),
                        None,
                        "并发信号量获取失败".to_string(),
                        std::time::Duration::from_secs(0),
                    );
                    let _ = result_tx.send(TaskMessage { task_id, result });
                    return;
                }
            };
            let result = execute_http_task(client_clone, request_clone, task_id).await;
            let _ = result_tx.send(TaskMessage { task_id, result });
        });

        // 保存 JoinHandle，主 Fiber 异常退出时 drop_scheduler 会 abort
        SCHEDULER.with(|s| {
            if let Some(scheduler) = s.borrow_mut().as_mut() {
                scheduler.task_handles.push(handle);
            }
        });

        // 为每个 task_id 注册当前 Fiber（同一个 Fiber 对应多个 task_id）
        SCHEDULER.with(|s| {
            s.borrow_mut()
                .as_mut()
                .expect("调度器未初始化")
                .pending
                .insert(task_id, current_fiber.shallow_clone());
        });
    }

    // 4. 循环挂起 N 次，每收到一个结果就调用回调
    //    事件泵收到结果 → 查 pending 表 → resume 当前 Fiber → Fiber 从 suspend 返回
    //    与 gather 不同：此处不累积结果，而是立即交给回调处理后丢弃（内存恒定）
    let suspend = ZendCallable::try_from_name("Fiber::suspend").map_err(|e| e.to_string())?;

    for _ in 0..total {
        let suspended_value = suspend.try_call(vec![]).map_err(|e| e.to_string())?;
        // 调用用户回调处理当前结果，回调签名：function(array $result): void
        //
        // try_call 已通过 take_exception 取出异常对象（Error::Exception 持有 ZBox<ZendObject>），
        // 直接提取 message 传播；不能用 Display（其内部 {e:?} Debug 遍历 trace 属性，
        // 可能含 NUL 字节导致 CString 转换失败，抛出 InvalidCString 掩盖原始异常）。
        callback_callable
            .try_call(vec![&suspended_value as &dyn IntoZvalDyn])
            .map_err(|e| match e {
                ext_php_rs::error::Error::Exception(obj) => extract_exception_message(&obj),
                other => format!("回调执行失败: {}", other),
            })?;
        // suspended_value 在此离开作用域 → 释放（不存任何地方，内存恒定）
    }

    Ok(total as i64)
}

// +----------------------------------------------------------------------+
// | 协程 API：run（事件泵）                                               |
// +----------------------------------------------------------------------+

/// 启动协程事件泵，执行主回调。
///
/// **PHP 签名**：`public static XHCurl::run(callable $main): mixed`
///
/// **实现步骤**：
/// 1. 初始化调度器（pending 表 + channel）
/// 2. 创建主 Fiber：`new Fiber($main)`，调用 `start()`
/// 3. 事件泵循环：
///    - 主 Fiber 运行到 `await()` → `Fiber::suspend()` 挂起
///    - 事件泵从 channel `recv_timeout` 获取完成的任务结果
///    - 查 pending 表找到对应 Fiber，调用 `$fiber->resume($result)`
///    - 被恢复的 Fiber 继续执行，可能再次 suspend
/// 4. 主 Fiber 终止（返回值或抛异常）时，返回其结果
pub fn fiber_run(main: &Zval) -> Result<Zval, String> {
    // 检测嵌套调用:如果调度器已存在,说明当前已在 run() 事件泵内
    let already_running = SCHEDULER.with(|s| s.borrow().is_some());
    if already_running {
        return Err("不支持嵌套调用 XHCurl::run".to_string());
    }

    // SAPI 检查：FPM 模式下 global_runtime 返回单线程运行时，
    // recv_timeout 会阻塞 PHP 线程导致 spawn 的 tokio 任务无法被驱动执行，陷入无限循环。
    // Fiber 协程桥接仅支持 CLI 模式（与 XHThreadPool 一致）。
    if !crate::php_ext::sapi_is_cli() {
        return Err("XHCurl::run 仅在 CLI 模式下可用（FPM 请用 XHMulti）".to_string());
    }

    // 1. 初始化调度器
    init_scheduler();
    // RAII 守卫：确保任何返回路径（含 ? 提前返回）都清理调度器，
    // 避免单次失败后 thread_local 永久残留导致后续 run() 误报"不支持嵌套调用"
    let _guard = SchedulerGuard;

    // 2. 创建主 Fiber 并启动
    //    new Fiber($main)
    let fiber_zval = create_fiber(main)?;
    // shallow_clone 增加 refcount，事件泵持有引用，原 zval 释放后仍可用
    let fiber = fiber_zval.shallow_clone();

    // 调用 $fiber->start() —— 主 Fiber 开始执行，遇到 await 会 suspend 回到此处
    fiber_zval
        .try_call_method("start", vec![])
        .map_err(|e| e.to_string())?;
    // ext-php-rs 0.15 的 try_call_method 不传播 PHP 异常，主动检查并取出
    take_php_exception()?;

    // 3. 事件泵循环（guard 在函数返回时自动 drop，清理调度器）
    run_event_loop(&fiber)
}

/// 事件泵主循环。
///
/// 循环逻辑：
/// 1. 检查主 Fiber 是否已终止（getReturn 或 isTerminated）
/// 2. 若未终止，从 channel recv_timeout 获取完成的任务结果
/// 3. 查 pending 表找到挂起的 Fiber，调用 resume($result)
/// 4. 被恢复的 Fiber 继续执行，可能再次 await/suspend
/// 5. 重复直到主 Fiber 终止
fn run_event_loop(main_fiber: &Zval) -> Result<Zval, String> {
    loop {
        // 检查主 Fiber 是否已终止
        if fiber_is_terminated(main_fiber)? {
            // 主 Fiber 已完成，获取返回值
            let ret = main_fiber
                .try_call_method("getReturn", vec![])
                .map_err(|e| e.to_string())?;
            // 取出 Fiber 内可能抛出的异常（优先于返回值传播）
            take_php_exception()?;
            return Ok(ret);
        }

        // 从 channel 获取完成的任务结果（带超时，避免永久阻塞 PHP 线程）
        // 超时设为 30 秒（而非原 300 秒），缩短无响应时的阻塞时间。
        // 超时后检查 pending 状态：有 pending 则继续等，无 pending 则报错。
        let result_rx = SCHEDULER.with(|s| {
            s.borrow()
                .as_ref()
                .expect("调度器未初始化")
                .result_rx
                .clone()
        });

        match result_rx.recv_timeout(Duration::from_secs(30)) {
            Ok(msg) => {
                // 查 pending 表找到挂起的 Fiber
                let pending_fiber = SCHEDULER.with(|s| {
                    s.borrow_mut()
                        .as_mut()
                        .expect("调度器未初始化")
                        .pending
                        .remove(&msg.task_id)
                });

                if let Some(fiber) = pending_fiber {
                    // 将 RequestResult 转为 PHP 数组
                    // 复用 php_ext::result_to_php_array，确保协程/批量/线程池字段一致
                    let result_array = crate::php_ext::result_to_php_array(&msg.result);

                    // 调用 $fiber->resume($result_array) 恢复挂起的 Fiber
                    // resume 后，await() 从 Fiber::suspend() 返回，继续执行
                    fiber
                        .try_call_method("resume", vec![&result_array as &dyn IntoZvalDyn])
                        .map_err(|e| e.to_string())?;
                    // 取出 Fiber 内可能抛出的异常（resume 恢复执行后可能抛异常）
                    take_php_exception()?;
                }
                // 回收已完成的 tokio 任务句柄，避免长轮询场景下 task_handles 无界增长
                SCHEDULER.with(|s| {
                    if let Some(scheduler) = s.borrow_mut().as_mut() {
                        scheduler.task_handles.retain(|h| !h.is_finished());
                    }
                });
                // 若找不到 pending fiber，可能是已取消的任务，忽略
            }
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                // 超时：检查是否有 pending 任务
                let pending_count =
                    SCHEDULER.with(|s| s.borrow().as_ref().map(|sc| sc.pending.len()).unwrap_or(0));
                if pending_count == 0 {
                    // 无 pending 任务但主 Fiber 仍挂起，可能是死锁
                    // （用户在 Fiber 内调用了非 await 的阻塞代码）
                    return Err(
                        "事件泵空闲但主 Fiber 未终止（可能未在 Fiber 内调用 await）".to_string()
                    );
                }
                // 有 pending 任务但超时，继续等待（HTTP 请求仍在进行中）
            }
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                return Err("结果 channel 已关闭".to_string());
            }
        }
    }
}

// +----------------------------------------------------------------------+
// | Fiber 辅助函数                                                        |
// +----------------------------------------------------------------------+

/// 从 PHP 异常对象提取 message。
///
/// 优先读 "message" 属性（String 未实现 FromZval，用 &str 再转 owned）；
/// 失败时回退到调用 Throwable::getMessage()；都失败则用占位文案。
///
/// 不使用 `Error::Exception` 的 Display（其内部用 `{:?}` Debug 遍历全部属性，
/// trace 属性可能包含 NUL 字节，导致后续 CString 转换失败抛 InvalidCString）。
pub(crate) fn extract_exception_message(exc: &ext_php_rs::types::ZendObject) -> String {
    exc.get_property::<&str>("message")
        .ok()
        .map(str::to_owned)
        .or_else(|| {
            exc.try_call_method("getMessage", vec![])
                .ok()
                .and_then(|z| z.string())
        })
        .unwrap_or_else(|| "未知 PHP 异常".to_string())
}

/// 检查并取出 PHP 全局异常，若存在则返回 `Err(异常 message)`。
///
/// ext-php-rs 0.15 的 `try_call_method` 不检查 `ExecutorGlobals::take_exception()`，
/// 导致 PHP Fiber 内抛出的异常被静默吞掉（事件泵误报"空闲"超时）。
/// 此函数在每次调用 Fiber 方法（start/resume/getReturn）后主动取出全局异常，
/// 若存在则提取其 message 并以 `Err` 返回，确保原始异常能向上传播。
fn take_php_exception() -> Result<(), String> {
    if let Some(exc) = ExecutorGlobals::take_exception() {
        return Err(format!("PHP 异常: {}", extract_exception_message(&exc)));
    }
    Ok(())
}

/// 创建 PHP Fiber 对象
/// 等价于 `new Fiber($main)`
///
/// 实现方式：
/// 1. 通过 ClassEntry::try_find("Fiber") 获取 Fiber 类入口
/// 2. 用 ZendObject::new(ce) 创建对象实例（调用 create_object handler）
/// 3. 通过 try_call_method("__construct", [$main]) 调用构造函数
fn create_fiber(main: &Zval) -> Result<Zval, String> {
    use ext_php_rs::types::ZendObject;
    use ext_php_rs::zend::ClassEntry;

    // 查找 Fiber 类入口
    let ce = ClassEntry::try_find("Fiber")
        .ok_or_else(|| "Fiber 类未找到（需要 PHP 8.1+）".to_string())?;

    // 创建对象实例（create_object handler 会初始化 zend_object_std）
    let mut obj = ZendObject::new(ce);

    // 调用 __construct($main) 完成构造
    obj.try_call_method("__construct", vec![main as &dyn IntoZvalDyn])
        .map_err(|e| e.to_string())?;
    // 检查 __construct 是否抛出 PHP 异常（try_call_method 不检查 EG exception）
    take_php_exception()?;

    // 包装为 Zval 返回
    // refcount 语义：set_object 内部调用 val.inc_count()（ext-php-rs 0.15 zval.rs:1024），
    // 使对象 refcount 从 1 增至 2。obj（ZBox）随后离开作用域 Drop 时调用
    // zend_object_release（dec_count），refcount 回到 1。最终 zv 独占引用（refcount=1）。
    let mut zv = Zval::new();
    zv.set_object(&mut obj);
    Ok(zv)
}

/// 检查 Fiber 是否已终止（terminated 状态）
/// 等价于 `$fiber->isTerminated()`
fn fiber_is_terminated(fiber: &Zval) -> Result<bool, String> {
    let result = fiber
        .try_call_method("isTerminated", vec![])
        .map_err(|e| e.to_string())?;
    Ok(result.bool().unwrap_or(false))
}

// +----------------------------------------------------------------------+
// | HTTP 任务执行（tokio 工作线程）                                       |
// +----------------------------------------------------------------------+

/// 在 tokio 工作线程上执行单个 HTTP 请求。
///
/// ⚠️ 此函数在 tokio 工作线程上运行，**绝不触碰任何 PHP API**。
/// 仅使用 reqwest 执行请求，返回 RequestResult（纯 Rust 类型，无 PHP 依赖）。
async fn execute_http_task(
    client: reqwest::Client,
    request: XhRequest,
    task_id: u64,
) -> RequestResult {
    let start = std::time::Instant::now();
    let request_id = request
        .get_id()
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("task-{}", task_id));

    // 提取用户自定义数据（随结果原样带回）
    let user_data = request.get_user_data().map(|s| s.to_string());

    // 执行请求（复用 XhMulti::execute_single，不启用流式回调）
    // 从全局配置读取 max_response_size，与 XHMulti/XHThreadPool 保持一致
    let max_response_size = crate::curl::XhCurlManager::global()
        .config()
        .max_response_size;
    match crate::multi::XhMulti::execute_single(
        client,
        request,
        request_id.clone(),
        None,
        max_response_size,
    )
    .await
    {
        Ok(response) => RequestResult::success(request_id, user_data, response, start.elapsed()),
        Err(e) => RequestResult::error(request_id, user_data, e.to_string(), start.elapsed()),
    }
}

// +----------------------------------------------------------------------+
// | 结果转换                                                              |
// +----------------------------------------------------------------------+

// 注：RequestResult → PHP 数组的转换已统一抽到 php_ext::result_to_php_array，
// 由 fiber / multi / threadpool 三条路径共用，确保字段集完全一致。

/// 将 resume 传入的 Zval 转为数组（await 的返回值）
///
/// 事件泵调用 `$fiber->resume($result_array)`，其中 $result_array 是
/// result_to_php_array 生成的。Fiber::suspend 返回此值，await 将其转为
/// ZBox<ZendHashTable> 返回给 PHP。
fn result_zval_to_array(zval: &Zval) -> Result<ZBox<ZendHashTable>, String> {
    if let Some(ht) = <&ZendHashTable as FromZval>::from_zval(zval) {
        // 深拷贝哈希表（resume 传入的 zval 是临时的）
        let mut new_ht = ZendHashTable::new();
        for_each_kv(ht, |key, val| {
            new_ht
                .insert(key.to_string(), val.shallow_clone())
                .map_err(|e| e.to_string())?;
            Ok(())
        })?;
        Ok(new_ht)
    } else {
        Err("await 恢复值不是数组".to_string())
    }
}
