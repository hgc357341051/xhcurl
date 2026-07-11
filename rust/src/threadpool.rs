// +----------------------------------------------------------------------+
// | XHCurl 扩展 - 线程池（XHThreadPool）                                   |
// | 基于 tokio + dispatcher 模式实现真正的多线程并发                       |
// |                                                                        |
// | 核心改进（对比 C 版本）：                                              |
// | 1. 工作线程通过 channel 与主线程通信，解决"不能调用 PHP 函数"限制      |
// | 2. 使用 tokio 运行时，自动管理工作线程生命周期                         |
// | 3. 编译期线程安全：Send + Sync bounds                                  |
// | 4. 统一 FPM/CLI 模式：FPM 使用单线程运行时，CLI 使用多线程运行时       |
// |                                                                        |
// | 架构（dispatcher 模式）：                                              |
// |   主线程 (PHP)                                                         |
// |     │                                                                  |
// |     ▼ task_tx (bounded)                                                |
// |   [dispatcher task] ──round-robin──► worker 0 (独立 channel)           |
// |                                  ──► worker 1 (独立 channel)           |
// |                                  ──► worker N (独立 channel)           |
// |   各 worker ──result_tx──► [result channel] ──► 主线程收集             |
// |                                                                        |
// | 对比旧设计的改进：                                                     |
// | - 消除 Arc<Mutex<Receiver>>：worker 不再争抢锁接收任务                 |
// |   旧设计中所有 worker 共享一个 Mutex<Receiver>，同一时刻只有一个       |
// |   worker 能 recv，且 idle_timeout 期间锁被持有导致其他 worker 全部阻塞 |
// | - idle_timeout 正确工作：timeout 直接作用于 worker 自己的 recv()       |
// | - 消除双 channel（bounded+unbounded）冗余                              |
// +----------------------------------------------------------------------+

use std::time::{Duration, Instant};

use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::error::{XhCurlError, XhCurlResult, DEFAULT_MAX_RESPONSE_SIZE};
use crate::multi::{RequestResult, StreamEvent};
use crate::request::XhRequest;
use crate::response::XhResponse;

/// 线程池任务消息
/// 主线程通过此消息类型向工作线程发送任务
#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum TaskMessage {
    /// 执行单个请求
    Request {
        /// 请求构建器
        request: XhRequest,
        /// 流式回调发送端（可选）
        stream_tx: Option<mpsc::Sender<(String, StreamEvent)>>,
    },

    /// 关闭工作线程
    Shutdown,
}

/// 线程池结果消息
/// 工作线程通过此消息类型向主线程发送结果
#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum ResultMessage {
    /// 请求完成
    Completed(RequestResult),

    /// 工作线程已关闭
    WorkerShutdown,
}

/// 线程池配置
#[derive(Debug, Clone)]
pub struct ThreadPoolConfig {
    /// 工作线程数量
    /// 默认: CPU 核心数
    pub worker_count: usize,

    /// 任务队列容量
    /// 0 = 使用默认值 1000
    pub queue_capacity: usize,

    /// 空闲线程超时（秒）
    /// 超过此时间无任务则关闭线程
    /// 0 = 永不超时（默认，线程池保持工作线程存活以复用连接）
    ///
    /// 注意：设置非零值可能导致 worker 在请求间隔较长时退出，
    /// 若此时仍有待处理任务，结果可能丢失。建议保持默认 0。
    pub idle_timeout: u64,

    /// 最大响应体大小（字节）
    /// 防止恶意服务器返回超大响应导致内存溢出
    /// 0 = 使用默认值 10MB
    pub max_response_size: usize,
}

/// 默认队列容量（queue_capacity == 0 时使用）
pub(crate) const DEFAULT_QUEUE_CAPACITY: usize = 1000;

/// 每个 worker 的任务 channel 缓冲区大小
/// 设为 1：dispatcher 在所有 worker 忙时阻塞，提供背压
const WORKER_CHANNEL_CAPACITY: usize = 1;

impl Default for ThreadPoolConfig {
    fn default() -> Self {
        // 获取 CPU 核心数
        let cpu_count = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4);

        Self {
            worker_count: cpu_count,
            queue_capacity: DEFAULT_QUEUE_CAPACITY,
            idle_timeout: 0, // 永不超时：线程池应保持 worker 存活以复用连接
            max_response_size: DEFAULT_MAX_RESPONSE_SIZE,
        }
    }
}

/// 线程池
/// 管理多个工作线程，并发执行 HTTP 请求
///
/// # 架构（dispatcher 模式）
/// - 主线程通过 `task_tx` 提交任务
/// - dispatcher task 拥有 `task_rx`，将任务 round-robin 分发给各 worker
/// - 每个 worker 拥有独立的 channel（无 Mutex，无锁竞争）
/// - worker 执行完成后通过 `result_tx` 发送结果
///
/// # 对比旧设计
/// - 旧设计：所有 worker 共享 `Arc<Mutex<Receiver>>`，同一时刻只有一个 worker 能 recv，
///   且 idle_timeout 期间锁被持有导致其他 worker 全部阻塞
/// - 新设计：dispatcher 独占 Receiver，分发给各 worker 的独立 channel，无锁竞争
///
/// # 线程安全
/// - 线程池本身通过 channel 通信
/// - 工作线程间无共享状态，通过 channel 通信
pub struct XhThreadPool {
    /// 线程池配置
    config: ThreadPoolConfig,

    /// 任务发送端（主线程 → dispatcher）
    task_tx: Option<mpsc::Sender<TaskMessage>>,

    /// 结果接收端（worker → 主线程）
    result_rx: Option<mpsc::Receiver<ResultMessage>>,

    /// dispatcher 任务句柄
    dispatcher_handle: Option<JoinHandle<()>>,

    /// 工作线程句柄
    workers: Vec<JoinHandle<()>>,

    /// reqwest 客户端（共享连接池）
    client: reqwest::Client,

    /// 是否已启动
    is_running: bool,
}

impl XhThreadPool {
    /// 创建新的线程池
    ///
    /// # 参数
    /// - `config`: 线程池配置
    /// - `client`: reqwest 客户端
    pub fn new(config: ThreadPoolConfig, client: reqwest::Client) -> Self {
        Self {
            config,
            task_tx: None,
            result_rx: None,
            dispatcher_handle: None,
            workers: Vec::new(),
            client,
            is_running: false,
        }
    }

    /// 使用默认配置创建
    pub fn with_default_config(client: reqwest::Client) -> Self {
        Self::new(ThreadPoolConfig::default(), client)
    }

    /// 启动线程池
    /// 创建 dispatcher 和工作线程，开始接收任务
    pub fn start(&mut self) -> XhCurlResult<()> {
        if self.is_running {
            return Ok(()); // 已经启动
        }

        if self.config.worker_count == 0 {
            return Err(XhCurlError::ThreadPool("worker_count 不能为 0".to_string()));
        }

        // 创建任务 channel（主线程 → dispatcher）
        let queue_cap = if self.config.queue_capacity > 0 {
            self.config.queue_capacity
        } else {
            DEFAULT_QUEUE_CAPACITY
        };
        let (task_tx, task_rx) = mpsc::channel(queue_cap);

        // 创建结果 channel（worker → 主线程）
        let result_cap = queue_cap.max(self.config.worker_count * 2);
        let (result_tx, result_rx) = mpsc::channel(result_cap);

        // 创建 per-worker channels 并启动 workers
        // 每个 worker 拥有独立的 Receiver，无需 Mutex
        let mut worker_txs = Vec::with_capacity(self.config.worker_count);
        for worker_id in 0..self.config.worker_count {
            let (worker_tx, worker_rx) = mpsc::channel::<TaskMessage>(WORKER_CHANNEL_CAPACITY);
            worker_txs.push(worker_tx);

            let result_tx = result_tx.clone();
            let client = self.client.clone();
            let idle_timeout = self.config.idle_timeout;
            let max_response_size = self.config.max_response_size;

            let handle = tokio::spawn(async move {
                Self::worker_loop(
                    worker_id,
                    worker_rx,
                    result_tx,
                    client,
                    idle_timeout,
                    max_response_size,
                )
                .await;
            });
            self.workers.push(handle);
        }

        // 启动 dispatcher（拥有 task_rx，分发任务到各 worker）
        let dispatcher_handle = tokio::spawn(async move {
            Self::dispatcher_loop(task_rx, worker_txs).await;
        });

        // 丢弃 result_tx（仅 workers 持有）
        // 当所有 result_tx 都 drop 后，result_rx 会收到 None
        drop(result_tx);

        self.task_tx = Some(task_tx);
        self.result_rx = Some(result_rx);
        self.dispatcher_handle = Some(dispatcher_handle);
        self.is_running = true;

        Ok(())
    }

    /// dispatcher 主循环
    ///
    /// 拥有 task_rx，将任务 round-robin 分发给各 worker：
    /// 1. 先尝试 try_send（非阻塞）给 round-robin 的下一个 worker
    /// 2. 若该 worker 忙（channel 满），尝试下一个
    /// 3. 若所有 worker 都忙，阻塞 send 给 round-robin 的下一个（提供背压）
    ///
    /// 收到 Shutdown 或 channel 关闭时，向所有 worker 发送 Shutdown 并退出
    async fn dispatcher_loop(
        mut task_rx: mpsc::Receiver<TaskMessage>,
        worker_txs: Vec<mpsc::Sender<TaskMessage>>,
    ) {
        let n = worker_txs.len();
        if n == 0 {
            return;
        }
        let mut next = 0usize;

        loop {
            let task = match task_rx.recv().await {
                Some(t) => t,
                None => break, // task_tx 全部 drop，主线程关闭
            };

            // Shutdown 信号：向所有 worker 广播并退出
            if matches!(task, TaskMessage::Shutdown) {
                for tx in &worker_txs {
                    let _ = tx.send(TaskMessage::Shutdown).await;
                }
                break;
            }

            // Round-robin 分发：先 try_send 遍历所有 worker（非阻塞）
            let mut task_opt = Some(task);
            let mut sent = false;
            for i in 0..n {
                let idx = (next + i) % n;
                if let Some(t) = task_opt.take() {
                    match worker_txs[idx].try_send(t) {
                        Ok(()) => {
                            next = (idx + 1) % n;
                            sent = true;
                            break;
                        }
                        Err(e) => {
                            // channel 满或关闭，取回任务尝试下一个 worker
                            task_opt = Some(e.into_inner());
                        }
                    }
                }
            }

            // 所有 worker 都忙，阻塞 send 给 round-robin 下一个（提供背压）
            if !sent {
                if let Some(t) = task_opt {
                    let _ = worker_txs[next].send(t).await;
                    next = (next + 1) % n;
                }
            }
        }

        // task channel 关闭，通知所有 worker 退出
        for tx in &worker_txs {
            let _ = tx.send(TaskMessage::Shutdown).await;
        }
    }

    /// 工作线程主循环
    ///
    /// 每个 worker 拥有独立的 Receiver（无 Mutex），idle_timeout 直接作用于 recv()
    ///
    /// # 参数
    /// - `worker_id`: 工作线程 ID
    /// - `task_rx`: worker 专属任务接收端（无需加锁）
    /// - `result_tx`: 结果发送端
    /// - `client`: reqwest 客户端（复用连接池）
    /// - `idle_timeout`: 空闲超时（秒），0 = 永不超时
    /// - `max_response_size`: 最大响应体大小
    async fn worker_loop(
        _worker_id: usize,
        mut task_rx: mpsc::Receiver<TaskMessage>,
        result_tx: mpsc::Sender<ResultMessage>,
        client: reqwest::Client,
        idle_timeout: u64,
        max_response_size: usize,
    ) {
        loop {
            // 获取任务
            // idle_timeout == 0 时永久等待（默认行为，线程池保持 worker 存活）
            // idle_timeout > 0 时空闲超时退出
            // 注意：timeout 直接作用于 worker 自己的 recv()，不持有任何锁
            let task = if idle_timeout > 0 {
                match tokio::time::timeout(Duration::from_secs(idle_timeout), task_rx.recv()).await
                {
                    Ok(Some(task)) => task,
                    Ok(None) => break, // channel 关闭
                    Err(_) => break,   // 空闲超时，退出工作线程
                }
            } else {
                match task_rx.recv().await {
                    Some(task) => task,
                    None => break,
                }
            };

            match task {
                TaskMessage::Request { request, stream_tx } => {
                    // 执行请求（复用 worker 闭包捕获的 client）
                    let request_id = request
                        .get_id()
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| request.get_url().to_string());

                    // 提取用户自定义数据（随结果原样带回）
                    let user_data = request.get_user_data().map(|s| s.to_string());

                    let start = Instant::now();

                    // 执行请求
                    let result = Self::execute_request(
                        client.clone(),
                        request,
                        request_id.clone(),
                        stream_tx,
                        max_response_size,
                    )
                    .await;

                    let elapsed = start.elapsed();

                    // 构建结果
                    let result_msg = match result {
                        Ok(response) => ResultMessage::Completed(RequestResult::success(
                            request_id, user_data, response, elapsed,
                        )),
                        Err(e) => ResultMessage::Completed(RequestResult::error(
                            request_id,
                            user_data,
                            e.to_string(),
                            elapsed,
                        )),
                    };

                    // 发送结果
                    if result_tx.send(result_msg).await.is_err() {
                        // 结果 channel 已关闭，退出工作线程
                        break;
                    }
                }
                TaskMessage::Shutdown => {
                    // 收到关闭信号
                    break;
                }
            }
        }

        // 通知主线程工作线程已关闭
        let _ = result_tx.send(ResultMessage::WorkerShutdown).await;
    }

    /// 执行单个 HTTP 请求
    /// 委托给 `crate::executor::execute_request`，与 XhMulti、fiber 共用同一实现
    async fn execute_request(
        client: reqwest::Client,
        request: XhRequest,
        request_id: String,
        stream_tx: Option<mpsc::Sender<(String, StreamEvent)>>,
        max_response_size: usize,
    ) -> XhCurlResult<XhResponse> {
        crate::executor::execute_request(client, request, request_id, stream_tx, max_response_size)
            .await
    }

    /// 提交单个请求到线程池
    ///
    /// # 参数
    /// - `request`: 请求构建器
    ///
    /// # 返回
    /// - `Ok(())`: 提交成功
    /// - `Err`: 线程池未启动或队列已满
    pub fn submit(&self, request: XhRequest) -> XhCurlResult<()> {
        let task = TaskMessage::Request {
            request,
            stream_tx: None,
        };

        let task_tx = self
            .task_tx
            .as_ref()
            .ok_or_else(|| XhCurlError::ThreadPool("线程池未启动".to_string()))?;

        task_tx
            .try_send(task)
            .map_err(|e| XhCurlError::ThreadPool(format!("提交任务失败: {}", e)))?;

        Ok(())
    }

    /// 提交单个请求（带流式回调）
    pub fn submit_with_stream(
        &self,
        request: XhRequest,
        stream_tx: mpsc::Sender<(String, StreamEvent)>,
    ) -> XhCurlResult<()> {
        let task = TaskMessage::Request {
            request,
            stream_tx: Some(stream_tx),
        };

        let task_tx = self
            .task_tx
            .as_ref()
            .ok_or_else(|| XhCurlError::ThreadPool("线程池未启动".to_string()))?;

        task_tx
            .try_send(task)
            .map_err(|e| XhCurlError::ThreadPool(format!("提交任务失败: {}", e)))?;

        Ok(())
    }

    /// 批量提交请求并等待所有结果
    ///
    /// # 参数
    /// - `requests`: 请求列表
    ///
    /// # 返回
    /// 所有请求的结果列表（数量等于成功提交的请求数）
    pub async fn execute_all(
        &mut self,
        requests: Vec<XhRequest>,
    ) -> XhCurlResult<Vec<RequestResult>> {
        // 空列表提前返回：避免无谓地启动 worker（资源浪费）。
        // 即使池已运行，空列表也无需提交/收集。
        if requests.is_empty() {
            return Ok(Vec::new());
        }

        if !self.is_running {
            self.start()?;
        }

        // drain 上次调用可能残留的陈旧消息（如 WorkerShutdown），
        // 防止跨调用结果污染（自定义 idle_timeout > 0 场景下 worker 退出会留下残留消息）
        if let Some(rx) = self.result_rx.as_mut() {
            while rx.try_recv().is_ok() {}
        }

        // 提交所有请求，统计成功与失败数量
        // 任一提交失败（队列满）即视为整体失败：已提交的请求不收集结果，
        // 直接返回错误，由调用方决定是否重试（避免静默返回部分结果）。
        let mut submitted_count = 0;
        let mut failed_count = 0;
        for request in requests {
            match self.submit(request) {
                Ok(()) => submitted_count += 1,
                Err(_) => {
                    failed_count += 1;
                }
            }
        }

        if failed_count > 0 {
            return Err(XhCurlError::ThreadPool(format!(
                "{} 个请求提交失败（队列容量 {}），请减少批量大小或增大队列容量",
                failed_count, DEFAULT_QUEUE_CAPACITY
            )));
        }

        // 收集结果（数量等于成功提交的请求数）
        let mut results = Vec::with_capacity(submitted_count);
        let mut shutdown_count = 0;

        let result_rx = self
            .result_rx
            .as_mut()
            .ok_or_else(|| XhCurlError::ThreadPool("结果接收端不存在".to_string()))?;

        // 只要结果未收齐就继续等待
        // 仅在所有 worker 关闭或 channel 关闭时退出
        while results.len() < submitted_count {
            match result_rx.recv().await {
                Some(ResultMessage::Completed(result)) => {
                    results.push(result);
                }
                Some(ResultMessage::WorkerShutdown) => {
                    shutdown_count += 1;
                    // 所有工作线程都已关闭，无法再收到结果
                    if shutdown_count >= self.config.worker_count {
                        break;
                    }
                }
                None => {
                    // channel 关闭，无法再收到结果
                    break;
                }
            }
        }

        // 完整性检查：worker panic 或提前退出可能导致结果数量不足
        // 此时返回错误而非静默返回不完整结果
        if results.len() != submitted_count {
            return Err(XhCurlError::ThreadPool(format!(
                "部分任务异常退出：预期 {} 个结果，实际收到 {} 个",
                submitted_count,
                results.len()
            )));
        }

        Ok(results)
    }

    /// 关闭线程池
    /// 发送关闭信号，等待 dispatcher 和所有工作线程退出
    pub async fn shutdown(&mut self) {
        // drop task_tx → dispatcher 的 recv() 返回 None → dispatcher 向所有 worker 发 Shutdown
        self.task_tx = None;

        // 等待 dispatcher 退出（它会向所有 worker 发送 Shutdown）
        if let Some(handle) = self.dispatcher_handle.take() {
            let _ = handle.await;
        }

        // 等待所有工作线程退出
        for handle in self.workers.drain(..) {
            let _ = handle.await;
        }

        // 清理 channel
        self.result_rx = None;
        self.is_running = false;
    }

    /// 获取工作线程数量
    pub fn worker_count(&self) -> usize {
        self.config.worker_count
    }

    /// 检查是否正在运行
    pub fn is_running(&self) -> bool {
        self.is_running
    }

    /// 获取结果接收端的可变引用
    ///
    /// 供 php_ext.rs 的 `executeEach` 流式回调使用：调用方先 `submit()` 提交请求，
    /// 再通过此方法拿到 `result_rx` 自行 `recv()`，每收到一个结果就调用回调，
    /// 不再通过 `execute_all()` 累积全部结果（内存恒定）。
    ///
    /// 与 `execute_all` 内部 `self.result_rx.as_mut()` 等价，仅暴露给同 crate 的流式路径。
    #[cfg(feature = "php")]
    pub(crate) fn result_rx_mut(&mut self) -> Option<&mut mpsc::Receiver<ResultMessage>> {
        self.result_rx.as_mut()
    }
}

impl Drop for XhThreadPool {
    fn drop(&mut self) {
        // drop task_tx 通知 dispatcher 退出
        self.task_tx = None;

        // drop 中不能 await，abort 所有 task
        if let Some(handle) = self.dispatcher_handle.take() {
            handle.abort();
        }
        // abort 所有仍在运行的 worker task，避免任务泄漏
        for handle in self.workers.drain(..) {
            handle.abort();
        }
    }
}

impl std::fmt::Debug for XhThreadPool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("XhThreadPool")
            .field("config", &self.config)
            .field("worker_count", &self.workers.len())
            .field("is_running", &self.is_running)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试线程池配置默认值
    #[test]
    fn test_default_config() {
        let config = ThreadPoolConfig::default();
        assert!(config.worker_count > 0);
        assert_eq!(config.queue_capacity, DEFAULT_QUEUE_CAPACITY);
        assert_eq!(config.idle_timeout, 0); // 默认永不超时
        assert_eq!(config.max_response_size, DEFAULT_MAX_RESPONSE_SIZE);
    }

    /// 测试线程池创建
    #[test]
    fn test_threadpool_creation() {
        let client = reqwest::Client::new();
        let pool = XhThreadPool::with_default_config(client);

        assert!(!pool.is_running());
        assert!(pool.worker_count() > 0);
    }

    /// 测试任务消息
    #[test]
    fn test_task_message() {
        let request = XhRequest::new("https://example.com");

        let msg = TaskMessage::Request {
            request,
            stream_tx: None,
        };

        assert!(matches!(msg, TaskMessage::Request { .. }));

        let shutdown = TaskMessage::Shutdown;
        assert!(matches!(shutdown, TaskMessage::Shutdown));
    }

    /// 测试结果消息
    #[test]
    fn test_result_message() {
        let result = RequestResult::success(
            "req1".to_string(),
            None,
            XhResponse::from_error(
                "test".to_string(),
                "https://example.com".to_string(),
                Duration::from_secs(0),
            ),
            Duration::from_secs(1),
        );

        let msg = ResultMessage::Completed(result);
        assert!(matches!(msg, ResultMessage::Completed(_)));

        let shutdown = ResultMessage::WorkerShutdown;
        assert!(matches!(shutdown, ResultMessage::WorkerShutdown));
    }

    /// start() 时 worker_count=0 应返回错误，且不启动（is_running 保持 false）。
    /// 错误路径在调用 tokio::spawn 之前返回，故无需运行时。
    #[test]
    fn test_start_with_zero_workers_returns_error() {
        let config = ThreadPoolConfig {
            worker_count: 0,
            ..Default::default()
        };
        let client = reqwest::Client::new();
        let mut pool = XhThreadPool::new(config, client);
        let result = pool.start();
        assert!(result.is_err());
        assert!(!pool.is_running());
    }

    /// start() 前 submit 应返回错误（task_tx 为 None）
    #[test]
    fn test_submit_before_start_returns_error() {
        let client = reqwest::Client::new();
        let pool = XhThreadPool::with_default_config(client);
        let result = pool.submit(XhRequest::new("https://x.com"));
        assert!(result.is_err());
    }

    /// execute_all 空列表应返回空结果，且不应启动线程池（避免无谓 worker 创建）。
    /// 当前实现先 start() 再判空，会浪费资源；应将空检查提前。
    #[tokio::test]
    async fn test_execute_all_empty_does_not_start_pool() {
        let client = reqwest::Client::new();
        let mut pool = XhThreadPool::with_default_config(client);
        let results = pool.execute_all(vec![]).await.unwrap();
        assert!(results.is_empty());
        // 空列表时无需启动 worker，避免资源浪费
        assert!(!pool.is_running());
    }
}
