// +----------------------------------------------------------------------+
// | XHCurl 扩展 - PHP 扩展入口（ext-php-rs 绑定）                         |
// |                                                                        |
// | 使用 ext-php-rs crate 直接生成 PHP 扩展                                |
// | 无需 C 桥接层，Rust 函数直接暴露为 PHP 函数/方法                       |
// |                                                                        |
// | PHP 类映射：                                                           |
// |   XHCurl      → 全局管理器（静态方法）                                 |
// |   XHRequest   → 请求构建器（链式调用）                                 |
// |   XHMulti     → 异步批量执行器                                         |
// |   XHThreadPool→ 线程池                                                 |
// |   （响应统一以关联数组返回，无 XHResponse PHP 类）                        |
// |                                                                        |
// | 链式调用实现：                                                          |
// |   ext-php-rs 0.14+：方法第一个参数命名为 self_（类型 &mut ZendClassObject<>），|
// |   修改后返回同一对象，实现 PHP 端 $this 链式调用。                      |
// +----------------------------------------------------------------------+

use std::sync::OnceLock;

use ext_php_rs::boxed::ZBox;
use ext_php_rs::convert::IntoZvalDyn;
use ext_php_rs::prelude::*;
use ext_php_rs::types::{ArrayKey, ZendCallable, ZendClassObject, ZendHashTable, Zval};
use ext_php_rs::zend::php_sapi_name;
use tokio::sync::mpsc;

use crate::curl::XhCurlManager;
use crate::error::MAX_REQUESTS_PER_BATCH;
use crate::multi::{StreamEvent, XhMulti, STREAM_CHANNEL_CAPACITY};
use crate::request::{clear_request_client_cache, HttpMethod, XhRequest};
use crate::response::XhResponse;
use crate::threadpool::{ResultMessage, ThreadPoolConfig, XhThreadPool};

// +----------------------------------------------------------------------+
// | 常量定义                                                              |
// +----------------------------------------------------------------------+

// MAX_REQUESTS_PER_BATCH 定义在 error.rs，供 multi / threadpool / php_ext 共用

// +----------------------------------------------------------------------+
// | 全局运行时与客户端复用                                                |
// +----------------------------------------------------------------------+

/// 检测当前是否为 CLI 模式（通过 PHP SAPI 名称）。
///
/// 对应 PHP 的 `php_sapi_name() === 'cli'`。
/// FPM/fpm-fcgi 等多进程 SAPI 下返回 false，此时禁止使用多线程运行时，
/// 避免 tokio 工作线程与 PHP 内存管理器（TSRM/内存池）冲突。
pub(crate) fn sapi_is_cli() -> bool {
    php_sapi_name() == "cli"
}

/// 获取全局共享的 reqwest 客户端。
///
/// reqwest Client 内部维护连接池（TCP keep-alive、TLS 会话缓存），
/// 全局复用可避免每次请求重新建连，显著提升批量请求性能。
/// 使用 OnceLock 保证线程安全的延迟初始化（仅创建一次）。
///
/// # 代理配置失败处理
/// 若全局代理地址无效（`create_client` 返回错误），返回 `Err`，由调用方传播为 PHP 异常。
/// 理由：
/// 1. 代理通常是安全/隐私相关配置，静默降级到无代理会让用户误以为
///    请求走了代理，实际暴露真实 IP，这比直接报错更危险。
/// 2. panic 会杀死 PHP 进程（FPM worker 崩溃重启），改为返回错误让用户可 try/catch。
/// 3. 与 `create_client_builder` 的设计一致：代理无效必须明确报错。
/// 4. 错误以 PHP 异常形式抛出，用户修正配置后可重试。
pub(crate) fn global_client() -> Result<&'static reqwest::Client, String> {
    static CLIENT: OnceLock<Result<reqwest::Client, String>> = OnceLock::new();
    let result = CLIENT.get_or_init(|| {
        XhCurlManager::global().create_client().map_err(|e| {
            format!(
                "全局 reqwest 客户端初始化失败：{}（请检查 proxy/SSL 等全局配置）",
                e
            )
        })
    });
    result.as_ref().map_err(|e| e.clone())
}

/// 获取全局共享的 tokio 运行时。
///
/// 运行时类型根据 SAPI 自动选择：
/// - CLI 模式：多线程运行时（真正的 M:N 并行，工作线程数 = CPU 核心数）
/// - FPM 模式：单线程运行时（协作式并发，避免线程安全问题）
///
/// 全局复用避免每次 execute() 都创建/销毁运行时（线程创建、IO 驱动注册开销大）。
/// FPM 下每个 worker 进程持有独立的运行时（进程级单例），请求间复用。
pub(crate) fn global_runtime() -> Result<&'static tokio::runtime::Runtime, String> {
    static RUNTIME: OnceLock<Result<tokio::runtime::Runtime, String>> = OnceLock::new();
    let result = RUNTIME.get_or_init(|| {
        let mut builder = if sapi_is_cli() {
            tokio::runtime::Builder::new_multi_thread()
        } else {
            tokio::runtime::Builder::new_current_thread()
        };
        builder
            .enable_all()
            .thread_name("xhcurl-worker")
            .build()
            .map_err(|e| format!("初始化全局 tokio 运行时失败：{}", e))
    });
    result.as_ref().map_err(|e| e.clone())
}

// +----------------------------------------------------------------------+
// | PHP 类：XHCurl（全局管理器）                                          |
// +----------------------------------------------------------------------+

/// PHP XHCurl 类的 Rust 表示
/// 对应 C 版本的 XHCurl 全局管理器
#[php_class]
#[php(name = "XHCurl")]
pub struct PhpXhCurl;

/// 从 PHP 数组中提取 XHRequest 对象，转为 Vec<XhRequest>
///
/// 供 coroutine_gather / coroutine_each 共用，消除重复解析逻辑。
fn extract_requests(requests: &ZendHashTable) -> Result<Vec<XhRequest>, String> {
    use ext_php_rs::convert::FromZval;

    // 批量上限检查：在克隆前检查，避免先克隆全部元素再拒绝导致内存浪费/OOM。
    // 与 XhMulti::add / XHThreadPool::add 的"检查先于操作"模式一致。
    let len = requests.len();
    if len > MAX_REQUESTS_PER_BATCH {
        return Err(format!(
            "请求数量 {} 超过单批上限 {}（请分组执行）",
            len, MAX_REQUESTS_PER_BATCH
        ));
    }

    let mut req_list: Vec<XhRequest> = Vec::new();
    for_each_kv(requests, |_key, val| {
        // 尝试将 Zval 转为 &ZendClassObject<PhpXhRequest>
        let class_obj: Option<&ZendClassObject<PhpXhRequest>> =
            <&ZendClassObject<PhpXhRequest> as FromZval>::from_zval(val);
        match class_obj {
            Some(obj) => req_list.push(obj.request.clone()),
            None => return Err("数组元素不是 XHRequest 对象".to_string()),
        }
        Ok(())
    })?;
    Ok(req_list)
}

/// PHP XHCurl 类的方法实现
#[php_impl]
impl PhpXhCurl {
    /// 获取扩展版本
    ///
    /// # PHP 签名
    /// public static XHCurl::version(): string
    pub fn version() -> String {
        env!("CARGO_PKG_VERSION").to_string()
    }

    /// 设置全局配置
    ///
    /// # PHP 签名
    /// public static XHCurl::setConfig(array $config): void
    pub fn set_config(config: &ZendHashTable) -> Result<(), String> {
        // 获取全局管理器单例
        let manager = XhCurlManager::global();

        // 收集类型不匹配的配置项名，便于向用户反馈哪些配置未生效
        let mut type_mismatches: Vec<&str> = Vec::new();

        // 使用闭包修改配置，避免手动管理锁
        manager.modify_config(|c| {
            // 从 PHP 数组读取配置项
            // 每个配置项都是可选的，只处理存在的键

            // 连接超时（秒），负值跳过
            if let Some(timeout) = config.get("connect_timeout") {
                if let Some(v) = timeout.long() {
                    if v >= 0 {
                        c.connect_timeout = v as u64;
                    }
                } else {
                    type_mismatches.push("connect_timeout");
                }
            }

            // 请求超时（秒），负值跳过
            if let Some(timeout) = config.get("request_timeout") {
                if let Some(v) = timeout.long() {
                    if v >= 0 {
                        c.request_timeout = v as u64;
                    }
                } else {
                    type_mismatches.push("request_timeout");
                }
            }

            // 最大响应体大小（字节），防止内存溢出，负值跳过
            if let Some(max_size) = config.get("max_response_size") {
                if let Some(v) = max_size.long() {
                    if v >= 0 {
                        c.max_response_size = v as usize;
                    }
                } else {
                    type_mismatches.push("max_response_size");
                }
            }

            // 是否跟随重定向
            if let Some(follow) = config.get("follow_redirects") {
                if let Some(v) = follow.bool() {
                    c.follow_redirects = v;
                } else {
                    type_mismatches.push("follow_redirects");
                }
            }

            // 最大重定向次数，负值跳过
            if let Some(max_redirects) = config.get("max_redirects") {
                if let Some(v) = max_redirects.long() {
                    if v >= 0 {
                        c.max_redirects = v as u32;
                    }
                } else {
                    type_mismatches.push("max_redirects");
                }
            }

            // 是否验证 SSL 证书
            if let Some(verify) = config.get("verify_ssl") {
                if let Some(v) = verify.bool() {
                    c.verify_ssl = v;
                } else {
                    type_mismatches.push("verify_ssl");
                }
            }

            // 自定义 User-Agent
            if let Some(ua) = config.get("user_agent") {
                if let Some(v) = ua.string() {
                    c.user_agent = v;
                } else {
                    type_mismatches.push("user_agent");
                }
            }

            // 代理地址
            // 接受 string（设置代理）或 null（清除代理），与 getConfig() 返回 null 对称，
            // 确保 getConfig() → setConfig($orig) 往返不报类型错误。
            if let Some(proxy) = config.get("proxy") {
                if proxy.is_null() {
                    c.proxy = None;
                } else if let Some(v) = proxy.string() {
                    c.proxy = Some(v);
                } else {
                    type_mismatches.push("proxy");
                }
            }

            // 是否启用 HTTP/2
            if let Some(v) = config.get("http2_enabled") {
                if let Some(b) = v.bool() {
                    c.http2_enabled = b;
                } else {
                    type_mismatches.push("http2_enabled");
                }
            }

            // 是否启用 TCP Keep-Alive
            if let Some(v) = config.get("tcp_keepalive") {
                if let Some(b) = v.bool() {
                    c.tcp_keepalive = b;
                } else {
                    type_mismatches.push("tcp_keepalive");
                }
            }

            // TCP Keep-Alive 间隔（秒），负值跳过
            if let Some(v) = config.get("tcp_keepalive_interval") {
                if let Some(l) = v.long() {
                    if l >= 0 {
                        c.tcp_keepalive_interval = l as u64;
                    }
                } else {
                    type_mismatches.push("tcp_keepalive_interval");
                }
            }

            // 默认并发连接数限制，负值跳过
            if let Some(v) = config.get("max_connections") {
                if let Some(l) = v.long() {
                    if l >= 0 {
                        c.max_connections = l as usize;
                    }
                } else {
                    type_mismatches.push("max_connections");
                }
            }

            // 协程 gather/each 并发上限，负值跳过
            // 0 = 不限制；默认 64
            if let Some(v) = config.get("fiber_max_concurrency") {
                if let Some(l) = v.long() {
                    if l >= 0 {
                        c.fiber_max_concurrency = l as usize;
                    }
                } else {
                    type_mismatches.push("fiber_max_concurrency");
                }
            }
        });

        // 全局配置变更后，请求级 Client 缓存（按 OverrideKey 缓存）中已有的 Client
        // 是基于旧全局配置构建的（UA/keepalive/连接池/TLS 等），需清空以使后续构建
        // 的 Client 反映新配置。global_client（无覆盖时的全局单例）走 OnceLock 不会
        // 重建，但请求级 Client 缓存必须主动失效。
        clear_request_client_cache();

        if !type_mismatches.is_empty() {
            return Err(format!(
                "以下配置项的类型与期望不符，未生效: {}",
                type_mismatches.join(", ")
            ));
        }
        Ok(())
    }

    /// 获取全局配置
    ///
    /// # PHP 签名
    /// public static XHCurl::getConfig(): array
    pub fn get_config() -> Result<ZBox<ZendHashTable>, String> {
        // 获取全局管理器单例
        let manager = XhCurlManager::global();
        // 读取配置快照（避免长时间持有锁）
        let config = manager.config();

        // 构建 PHP 关联数组
        let mut ht = ZendHashTable::new();
        // insert 返回 Result<()>，使用 let _ = 忽略潜在错误
        let _ = ht.insert("connect_timeout", config.connect_timeout as i64);
        let _ = ht.insert("request_timeout", config.request_timeout as i64);
        let _ = ht.insert("max_response_size", config.max_response_size as i64);
        let _ = ht.insert("follow_redirects", config.follow_redirects);
        let _ = ht.insert("max_redirects", config.max_redirects as i64);
        let _ = ht.insert("verify_ssl", config.verify_ssl);
        let _ = ht.insert("http2_enabled", config.http2_enabled);
        let _ = ht.insert("user_agent", config.user_agent.as_str());
        let _ = ht.insert("tcp_keepalive", config.tcp_keepalive);
        let _ = ht.insert(
            "tcp_keepalive_interval",
            config.tcp_keepalive_interval as i64,
        );
        let _ = ht.insert("max_connections", config.max_connections as i64);
        let _ = ht.insert("fiber_max_concurrency", config.fiber_max_concurrency as i64);

        // 代理地址（None 时为 null，与 setConfig 接受 null 对称）
        let _ = ht.insert("proxy", config.proxy.as_deref());

        Ok(ht)
    }

    /// 检查是否为 CLI 模式
    ///
    /// # PHP 签名
    /// public static XHCurl::isCli(): bool
    pub fn is_cli() -> bool {
        sapi_is_cli()
    }

    /// 创建请求构建器
    ///
    /// # PHP 签名
    /// public static XHCurl::createRequest(string $url): XHRequest
    pub fn create_request(url: String) -> Result<PhpXhRequest, String> {
        Ok(PhpXhRequest {
            request: XhRequest::new(url),
        })
    }

    /// 协程式等待单个 HTTP 请求完成
    ///
    /// 必须在 XHCurl::run() 的回调内、且在 Fiber 上下文中调用。
    /// 调用后挂起当前 Fiber，HTTP 请求在 tokio 工作线程上异步执行，
    /// 完成后由事件泵恢复 Fiber 并返回结果数组。
    ///
    /// # PHP 签名
    /// public static XHCurl::await(XHRequest $request): array
    #[php(name = "await")]
    pub fn coroutine_await(
        request: &ZendClassObject<PhpXhRequest>,
    ) -> Result<ZBox<ZendHashTable>, String> {
        crate::fiber::fiber_await(&request.request).map_err(|e| e.to_string())
    }

    /// 启动协程事件泵，执行主回调
    ///
    /// 主回调中可调用 XHCurl::await() 挂起当前 Fiber 等待 HTTP 请求，
    /// 实现协程式异步编程（类似 ReactPHP/AMPHP）。
    ///
    /// # PHP 签名
    /// public static XHCurl::run(callable $main): mixed
    #[php(name = "run")]
    pub fn coroutine_run(main: &Zval) -> Result<Zval, String> {
        crate::fiber::fiber_run(main).map_err(|e| e.to_string())
    }

    /// 并发发起多个 HTTP 请求，按完成顺序返回结果
    ///
    /// 必须在 XHCurl::run() 的回调内、且在 Fiber 上下文中调用。
    /// 一次性将所有请求提交到 tokio 工作线程并行执行，
    /// 返回结果按**完成顺序**排列（非请求提交顺序）。
    ///
    /// # PHP 签名
    /// public static XHCurl::gather(array $requests): array
    #[php(name = "gather")]
    pub fn coroutine_gather(requests: &ZendHashTable) -> Result<ZBox<ZendHashTable>, String> {
        let req_list = extract_requests(requests)?;
        crate::fiber::fiber_gather(req_list).map_err(|e| e.to_string())
    }

    /// XHCurl::each - 流式回调并发执行
    /// 每完成一个请求立即调用回调，不累积全部结果（内存恒定）
    ///
    /// # PHP 签名
    /// public static XHCurl::each(array $requests, callable $callback): int
    #[php(name = "each")]
    pub fn coroutine_each(requests: &ZendHashTable, callback: &Zval) -> Result<i64, String> {
        let req_list = extract_requests(requests)?;
        crate::fiber::fiber_each(req_list, callback)
    }
}

// +----------------------------------------------------------------------+
// | PHP 类：XHRequest（请求构建器，链式调用）                             |
// +----------------------------------------------------------------------+

/// PHP XHRequest 类的 Rust 表示
#[php_class]
#[php(name = "XHRequest")]
pub struct PhpXhRequest {
    /// 内部请求构建器（pub 供 fiber 模块访问）
    pub request: XhRequest,
}

/// PHP XHRequest 类的方法实现
///
/// 链式调用实现：
/// ext-php-rs 0.14+：方法第一个参数命名为 self_（&mut ZendClassObject<PhpXhCurl>），
/// 修改内部状态后返回同一对象引用，
/// 使 PHP 端可以 $req->get()->header("X","Y")->timeout(30) 链式调用。
#[php_impl]
impl PhpXhRequest {
    /// 构造函数
    ///
    /// # PHP 签名
    /// public XHRequest::__construct(string $url)
    pub fn __construct(url: String) -> Self {
        Self {
            request: XhRequest::new(url),
        }
    }

    /// 设置 HTTP 方法
    ///
    /// # PHP 签名
    /// public XHRequest::method(string $method): $self_
    ///
    /// 无效方法名时跳过设置（保留原值），链式调用不中断。
    pub fn method(
        self_: &mut ZendClassObject<PhpXhRequest>,
        method: String,
    ) -> &mut ZendClassObject<PhpXhRequest> {
        // 无效方法名时跳过设置，保持链式调用不中断
        if let Ok(m) = HttpMethod::from_str(&method) {
            self_.request = self_.request.clone().method(m);
        }
        self_
    }

    /// 设置为 GET 方法
    ///
    /// # PHP 签名
    /// public XHRequest::get(): $self_
    pub fn get(self_: &mut ZendClassObject<PhpXhRequest>) -> &mut ZendClassObject<PhpXhRequest> {
        self_.request = self_.request.clone().get();
        self_
    }

    /// 设置为 POST 方法
    pub fn post(self_: &mut ZendClassObject<PhpXhRequest>) -> &mut ZendClassObject<PhpXhRequest> {
        self_.request = self_.request.clone().post();
        self_
    }

    /// 设置为 PUT 方法
    pub fn put(self_: &mut ZendClassObject<PhpXhRequest>) -> &mut ZendClassObject<PhpXhRequest> {
        self_.request = self_.request.clone().put();
        self_
    }

    /// 设置为 DELETE 方法
    pub fn delete(self_: &mut ZendClassObject<PhpXhRequest>) -> &mut ZendClassObject<PhpXhRequest> {
        self_.request = self_.request.clone().delete();
        self_
    }

    /// 设置为 PATCH 方法
    pub fn patch(self_: &mut ZendClassObject<PhpXhRequest>) -> &mut ZendClassObject<PhpXhRequest> {
        self_.request = self_.request.clone().patch();
        self_
    }

    /// 设置为 HEAD 方法（仅获取响应头，不返回响应体）
    pub fn head(self_: &mut ZendClassObject<PhpXhRequest>) -> &mut ZendClassObject<PhpXhRequest> {
        self_.request = self_.request.clone().head();
        self_
    }

    /// 设置为 OPTIONS 方法
    ///
    /// # PHP 签名
    /// public XHRequest::options(): $self_
    pub fn options(
        self_: &mut ZendClassObject<PhpXhRequest>,
    ) -> &mut ZendClassObject<PhpXhRequest> {
        self_.request = self_.request.clone().options();
        self_
    }

    /// 设置请求头
    ///
    /// # PHP 签名
    /// public XHRequest::header(string $name, string $value): $self_
    pub fn header(
        self_: &mut ZendClassObject<PhpXhRequest>,
        name: String,
        value: String,
    ) -> &mut ZendClassObject<PhpXhRequest> {
        self_.request = self_.request.clone().header(&name, &value);
        self_
    }

    /// 设置 JSON 请求体
    ///
    /// # PHP 签名
    /// public XHRequest::json(array $data): $self_
    ///
    /// JSON 序列化失败时跳过设置（保留原值），链式调用不中断。
    pub fn json<'a>(
        self_: &'a mut ZendClassObject<PhpXhRequest>,
        data: &ZendHashTable,
    ) -> &'a mut ZendClassObject<PhpXhRequest> {
        // 序列化或设置失败时跳过，保持链式调用不中断
        if let Ok(json_str) = php_array_to_json(data) {
            if let Ok(req) = self_.request.clone().body_json_str(&json_str) {
                self_.request = req;
            }
        }
        self_
    }

    /// 设置表单数据
    ///
    /// # PHP 签名
    /// public XHRequest::form(array $data): $self_
    pub fn form<'a>(
        self_: &'a mut ZendClassObject<PhpXhRequest>,
        data: &ZendHashTable,
    ) -> &'a mut ZendClassObject<PhpXhRequest> {
        let form = php_array_to_form(data);
        self_.request = self_.request.clone().body_form(form);
        self_
    }

    /// 设置原始请求体（二进制安全）
    ///
    /// PHP 字符串本质是字节序列，可包含任意字节（图片、压缩数据等）。
    /// 这里通过 `&Zval` 直接取原始字节，避免 `String` 参数走严格 UTF-8
    /// 校验导致二进制数据丢失。
    ///
    /// # PHP 签名
    /// public XHRequest::body(string $data): $self_
    pub fn body<'a>(
        self_: &'a mut ZendClassObject<PhpXhRequest>,
        data: &Zval,
    ) -> &'a mut ZendClassObject<PhpXhRequest> {
        // 优先二进制安全读取；回退到 string()（兼容 PHP 端传入数值等隐式转换场景）
        let bytes = if let Some(b) = data.binary::<u8>() {
            b
        } else if let Some(s) = data.string() {
            s.into_bytes()
        } else {
            Vec::new()
        };
        self_.request = self_.request.clone().body_bytes(bytes);
        self_
    }

    /// 设置请求超时（秒）
    ///
    /// # PHP 签名
    /// public XHRequest::timeout(int $seconds): $self_
    ///
    /// 负值跳过设置（保留原值），与 `setConfig` 的"负值跳过"行为一致。
    pub fn timeout(
        self_: &mut ZendClassObject<PhpXhRequest>,
        seconds: i64,
    ) -> &mut ZendClassObject<PhpXhRequest> {
        // 负值跳过（保留原值），避免 i64→u64 转换产生巨大数值
        if seconds >= 0 {
            self_.request = self_.request.clone().request_timeout(seconds as u64);
        }
        self_
    }

    /// 设置连接超时（秒）
    /// 与 timeout 不同，此方法仅控制连接阶段的超时
    ///
    /// # PHP 签名
    /// public XHRequest::connectTimeout(int $seconds): $self_
    ///
    /// 负值跳过设置（保留原值），与 `setConfig` 的"负值跳过"行为一致。
    pub fn connect_timeout(
        self_: &mut ZendClassObject<PhpXhRequest>,
        seconds: i64,
    ) -> &mut ZendClassObject<PhpXhRequest> {
        // 负值跳过（保留原值）
        if seconds >= 0 {
            self_.request = self_.request.clone().connect_timeout(seconds as u64);
        }
        self_
    }

    /// 设置是否验证 SSL 证书
    ///
    /// # PHP 签名
    /// public XHRequest::verifySsl(bool $verify): $self_
    pub fn verify_ssl(
        self_: &mut ZendClassObject<PhpXhRequest>,
        verify: bool,
    ) -> &mut ZendClassObject<PhpXhRequest> {
        self_.request = self_.request.clone().verify_ssl(verify);
        self_
    }

    /// 设置 User-Agent
    ///
    /// # PHP 签名
    /// public XHRequest::userAgent(string $ua): $self_
    pub fn user_agent(
        self_: &mut ZendClassObject<PhpXhRequest>,
        ua: String,
    ) -> &mut ZendClassObject<PhpXhRequest> {
        self_.request = self_.request.clone().user_agent(ua);
        self_
    }

    /// 设置代理地址
    /// 支持 HTTP/HTTPS/SOCKS5 代理
    ///
    /// # PHP 签名
    /// public XHRequest::proxy(string $proxy): $self_
    ///
    /// # 示例
    /// $req->proxy("http://127.0.0.1:7890");
    /// $req->proxy("socks5://127.0.0.1:1080");
    pub fn proxy(
        self_: &mut ZendClassObject<PhpXhRequest>,
        proxy: String,
    ) -> &mut ZendClassObject<PhpXhRequest> {
        self_.request = self_.request.clone().proxy(proxy);
        self_
    }

    /// 设置是否跟随重定向
    ///
    /// # PHP 签名
    /// public XHRequest::followRedirects(bool $follow): $self_
    pub fn follow_redirects(
        self_: &mut ZendClassObject<PhpXhRequest>,
        follow: bool,
    ) -> &mut ZendClassObject<PhpXhRequest> {
        self_.request = self_.request.clone().follow_redirects(follow);
        self_
    }

    /// 设置最大重定向次数
    ///
    /// # PHP 签名
    /// public XHRequest::maxRedirects(int $max): $self_
    ///
    /// 负值跳过设置（保留原值），与 `setConfig` 的"负值跳过"行为一致。
    pub fn max_redirects(
        self_: &mut ZendClassObject<PhpXhRequest>,
        max: i64,
    ) -> &mut ZendClassObject<PhpXhRequest> {
        // 负值跳过（保留原值）
        if max >= 0 {
            self_.request = self_.request.clone().max_redirects(max as u32);
        }
        self_
    }

    /// 设置用户自定义数据
    ///
    /// 可携带任意结构化数据（数组/对象），随请求传递到结果中返回。
    /// 用于批量请求时关联业务上下文（如任务索引、回调标识、业务参数等）。
    ///
    /// 数据以 JSON 字符串形式存储，结果中通过 `user_data` 字段返回，
    /// PHP 端可用 `json_decode($result['user_data'], true)` 还原。
    ///
    /// # PHP 签名
    /// public XHRequest::setUserData(mixed $data): $self_
    ///
    /// JSON 序列化失败时跳过设置（保留原值），链式调用不中断。
    pub fn set_user_data<'a>(
        self_: &'a mut ZendClassObject<PhpXhRequest>,
        data: &ZendHashTable,
    ) -> &'a mut ZendClassObject<PhpXhRequest> {
        // 序列化失败时跳过，保持链式调用不中断
        if let Ok(json_str) = php_array_to_json(data) {
            self_.request = self_.request.clone().user_data(json_str);
        }
        self_
    }

    /// 设置用户自定义数据（`set_user_data` 的无前缀别名，向后兼容）
    ///
    /// 与 `setUserData()` 等价。ext-php-rs 自动将 Rust snake_case `user_data`
    /// 映射为 PHP `userData()`。
    ///
    /// JSON 序列化失败时跳过设置（保留原值），链式调用不中断。
    ///
    /// # PHP 签名
    /// public XHRequest::userData(array $data): $self_
    pub fn user_data<'a>(
        self_: &'a mut ZendClassObject<PhpXhRequest>,
        data: &ZendHashTable,
    ) -> &'a mut ZendClassObject<PhpXhRequest> {
        // 序列化失败时跳过，保持链式调用不中断
        if let Ok(json_str) = php_array_to_json(data) {
            self_.request = self_.request.clone().user_data(json_str);
        }
        self_
    }

    /// 设置请求 ID（用于批量请求时标识结果）
    ///
    /// 批量请求结果中通过 `id` 字段返回此值，便于 PHP 端关联请求与响应。
    /// 若未设置，结果中 `id` 默认为请求 URL。
    ///
    /// # PHP 签名
    /// public XHRequest::setId(string $id): $self_
    pub fn set_id(
        self_: &mut ZendClassObject<PhpXhRequest>,
        id: String,
    ) -> &mut ZendClassObject<PhpXhRequest> {
        self_.request = self_.request.clone().id(id);
        self_
    }

    /// 设置请求 ID（`set_id` 的无前缀别名，向后兼容）
    ///
    /// 与 `setId()` 等价，结果中通过 `id` 字段返回此值。
    /// ext-php-rs 自动将 Rust snake_case `id` 映射为 PHP `id()`。
    ///
    /// # PHP 签名
    /// public XHRequest::id(string $id): $self_
    pub fn id(
        self_: &mut ZendClassObject<PhpXhRequest>,
        id: String,
    ) -> &mut ZendClassObject<PhpXhRequest> {
        self_.request = self_.request.clone().id(id);
        self_
    }

    /// 设置 HTTP 基本认证
    ///
    /// 对应 curl 的 CURLOPT_USERPWD。
    /// 格式: "username:password"
    ///
    /// # PHP 签名
    /// public XHRequest::basicAuth(string $credentials): $self_
    pub fn basic_auth(
        self_: &mut ZendClassObject<PhpXhRequest>,
        credentials: String,
    ) -> &mut ZendClassObject<PhpXhRequest> {
        self_.request = self_.request.clone().basic_auth(credentials);
        self_
    }

    /// 设置 Bearer Token 认证
    ///
    /// 对应 curl 的 CURLOPT_XOAUTH2_BEARER。
    /// 自动设置 Authorization: Bearer {token}
    ///
    /// # PHP 签名
    /// public XHRequest::bearerToken(string $token): $self_
    pub fn bearer_token(
        self_: &mut ZendClassObject<PhpXhRequest>,
        token: String,
    ) -> &mut ZendClassObject<PhpXhRequest> {
        self_.request = self_.request.clone().bearer_token(token);
        self_
    }

    /// 设置 Cookie 字符串
    ///
    /// 对应 curl 的 CURLOPT_COOKIE。
    /// 格式: "name1=value1; name2=value2"
    ///
    /// # PHP 签名
    /// public XHRequest::cookies(string $cookies): $self_
    pub fn cookies(
        self_: &mut ZendClassObject<PhpXhRequest>,
        cookies: String,
    ) -> &mut ZendClassObject<PhpXhRequest> {
        self_.request = self_.request.clone().cookies(cookies);
        self_
    }

    /// 设置 Accept-Encoding
    ///
    /// 对应 curl 的 CURLOPT_ENCODING。
    /// 如 "gzip, deflate, br"
    ///
    /// # PHP 签名
    /// public XHRequest::encoding(string $encoding): $self_
    pub fn encoding(
        self_: &mut ZendClassObject<PhpXhRequest>,
        encoding: String,
    ) -> &mut ZendClassObject<PhpXhRequest> {
        self_.request = self_.request.clone().encoding(encoding);
        self_
    }

    /// 设置自定义请求方法
    ///
    /// 对应 curl 的 CURLOPT_CUSTOMREQUEST。
    /// 用于非标准 HTTP 方法（CONNECT/TRACE/PROPFIND 等）。
    ///
    /// # PHP 签名
    /// public XHRequest::customMethod(string $method): $self_
    pub fn custom_method(
        self_: &mut ZendClassObject<PhpXhRequest>,
        method: String,
    ) -> &mut ZendClassObject<PhpXhRequest> {
        self_.request = self_.request.clone().custom_method(method);
        self_
    }

    /// 设置 Range 请求范围
    ///
    /// 对应 curl 的 CURLOPT_RANGE。
    /// 格式: "0-1023" 或 "0-" 或 "-1023"
    ///
    /// # PHP 签名
    /// public XHRequest::range(string $range): $self_
    pub fn range(
        self_: &mut ZendClassObject<PhpXhRequest>,
        range: String,
    ) -> &mut ZendClassObject<PhpXhRequest> {
        self_.request = self_.request.clone().range(range);
        self_
    }

    /// 设置多部分表单数据（支持文件上传）
    ///
    /// 对应 curl 的 CURLOPT_HTTPPOST。
    /// 参数格式：
    ///   [
    ///     ['name' => 'field1', 'value' => 'text value'],
    ///     ['name' => 'file1', 'value' => 'file content', 'filename' => 'test.txt', 'content_type' => 'text/plain'],
    ///   ]
    ///
    /// # PHP 签名
    /// public XHRequest::multipart(array $fields): $self_
    ///
    /// 解析失败时跳过设置（保留原值），链式调用不中断。
    pub fn multipart<'a>(
        self_: &'a mut ZendClassObject<PhpXhRequest>,
        fields: &ZendHashTable,
    ) -> &'a mut ZendClassObject<PhpXhRequest> {
        use crate::request::MultipartField;

        let mut mp_fields = Vec::new();
        let len = fields.len();
        let mut iter = fields.iter();
        for _ in 0..len {
            match iter.next() {
                Some((_key, val)) => {
                    // 字段非数组时跳过整个设置，保持链式调用不中断
                    let field_ht = match val.array() {
                        Some(ht) => ht,
                        None => return self_,
                    };
                    let mut name = String::new();
                    let mut value: Vec<u8> = Vec::new();
                    let mut filename: Option<String> = None;
                    let mut content_type: Option<String> = None;

                    let f_len = field_ht.len();
                    let mut f_iter = field_ht.iter();
                    for _ in 0..f_len {
                        if let Some((fkey, fval)) = f_iter.next() {
                            let key_str = fkey.to_string();
                            match key_str.as_str() {
                                "name" => {
                                    if let Some(s) = fval.string() {
                                        name = s;
                                    }
                                }
                                "value" => {
                                    // 二进制安全读取：PHP 字符串本质是字节序列，
                                    // 可能包含任意字节（图片、压缩数据等）。
                                    // Zval::string() 内部做严格 UTF-8 校验，
                                    // 遇到非 UTF-8 字节会返回 None 导致数据丢失。
                                    // 优先用 binary::<u8>() 取原始字节，
                                    // 仅在不是字符串时回退到其他类型转换。
                                    if let Some(bytes) = fval.binary::<u8>() {
                                        value = bytes;
                                    } else if let Some(s) = fval.string() {
                                        value = s.into_bytes();
                                    }
                                }
                                "filename" => {
                                    if let Some(s) = fval.string() {
                                        filename = Some(s);
                                    }
                                }
                                "content_type" => {
                                    if let Some(s) = fval.string() {
                                        content_type = Some(s);
                                    }
                                }
                                _ => {}
                            }
                        }
                    }

                    if let Some(fname) = filename {
                        mp_fields.push(MultipartField::file(
                            name,
                            fname,
                            value,
                            content_type.unwrap_or_else(|| "application/octet-stream".to_string()),
                        ));
                    } else {
                        // 二进制安全：直接传原始字节，不做 UTF-8 转换
                        mp_fields.push(MultipartField::text_bytes(name, value));
                    }
                }
                None => break,
            }
        }

        self_.request = self_.request.clone().body_multipart(mp_fields);
        self_
    }

    /// 获取请求 URL
    pub fn get_url(&self) -> String {
        self.request.get_url().to_string()
    }

    /// 获取 HTTP 方法
    pub fn get_method(&self) -> String {
        self.request.get_method().to_string()
    }

    /// 同步执行单个 HTTP 请求，一次性返回完整响应
    ///
    /// 适用于用户已知响应数据不大的场景，无需创建 XHMulti/XHThreadPool，
    /// 直接在当前请求对象上调用 execute() 即可获得完整响应。
    ///
    /// 返回的数组包含全部字段：
    ///   - success: bool       是否成功（2xx 且无错误）
    ///   - status: int         HTTP 状态码（请求失败时为 0）
    ///   - body: string        响应体（UTF-8 文本）
    ///   - body_size: int      响应体大小（字节）
    ///   - headers: array      所有响应头（键值对）
    ///   - url: string         最终 URL（可能因重定向变化）
    ///   - elapsed_ms: int     请求耗时（毫秒）
    ///   - remote_addr: ?string 远程服务器地址（IP:Port）
    ///   - version: ?string    HTTP 协议版本（如 "HTTP/1.1"）
    ///   - error: ?string      错误信息（请求成功时为 null）
    ///
    /// # PHP 签名
    /// public XHRequest::execute(): array
    ///
    /// # 示例
    /// $resp = XHCurl::createRequest("https://example.com/api")->get()->execute();
    /// if ($resp['success']) {
    ///     echo $resp['status'];      // 200
    ///     echo $resp['body'];        // 响应体
    ///     echo $resp['elapsed_ms'];  // 耗时
    /// }
    pub fn execute(&mut self) -> Result<ZBox<ZendHashTable>, String> {
        let client = global_client()?.clone();
        let request = self.request.clone();
        let max_response_size = XhCurlManager::global().config().max_response_size;

        // 预取 id 和 user_data（request 会被 move 进 async 块）
        let id = request
            .get_id()
            .map(|s| s.to_string())
            .unwrap_or_else(|| request.get_url().to_string());
        let user_data = request.get_user_data().map(|s| s.to_string());

        let id_for_task = id.clone();
        // 统一错误处理：网络/DNS/TLS 错误包装为 success=false 结果数组，
        // 不抛异常（与 XHMulti/fiber 路径一致）
        let result_array = global_runtime()?.block_on(async move {
            match XhMulti::execute_single(client, request, id_for_task, None, max_response_size)
                .await
            {
                Ok(resp) => {
                    // 成功：构造完整结果数组
                    let mut ht = response_to_php_array(&resp);
                    let _ = ht.insert("id", id);
                    if let Some(ud) = user_data {
                        let _ = ht.insert("user_data", ud);
                    }
                    ht
                }
                Err(e) => {
                    // 失败：构造 success=false 结果数组（与 result_to_php_array 失败路径一致）
                    let result = crate::multi::RequestResult::error(
                        id,
                        user_data,
                        e.to_string(),
                        std::time::Duration::from_secs(0),
                    );
                    crate::php_ext::result_to_php_array(&result)
                }
            }
        });

        Ok(result_array)
    }
}

// +----------------------------------------------------------------------+
// | PHP 类：XHMulti（异步批量执行器）                                     |
// +----------------------------------------------------------------------+

/// PHP XHMulti 类的 Rust 表示
#[php_class]
#[php(name = "XHMulti")]
pub struct PhpXhMulti {
    /// 待执行的请求列表
    requests: Vec<XhRequest>,

    /// 最大并发数（0 = 无限制）
    max_concurrency: usize,

    /// 最大响应体大小（字节，0 = 使用全局配置）
    max_response_size: usize,

    /// 批量级超时（秒，0 = 无超时）
    timeout: u64,
}

/// PHP XHMulti 类的方法实现
#[php_impl]
impl PhpXhMulti {
    /// 构造函数
    pub fn __construct() -> Self {
        Self {
            requests: Vec::new(),
            max_concurrency: 0,
            max_response_size: 0,
            timeout: 0,
        }
    }

    /// 添加请求到批量执行器
    /// 带数量上限检查，防止内存溢出
    ///
    /// # PHP 签名
    /// public XHMulti::add(XHRequest $request): $self_
    pub fn add<'a>(
        self_: &'a mut ZendClassObject<PhpXhMulti>,
        request: &ZendClassObject<PhpXhRequest>,
    ) -> Result<&'a mut ZendClassObject<PhpXhMulti>, String> {
        // 检查请求数量是否超过上限
        if self_.requests.len() >= MAX_REQUESTS_PER_BATCH {
            return Err(format!(
                "批量请求数量超过上限 {}，请分批执行",
                MAX_REQUESTS_PER_BATCH
            ));
        }
        // 通过 Deref 访问 PhpXhRequest.request，克隆后添加
        self_.requests.push(request.request.clone());
        Ok(self_)
    }

    /// 设置最大并发数
    ///
    /// # PHP 签名
    /// public XHMulti::maxConcurrency(int $max): $self_
    ///
    /// 负值跳过设置（保留原值），与 `setConfig` 的"负值跳过"行为一致。
    pub fn max_concurrency(
        self_: &mut ZendClassObject<PhpXhMulti>,
        max: i64,
    ) -> &mut ZendClassObject<PhpXhMulti> {
        // 负值跳过（保留原值），避免 i64→usize 转换产生巨大数值
        if max >= 0 {
            self_.max_concurrency = max as usize;
        }
        self_
    }

    /// 设置单个响应的最大响应体大小（字节）
    /// 防止恶意服务器返回超大响应导致内存溢出。
    /// 0 = 使用全局配置（默认 10MB）。
    ///
    /// # PHP 签名
    /// public XHMulti::maxResponseSize(int $size): $self_
    ///
    /// 负值跳过设置（保留原值），与 `setConfig` 的"负值跳过"行为一致。
    pub fn max_response_size(
        self_: &mut ZendClassObject<PhpXhMulti>,
        size: i64,
    ) -> &mut ZendClassObject<PhpXhMulti> {
        // 负值跳过（保留原值），避免 i64→usize 转换产生巨大数值
        if size >= 0 {
            self_.max_response_size = size as usize;
        }
        self_
    }

    /// 设置批量级超时（秒）
    /// 超时后 abort 未完成的任务并返回错误。
    /// 0 = 无超时（默认）。
    /// 注意：此超时是整个批量请求的总时限，
    /// 单请求超时由 XHRequest::timeout() 单独控制。
    ///
    /// # PHP 签名
    /// public XHMulti::timeout(int $secs): $self_
    ///
    /// 负值跳过设置（保留原值），与 `setConfig` 的"负值跳过"行为一致。
    pub fn timeout(
        self_: &mut ZendClassObject<PhpXhMulti>,
        secs: i64,
    ) -> &mut ZendClassObject<PhpXhMulti> {
        // 负值跳过（保留原值）
        if secs >= 0 {
            self_.timeout = secs as u64;
        }
        self_
    }

    /// 执行所有请求
    ///
    /// # PHP 签名
    /// public XHMulti::execute(): array
    pub fn execute(&mut self) -> Result<ZBox<ZendHashTable>, String> {
        // 复用全局运行时与客户端（避免每次创建/销毁的开销）
        // 运行时类型由 SAPI 决定：CLI 多线程并行，FPM 单线程并发
        let client = global_client()?.clone();
        let max_concurrency = self.max_concurrency;
        let timeout = self.timeout;
        let requests = std::mem::take(&mut self.requests);
        // 0 表示使用全局配置值
        let max_resp_size = if self.max_response_size > 0 {
            self.max_response_size
        } else {
            XhCurlManager::global().config().max_response_size
        };

        let results = global_runtime()?
            .block_on(async move {
                let mut multi = XhMulti::new(client);
                if max_concurrency > 0 {
                    multi = multi.max_concurrency(max_concurrency);
                }
                multi = multi.max_response_size(max_resp_size);
                if timeout > 0 {
                    multi = multi.timeout(timeout);
                }
                multi.add_many(requests).map_err(|e| e.to_string())?;
                multi.execute().await.map_err(|e| e.to_string())
            })
            .map_err(|e| e.to_string())?;

        // 转换为 PHP 数组
        results_to_php_array(&results)
    }

    /// 流式回调并发执行
    ///
    /// 每完成一个请求立即调用回调处理，不累积全部结果（内存恒定）。
    /// 与 `execute` 的区别：
    /// - execute：等全部完成，累积到数组一次性返回（内存随请求总数增长）
    /// - executeEach：每完成一个立即调回调处理，不累积（内存恒定）
    ///
    /// # PHP 签名
    /// public XHMulti::executeEach(callable $onResult, ?callable $onChunk = null, ?callable $onHeaders = null): int
    ///
    /// 回调签名：
    /// - $onResult: function(array $result): void —— 每个请求完成时调用
    /// - $onChunk: function(string $requestId, string $chunk): void —— 响应体分块（二进制安全）
    /// - $onHeaders: function(string $requestId, int $status, array $headers): void —— 收到响应头时调用
    ///
    /// 当 $onChunk 或 $onHeaders 非 null 时，内部启用流式回调。
    /// 不传这两个参数时行为与之前完全一致（向后兼容）。
    /// 返回值：处理的结果总数（等于请求数；空请求列表返回 0）
    #[php(name = "executeEach")]
    pub fn execute_each(
        &mut self,
        callback: &Zval,
        on_chunk: Option<&Zval>,
        on_headers: Option<&Zval>,
    ) -> Result<i64, String> {
        // 空请求列表：不 spawn，提前返回
        if self.requests.is_empty() {
            return Ok(0);
        }

        // 提前校验回调，避免 spawn 后才发现回调无效
        // ZendCallable::new 借用 callback 的生命周期，在整个函数内可用
        let callback_callable =
            ZendCallable::new(callback).map_err(|e| format!("无效的回调: {}", e))?;

        // 校验可选流式回调（onChunk / onHeaders），任一非 None 时启用流式
        // ext-php-rs 的 Option<&Zval> 不区分"未传"和"传 null"：
        // PHP 传 null 时收到 Some(null_zval)，需用 is_null() 判定视为 None。
        let on_chunk_callable = match on_chunk {
            Some(zv) if !zv.is_null() => {
                Some(ZendCallable::new(zv).map_err(|e| format!("无效的 onChunk 回调: {}", e))?)
            }
            _ => None,
        };
        let on_headers_callable = match on_headers {
            Some(zv) if !zv.is_null() => {
                Some(ZendCallable::new(zv).map_err(|e| format!("无效的 onHeaders 回调: {}", e))?)
            }
            _ => None,
        };
        let streaming_enabled = on_chunk_callable.is_some() || on_headers_callable.is_some();

        // 复用 execute 的全局运行时与客户端配置
        let client = global_client()?.clone();
        let max_concurrency = self.max_concurrency;
        let timeout = self.timeout;
        let requests = std::mem::take(&mut self.requests);
        // 0 表示使用全局配置值
        let max_resp_size = if self.max_response_size > 0 {
            self.max_response_size
        } else {
            XhCurlManager::global().config().max_response_size
        };

        // 复用 execute 的「XhMulti 创建 + 配置 + spawn_all」模式，
        // 仅收集循环不同：recv 一个 result → result_to_php_array → 调回调 → 不累积
        // spawn 逻辑统一委托给 XhMulti::spawn_all，消除重复代码
        global_runtime()?.block_on(async move {
            let mut multi = XhMulti::new(client);
            if max_concurrency > 0 {
                multi = multi.max_concurrency(max_concurrency);
            }
            multi = multi.max_response_size(max_resp_size);
            if timeout > 0 {
                multi = multi.timeout(timeout);
            }

            // 启用流式回调（必须在 spawn_all 之前调用，使 stream_tx 生效）
            // stream_tx 由 multi 持有，loop 期间 channel 不会关闭，
            // 故 stream_rx.recv() 在此期间不会返回 None（仅 Pending 或 Some）
            let mut stream_rx = if streaming_enabled {
                Some(multi.enable_streaming())
            } else {
                None
            };

            multi.add_many(requests).map_err(|e| e.to_string())?;

            let (mut result_rx, expected) = multi.spawn_all().await;

            // 流式收集：每收到一个结果就调用回调处理，不累积（内存恒定）
            let mut count: i64 = 0;
            if timeout > 0 {
                // 带批量级超时的结果收集
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout);
                loop {
                    let now = std::time::Instant::now();
                    if now >= deadline {
                        // 超时：abort 剩余任务，避免任务泄漏
                        multi.abort_tasks();
                        return Err(format!(
                            "批量请求超时（{} 秒），已完成 {}/{} 个",
                            timeout, count, expected
                        ));
                    }
                    let remaining = deadline - now;
                    if streaming_enabled {
                        let stream_rx = stream_rx.as_mut().unwrap();
                        tokio::select! {
                            result = tokio::time::timeout(remaining, result_rx.recv()) => {
                                match result {
                                    Ok(Some(result)) => {
                                        // 转换为 PHP 数组（复用 result_to_php_array，字段与 execute 一致）
                                        let result_array = result_to_php_array(&result);
                                        // 调用用户回调，异常时 abort 剩余任务再向上传播
                                        if let Err(e) = invoke_streaming_callback(&callback_callable, &result_array) {
                                            multi.abort_tasks();
                                            return Err(e);
                                        }
                                        count += 1;
                                        if (count as usize) >= expected {
                                            break;
                                        }
                                    }
                                    Ok(None) => break, // channel 关闭
                                    Err(_) => {
                                        // 批量超时：abort 剩余任务，避免任务泄漏
                                        multi.abort_tasks();
                                        return Err(format!(
                                            "批量请求超时（{} 秒），已完成 {}/{} 个",
                                            timeout, count, expected
                                        ));
                                    }
                                }
                            }
                            stream_msg = stream_rx.recv() => {
                                if let Some((req_id, event)) = stream_msg {
                                    if let Err(e) = dispatch_stream_event(&req_id, &event, &on_chunk_callable, &on_headers_callable) {
                                        multi.abort_tasks();
                                        return Err(e);
                                    }
                                }
                                // None: stream channel 关闭（所有任务完成），继续等待结果
                            }
                        }
                    } else {
                        // 原始逻辑（未启用流式，向后兼容）
                        match tokio::time::timeout(remaining, result_rx.recv()).await {
                            Ok(Some(result)) => {
                                // 转换为 PHP 数组（复用 result_to_php_array，字段与 execute 一致）
                                let result_array = result_to_php_array(&result);
                                // 调用用户回调，异常时 abort 剩余任务再向上传播
                                if let Err(e) =
                                    invoke_streaming_callback(&callback_callable, &result_array)
                                {
                                    multi.abort_tasks();
                                    return Err(e);
                                }
                                count += 1;
                                if (count as usize) >= expected {
                                    break;
                                }
                            }
                            Ok(None) => break, // channel 关闭
                            Err(_) => {
                                // 批量超时：abort 剩余任务，避免任务泄漏
                                multi.abort_tasks();
                                return Err(format!(
                                    "批量请求超时（{} 秒），已完成 {}/{} 个",
                                    timeout, count, expected
                                ));
                            }
                        }
                    }
                }
            } else {
                // 无超时的结果收集
                if streaming_enabled {
                    let stream_rx = stream_rx.as_mut().unwrap();
                    while (count as usize) < expected {
                        tokio::select! {
                            result = result_rx.recv() => {
                                match result {
                                    Some(result) => {
                                        let result_array = result_to_php_array(&result);
                                        if let Err(e) = invoke_streaming_callback(&callback_callable, &result_array) {
                                            multi.abort_tasks();
                                            return Err(e);
                                        }
                                        count += 1;
                                    }
                                    None => break,
                                }
                            }
                            stream_msg = stream_rx.recv() => {
                                if let Some((req_id, event)) = stream_msg {
                                    if let Err(e) = dispatch_stream_event(&req_id, &event, &on_chunk_callable, &on_headers_callable) {
                                        multi.abort_tasks();
                                        return Err(e);
                                    }
                                }
                                // None: stream channel 关闭（所有任务完成），继续等待结果
                            }
                        }
                    }
                } else {
                    // 原始逻辑（未启用流式，向后兼容）
                    while let Some(result) = result_rx.recv().await {
                        let result_array = result_to_php_array(&result);
                        // 调用用户回调，异常时 abort 剩余任务再向上传播
                        if let Err(e) = invoke_streaming_callback(&callback_callable, &result_array) {
                            multi.abort_tasks();
                            return Err(e);
                        }
                        count += 1;
                    }
                }
            }

            // 主循环结束后，处理可能残留的流式事件
            // result 已收齐但 stream channel 可能仍有积压的 Chunk/Headers 事件未消费，
            // 需 drain 确保用户回调收到完整的分块数据（否则 onChunk 拼接会缺失尾部 chunk）。
            if let Some(stream_rx) = stream_rx.as_mut() {
                while let Ok((req_id, event)) = stream_rx.try_recv() {
                    if let Err(e) = dispatch_stream_event(
                        &req_id,
                        &event,
                        &on_chunk_callable,
                        &on_headers_callable,
                    ) {
                        multi.abort_tasks();
                        return Err(e);
                    }
                }
            }

            // 等待所有任务完成（确保没有任务泄漏）
            // 检测 task panic（JoinError），避免静默丢失结果
            multi.join_tasks().await;

            // 完整性检查：task panic 会导致结果数量少于预期
            if (count as usize) != expected {
                return Err(format!(
                    "部分任务异常退出：预期 {} 个结果，实际收到 {} 个",
                    expected, count
                ));
            }

            Ok(count)
        })
    }
}

// +----------------------------------------------------------------------+
// | PHP 类：XHThreadPool（线程池）                                        |
// +----------------------------------------------------------------------+

/// PHP XHThreadPool 类的 Rust 表示
#[php_class]
#[php(name = "XHThreadPool")]
pub struct PhpXhThreadPool {
    /// 内部线程池（首次 execute 时创建，同对象多次 execute 复用，实现线程复用）
    pool: Option<XhThreadPool>,

    /// 待执行的请求列表
    requests: Vec<XhRequest>,

    /// 最大并发数（工作线程数量）
    max_concurrency: usize,
}

/// PHP XHThreadPool 类的方法实现
#[php_impl]
impl PhpXhThreadPool {
    /// 构造函数
    ///
    /// # PHP 签名
    /// public XHThreadPool::__construct(int $workers = 0)
    ///
    /// 负值跳过（使用默认值 0），与 `setConfig` 的"负值跳过"行为一致。
    pub fn __construct(workers: Option<i64>) -> Self {
        Self {
            pool: None,
            requests: Vec::new(),
            // 负值或 None：使用默认值 0（避免 i64→usize 转换产生巨大数值）
            max_concurrency: match workers {
                Some(n) if n >= 0 => n as usize,
                _ => 0,
            },
        }
    }

    /// 添加请求到线程池
    /// 带数量上限检查，防止内存溢出
    pub fn add<'a>(
        self_: &'a mut ZendClassObject<PhpXhThreadPool>,
        request: &ZendClassObject<PhpXhRequest>,
    ) -> Result<&'a mut ZendClassObject<PhpXhThreadPool>, String> {
        if self_.requests.len() >= MAX_REQUESTS_PER_BATCH {
            return Err(format!(
                "批量请求数量超过上限 {}，请分批执行",
                MAX_REQUESTS_PER_BATCH
            ));
        }
        self_.requests.push(request.request.clone());
        Ok(self_)
    }

    /// 执行所有请求
    ///
    /// # PHP 签名
    /// public XHThreadPool::execute(): array
    pub fn execute(&mut self) -> Result<ZBox<ZendHashTable>, String> {
        // 安全检查：线程池仅在 CLI 模式下可用
        // FPM 模式下多线程会与 PHP 内存管理器（TSRM/内存池）冲突
        if !sapi_is_cli() {
            return Err("XHThreadPool 仅在 CLI 模式下可用".to_string());
        }

        let requests = std::mem::take(&mut self.requests);
        let max_concurrency = self.max_concurrency;
        // 取出现有线程池以便复用（同对象多次 execute 复用工作线程）
        let mut pool = self.pool.take();

        // 预取全局客户端（首次创建线程池时需要 clone，复用时不需要）
        let client = global_client()?.clone();

        // 复用全局运行时与客户端；通过 take/存回 模式避免借用 self
        let (returned_pool, result) = global_runtime()?.block_on(async move {
            // 首次调用时创建线程池，后续复用
            if pool.is_none() {
                let mut config = ThreadPoolConfig::default();
                if max_concurrency > 0 {
                    config.worker_count = max_concurrency;
                }
                pool = Some(XhThreadPool::new(config, client));
            }
            let p = pool
                .as_mut()
                .ok_or_else(|| "内部错误：线程池未初始化".to_string())?;
            let res = p.execute_all(requests).await;
            Ok::<_, String>((pool, res))
        })?;

        // block_on 已 ? 解包 String 错误，此处 returned_pool 是 Option<XhThreadPool>
        self.pool = returned_pool;
        let results = result.map_err(|e| e.to_string())?;

        // 转换为 PHP 数组
        results_to_php_array(&results)
    }

    /// 流式回调并发执行
    ///
    /// 每完成一个请求立即调用回调处理，不累积全部结果（内存恒定）。
    /// 与 `execute` 的区别：
    /// - execute：等全部完成，累积到数组一次性返回（内存随请求总数增长）
    /// - executeEach：每完成一个立即调回调处理，不累积（内存恒定）
    ///
    /// # PHP 签名
    /// public XHThreadPool::executeEach(callable $onResult, ?callable $onChunk = null, ?callable $onHeaders = null): int
    ///
    /// 回调签名：
    /// - $onResult: function(array $result): void —— 每个请求完成时调用
    /// - $onChunk: function(string $requestId, string $chunk): void —— 响应体分块（二进制安全）
    /// - $onHeaders: function(string $requestId, int $status, array $headers): void —— 收到响应头时调用
    ///
    /// 当 $onChunk 或 $onHeaders 非 null 时，内部启用流式回调。
    /// 不传这两个参数时行为与之前完全一致（向后兼容）。
    /// 返回值：处理的结果总数（等于成功提交的请求数；空请求列表返回 0）
    #[php(name = "executeEach")]
    pub fn execute_each(
        &mut self,
        callback: &Zval,
        on_chunk: Option<&Zval>,
        on_headers: Option<&Zval>,
    ) -> Result<i64, String> {
        // 安全检查：线程池仅在 CLI 模式下可用（与 execute 一致）
        // FPM 模式下多线程会与 PHP 内存管理器（TSRM/内存池）冲突
        if !sapi_is_cli() {
            return Err("XHThreadPool 仅在 CLI 模式下可用".to_string());
        }

        let requests = std::mem::take(&mut self.requests);
        // 空请求列表：不启动线程池，提前返回
        if requests.is_empty() {
            return Ok(0);
        }

        // 提前校验回调，避免 spawn 后才发现回调无效
        let callback_callable =
            ZendCallable::new(callback).map_err(|e| format!("无效的回调: {}", e))?;

        // 校验可选流式回调（onChunk / onHeaders），任一非 None 时启用流式
        // ext-php-rs 的 Option<&Zval> 不区分"未传"和"传 null"：
        // PHP 传 null 时收到 Some(null_zval)，需用 is_null() 判定视为 None。
        let on_chunk_callable = match on_chunk {
            Some(zv) if !zv.is_null() => {
                Some(ZendCallable::new(zv).map_err(|e| format!("无效的 onChunk 回调: {}", e))?)
            }
            _ => None,
        };
        let on_headers_callable = match on_headers {
            Some(zv) if !zv.is_null() => {
                Some(ZendCallable::new(zv).map_err(|e| format!("无效的 onHeaders 回调: {}", e))?)
            }
            _ => None,
        };
        let streaming_enabled = on_chunk_callable.is_some() || on_headers_callable.is_some();

        let max_concurrency = self.max_concurrency;
        // 取出现有线程池以便复用（同对象多次调用复用工作线程）
        let mut pool = self.pool.take();

        // 预取全局客户端（首次创建线程池时需要 clone）
        let client = global_client()?.clone();

        // 复用 execute 的「ThreadPool 创建 + submit」模式，
        // 但收集循环改为：recv 一个 result → result_to_php_array → 调回调 → 不累积
        // 通过 take/存回 模式避免借用 self；回调异常时仍需存回 pool，故用显式错误捕获
        let (returned_pool, count) = global_runtime()?.block_on(async move {
            // 首次调用时创建线程池，后续复用
            if pool.is_none() {
                let mut config = ThreadPoolConfig::default();
                if max_concurrency > 0 {
                    config.worker_count = max_concurrency;
                }
                pool = Some(XhThreadPool::new(config, client));
            }
            let p = match pool.as_mut() {
                Some(p) => p,
                None => return (pool, Err("内部错误：线程池未初始化".to_string())),
            };

            // 启动线程池（若未启动）
            if !p.is_running() {
                if let Err(e) = p.start() {
                    return (pool, Err(e.to_string()));
                }
            }

            // 创建流式 channel（与 XhMulti::enable_streaming 使用相同容量）
            // stream_rx 为本地 owned 变量，不借用 pool，可与 result_rx 在 select! 中共存
            let (mut stream_tx_opt, mut stream_rx) = if streaming_enabled {
                let (tx, rx) = mpsc::channel::<(String, StreamEvent)>(STREAM_CHANNEL_CAPACITY);
                (Some(tx), Some(rx))
            } else {
                (None, None)
            };

            // 提交所有请求，记录成功提交的数量
            // 若中途 submit 失败（队列满），已提交的请求仍需收集结果
            // 启用流式时使用 submit_with_stream 将 stream_tx 传入 worker
            let mut submitted = 0usize;
            for request in requests {
                let submit_result = match &stream_tx_opt {
                    Some(tx) => p.submit_with_stream(request, tx.clone()),
                    None => p.submit(request),
                };
                match submit_result {
                    Ok(()) => submitted += 1,
                    Err(e) => {
                        // 提交失败，但已提交的请求需要继续收集结果
                        eprintln!("警告: 提交任务失败: {}", e);
                    }
                }
            }
            if submitted == 0 {
                return (pool, Err("所有请求提交失败".to_string()));
            }
            // 提交完成后丢弃 stream_tx 副本，使 channel 在所有 worker 完成后能正确关闭
            drop(stream_tx_opt.take());

            let worker_count = p.worker_count();
            // 自己接管 result_rx：每收到一个就调回调，不通过 execute_all 累积
            let result_rx = match p.result_rx_mut() {
                Some(rx) => rx,
                None => return (pool, Err("结果接收端不存在".to_string())),
            };

            // 流式收集：每收到一个结果就调用回调处理，不累积（内存恒定）
            let mut count: i64 = 0;
            let mut shutdown_count = 0usize;
            // 回调异常需向上传播，但又要存回 pool，故先捕获错误信息，循环结束后统一返回
            let mut callback_err: Option<String> = None;

            // 只要结果未收齐就继续等待
            if streaming_enabled {
                let stream_rx = stream_rx.as_mut().unwrap();
                while (count as usize) < submitted {
                    tokio::select! {
                        msg = result_rx.recv() => {
                            match msg {
                                Some(ResultMessage::Completed(result)) => {
                                    // 转换为 PHP 数组（复用 result_to_php_array，字段与 execute 一致）
                                    let result_array = result_to_php_array(&result);
                                    // 调用用户回调（通过 invoke_streaming_callback 统一异常提取）
                                    if let Err(e) = invoke_streaming_callback(&callback_callable, &result_array) {
                                        callback_err = Some(e);
                                        break;
                                    }
                                    count += 1;
                                }
                                Some(ResultMessage::WorkerShutdown) => {
                                    shutdown_count += 1;
                                    // 所有工作线程都已关闭，无法再收到结果
                                    if shutdown_count >= worker_count {
                                        break;
                                    }
                                }
                                None => {
                                    // channel 关闭，无法再收到结果
                                    break;
                                }
                            }
                        }
                        stream_msg = stream_rx.recv() => {
                            if let Some((req_id, event)) = stream_msg {
                                if let Err(e) = dispatch_stream_event(&req_id, &event, &on_chunk_callable, &on_headers_callable) {
                                    callback_err = Some(e);
                                    break;
                                }
                            }
                            // None: stream channel 关闭（所有 worker 完成），继续等待结果
                        }
                    }
                }
            } else {
                // 原始逻辑（未启用流式，向后兼容）
                while (count as usize) < submitted {
                    match result_rx.recv().await {
                        Some(ResultMessage::Completed(result)) => {
                            // 转换为 PHP 数组（复用 result_to_php_array，字段与 execute 一致）
                            let result_array = result_to_php_array(&result);
                            // 调用用户回调（通过 invoke_streaming_callback 统一异常提取）
                            if let Err(e) = invoke_streaming_callback(&callback_callable, &result_array)
                            {
                                callback_err = Some(e);
                                break;
                            }
                            count += 1;
                        }
                        Some(ResultMessage::WorkerShutdown) => {
                            shutdown_count += 1;
                            // 所有工作线程都已关闭，无法再收到结果
                            if shutdown_count >= worker_count {
                                break;
                            }
                        }
                        None => {
                            // channel 关闭，无法再收到结果
                            break;
                        }
                    }
                }
            }

            // 主循环结束后，处理可能残留的流式事件（与 XHMulti 一致）
            // result 已收齐但 stream channel 可能仍有积压事件未消费。
            if let Some(stream_rx) = stream_rx.as_mut() {
                while let Ok((req_id, event)) = stream_rx.try_recv() {
                    if let Err(e) = dispatch_stream_event(
                        &req_id,
                        &event,
                        &on_chunk_callable,
                        &on_headers_callable,
                    ) {
                        callback_err = Some(e);
                        break;
                    }
                }
            }

            // 完整性检查：worker panic 或提前退出可能导致结果数量不足
            // 与 execute_all 行为一致，返回错误而非静默返回不完整结果
            if let Some(msg) = callback_err {
                (pool, Err(msg))
            } else if (count as usize) < submitted {
                (
                    pool,
                    Err(format!(
                        "部分任务异常退出：预期 {} 个结果，实际收到 {} 个",
                        submitted, count
                    )),
                )
            } else {
                (pool, Ok(count))
            }
        });

        // 存回线程池以便下次 execute 复用
        self.pool = returned_pool;
        count
    }
}

// +----------------------------------------------------------------------+
// | 辅助函数                                                              |
// +----------------------------------------------------------------------+

// +----------------------------------------------------------------------+
// | 安全迭代辅助函数                                                      |
// +----------------------------------------------------------------------+

/// 安全地遍历 PHP 哈希表的所有键值对。
///
/// 手动控制迭代次数（`ht.len()`），防止 `Iter` 提前终止或越界：
/// `Iter::next_zval` 用 `key_type == -1` 判断结束，
/// 但 PHP 的 `HASH_KEY_NON_EXISTENT == 3`，该判断永不成立，
/// 导致遍历完最后一个元素后再调用 `next()` 会触发空指针解引用。
///
/// 本函数通过精确迭代 `ht.len()` 次来避免触发有缺陷的终止路径，
/// 仅在剩余次数内调用 `iter.next()`。
pub(crate) fn for_each_kv<F>(ht: &ZendHashTable, mut f: F) -> Result<(), String>
where
    F: FnMut(&ArrayKey, &Zval) -> Result<(), String>,
{
    let len = ht.len();
    let mut iter = ht.iter();
    for _ in 0..len {
        match iter.next() {
            Some((key, val)) => f(&key, val)?,
            None => break,
        }
    }
    // 关键：此处不再调用 iter.next()，避免触发 Iter 终止路径的空指针问题
    Ok(())
}

/// 将单个 `RequestResult` 转换为 PHP 关联数组。
///
/// 公共字段集（与单请求 `execute()` 返回字段保持一致 + 批量特有的 id/user_data）：
/// - id, success, elapsed_ms
/// - user_data?（可选，原样回传 JSON 字符串）
/// - error?（可选，失败时）
/// - fill_response_fields: status, body, body_size, url, headers,
///   remote_addr?, version?, error?, elapsed_ms
///
/// pub(crate) 可见性：fiber.rs 的 `result_to_php_array` 复用此函数，
/// 确保协程/批量/线程池三条路径返回字段完全一致，避免逻辑重复。
pub(crate) fn result_to_php_array(result: &crate::multi::RequestResult) -> ZBox<ZendHashTable> {
    let mut response_ht = ZendHashTable::new();
    let _ = response_ht.insert("id", result.id.clone());
    let _ = response_ht.insert("success", result.is_success());

    // 用户自定义数据：原样回传 JSON 字符串
    if let Some(ud) = &result.user_data {
        let _ = response_ht.insert("user_data", ud.clone());
    }

    // 写入完整响应信息（status/body/headers/url/remote_addr/version 等）
    if let Some(resp) = &result.response {
        // 有响应时，elapsed_ms/error 由 fill_response_fields 统一写入，避免双重插入
        fill_response_fields(&mut response_ht, resp);
    } else {
        // 无响应（请求失败）时，补充 status/body/headers/body_size/url 等，
        // 确保失败路径字段集与成功路径（fill_response_fields）完全一致
        // status 为 0（哨兵值，表示无 HTTP 响应），body 插入空字符串
        let _ = response_ht.insert("status", 0_i64);
        let _ = response_ht.insert("elapsed_ms", result.elapsed.as_millis() as i64);
        let _ = response_ht.insert("body", String::new());
        let _ = response_ht.insert("body_size", 0_i64);
        let _ = response_ht.insert("headers", ZendHashTable::new());
        let _ = response_ht.insert("url", String::new());
        if let Some(err) = &result.error {
            let _ = response_ht.insert("error", err.clone());
        }
    }
    response_ht
}

/// 将请求结果列表转换为 PHP 数组
/// 用于 XHMulti::execute() 和 XHThreadPool::execute() 的返回值
///
/// 注意：使用 `insert_at_index`（整数键）而非 `insert(&str)`（字符串键）。
/// 因为 `insert` 内部调用 `zend_hash_str_update`，对 "0"/"1" 等数字字符串
/// 不会规范化为整数键，导致 PHP 端 `$res["0"]` 与 `$res[0]` 均无法命中。
/// 使用 `insert_at_index` 直接以整数键写入，PHP 端可用 `$res[0]` 访问。
fn results_to_php_array(
    results: &[crate::multi::RequestResult],
) -> Result<ZBox<ZendHashTable>, String> {
    let mut ht = ZendHashTable::new();
    for (i, result) in results.iter().enumerate() {
        let response_ht = result_to_php_array(result);
        // 使用整数键，确保 PHP 端 $res[0] 可访问
        let _ = ht.insert_at_index(i as i64, response_ht);
    }
    Ok(ht)
}

/// 调用流式回调（executeEach 共用），封装 try_call + 异常 message 提取。
///
/// 异常 message 提取使用 `fiber::extract_exception_message`（读 "message" 属性），
/// 而非 `Error::Exception` 的 Display（其内部 `{e:?}` Debug 遍历 trace 属性，
/// 可能含 NUL 字节导致 CString 转换失败，抛 InvalidCString 掩盖原始异常）。
///
/// 供 `XhMulti::execute_each` 和 `XHThreadPool::execute_each` 共用，消除重复。
fn invoke_streaming_callback(
    callback: &ZendCallable,
    result_array: &ZBox<ZendHashTable>,
) -> Result<(), String> {
    callback
        .try_call(vec![result_array as &dyn IntoZvalDyn])
        .map(|_| ())
        .map_err(extract_callback_error)
}

/// 提取回调执行错误的可读消息。
///
/// 使用 `fiber::extract_exception_message` 读 Exception 对象的 message 属性，
/// 而非 Error::Exception 的 Display（其 Debug 遍历 trace 属性可能含 NUL 字节）。
fn extract_callback_error(e: ext_php_rs::error::Error) -> String {
    match e {
        ext_php_rs::error::Error::Exception(obj) => crate::fiber::extract_exception_message(&obj),
        other => format!("回调执行失败: {}", other),
    }
}

/// 分发流式事件到对应的 PHP 回调（on_chunk / on_headers）。
///
/// - `Headers` 事件 → 调用 `on_headers(string $requestId, int $status, array $headers)`
/// - `Chunk` 事件 → 调用 `on_chunk(string $requestId, string $chunk)`（二进制安全）
/// - `Complete`/`Error` → 不单独调回调（结果回调 onResult 已覆盖终结语义）
///
/// 当对应的回调为 None 时（用户未传该回调），跳过不调用。
/// 回调异常时返回 Err(message)，与 invoke_streaming_callback 的错误提取一致。
fn dispatch_stream_event(
    request_id: &str,
    event: &StreamEvent,
    on_chunk: &Option<ZendCallable>,
    on_headers: &Option<ZendCallable>,
) -> Result<(), String> {
    match event {
        StreamEvent::Headers { status, headers } => {
            let cb = match on_headers {
                Some(cb) => cb,
                None => return Ok(()),
            };
            // 构建 headers 关联数组（复用 fill_response_fields 中的头部转换逻辑）
            let mut headers_ht = ZendHashTable::new();
            for (name, value) in headers {
                let _ = headers_ht.insert(name.as_str(), value.as_str());
            }
            let status_i64 = *status as i64;
            // request_id 借用为 String 以满足 IntoZvalDyn（&str 不可直接转 trait 对象）
            let rid = request_id.to_string();
            cb.try_call(vec![
                &rid as &dyn IntoZvalDyn,
                &status_i64 as &dyn IntoZvalDyn,
                &headers_ht as &dyn IntoZvalDyn,
            ])
            .map(|_| ())
            .map_err(extract_callback_error)
        }
        StreamEvent::Chunk { data } => {
            let cb = match on_chunk {
                Some(cb) => cb,
                None => return Ok(()),
            };
            // 二进制安全字符串（与 fill_response_fields 的 body 处理一致）
            let mut zv = Zval::new();
            zv.set_binary::<u8>(data.clone());
            let rid = request_id.to_string();
            cb.try_call(vec![&rid as &dyn IntoZvalDyn, &zv as &dyn IntoZvalDyn])
                .map(|_| ())
                .map_err(extract_callback_error)
        }
        StreamEvent::Complete { .. } | StreamEvent::Error { .. } => Ok(()),
    }
}

/// 将单个 XhResponse 的完整信息填充到 PHP 哈希表中
/// 供 XHRequest::execute() 和 results_to_php_array() 共用，
/// 确保单请求与批量请求返回的字段完全一致。
fn response_to_php_array(response: &XhResponse) -> ZBox<ZendHashTable> {
    let mut ht = ZendHashTable::new();
    let _ = ht.insert("success", response.is_success());
    fill_response_fields(&mut ht, response);
    ht
}

/// 将 XhResponse 的所有字段填充到 PHP 哈希表
/// 包含 status/body/body_size/headers/url/remote_addr/version/error/elapsed_ms
///
/// pub(crate) 可见性：fiber.rs 的 result_to_php_array 也复用此函数，
/// 确保 Fiber 协程与同步/批量 API 返回的响应字段完全一致。
pub(crate) fn fill_response_fields(ht: &mut ZBox<ZendHashTable>, response: &XhResponse) {
    let _ = ht.insert("status", response.status() as i64);
    let _ = ht.insert("body_size", response.body_size() as i64);

    // 响应体：直接设置二进制安全的 PHP 字符串
    // PHP 字符串本质是字节序列（zend_string 带长度），二进制安全。
    // 不能用 String::from_utf8_lossy —— 它会把无效 UTF-8 字节替换为
    // U+FFFD（3 字节），导致二进制响应体长度变化、数据损坏
    // （例如 50 字节随机数据会膨胀为 ~98 字节）。
    // 也不能用 Vec<u8> 直接 insert —— Vec<u8> 的 IntoZval 实现会
    // 转为 PHP 数组而非字符串。
    // 解决方案：构造 Zval 后调用 set_binary::<u8>() 设置二进制字符串，
    // 再通过 Zval: IntoZval 移动插入哈希表。
    if let Some(body_bytes) = response.body() {
        let mut zv = Zval::new();
        zv.set_binary::<u8>(body_bytes.to_vec());
        let _ = ht.insert("body", zv);
    } else {
        // body 为空时插入空字符串，避免 PHP 端 $resp['body'] 触发 Undefined index
        let _ = ht.insert("body", String::new());
    }

    // 最终 URL（可能因重定向变化）
    let _ = ht.insert("url", response.url().to_string());

    // 所有响应头
    let mut headers_ht = ZendHashTable::new();
    for (name, value) in response.headers().all() {
        let _ = headers_ht.insert(name.as_str(), value);
    }
    let _ = ht.insert("headers", headers_ht);

    // 远程服务器地址
    if let Some(addr) = response.remote_addr() {
        let _ = ht.insert("remote_addr", addr);
    }

    // HTTP 协议版本
    if let Some(version) = response.version() {
        let _ = ht.insert("version", version);
    }

    // 错误信息
    if let Some(err) = response.error() {
        let _ = ht.insert("error", err);
    }

    // 请求耗时（毫秒）
    let _ = ht.insert("elapsed_ms", response.elapsed().as_millis() as i64);
}

/// 将 PHP 数组转换为 JSON 字符串。
///
/// 列表数组（键为 0,1,2,... 连续整数）转为 JSON 数组，
/// 关联数组转为 JSON 对象（与 PHP json_encode 行为一致）。
fn php_array_to_json(ht: &ZendHashTable) -> Result<String, String> {
    let value = ht_to_json_value(ht)?;
    serde_json::to_string(&value).map_err(|e| e.to_string())
}

/// 将 PHP 哈希表转为 JSON 值。
///
/// 列表判定：第 i 个元素（按迭代顺序）的键恰好为 i → JSON 数组，
/// 否则（含字符串键或非连续整数键）→ JSON 对象。
fn ht_to_json_value(ht: &ZendHashTable) -> Result<serde_json::Value, String> {
    // 在闭包内即时转换为 owned 的 serde_json::Value（&Zval 不能逃逸闭包）
    let mut keys: Vec<(Option<i64>, String)> = Vec::new();
    let mut values: Vec<serde_json::Value> = Vec::new();
    for_each_kv(ht, |key, val| {
        let idx = match key {
            ArrayKey::Long(i) => Some(*i),
            _ => None,
        };
        keys.push((idx, key.to_string()));
        values.push(zval_to_json(val)?);
        Ok(())
    })?;

    if values.is_empty() {
        return Ok(serde_json::Value::Array(Vec::new()));
    }

    let is_list = keys
        .iter()
        .enumerate()
        .all(|(i, (idx, _))| *idx == Some(i as i64));

    if is_list {
        Ok(serde_json::Value::Array(values))
    } else {
        let mut map = serde_json::Map::new();
        for ((_, key), val) in keys.into_iter().zip(values) {
            map.insert(key, val);
        }
        Ok(serde_json::Value::Object(map))
    }
}

/// 将 Zval 转换为 JSON 值
/// 支持 string / int / float / bool / array 类型
fn zval_to_json(val: &Zval) -> Result<serde_json::Value, String> {
    if let Some(s) = val.string() {
        Ok(serde_json::Value::String(s))
    } else if let Some(l) = val.long() {
        Ok(serde_json::Value::Number(l.into()))
    } else if let Some(d) = val.double() {
        serde_json::Number::from_f64(d)
            .map(serde_json::Value::Number)
            .ok_or_else(|| "无效的浮点数".to_string())
    } else if let Some(b) = val.bool() {
        Ok(serde_json::Value::Bool(b))
    } else if let Some(ht) = val.array() {
        ht_to_json_value(ht)
    } else {
        Ok(serde_json::Value::Null)
    }
}

/// 将 PHP 数组转换为表单键值对
/// 用于 application/x-www-form-urlencoded 请求
fn php_array_to_form(ht: &ZendHashTable) -> Vec<(String, String)> {
    let mut form = Vec::new();

    // 使用安全迭代器（手动控制迭代次数，防止 Iter 提前终止）
    let _ = for_each_kv(ht, |key, val| {
        let key_str = key.to_string();
        // 二进制安全读取：PHP 字符串本质是字节序列，可能含非 UTF-8 字节。
        // Zval::string() 遇到非 UTF-8 字节会返回 None 导致表单项被静默丢弃，
        // 这里优先用 binary::<u8>() 取原始字节，再用 lossy 转为字符串。
        // 与 body()/multipart() 的二进制安全读取保持一致。
        let val_str = if let Some(bytes) = val.binary::<u8>() {
            String::from_utf8_lossy(&bytes).into_owned()
        } else if let Some(s) = val.string() {
            s
        } else if let Some(l) = val.long() {
            l.to_string()
        } else if let Some(d) = val.double() {
            d.to_string()
        } else if let Some(b) = val.bool() {
            if b { "1" } else { "0" }.to_string()
        } else {
            return Ok(());
        };
        form.push((key_str, val_str));
        Ok(())
    });

    form
}

// +----------------------------------------------------------------------+
// | xhrun - 安全的跨平台 shell 命令执行函数                              |
// +----------------------------------------------------------------------+

/// `xhrun` 默认超时（秒）。
/// 防止命令无限期挂起；调用方可通过 `timeout` 选项覆盖。
const XHRUN_DEFAULT_TIMEOUT_SECS: u64 = 60;

/// `xhrun` 每个流（stdout/stderr）允许的最大输出字节数。
/// 防止恶意/失控命令耗尽内存；调用方可通过 `max_output` 选项覆盖。
const XHRUN_DEFAULT_MAX_OUTPUT: usize = 64 * 1024 * 1024; // 64MB

/// Unix shell 参数转义：用单引号包裹，内部单引号用 `'\''` 转义。
///
/// `sh -c` 接受单个字符串，若直接拼接参数会被 shell 重新解析，
/// 攻击者可借含 `;` / `$(...)` / 反引号的参数注入命令。
/// 本函数确保每个参数作为字面量传入，不被 shell 解释。
///
/// 例：`a'b` → `'a'\''b'`
fn shell_quote_unix(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    for ch in s.chars() {
        if ch == '\'' {
            // 关闭单引号 → 转义单引号 → 重开单引号
            out.push_str("'\\''");
        } else {
            out.push(ch);
        }
    }
    out.push('\'');
    out
}

/// Windows `cmd /C` 参数转义：用双引号包裹，转义内部双引号与 cmd 元字符。
///
/// `cmd` 的引用规则远比 POSIX 复杂且存在历史包袱（如 `%VAR%` 展开
/// 无法通过转义完全消除），此函数采用业界常见的 best-effort 策略：
/// 双引号包裹 + 内部 `"` 转义为 `\"` + 危险元字符用 `^` 抑制。
/// 生产环境若处理不可信输入，仍建议避免 `shell => true`，改用默认非 shell 路径。
///
/// 需抑制的 cmd 元字符（在双引号外才有特殊含义，此处仍抑制以兜底）：
/// `& | < > ^ ( ) %`
fn shell_quote_windows(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            // 在双引号内这些本就大多无特殊含义，但 `^` 与 `%` 在 cmd 中仍有行为，
            // 用 `^` 前缀抑制（best-effort）
            '&' | '|' | '<' | '>' | '^' | '(' | ')' | '%' => {
                out.push('^');
                out.push(ch);
            }
            _ => out.push(ch),
        }
    }
    out.push('"');
    out
}

/// 安全的跨平台 shell 命令执行函数。
///
/// 用于替代 PHP 内置的 `shell_exec` / `exec` / `system` / `passthru` /
/// `proc_open`，提升安全性。
///
/// # 安全特性
///
/// 1. **默认不经过 shell 解析**：命令和参数通过 `Command::arg()` 逐个传递，
///    天然避免 shell 注入（这是相比 PHP 内置函数最大的安全提升）。
///    需要管道/通配符/重定向时，显式设置 `shell => true` 走系统 shell。
/// 2. **超时控制**：命令超时后自动终止，避免卡死。
/// 3. **输出大小限制**：防止失控命令耗尽内存。
/// 4. **可选命令白名单/黑名单**：限制可执行的命令。
///
/// # PHP 签名
///
/// ```php
/// xhrun(string $command, array $args = [], array $options = []): array
/// ```
///
/// # 参数
///
/// - `command`: 要执行的命令（如 `"ls"`、`"ping"`、`"cmd"`）。
/// - `args`: 命令参数数组（如 `["-la", "/tmp"]`）。每个元素作为一个独立参数，
///   不经过 shell 解析。`shell => true` 时，`args` 会做 shell 转义后拼接进命令行，
///   防止参数中的元字符（`;`/`$()`/反引号等）注入命令。
/// - `options`: 选项数组，支持以下键：
///   - `timeout` (int): 超时秒数，0 = 无超时。默认 60。
///   - `max_output` (int): 每个流（stdout/stderr）的最大输出字节数，0 = 无限制。默认 64MB。
///   - `cwd` (string): 工作目录。默认继承当前进程。
///   - `env` (array): 环境变量键值对。默认继承当前进程。
///   - `shell` (bool): 是否通过系统 shell 执行（启用管道/通配符/重定向支持；
///     `args` 会被 shell 转义，但 `command` 本身仍按字面传给 shell，
///     处理不可信输入时仍建议优先用默认非 shell 路径）。默认 false。
///   - `allow` (array): 命令白名单（如 `["ls", "cat"]`）。设置后仅允许这些命令。
///   - `deny` (array): 命令黑名单（如 `["rm", "shutdown"]`）。
///   - `input` (string): 传给命令 stdin 的数据（二进制安全）。
///
/// # 返回
///
/// 关联数组，字段：
/// - `success` (bool): 是否成功执行（exit_code == 0 且未超时/超限）。
/// - `exit_code` (int): 进程退出码。超时/启动失败时为 -1。
/// - `stdout` (string): 标准输出（二进制安全）。
/// - `stderr` (string): 标准错误输出（二进制安全）。
/// - `elapsed_ms` (int): 执行耗时（毫秒）。
/// - `pid` (int): 子进程 PID。
/// - `timed_out` (bool): 是否因超时被终止。
/// - `truncated` (bool): 输出是否因超过 max_output 被截断。
/// - `error` (string): 错误信息（启动失败、超限等，可选）。
///
/// # 示例
///
/// ```php
/// // 基本用法（不经过 shell，安全）
/// $r = xhrun('ls', ['-la', '/tmp']);
/// if ($r['success']) { echo $r['stdout']; }
///
/// // 带超时和环境变量
/// $r = xhrun('ping', ['-c', '4', 'example.com'], ['timeout' => 10, 'env' => ['PATH' => '/usr/bin']]);
///
/// // 需要管道时显式启用 shell（args 会被 shell 转义，防注入）
/// $r = xhrun('ls -la /tmp | grep foo', [], ['shell' => true]);
///
/// // shell 模式下含特殊字符的参数会被安全转义（不会执行 rm）
/// $r = xhrun('echo', ['foo; rm -rf /'], ['shell' => true]);
///
/// // 白名单限制
/// $r = xhrun('ls', ['-la'], ['allow' => ['ls', 'cat']]);
/// ```
#[php_function]
pub fn xhrun(
    command: &str,
    args: Option<&ZendHashTable>,
    options: Option<&ZendHashTable>,
) -> Result<ZBox<ZendHashTable>, String> {
    use std::process::{Command, Stdio};
    use std::time::Instant;

    // ===== 1. 解析参数数组 =====
    let arg_vec: Vec<String> = match args {
        Some(ht) => {
            let mut v = Vec::with_capacity(ht.len());
            let mut iter = ht.iter();
            for _ in 0..ht.len() {
                match iter.next() {
                    Some((_, val)) => {
                        if let Some(s) = val.string() {
                            v.push(s.to_string());
                        } else if let Some(l) = val.long() {
                            v.push(l.to_string());
                        } else if let Some(d) = val.double() {
                            v.push(d.to_string());
                        } else {
                            return Err("args 数组元素必须是标量类型".to_string());
                        }
                    }
                    None => break,
                }
            }
            v
        }
        None => Vec::new(),
    };

    // ===== 2. 解析 options =====
    let timeout_raw = opt_long(options, "timeout", XHRUN_DEFAULT_TIMEOUT_SECS as i64);
    if timeout_raw < 0 {
        return Err(format!("timeout 不能为负数，得到 {}", timeout_raw));
    }
    let timeout_secs: u64 = timeout_raw as u64;
    // max_output = 0 表示无限制（与 timeout = 0 语义一致）
    let max_output: usize = {
        let v = opt_long(options, "max_output", XHRUN_DEFAULT_MAX_OUTPUT as i64);
        if v <= 0 {
            usize::MAX
        } else {
            v as usize
        }
    };
    let cwd: Option<String> = opt_string(options, "cwd");
    let env_ht: Option<&ZendHashTable> = options
        .and_then(|ht| ht.get("env"))
        .and_then(<&ZendHashTable as ext_php_rs::convert::FromZval>::from_zval);
    let use_shell: bool = opt_bool(options, "shell", false);
    let input: Option<Vec<u8>> = options
        .and_then(|ht| ht.get("input"))
        .and_then(|v| v.binary::<u8>());
    let allow_list: Vec<String> = opt_string_vec(options, "allow");
    let deny_list: Vec<String> = opt_string_vec(options, "deny");

    // ===== 3. 安全校验：白名单/黑名单 =====
    // 提取命令主名（去掉路径前缀，如 /usr/bin/ls → ls）
    let cmd_basename = command
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(command)
        .to_lowercase();

    if !allow_list.is_empty() && !allow_list.iter().any(|c| c.to_lowercase() == cmd_basename) {
        return Ok(failure_result(command, -1, "命令不在白名单中"));
    }

    if deny_list.iter().any(|c| c.to_lowercase() == cmd_basename) {
        return Ok(failure_result(command, -1, "命令在黑名单中"));
    }

    // ===== 4. 构建命令 =====
    let mut cmd = if use_shell {
        // shell 模式：通过系统 shell 执行，支持管道/通配符/重定向
        // 安全修复：对每个 arg 做 shell 转义，防止参数中的 `;`/`$(...)`/反引号
        // 被当 shell 元字符解析而注入命令。command 本身仍按字面传给 shell
        // （用户显式启用 shell 模式即为使用其管道/通配符等特性）。
        let mut full = String::from(command);
        let quote_fn = if cfg!(target_os = "windows") {
            shell_quote_windows
        } else {
            shell_quote_unix
        };
        for a in &arg_vec {
            full.push(' ');
            full.push_str(&quote_fn(a));
        }
        let c = if cfg!(target_os = "windows") {
            let mut c = Command::new("cmd");
            c.arg("/C").arg(&full);
            c
        } else {
            let mut c = Command::new("sh");
            c.arg("-c").arg(&full);
            c
        };
        c
    } else {
        // 默认模式：直接执行，不经过 shell，参数逐个传递
        // 这是相比 PHP shell_exec 最主要的安全提升
        let mut c = Command::new(command);
        for a in &arg_vec {
            c.arg(a);
        }
        c
    };

    // 设置工作目录
    if let Some(dir) = &cwd {
        cmd.current_dir(dir);
    }

    // 设置环境变量
    if let Some(env) = env_ht {
        let mut iter = env.iter();
        for _ in 0..env.len() {
            if let Some((k, v)) = iter.next() {
                // ArrayKey 的 Display 实现会返回键的字符串形式
                let key_str = k.to_string();
                if let Some(vs) = v.string() {
                    cmd.env(key_str, vs);
                }
            }
        }
    }

    // 配置 stdin/stdout/stderr
    let needs_stdin = input.is_some();
    cmd.stdin(if needs_stdin {
        Stdio::piped()
    } else {
        Stdio::null()
    });
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    // ===== 5. 启动进程 =====
    let start = Instant::now();
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            return Ok(failure_result(command, -1, &format!("启动命令失败: {}", e)));
        }
    };

    let pid = child.id() as i64;

    // ===== 6. 写入 stdin（如果有）=====
    // 在独立线程写入 stdin，避免主线程阻塞。
    // 原因：若子进程不读 stdin 但产生大量 stdout，write_all 会因管道缓冲区满
    // 而阻塞，而主线程的阻塞会延迟进入输出读取阶段，形成死锁。
    let stdin_handle = input.and_then(|input_bytes| {
        child.stdin.take().map(|mut stdin| {
            std::thread::spawn(move || -> std::io::Result<()> {
                use std::io::Write;
                let _ = stdin.write_all(&input_bytes);
                // stdin drop 时关闭管道
                Ok(())
            })
        })
    });

    // ===== 7. 异步读取输出 + 超时控制 =====
    // 用独立线程读取 stdout/stderr，主线程用 wait_with_output 无法做超时，
    // 因此手动用 recv_timeout 轮询。
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let mut truncated = false;
    let mut timed_out = false;

    // 读线程：独立限制 stdout/stderr 各自的大小为 max_output
    let total_limit = max_output;
    let stdout_handle = spawn_output_reader(child.stdout.take(), total_limit);

    let stderr_handle = spawn_output_reader(child.stderr.take(), total_limit);

    // 主线程：等待子进程，带超时
    let exit_code: i64;
    let timeout_dur = if timeout_secs > 0 {
        Some(std::time::Duration::from_secs(timeout_secs))
    } else {
        None
    };

    loop {
        // 非阻塞检查子进程是否退出
        match child.try_wait() {
            Ok(Some(status)) => {
                exit_code = status.code().unwrap_or(-1) as i64;
                break;
            }
            Ok(None) => {
                // 仍在运行，检查超时
                if let Some(dur) = timeout_dur {
                    if start.elapsed() >= dur {
                        // 超时前二次确认：子进程可能在 10ms 休眠窗口内已退出，
                        // 此时不应误报为超时，应使用真实退出码。
                        match child.try_wait() {
                            Ok(Some(status)) => {
                                exit_code = status.code().unwrap_or(-1) as i64;
                            }
                            _ => {
                                // 子进程仍在运行，强制终止并回收
                                let _ = child.kill();
                                let _ = child.wait();
                                timed_out = true;
                                exit_code = -1;
                            }
                        }
                        break;
                    }
                }
                // 短暂休眠避免 busy-loop
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            Err(_) => {
                // try_wait 出错（如 EINTR）：确保子进程被终止和回收，避免僵尸进程
                let _ = child.kill();
                let _ = child.wait();
                exit_code = -1;
                break;
            }
        }
    }

    // 收集读线程结果
    match stdout_handle.join() {
        Ok(Ok((buf, exc))) => {
            stdout = buf;
            if exc {
                truncated = true;
            }
        }
        Ok(Err(e)) => {
            let msg = format!("\n[xhrun] 读取 stdout 失败: {}", e);
            stderr.extend_from_slice(msg.as_bytes());
        }
        Err(_) => {}
    }
    match stderr_handle.join() {
        Ok(Ok((buf, exc))) => {
            // 合并读线程已收集的 stderr
            stderr.extend_from_slice(&buf);
            if exc {
                truncated = true;
            }
        }
        Ok(Err(e)) => {
            let msg = format!("\n[xhrun] 读取 stderr 失败: {}", e);
            stderr.extend_from_slice(msg.as_bytes());
        }
        Err(_) => {}
    }

    // 等待 stdin 线程结束（子进程被 kill 后管道关闭，write 线程会退出）
    // 忽略 stdin 写入错误（子进程可能已提前关闭 stdin）
    if let Some(handle) = stdin_handle {
        let _ = handle.join();
    }

    let elapsed_ms = start.elapsed().as_millis() as i64;

    // ===== 8. 构建返回数组 =====
    let mut result = ZendHashTable::new();
    let success = exit_code == 0 && !timed_out && !truncated;
    let _ = result.insert("success", success);
    let _ = result.insert("exit_code", exit_code);

    // stdout/stderr：二进制安全写入
    if !stdout.is_empty() {
        let mut zv = Zval::new();
        zv.set_binary::<u8>(stdout);
        let _ = result.insert("stdout", zv);
    } else {
        let _ = result.insert("stdout", "");
    }

    if !stderr.is_empty() {
        let mut zv = Zval::new();
        zv.set_binary::<u8>(stderr);
        let _ = result.insert("stderr", zv);
    } else {
        let _ = result.insert("stderr", "");
    }

    let _ = result.insert("elapsed_ms", elapsed_ms);
    let _ = result.insert("pid", pid);
    let _ = result.insert("timed_out", timed_out);
    let _ = result.insert("truncated", truncated);

    if timed_out {
        let _ = result.insert("error", format!("命令执行超时（{} 秒）", timeout_secs));
        let _ = result.insert("command", command);
    } else if truncated {
        let _ = result.insert(
            "error",
            format!("输出超过最大限制 {} 字节，已截断", max_output),
        );
        let _ = result.insert("command", command);
    }

    Ok(result)
}

/// 从 options 数组读取整型选项，未设置时返回默认值。
fn opt_long(options: Option<&ZendHashTable>, key: &str, default: i64) -> i64 {
    options
        .and_then(|ht| ht.get(key))
        .and_then(|v| v.long())
        .unwrap_or(default)
}

/// 从 options 数组读取布尔选项，未设置时返回默认值。
fn opt_bool(options: Option<&ZendHashTable>, key: &str, default: bool) -> bool {
    options
        .and_then(|ht| ht.get(key))
        .and_then(|v| v.bool())
        .unwrap_or(default)
}

/// 从 options 数组读取字符串选项，未设置时返回 None。
fn opt_string(options: Option<&ZendHashTable>, key: &str) -> Option<String> {
    options
        .and_then(|ht| ht.get(key))
        .and_then(|v| v.string().map(|s| s.to_string()))
}

/// 从 options 数组读取字符串数组选项（白名单/黑名单）。
fn opt_string_vec(options: Option<&ZendHashTable>, key: &str) -> Vec<String> {
    let ht = match options.and_then(|o| o.get(key)) {
        Some(v) => v,
        None => return Vec::new(),
    };
    let arr: Option<&ZendHashTable> =
        <&ZendHashTable as ext_php_rs::convert::FromZval>::from_zval(ht);
    match arr {
        Some(a) => {
            let mut v = Vec::with_capacity(a.len());
            let _ = for_each_kv(a, |_key, val| {
                if let Some(s) = val.string() {
                    v.push(s.to_string());
                }
                Ok(())
            });
            v
        }
        None => Vec::new(),
    }
}

/// 构建 xhrun 的失败结果数组（命令未执行的情况）。
fn failure_result(command: &str, exit_code: i64, error: &str) -> ZBox<ZendHashTable> {
    let mut result = ZendHashTable::new();
    let _ = result.insert("success", false);
    let _ = result.insert("exit_code", exit_code);
    let _ = result.insert("stdout", "");
    let _ = result.insert("stderr", "");
    let _ = result.insert("elapsed_ms", 0_i64);
    let _ = result.insert("pid", 0_i64);
    let _ = result.insert("timed_out", false);
    let _ = result.insert("truncated", false);
    // error 与 command 分离，避免 error 字段内命令名重复
    let _ = result.insert("error", error);
    let _ = result.insert("command", command);
    result
}

/// 在子线程中读取子进程输出流，带大小限制。
///
/// 用于 xhrun 的 stdout/stderr 读取，避免主线程阻塞导致的死锁。
/// 返回 (读取到的字节, 是否因超过 limit 被截断)。
fn spawn_output_reader<R: std::io::Read + Send + 'static>(
    mut stream: Option<R>,
    limit: usize,
) -> std::thread::JoinHandle<std::io::Result<(Vec<u8>, bool)>> {
    std::thread::spawn(move || -> std::io::Result<(Vec<u8>, bool)> {
        let mut buf = Vec::new();
        let mut exceeded = false;
        if let Some(out) = stream.as_mut() {
            let mut tmp = [0u8; 8192];
            loop {
                match out.read(&mut tmp) {
                    Ok(0) => break,
                    Ok(n) => {
                        // 用 checked_add 防止 limit=usize::MAX 时 buf.len()+n 整数回绕
                        if buf.len().checked_add(n).is_none_or(|s| s > limit) {
                            // 超限或溢出：写入剩余可用部分，标记 exceeded 并停止
                            let remaining = limit.saturating_sub(buf.len());
                            if remaining > 0 {
                                buf.extend_from_slice(&tmp[..remaining]);
                            }
                            exceeded = true;
                            break;
                        }
                        buf.extend_from_slice(&tmp[..n]);
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(e) => return Err(e),
                }
            }
        }
        Ok((buf, exceeded))
    })
}

// +----------------------------------------------------------------------+
// | 单元测试：xhrun shell 参数转义                                        |
// +----------------------------------------------------------------------+

#[cfg(all(test, feature = "php"))]
mod tests {
    use super::{shell_quote_unix, shell_quote_windows};

    #[test]
    fn test_shell_quote_unix_simple() {
        assert_eq!(shell_quote_unix("hello"), "'hello'");
    }

    #[test]
    fn test_shell_quote_unix_with_metachars() {
        // 元字符应被单引号原样包裹，不被 shell 解释
        let q = shell_quote_unix("foo; rm -rf /");
        assert_eq!(q, "'foo; rm -rf /'");
    }

    #[test]
    fn test_shell_quote_unix_with_single_quote() {
        // 内部单引号需用 '\'' 转义
        assert_eq!(shell_quote_unix("a'b"), "'a'\\''b'");
    }

    #[test]
    fn test_shell_quote_unix_empty() {
        assert_eq!(shell_quote_unix(""), "''");
    }

    #[test]
    fn test_shell_quote_windows_escapes_double_quote() {
        let q = shell_quote_windows("a\"b");
        assert_eq!(q, "\"a\\\"b\"");
    }

    #[test]
    fn test_shell_quote_windows_escapes_metachars() {
        let q = shell_quote_windows("foo&bar|baz");
        // & 和 | 应被 ^ 前缀抑制
        assert!(q.contains("^&"));
        assert!(q.contains("^|"));
    }
}

// +----------------------------------------------------------------------+
// | PHP 模块入口                                                          |
// | 注意：#[php_module] 宏必须放在文件最后                                 |
// |                                                                       |
// | ext-php-rs 0.14+ 改用 inventory crate，不再通过全局状态自动收集类，  |
// | 必须在 #[php_module] 函数体中显式调用 .class::<T>() 注册每个类。      |
// +----------------------------------------------------------------------+

#[php_module]
pub fn get_module(module: ModuleBuilder) -> ModuleBuilder {
    module
        .class::<PhpXhCurl>()
        .class::<PhpXhRequest>()
        .class::<PhpXhMulti>()
        .class::<PhpXhThreadPool>()
        .function(wrap_function!(xhrun))
}
