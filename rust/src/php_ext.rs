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
pub(crate) fn global_client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        XhCurlManager::global()
            .create_client()
            .expect("初始化全局 HTTP 客户端失败")
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
        use ext_php_rs::convert::FromZval;
        use ext_php_rs::types::ZendClassObject;

        // 从 PHP 数组中提取 XHRequest 对象，转为 Vec<XhRequest>
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

        crate::fiber::fiber_gather(req_list).map_err(|e| e.to_string())
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

    /// 设置原始请求体
    ///
    /// # PHP 签名
    /// public XHRequest::body(string $data): $self_

    pub fn body(
        self_: &mut ZendClassObject<PhpXhRequest>,
        data: String,
    ) -> &mut ZendClassObject<PhpXhRequest> {
        self_.request = self_.request.clone().body_bytes(data.into_bytes());
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
                                    if let Some(s) = fval.string() {
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

                    if filename.is_some() {
                        mp_fields.push(MultipartField::file(
                            name,
                            filename.unwrap(),
                            value,
                            content_type.unwrap_or_else(|| "application/octet-stream".to_string()),
                        ));
                    } else {
                        mp_fields.push(MultipartField::text(
                            name,
                            String::from_utf8_lossy(&value).to_string(),
                        ));
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

        let result = global_runtime()
            .block_on(async move {
                let request_id = request
                    .get_id()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| request.get_url().to_string());
                XhMulti::execute_single(client, request, request_id, None, max_response_size).await
            })
            .map_err(|e| e.to_string())?;

        Ok(response_to_php_array(&result))
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

    /// 执行所有请求
    ///
    /// # PHP 签名
    /// public XHMulti::execute(): array

    pub fn execute(&mut self) -> Result<ZBox<ZendHashTable>, String> {
        // 复用全局运行时与客户端（避免每次创建/销毁的开销）
        // 运行时类型由 SAPI 决定：CLI 多线程并行，FPM 单线程并发
        let client = global_client().clone();
        let max_concurrency = self.max_concurrency;
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
/// 规避 ext-php-rs 0.12.0 中 `Iter` 的终止条件 bug：
/// `Iter::next_zval` 用 `key_type == -1` 判断结束，
/// 但 PHP 的 `HASH_KEY_NON_EXISTENT == 3`，该判断永不成立，
/// 导致遍历完最后一个元素后再调用 `next()` 会触发空指针解引用。
///
/// 本函数通过精确迭代 `ht.len()` 次来避免触发有缺陷的终止路径，
/// 仅在剩余次数内调用 `iter.next()`。
fn for_each_kv<F>(ht: &ZendHashTable, mut f: F) -> Result<(), String>
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
    // 关键：此处不再调用 iter.next()，避免触发 ext-php-rs 的终止 bug
    Ok(())
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
        let mut response_ht = ZendHashTable::new();
        let _ = response_ht.insert("id", result.id.clone());
        let _ = response_ht.insert("success", result.is_success());
        let _ = response_ht.insert("elapsed_ms", result.elapsed.as_millis() as i64);

        // 用户自定义数据：原样回传 JSON 字符串
        if let Some(ud) = &result.user_data {
            let _ = response_ht.insert("user_data", ud.clone());
        }

        if let Some(err) = &result.error {
            let _ = response_ht.insert("error", err.clone());
        }

        // 写入完整响应信息（status/body/headers/url/remote_addr/version 等）
        if let Some(resp) = &result.response {
            fill_response_fields(&mut response_ht, resp);
        }

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

/// 将 PHP 数组转换为 JSON 字符串
/// 递归处理嵌套数组
fn php_array_to_json(ht: &ZendHashTable) -> Result<String, String> {
    let mut map = serde_json::Map::new();

    // 使用安全迭代器（规避 ext-php-rs 0.12.0 的 Iter 终止 bug）
    for_each_kv(ht, |key, val| {
        let key_str = key.to_string();
        let json_val = zval_to_json(val)?;
        map.insert(key_str, json_val);
        Ok(())
    })?;

    let value = serde_json::Value::Object(map);
    serde_json::to_string(&value).map_err(|e| e.to_string())
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
        let mut map = serde_json::Map::new();
        // 嵌套数组同样使用安全迭代器
        for_each_kv(ht, |key, v| {
            map.insert(key.to_string(), zval_to_json(v)?);
            Ok(())
        })?;
        Ok(serde_json::Value::Object(map))
    } else {
        Ok(serde_json::Value::Null)
    }
}

/// 将 PHP 数组转换为表单键值对
/// 用于 application/x-www-form-urlencoded 请求
fn php_array_to_form(ht: &ZendHashTable) -> Vec<(String, String)> {
    let mut form = Vec::new();

    // 使用安全迭代器（规避 ext-php-rs 0.12.0 的 Iter 终止 bug）
    let _ = for_each_kv(ht, |key, val| {
        let key_str = key.to_string();
        let val_str = if let Some(s) = val.string() {
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

/// 将 JSON 值转换为 PHP 数组
/// 递归处理嵌套对象和数组
fn json_to_php_array(json: &serde_json::Value) -> Result<ZBox<ZendHashTable>, String> {
    let mut ht = ZendHashTable::new();

    if let Some(obj) = json.as_object() {
        for (key, val) in obj {
            match val {
                serde_json::Value::String(s) => {
                    let _ = ht.insert(key.as_str(), s.as_str());
                }
                serde_json::Value::Number(n) => {
                    if let Some(i) = n.as_i64() {
                        let _ = ht.insert(key.as_str(), i);
                    } else if let Some(f) = n.as_f64() {
                        let _ = ht.insert(key.as_str(), f);
                    }
                }
                serde_json::Value::Bool(b) => {
                    let _ = ht.insert(key.as_str(), *b);
                }
                serde_json::Value::Null => {
                    let _ = ht.insert(key.as_str(), "");
                }
                serde_json::Value::Array(arr) => {
                    let mut arr_ht = ZendHashTable::new();
                    for (i, item) in arr.iter().enumerate() {
                        // 使用整数键（与 PHP 数组索引语义一致）
                        if let Some(s) = item.as_str() {
                            let _ = arr_ht.insert_at_index(i as i64, s);
                        } else if let Some(n) = item.as_i64() {
                            let _ = arr_ht.insert_at_index(i as i64, n);
                        } else if let Some(f) = item.as_f64() {
                            let _ = arr_ht.insert_at_index(i as i64, f);
                        } else if let Some(b) = item.as_bool() {
                            let _ = arr_ht.insert_at_index(i as i64, b);
                        }
                    }
                    let _ = ht.insert(key.as_str(), arr_ht);
                }
                serde_json::Value::Object(inner) => {
                    let inner_ht = json_to_php_array(&serde_json::Value::Object(inner.clone()))?;
                    let _ = ht.insert(key.as_str(), inner_ht);
                }
            }
        }
    }

    Ok(ht)
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
}
