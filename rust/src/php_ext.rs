// +----------------------------------------------------------------------+
// | XHCurl 扩展 - PHP 扩展入口（ext-php-rs 绑定）                         |
// |                                                                        |
// | 使用 ext-php-rs crate 直接生成 PHP 扩展                                |
// | 无需 C 桥接层，Rust 函数直接暴露为 PHP 函数/方法                       |
// |                                                                        |
// | PHP 类映射：                                                           |
// |   XHCurl      → 全局管理器（静态方法）                                 |
// |   XHRequest   → 请求构建器（链式调用）                                 |
// |   XHResponse  → 响应对象                                               |
// |   XHMulti     → 异步批量执行器                                         |
// |   XHThreadPool→ 线程池                                                 |
// |                                                                        |
// | 链式调用实现：                                                          |
// |   ext-php-rs 0.14+：方法第一个参数命名为 self_（类型 &mut ZendClassObject<>），|
// |   修改后返回同一对象，实现 PHP 端 $this 链式调用。                      |
// +----------------------------------------------------------------------+

use std::sync::OnceLock;

use ext_php_rs::boxed::ZBox;
use ext_php_rs::prelude::*;
use ext_php_rs::types::{ArrayKey, ZendClassObject, ZendHashTable, Zval};
use ext_php_rs::zend::php_sapi_name;

use crate::curl::XhCurlManager;
use crate::error::MAX_REQUESTS_PER_BATCH;
use crate::multi::XhMulti;
use crate::request::{HttpMethod, XhRequest};
use crate::response::XhResponse;
use crate::threadpool::{ThreadPoolConfig, XhThreadPool};

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
fn sapi_is_cli() -> bool {
    php_sapi_name() == "cli"
}

/// 获取全局共享的 reqwest 客户端。
///
/// reqwest Client 内部维护连接池（TCP keep-alive、TLS 会话缓存），
/// 全局复用可避免每次请求重新建连，显著提升批量请求性能。
/// 使用 OnceLock 保证线程安全的延迟初始化（仅创建一次）。
///
/// # 代理配置失败处理
/// 若全局代理地址无效（`create_client` 返回错误），直接 panic。
/// 理由：
/// 1. 代理通常是安全/隐私相关配置，静默降级到无代理会让用户误以为
///    请求走了代理，实际暴露真实 IP，这比直接报错更危险。
/// 2. OnceLock 只初始化一次，降级后整个进程生命周期内不会重试，
///    用户无法发现配置问题。
/// 3. 与 `create_client_builder` 的设计一致：代理无效必须明确报错。
/// 4. panic 只在首次调用 `global_client()` 时发生，用户修正配置后即可恢复。
pub(crate) fn global_client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        XhCurlManager::global()
            .create_client()
            .expect("全局 reqwest 客户端初始化失败（请检查 proxy/SSL 等全局配置）")
    })
}

/// 获取全局共享的 tokio 运行时。
///
/// 运行时类型根据 SAPI 自动选择：
/// - CLI 模式：多线程运行时（真正的 M:N 并行，工作线程数 = CPU 核心数）
/// - FPM 模式：单线程运行时（协作式并发，避免线程安全问题）
///
/// 全局复用避免每次 execute() 都创建/销毁运行时（线程创建、IO 驱动注册开销大）。
/// FPM 下每个 worker 进程持有独立的运行时（进程级单例），请求间复用。
pub(crate) fn global_runtime() -> &'static tokio::runtime::Runtime {
    static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| {
        let mut builder = if sapi_is_cli() {
            tokio::runtime::Builder::new_multi_thread()
        } else {
            tokio::runtime::Builder::new_current_thread()
        };
        builder
            .enable_all()
            .thread_name("xhcurl-worker")
            .build()
            .expect("初始化全局 tokio 运行时失败")
    })
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

    let mut req_list: Vec<XhRequest> = Vec::new();
    let len = requests.len();
    let mut iter = requests.iter();
    for _ in 0..len {
        match iter.next() {
            Some((_key, val)) => {
                // 尝试将 Zval 转为 &ZendClassObject<PhpXhRequest>
                let class_obj: Option<&ZendClassObject<PhpXhRequest>> =
                    <&ZendClassObject<PhpXhRequest> as FromZval>::from_zval(val);
                match class_obj {
                    Some(obj) => req_list.push(obj.request.clone()),
                    None => return Err("数组元素不是 XHRequest 对象".to_string()),
                }
            }
            None => break,
        }
    }
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

        // 使用闭包修改配置，避免手动管理锁
        manager.modify_config(|c| {
            // 从 PHP 数组读取配置项
            // 每个配置项都是可选的，只处理存在的键

            // 连接超时（秒）
            if let Some(timeout) = config.get("connect_timeout") {
                if let Some(v) = timeout.long() {
                    c.connect_timeout = v as u64;
                }
            }

            // 请求超时（秒）
            if let Some(timeout) = config.get("request_timeout") {
                if let Some(v) = timeout.long() {
                    c.request_timeout = v as u64;
                }
            }

            // 最大响应体大小（字节），防止内存溢出
            if let Some(max_size) = config.get("max_response_size") {
                if let Some(v) = max_size.long() {
                    c.max_response_size = v as usize;
                }
            }

            // 是否跟随重定向
            if let Some(follow) = config.get("follow_redirects") {
                if let Some(v) = follow.bool() {
                    c.follow_redirects = v;
                }
            }

            // 最大重定向次数
            if let Some(max_redirects) = config.get("max_redirects") {
                if let Some(v) = max_redirects.long() {
                    c.max_redirects = v as u32;
                }
            }

            // 是否验证 SSL 证书
            if let Some(verify) = config.get("verify_ssl") {
                if let Some(v) = verify.bool() {
                    c.verify_ssl = v;
                }
            }

            // 自定义 User-Agent
            if let Some(ua) = config.get("user_agent") {
                if let Some(v) = ua.string() {
                    c.user_agent = v;
                }
            }

            // 代理地址
            if let Some(proxy) = config.get("proxy") {
                if let Some(v) = proxy.string() {
                    c.proxy = Some(v);
                }
            }

            // 是否启用 HTTP/2
            if let Some(v) = config.get("http2_enabled") {
                if let Some(b) = v.bool() {
                    c.http2_enabled = b;
                }
            }

            // 是否启用 TCP Keep-Alive
            if let Some(v) = config.get("tcp_keepalive") {
                if let Some(b) = v.bool() {
                    c.tcp_keepalive = b;
                }
            }

            // TCP Keep-Alive 间隔（秒）
            if let Some(v) = config.get("tcp_keepalive_interval") {
                if let Some(l) = v.long() {
                    c.tcp_keepalive_interval = l as u64;
                }
            }

            // 默认并发连接数限制
            if let Some(v) = config.get("max_connections") {
                if let Some(l) = v.long() {
                    c.max_connections = l as usize;
                }
            }
        });

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
        let _ = ht.insert("max_connections", config.max_connections as i64);

        // 代理地址（可选）
        if let Some(proxy) = &config.proxy {
            let _ = ht.insert("proxy", proxy.as_str());
        }

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
    pub fn method(
        self_: &mut ZendClassObject<PhpXhRequest>,
        method: String,
    ) -> Result<&mut ZendClassObject<PhpXhRequest>, String> {
        let m = HttpMethod::from_str(&method).map_err(|e| e.to_string())?;
        self_.request = self_.request.clone().method(m);
        Ok(self_)
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
    pub fn json<'a>(
        self_: &'a mut ZendClassObject<PhpXhRequest>,
        data: &ZendHashTable,
    ) -> Result<&'a mut ZendClassObject<PhpXhRequest>, String> {
        let json_str = php_array_to_json(data)?;
        self_.request = self_
            .request
            .clone()
            .body_json_str(&json_str)
            .map_err(|e| e.to_string())?;
        Ok(self_)
    }

    /// 设置表单数据
    ///
    /// # PHP 签名
    /// public XHRequest::form(array $data): $self_
    pub fn form<'a>(
        self_: &'a mut ZendClassObject<PhpXhRequest>,
        data: &ZendHashTable,
    ) -> Result<&'a mut ZendClassObject<PhpXhRequest>, String> {
        let form = php_array_to_form(data);
        self_.request = self_.request.clone().body_form(form);
        Ok(self_)
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
    pub fn timeout(
        self_: &mut ZendClassObject<PhpXhRequest>,
        seconds: i64,
    ) -> &mut ZendClassObject<PhpXhRequest> {
        self_.request = self_.request.clone().request_timeout(seconds as u64);
        self_
    }

    /// 设置连接超时（秒）
    /// 与 timeout 不同，此方法仅控制连接阶段的超时
    ///
    /// # PHP 签名
    /// public XHRequest::connectTimeout(int $seconds): $self_
    pub fn connect_timeout(
        self_: &mut ZendClassObject<PhpXhRequest>,
        seconds: i64,
    ) -> &mut ZendClassObject<PhpXhRequest> {
        self_.request = self_.request.clone().connect_timeout(seconds as u64);
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
    pub fn max_redirects(
        self_: &mut ZendClassObject<PhpXhRequest>,
        max: i64,
    ) -> &mut ZendClassObject<PhpXhRequest> {
        self_.request = self_.request.clone().max_redirects(max as u32);
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
    pub fn set_user_data<'a>(
        self_: &'a mut ZendClassObject<PhpXhRequest>,
        data: &ZendHashTable,
    ) -> Result<&'a mut ZendClassObject<PhpXhRequest>, String> {
        // 将 PHP 数组序列化为 JSON 字符串存储
        let json_str = php_array_to_json(data)?;
        self_.request = self_.request.clone().user_data(json_str);
        Ok(self_)
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
    pub fn multipart<'a>(
        self_: &'a mut ZendClassObject<PhpXhRequest>,
        fields: &ZendHashTable,
    ) -> Result<&'a mut ZendClassObject<PhpXhRequest>, String> {
        use crate::request::MultipartField;

        let mut mp_fields = Vec::new();
        let len = fields.len();
        let mut iter = fields.iter();
        for _ in 0..len {
            match iter.next() {
                Some((_key, val)) => {
                    let field_ht = val.array().ok_or("multipart 字段必须是数组")?;
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
        Ok(self_)
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
        let client = global_client().clone();
        let request = self.request.clone();
        let max_response_size = XhCurlManager::global().config().max_response_size;

        // 预取 id 和 user_data（request 会被 move 进 async 块）
        let id = request
            .get_id()
            .map(|s| s.to_string())
            .unwrap_or_else(|| request.get_url().to_string());
        let user_data = request.get_user_data().map(|s| s.to_string());

        let id_for_task = id.clone();
        let result = global_runtime()
            .block_on(async move {
                XhMulti::execute_single(client, request, id_for_task, None, max_response_size).await
            })
            .map_err(|e| e.to_string())?;

        // response_to_php_array 仅填充响应字段（status/body/headers/...），
        // 需补充 id 和 user_data 以与其他 API（await/gather/multi/threadpool）返回字段一致。
        let mut ht = response_to_php_array(&result);
        let _ = ht.insert("id", id);
        if let Some(ud) = user_data {
            let _ = ht.insert("user_data", ud);
        }
        Ok(ht)
    }
}

// +----------------------------------------------------------------------+
// | PHP 类：XHResponse（响应对象）                                        |
// +----------------------------------------------------------------------+

/// PHP XHResponse 类的 Rust 表示
#[php_class]
#[php(name = "XHResponse")]
pub struct PhpXhResponse {
    /// 内部响应对象
    response: Option<XhResponse>,
}

/// PHP XHResponse 类的方法实现
#[php_impl]
impl PhpXhResponse {
    /// 获取状态码
    pub fn status(&self) -> i64 {
        self.response
            .as_ref()
            .map(|r| r.status() as i64)
            .unwrap_or(0)
    }

    /// 检查是否成功（2xx 状态码）
    pub fn is_success(&self) -> bool {
        self.response
            .as_ref()
            .map(|r| r.is_success())
            .unwrap_or(false)
    }

    /// 获取指定响应头
    pub fn header(&self, name: String) -> Option<String> {
        self.response.as_ref().and_then(|r| r.header(&name))
    }

    /// 获取所有响应头
    pub fn headers(&self) -> ZBox<ZendHashTable> {
        let mut ht = ZendHashTable::new();
        if let Some(response) = &self.response {
            for (name, value) in response.headers().all() {
                let _ = ht.insert(name.as_str(), value);
            }
        }
        ht
    }

    /// 获取响应体（字符串）
    pub fn body(&self) -> Result<String, String> {
        self.response
            .as_ref()
            .ok_or("响应不存在".to_string())?
            .body_text()
            .map_err(|e| e.to_string())
    }

    /// 获取响应体（JSON 解析为数组）
    pub fn json(&self) -> Result<ZBox<ZendHashTable>, String> {
        let response = self.response.as_ref().ok_or("响应不存在".to_string())?;
        let json = response.body_json().map_err(|e| e.to_string())?;
        json_to_php_array(&json)
    }

    /// 获取响应体大小（字节）
    pub fn body_size(&self) -> i64 {
        self.response
            .as_ref()
            .map(|r| r.body_size() as i64)
            .unwrap_or(0)
    }

    /// 获取最终 URL（可能因重定向而与请求 URL 不同）
    pub fn url(&self) -> String {
        self.response
            .as_ref()
            .map(|r| r.url().to_string())
            .unwrap_or_default()
    }

    /// 获取请求耗时（毫秒）
    pub fn elapsed_ms(&self) -> i64 {
        self.response
            .as_ref()
            .map(|r| r.elapsed().as_millis() as i64)
            .unwrap_or(0)
    }

    /// 获取错误信息
    pub fn error(&self) -> Option<String> {
        self.response
            .as_ref()
            .and_then(|r| r.error().map(|s| s.to_string()))
    }

    /// 获取远程服务器地址（IP:Port）
    pub fn remote_addr(&self) -> Option<String> {
        self.response
            .as_ref()
            .and_then(|r| r.remote_addr().map(|s| s.to_string()))
    }

    /// 获取 HTTP 协议版本
    pub fn version(&self) -> Option<String> {
        self.response
            .as_ref()
            .and_then(|r| r.version().map(|s| s.to_string()))
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
    pub fn max_concurrency(
        self_: &mut ZendClassObject<PhpXhMulti>,
        max: i64,
    ) -> &mut ZendClassObject<PhpXhMulti> {
        self_.max_concurrency = max as usize;
        self_
    }

    /// 设置单个响应的最大响应体大小（字节）
    /// 防止恶意服务器返回超大响应导致内存溢出。
    /// 0 = 使用全局配置（默认 10MB）。
    ///
    /// # PHP 签名
    /// public XHMulti::maxResponseSize(int $size): $self_
    pub fn max_response_size(
        self_: &mut ZendClassObject<PhpXhMulti>,
        size: i64,
    ) -> &mut ZendClassObject<PhpXhMulti> {
        self_.max_response_size = size as usize;
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
    pub fn timeout(
        self_: &mut ZendClassObject<PhpXhMulti>,
        secs: i64,
    ) -> &mut ZendClassObject<PhpXhMulti> {
        self_.timeout = if secs > 0 { secs as u64 } else { 0 };
        self_
    }

    /// 执行所有请求
    ///
    /// # PHP 签名
    /// public XHMulti::execute(): array
    pub fn execute(&mut self) -> Result<ZBox<ZendHashTable>, String> {
        // 复用全局运行时与客户端（避免每次创建/销毁的开销）
        // 运行时类型由 SAPI 决定：CLI 多线程并行，FPM 单线程并发
        let client = global_client().clone();
        let max_concurrency = self.max_concurrency;
        let timeout = self.timeout;
        let requests = std::mem::take(&mut self.requests);
        // 0 表示使用全局配置值
        let max_resp_size = if self.max_response_size > 0 {
            self.max_response_size
        } else {
            XhCurlManager::global().config().max_response_size
        };

        let results = global_runtime()
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
    pub fn __construct(workers: Option<i64>) -> Self {
        Self {
            pool: None,
            requests: Vec::new(),
            max_concurrency: workers.unwrap_or(0) as usize,
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

        // 复用全局运行时与客户端；通过 take/存回 模式避免借用 self
        let (returned_pool, result) = global_runtime().block_on(async move {
            // 首次调用时创建线程池，后续复用
            if pool.is_none() {
                let client = global_client().clone();
                let mut config = ThreadPoolConfig::default();
                if max_concurrency > 0 {
                    config.worker_count = max_concurrency;
                }
                pool = Some(XhThreadPool::new(config, client));
            }
            let p = pool.as_mut().expect("线程池已初始化");
            let res = p.execute_all(requests).await;
            (pool, res)
        });

        // 存回线程池以便下次 execute 复用
        self.pool = returned_pool;
        let results = result.map_err(|e| e.to_string())?;

        // 转换为 PHP 数组
        results_to_php_array(&results)
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
        // 无响应（请求失败）时，补充 elapsed_ms/error/body，确保失败路径字段完整
        // body 插入空字符串，与 fill_response_fields 的成功路径保持字段一致
        let _ = response_ht.insert("elapsed_ms", result.elapsed.as_millis() as i64);
        let _ = response_ht.insert("body", String::new());
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

/// 将 JSON 值转换为 PHP 数组（ZendHashTable）。
///
/// 递归处理嵌套对象和数组，确保所有 JSON 类型（含 Null、嵌套 Object/Array）
/// 都被正确转换，而非静默丢弃。
///
/// - 顶层为 JSON Object → 返回字符串键的关联数组
/// - 顶层为 JSON Array → 返回整数键的索引数组
/// - 其他标量类型 → 返回空数组（调用方应确保传入的是 Object 或 Array）
fn json_to_php_array(json: &serde_json::Value) -> Result<ZBox<ZendHashTable>, String> {
    match json {
        serde_json::Value::Object(obj) => json_object_to_php_array(obj),
        serde_json::Value::Array(arr) => json_array_to_php_array(arr),
        _ => Ok(ZendHashTable::new()),
    }
}

/// 将 JSON Object 转为 PHP 关联数组（字符串键）。
fn json_object_to_php_array(
    obj: &serde_json::Map<String, serde_json::Value>,
) -> Result<ZBox<ZendHashTable>, String> {
    let mut ht = ZendHashTable::new();
    for (key, val) in obj {
        json_insert_value(&mut ht, key.clone(), val)?;
    }
    Ok(ht)
}

/// 将 JSON Array 转为 PHP 索引数组（整数键）。
fn json_array_to_php_array(arr: &[serde_json::Value]) -> Result<ZBox<ZendHashTable>, String> {
    let mut ht = ZendHashTable::new();
    for (i, val) in arr.iter().enumerate() {
        json_insert_value(&mut ht, i as i64, val)?;
    }
    Ok(ht)
}

/// 将单个 JSON 值插入哈希表的指定键位置（支持字符串键或整数索引）。
///
/// 递归处理嵌套类型，确保 Null/Object/Array 不被静默丢弃。
/// `K` 须满足 `Into<ArrayKey<'static>>`，即 `String` 或 `i64` 等所有权类型。
fn json_insert_value<K: Into<ext_php_rs::types::ArrayKey<'static>>>(
    ht: &mut ZendHashTable,
    key: K,
    val: &serde_json::Value,
) -> Result<(), String> {
    match val {
        serde_json::Value::String(s) => {
            let _ = ht.insert(key, s.as_str());
        }
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                let _ = ht.insert(key, i);
            } else if let Some(f) = n.as_f64() {
                let _ = ht.insert(key, f);
            }
        }
        serde_json::Value::Bool(b) => {
            let _ = ht.insert(key, *b);
        }
        serde_json::Value::Null => {
            let _ = ht.insert(key, ());
        }
        serde_json::Value::Object(inner) => {
            let inner_ht = json_object_to_php_array(inner)?;
            let _ = ht.insert(key, inner_ht);
        }
        serde_json::Value::Array(inner) => {
            let inner_ht = json_array_to_php_array(inner)?;
            let _ = ht.insert(key, inner_ht);
        }
    }
    Ok(())
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
///   不经过 shell 解析。仅在 `shell => true` 时，`args` 会拼接进命令行。
/// - `options`: 选项数组，支持以下键：
///   - `timeout` (int): 超时秒数，0 = 无超时。默认 60。
///   - `max_output` (int): 每个流（stdout/stderr）的最大输出字节数，0 = 无限制。默认 64MB。
///   - `cwd` (string): 工作目录。默认继承当前进程。
///   - `env` (array): 环境变量键值对。默认继承当前进程。
///   - `shell` (bool): 是否通过系统 shell 执行（启用管道/通配符/重定向支持，
///     但会引入 shell 注入风险，需谨慎）。默认 false。
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
/// // 需要管道时显式启用 shell（注意：此时 $args 不会转义，需自行确保安全）
/// $r = xhrun('ls -la /tmp | grep foo', [], ['shell' => true]);
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
        // 注意：此模式有 shell 注入风险，需调用方自行确保输入安全
        let mut full = String::from(command);
        for a in &arg_vec {
            full.push(' ');
            full.push_str(a);
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
            let mut iter = a.iter();
            for _ in 0..a.len() {
                if let Some((_, val)) = iter.next() {
                    if let Some(s) = val.string() {
                        v.push(s.to_string());
                    }
                }
            }
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
                        if buf.len() + n <= limit {
                            buf.extend_from_slice(&tmp[..n]);
                        } else {
                            let remaining = limit.saturating_sub(buf.len());
                            if remaining > 0 {
                                buf.extend_from_slice(&tmp[..remaining]);
                            }
                            exceeded = true;
                            break;
                        }
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
        .class::<PhpXhResponse>()
        .class::<PhpXhMulti>()
        .class::<PhpXhThreadPool>()
        .function(wrap_function!(xhrun))
}
