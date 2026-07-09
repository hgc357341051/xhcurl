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
// |   使用 #[this] 参数属性获取 &mut ZendClassObject<Self>，              |
// |   修改后返回同一对象，实现 PHP 端 $this 链式调用。                      |
// +----------------------------------------------------------------------+

use ext_php_rs::boxed::ZBox;
use ext_php_rs::prelude::*;
use ext_php_rs::types::{ZendClassObject, ZendHashTable, Zval};

use crate::curl::XhCurlManager;
use crate::multi::XhMulti;
use crate::request::{HttpMethod, XhRequest};
use crate::response::XhResponse;
use crate::threadpool::{ThreadPoolConfig, XhThreadPool};

// +----------------------------------------------------------------------+
// | 常量定义                                                              |
// +----------------------------------------------------------------------+

/// 单次批量请求的最大数量限制
/// 防止用户传入过多请求导致内存溢出
const MAX_REQUESTS_PER_BATCH: usize = 10000;

// +----------------------------------------------------------------------+
// | PHP 类：XHCurl（全局管理器）                                          |
// +----------------------------------------------------------------------+

/// PHP XHCurl 类的 Rust 表示
/// 对应 C 版本的 XHCurl 全局管理器
#[php_class(name = "XHCurl")]
pub struct PhpXhCurl;

/// PHP XHCurl 类的方法实现
#[php_impl]
impl PhpXhCurl {
    /// 获取扩展版本
    ///
    /// # PHP 签名
    /// public static XHCurl::version(): string
    #[php_method]
    pub fn version() -> String {
        env!("CARGO_PKG_VERSION").to_string()
    }

    /// 设置全局配置
    ///
    /// # PHP 签名
    /// public static XHCurl::setConfig(array $config): void
    #[php_method]
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
    #[php_method]
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
    #[php_method]
    pub fn is_cli() -> bool {
        XhCurlManager::is_cli_mode()
    }

    /// 创建请求构建器
    ///
    /// # PHP 签名
    /// public static XHCurl::createRequest(string $url): XHRequest
    #[php_method]
    pub fn create_request(url: String) -> Result<PhpXhRequest, String> {
        Ok(PhpXhRequest {
            request: XhRequest::new(url),
        })
    }
}

// +----------------------------------------------------------------------+
// | PHP 类：XHRequest（请求构建器，链式调用）                             |
// +----------------------------------------------------------------------+

/// PHP XHRequest 类的 Rust 表示
#[php_class(name = "XHRequest")]
pub struct PhpXhRequest {
    /// 内部请求构建器
    request: XhRequest,
}

/// PHP XHRequest 类的方法实现
///
/// 链式调用实现：
/// 使用 #[this] 获取 &mut ZendClassObject<Self>，
/// 修改内部状态后返回同一对象引用，
/// 使 PHP 端可以 $req->get()->header("X","Y")->timeout(30) 链式调用。
#[php_impl]
impl PhpXhRequest {
    /// 构造函数
    ///
    /// # PHP 签名
    /// public XHRequest::__construct(string $url)
    #[php_method]
    pub fn __construct(url: String) -> Self {
        Self {
            request: XhRequest::new(url),
        }
    }

    /// 设置 HTTP 方法
    ///
    /// # PHP 签名
    /// public XHRequest::method(string $method): $this
    #[php_method]
    pub fn method(
        #[this] this: &mut ZendClassObject<Self>,
        method: String,
    ) -> Result<&mut ZendClassObject<Self>, String> {
        let m = HttpMethod::from_str(&method).map_err(|e| e.to_string())?;
        this.request = this.request.clone().method(m);
        Ok(this)
    }

    /// 设置为 GET 方法
    ///
    /// # PHP 签名
    /// public XHRequest::get(): $this
    #[php_method]
    pub fn get(#[this] this: &mut ZendClassObject<Self>) -> &mut ZendClassObject<Self> {
        this.request = this.request.clone().get();
        this
    }

    /// 设置为 POST 方法
    #[php_method]
    pub fn post(#[this] this: &mut ZendClassObject<Self>) -> &mut ZendClassObject<Self> {
        this.request = this.request.clone().post();
        this
    }

    /// 设置为 PUT 方法
    #[php_method]
    pub fn put(#[this] this: &mut ZendClassObject<Self>) -> &mut ZendClassObject<Self> {
        this.request = this.request.clone().put();
        this
    }

    /// 设置为 DELETE 方法
    #[php_method]
    pub fn delete(#[this] this: &mut ZendClassObject<Self>) -> &mut ZendClassObject<Self> {
        this.request = this.request.clone().delete();
        this
    }

    /// 设置为 PATCH 方法
    #[php_method]
    pub fn patch(#[this] this: &mut ZendClassObject<Self>) -> &mut ZendClassObject<Self> {
        this.request = this.request.clone().patch();
        this
    }

    /// 设置为 HEAD 方法（仅获取响应头，不返回响应体）
    #[php_method]
    pub fn head(#[this] this: &mut ZendClassObject<Self>) -> &mut ZendClassObject<Self> {
        this.request = this.request.clone().head();
        this
    }

    /// 设置请求头
    ///
    /// # PHP 签名
    /// public XHRequest::header(string $name, string $value): $this
    #[php_method]
    pub fn header(
        #[this] this: &mut ZendClassObject<Self>,
        name: String,
        value: String,
    ) -> &mut ZendClassObject<Self> {
        this.request = this.request.clone().header(&name, &value);
        this
    }

    /// 设置 JSON 请求体
    ///
    /// # PHP 签名
    /// public XHRequest::json(array $data): $this
    #[php_method]
    pub fn json<'a>(
        #[this] this: &'a mut ZendClassObject<Self>,
        data: &ZendHashTable,
    ) -> Result<&'a mut ZendClassObject<Self>, String> {
        let json_str = php_array_to_json(data)?;
        this.request = this
            .request
            .clone()
            .body_json_str(&json_str)
            .map_err(|e| e.to_string())?;
        Ok(this)
    }

    /// 设置表单数据
    ///
    /// # PHP 签名
    /// public XHRequest::form(array $data): $this
    #[php_method]
    pub fn form<'a>(
        #[this] this: &'a mut ZendClassObject<Self>,
        data: &ZendHashTable,
    ) -> Result<&'a mut ZendClassObject<Self>, String> {
        let form = php_array_to_form(data);
        this.request = this.request.clone().body_form(form);
        Ok(this)
    }

    /// 设置原始请求体
    ///
    /// # PHP 签名
    /// public XHRequest::body(string $data): $this
    #[php_method]
    pub fn body(
        #[this] this: &mut ZendClassObject<Self>,
        data: String,
    ) -> &mut ZendClassObject<Self> {
        this.request = this.request.clone().body_bytes(data.into_bytes());
        this
    }

    /// 设置请求超时（秒）
    ///
    /// # PHP 签名
    /// public XHRequest::timeout(int $seconds): $this
    #[php_method]
    pub fn timeout(
        #[this] this: &mut ZendClassObject<Self>,
        seconds: i64,
    ) -> &mut ZendClassObject<Self> {
        this.request = this.request.clone().request_timeout(seconds as u64);
        this
    }

    /// 设置连接超时（秒）
    /// 与 timeout 不同，此方法仅控制连接阶段的超时
    ///
    /// # PHP 签名
    /// public XHRequest::connectTimeout(int $seconds): $this
    #[php_method]
    pub fn connect_timeout(
        #[this] this: &mut ZendClassObject<Self>,
        seconds: i64,
    ) -> &mut ZendClassObject<Self> {
        this.request = this.request.clone().connect_timeout(seconds as u64);
        this
    }

    /// 设置是否验证 SSL 证书
    ///
    /// # PHP 签名
    /// public XHRequest::verifySsl(bool $verify): $this
    #[php_method]
    pub fn verify_ssl(
        #[this] this: &mut ZendClassObject<Self>,
        verify: bool,
    ) -> &mut ZendClassObject<Self> {
        this.request = this.request.clone().verify_ssl(verify);
        this
    }

    /// 设置 User-Agent
    ///
    /// # PHP 签名
    /// public XHRequest::userAgent(string $ua): $this
    #[php_method]
    pub fn user_agent(
        #[this] this: &mut ZendClassObject<Self>,
        ua: String,
    ) -> &mut ZendClassObject<Self> {
        this.request = this.request.clone().user_agent(ua);
        this
    }

    /// 设置代理地址
    /// 支持 HTTP/HTTPS/SOCKS5 代理
    ///
    /// # PHP 签名
    /// public XHRequest::proxy(string $proxy): $this
    ///
    /// # 示例
    /// $req->proxy("http://127.0.0.1:7890");
    /// $req->proxy("socks5://127.0.0.1:1080");
    #[php_method]
    pub fn proxy(
        #[this] this: &mut ZendClassObject<Self>,
        proxy: String,
    ) -> &mut ZendClassObject<Self> {
        this.request = this.request.clone().proxy(proxy);
        this
    }

    /// 设置是否跟随重定向
    ///
    /// # PHP 签名
    /// public XHRequest::followRedirects(bool $follow): $this
    #[php_method]
    pub fn follow_redirects(
        #[this] this: &mut ZendClassObject<Self>,
        follow: bool,
    ) -> &mut ZendClassObject<Self> {
        this.request = this.request.clone().follow_redirects(follow);
        this
    }

    /// 设置最大重定向次数
    ///
    /// # PHP 签名
    /// public XHRequest::maxRedirects(int $max): $this
    #[php_method]
    pub fn max_redirects(
        #[this] this: &mut ZendClassObject<Self>,
        max: i64,
    ) -> &mut ZendClassObject<Self> {
        this.request = this.request.clone().max_redirects(max as u32);
        this
    }

    /// 设置请求 ID（用于批量请求时的结果关联）
    ///
    /// # PHP 签名
    /// public XHRequest::setId(string $id): $this
    #[php_method]
    pub fn set_id(
        #[this] this: &mut ZendClassObject<Self>,
        id: String,
    ) -> &mut ZendClassObject<Self> {
        this.request = this.request.clone().id(id);
        this
    }

    /// 设置请求优先级（线程池模式下生效）
    /// 数值越大优先级越高
    ///
    /// # PHP 签名
    /// public XHRequest::setPriority(int $priority): $this
    #[php_method]
    pub fn set_priority(
        #[this] this: &mut ZendClassObject<Self>,
        priority: i64,
    ) -> &mut ZendClassObject<Self> {
        this.request = this.request.clone().priority(priority as i32);
        this
    }

    /// 获取请求 URL
    #[php_method]
    pub fn get_url(&self) -> String {
        self.request.get_url().to_string()
    }

    /// 获取 HTTP 方法
    #[php_method]
    pub fn get_method(&self) -> String {
        self.request.get_method().to_string()
    }

    /// 获取请求 ID
    #[php_method]
    pub fn get_id(&self) -> Option<String> {
        self.request.get_id().map(|s| s.to_string())
    }
}

// +----------------------------------------------------------------------+
// | PHP 类：XHResponse（响应对象）                                        |
// +----------------------------------------------------------------------+

/// PHP XHResponse 类的 Rust 表示
#[php_class(name = "XHResponse")]
pub struct PhpXhResponse {
    /// 内部响应对象
    response: Option<XhResponse>,
}

/// PHP XHResponse 类的方法实现
#[php_impl]
impl PhpXhResponse {
    /// 获取状态码
    #[php_method]
    pub fn status(&self) -> i64 {
        self.response
            .as_ref()
            .map(|r| r.status() as i64)
            .unwrap_or(0)
    }

    /// 检查是否成功（2xx 状态码）
    #[php_method]
    pub fn is_success(&self) -> bool {
        self.response
            .as_ref()
            .map(|r| r.is_success())
            .unwrap_or(false)
    }

    /// 获取指定响应头
    #[php_method]
    pub fn header(&self, name: String) -> Option<String> {
        self.response.as_ref().and_then(|r| r.header(&name))
    }

    /// 获取所有响应头
    #[php_method]
    pub fn headers(&self) -> ZBox<ZendHashTable> {
        let mut ht = ZendHashTable::new();
        if let Some(response) = &self.response {
            for (name, value) in response.headers().all() {
                let _ = ht.insert(&name, value);
            }
        }
        ht
    }

    /// 获取响应体（字符串）
    #[php_method]
    pub fn body(&self) -> Result<String, String> {
        self.response
            .as_ref()
            .ok_or("响应不存在".to_string())?
            .body_text()
            .map_err(|e| e.to_string())
    }

    /// 获取响应体（JSON 解析为数组）
    #[php_method]
    pub fn json(&self) -> Result<ZBox<ZendHashTable>, String> {
        let response = self.response.as_ref().ok_or("响应不存在".to_string())?;
        let json = response.body_json().map_err(|e| e.to_string())?;
        json_to_php_array(&json)
    }

    /// 获取响应体大小（字节）
    #[php_method]
    pub fn body_size(&self) -> i64 {
        self.response
            .as_ref()
            .map(|r| r.body_size() as i64)
            .unwrap_or(0)
    }

    /// 获取最终 URL（可能因重定向而与请求 URL 不同）
    #[php_method]
    pub fn url(&self) -> String {
        self.response
            .as_ref()
            .map(|r| r.url().to_string())
            .unwrap_or_default()
    }

    /// 获取请求耗时（毫秒）
    #[php_method]
    pub fn elapsed_ms(&self) -> i64 {
        self.response
            .as_ref()
            .map(|r| r.elapsed().as_millis() as i64)
            .unwrap_or(0)
    }

    /// 获取错误信息
    #[php_method]
    pub fn error(&self) -> Option<String> {
        self.response
            .as_ref()
            .and_then(|r| r.error().map(|s| s.to_string()))
    }

    /// 获取远程服务器地址（IP:Port）
    #[php_method]
    pub fn remote_addr(&self) -> Option<String> {
        self.response
            .as_ref()
            .and_then(|r| r.remote_addr().map(|s| s.to_string()))
    }

    /// 获取 HTTP 协议版本
    #[php_method]
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
#[php_class(name = "XHMulti")]
pub struct PhpXhMulti {
    /// 内部批量执行器（延迟创建）
    multi: Option<XhMulti>,

    /// 待执行的请求列表
    requests: Vec<XhRequest>,

    /// 最大并发数（0 = 无限制）
    max_concurrency: usize,
}

/// PHP XHMulti 类的方法实现
#[php_impl]
impl PhpXhMulti {
    /// 构造函数
    #[php_method]
    pub fn __construct() -> Self {
        Self {
            multi: None,
            requests: Vec::new(),
            max_concurrency: 0,
        }
    }

    /// 添加请求到批量执行器
    /// 带数量上限检查，防止内存溢出
    ///
    /// # PHP 签名
    /// public XHMulti::add(XHRequest $request): $this
    #[php_method]
    pub fn add<'a>(
        #[this] this: &'a mut ZendClassObject<Self>,
        request: &ZendClassObject<PhpXhRequest>,
    ) -> Result<&'a mut ZendClassObject<Self>, String> {
        // 检查请求数量是否超过上限
        if this.requests.len() >= MAX_REQUESTS_PER_BATCH {
            return Err(format!(
                "批量请求数量超过上限 {}，请分批执行",
                MAX_REQUESTS_PER_BATCH
            ));
        }
        // 通过 Deref 访问 PhpXhRequest.request，克隆后添加
        this.requests.push(request.request.clone());
        Ok(this)
    }

    /// 设置最大并发数
    ///
    /// # PHP 签名
    /// public XHMulti::maxConcurrency(int $max): $this
    #[php_method]
    pub fn max_concurrency(
        #[this] this: &mut ZendClassObject<Self>,
        max: i64,
    ) -> &mut ZendClassObject<Self> {
        this.max_concurrency = max as usize;
        this
    }

    /// 执行所有请求
    ///
    /// # PHP 签名
    /// public XHMulti::execute(): array
    #[php_method]
    pub fn execute(&mut self) -> Result<ZBox<ZendHashTable>, String> {
        // 创建单线程运行时（FPM 安全）
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| format!("创建运行时失败: {}", e))?;

        // 阻塞执行异步任务
        let results = runtime
            .block_on(async {
                let client = XhCurlManager::global()
                    .create_client()
                    .map_err(|e| e.to_string())?;
                let mut multi = XhMulti::new(client);

                if self.max_concurrency > 0 {
                    multi = multi.max_concurrency(self.max_concurrency);
                }

                let requests = std::mem::take(&mut self.requests);
                multi.add_many(requests);

                // 统一错误类型为 String
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
#[php_class(name = "XHThreadPool")]
pub struct PhpXhThreadPool {
    /// 内部线程池（延迟创建）
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
    #[php_method]
    pub fn __construct(workers: Option<i64>) -> Self {
        Self {
            pool: None,
            requests: Vec::new(),
            max_concurrency: workers.unwrap_or(0) as usize,
        }
    }

    /// 添加请求到线程池
    /// 带数量上限检查，防止内存溢出
    #[php_method]
    pub fn add<'a>(
        #[this] this: &'a mut ZendClassObject<Self>,
        request: &ZendClassObject<PhpXhRequest>,
    ) -> Result<&'a mut ZendClassObject<Self>, String> {
        if this.requests.len() >= MAX_REQUESTS_PER_BATCH {
            return Err(format!(
                "批量请求数量超过上限 {}，请分批执行",
                MAX_REQUESTS_PER_BATCH
            ));
        }
        this.requests.push(request.request.clone());
        Ok(this)
    }

    /// 执行所有请求
    ///
    /// # PHP 签名
    /// public XHThreadPool::execute(): array
    #[php_method]
    pub fn execute(&mut self) -> Result<ZBox<ZendHashTable>, String> {
        // 安全检查：线程池仅在 CLI 模式下可用
        // FPM 模式下多线程会与 PHP 内存管理器冲突
        if !XhCurlManager::is_cli_mode() {
            return Err("XHThreadPool 仅在 CLI 模式下可用".to_string());
        }

        // 创建多线程运行时（真正的并行执行）
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(|e| format!("创建运行时失败: {}", e))?;

        // 阻塞执行
        let results = runtime
            .block_on(async {
                let client = XhCurlManager::global()
                    .create_client()
                    .map_err(|e| e.to_string())?;

                let mut config = ThreadPoolConfig::default();
                if self.max_concurrency > 0 {
                    config.worker_count = self.max_concurrency;
                }

                let mut pool = XhThreadPool::new(config, client);
                let requests = std::mem::take(&mut self.requests);

                pool.execute_all(requests).await.map_err(|e| e.to_string())
            })
            .map_err(|e| e.to_string())?;

        // 转换为 PHP 数组
        results_to_php_array(&results)
    }
}

// +----------------------------------------------------------------------+
// | 辅助函数                                                              |
// +----------------------------------------------------------------------+

/// 将请求结果列表转换为 PHP 数组
/// 用于 XHMulti::execute() 和 XHThreadPool::execute() 的返回值
fn results_to_php_array(results: &[crate::multi::RequestResult]) -> Result<ZBox<ZendHashTable>, String> {
    let mut ht = ZendHashTable::new();
    for (i, result) in results.iter().enumerate() {
        let mut response_ht = ZendHashTable::new();
        let _ = response_ht.insert("id", result.id.clone());
        let _ = response_ht.insert("success", result.is_success());
        let _ = response_ht.insert("elapsed_ms", result.elapsed.as_millis() as i64);

        if let Some(err) = &result.error {
            let _ = response_ht.insert("error", err.clone());
        }

        if let Some(resp) = &result.response {
            let _ = response_ht.insert("status", resp.status() as i64);
            let _ = response_ht.insert("body_size", resp.body_size() as i64);
            if let Ok(body) = resp.body_text() {
                let _ = response_ht.insert("body", body);
            }
            let _ = response_ht.insert("url", resp.url().to_string());
        }

        let _ = ht.insert(&i.to_string(), response_ht);
    }
    Ok(ht)
}

/// 将 PHP 数组转换为 JSON 字符串
/// 递归处理嵌套数组
fn php_array_to_json(ht: &ZendHashTable) -> Result<String, String> {
    let mut map = serde_json::Map::new();

    // 遍历 PHP 数组（iter 产出 (ArrayKey, &Zval)）
    for (key, val) in ht.iter() {
        let key_str = key.to_string();
        let json_val = zval_to_json(val)?;
        map.insert(key_str, json_val);
    }

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
        for (key, v) in ht.iter() {
            map.insert(key.to_string(), zval_to_json(v)?);
        }
        Ok(serde_json::Value::Object(map))
    } else {
        Ok(serde_json::Value::Null)
    }
}

/// 将 PHP 数组转换为表单键值对
/// 用于 application/x-www-form-urlencoded 请求
fn php_array_to_form(ht: &ZendHashTable) -> Vec<(String, String)> {
    let mut form = Vec::new();

    for (key, val) in ht.iter() {
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
            continue;
        };
        form.push((key_str, val_str));
    }

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
                        if let Some(s) = item.as_str() {
                            let _ = arr_ht.insert(&i.to_string(), s);
                        } else if let Some(n) = item.as_i64() {
                            let _ = arr_ht.insert(&i.to_string(), n);
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
// | 宏会自动收集前面所有 #[php_class] 标注的类进行注册                    |
// | 不需要手动调用 .class::<>() 方法                                       |
// +----------------------------------------------------------------------+

#[php_module]
pub fn get_module(module: ModuleBuilder) -> ModuleBuilder {
    // 类注册由 #[php_class] 宏自动完成
    module
}
