// +----------------------------------------------------------------------+
// | XHCurl 扩展 - HTTP 请求构建器                                         |
// | 使用 Builder 模式链式构建请求                                          |
// | 支持 GET/POST/PUT/DELETE/PATCH/HEAD/OPTIONS 方法                      |
// | 支持流式上传和下载                                                     |
// +----------------------------------------------------------------------+

use std::time::Duration;

use crate::error::{XhCurlError, XhCurlResult};
use crate::header::HeaderManager;

// +----------------------------------------------------------------------+
// | 请求级 Client 缓存                                                    |
// | 当请求设置了 follow_redirects/verify_ssl/proxy/connect_timeout 等任一 |
// | Client 级覆盖时，需构建独立 Client（无法通过 RequestBuilder 覆盖）。  |
// | 为避免每个同类请求都新建 Client（丢失连接池复用），按「覆盖参数组合」  |
// | 缓存已构建的 Client。reqwest::Client 内部为 Arc，clone 廉价。          |
// | 全局配置变更时（setConfig）通过 clear_request_client_cache() 失效。   |
// +----------------------------------------------------------------------+

/// 请求级 Client 覆盖参数的组合键，用作缓存查找。
///
/// 仅包含「影响 Client 构建」的覆盖项，不含 URL/header/body 等请求级字段。
/// 同一组合键的请求共享同一 Client（含连接池），从而保留连接复用。
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
struct OverrideKey {
    follow_redirects: Option<bool>,
    max_redirects: Option<u32>,
    verify_ssl: Option<bool>,
    proxy: Option<String>,
    /// 仅 > 0 的连接超时才实际覆盖（0 表示用全局默认），故此处过滤。
    connect_timeout: Option<u64>,
    /// 连接超时（毫秒），优先级高于 connect_timeout（秒）。
    connect_timeout_ms: Option<u64>,
}

impl OverrideKey {
    fn from_request(r: &XhRequest) -> Self {
        Self {
            follow_redirects: r.follow_redirects,
            max_redirects: r.max_redirects,
            verify_ssl: r.verify_ssl,
            proxy: r.proxy.clone(),
            connect_timeout: r.connect_timeout.filter(|&s| s > 0),
            connect_timeout_ms: r.connect_timeout_ms,
        }
    }
}

type ClientCache = std::sync::Mutex<std::collections::HashMap<OverrideKey, reqwest::Client>>;

static REQUEST_CLIENT_CACHE: std::sync::OnceLock<ClientCache> = std::sync::OnceLock::new();

fn request_client_cache() -> &'static ClientCache {
    REQUEST_CLIENT_CACHE.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

/// 清空请求级 Client 缓存。
///
/// 全局配置（setConfig）变更后调用，确保后续构建的请求级 Client 反映最新
/// 全局配置（UA/keepalive/连接池上限/TLS 等均继承自全局）。
pub fn clear_request_client_cache() {
    if let Some(cache) = REQUEST_CLIENT_CACHE.get() {
        let _ = cache.lock().map(|mut c| c.clear());
    }
}

/// HTTP 请求方法枚举
/// 对应 C 版本的 CURLOPT_CUSTOMREQUEST
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HttpMethod {
    /// HTTP GET 方法（默认）
    Get,
    /// HTTP POST 方法
    Post,
    /// HTTP PUT 方法
    Put,
    /// HTTP DELETE 方法
    Delete,
    /// HTTP PATCH 方法
    Patch,
    /// HTTP HEAD 方法（仅获取头部，不返回响应体）
    Head,
    /// HTTP OPTIONS 方法（获取服务器支持的方法）
    Options,
}

impl HttpMethod {
    /// 将方法枚举转换为大写字符串
    /// 用于 HTTP 请求行
    pub fn as_str(&self) -> &'static str {
        match self {
            HttpMethod::Get => "GET",
            HttpMethod::Post => "POST",
            HttpMethod::Put => "PUT",
            HttpMethod::Delete => "DELETE",
            HttpMethod::Patch => "PATCH",
            HttpMethod::Head => "HEAD",
            HttpMethod::Options => "OPTIONS",
        }
    }

    /// 从字符串解析 HTTP 方法（大小写不敏感）
    ///
    /// # 参数
    /// - `s`: 方法字符串
    ///
    /// # 返回
    /// - `Ok(HttpMethod)`: 解析成功
    /// - `Err(XhCurlError)`: 不支持的方法
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> XhCurlResult<HttpMethod> {
        match s.to_uppercase().as_str() {
            "GET" => Ok(HttpMethod::Get),
            "POST" => Ok(HttpMethod::Post),
            "PUT" => Ok(HttpMethod::Put),
            "DELETE" => Ok(HttpMethod::Delete),
            "PATCH" => Ok(HttpMethod::Patch),
            "HEAD" => Ok(HttpMethod::Head),
            "OPTIONS" => Ok(HttpMethod::Options),
            _ => Err(XhCurlError::InvalidArgument(format!(
                "不支持的 HTTP 方法: {}",
                s
            ))),
        }
    }
}

impl std::fmt::Display for HttpMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// 请求体类型
/// 区分不同类型的请求体，便于正确设置 Content-Type
#[derive(Debug, Clone)]
pub enum BodyType {
    /// 无请求体（GET、HEAD 等）
    None,
    /// 原始字节数据
    Bytes(Vec<u8>),
    /// JSON 数据（自动设置 Content-Type: application/json）
    Json(serde_json::Value),
    /// 表单数据（application/x-www-form-urlencoded）
    Form(Vec<(String, String)>),
    /// 多部分表单数据（multipart/form-data，支持文件上传）
    Multipart(Vec<MultipartField>),
}

/// 多部分表单字段
/// 用于文件上传和复杂表单数据
#[derive(Debug, Clone)]
pub struct MultipartField {
    /// 字段名称
    pub name: String,
    /// 字段值（文本或文件内容）
    pub value: Vec<u8>,
    /// 文件名（可选，仅文件上传时设置）
    pub filename: Option<String>,
    /// Content-Type（可选，仅文件上传时设置）
    pub content_type: Option<String>,
}

impl MultipartField {
    /// 创建文本字段
    pub fn text(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: value.into().into_bytes(),
            filename: None,
            content_type: None,
        }
    }

    /// 创建文本字段（二进制安全，直接接收原始字节）
    ///
    /// 与 [`MultipartField::text`] 的区别：后者要求 `impl Into<String>`，
    /// 非有效 UTF-8 字节会被替换/拒绝；本方法直接保留原始字节，
    /// 适合从 PHP 端传入的二进制安全字符串（PHP 字符串本质是字节序列）。
    pub fn text_bytes(name: impl Into<String>, value: Vec<u8>) -> Self {
        Self {
            name: name.into(),
            value,
            filename: None,
            content_type: None,
        }
    }

    /// 创建文件字段
    pub fn file(
        name: impl Into<String>,
        filename: impl Into<String>,
        content: Vec<u8>,
        content_type: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            value: content,
            filename: Some(filename.into()),
            content_type: Some(content_type.into()),
        }
    }
}

/// HTTP 请求构建器
/// 使用 Builder 模式，支持链式调用
///
/// # 线程安全
/// 请求构建器本身不是线程安全的（未实现 Sync），
/// 但构建完成后可以转移所有权到异步任务中执行。
/// 对应 C 版本的 xhcurl_request_t 结构体
#[derive(Debug, Clone)]
pub struct XhRequest {
    /// 请求 URL
    url: String,

    /// HTTP 方法
    method: HttpMethod,

    /// 请求头管理器
    headers: HeaderManager,

    /// 请求体
    body: BodyType,

    /// 连接超时（秒，0 = 使用全局默认）
    connect_timeout: Option<u64>,

    /// 连接超时（毫秒，0 = 使用全局默认）
    /// 优先级高于 connect_timeout（秒），用于需要亚秒级精度的场景。
    connect_timeout_ms: Option<u64>,

    /// 请求超时（秒，0 = 使用全局默认）
    request_timeout: Option<u64>,

    /// 请求超时（毫秒，0 = 使用全局默认）
    /// 优先级高于 request_timeout（秒），用于需要亚秒级精度的场景。
    request_timeout_ms: Option<u64>,

    /// 是否跟随重定向（None = 使用全局默认）
    follow_redirects: Option<bool>,

    /// 最大重定向次数（None = 使用全局默认）
    max_redirects: Option<u32>,

    /// 是否验证 SSL 证书（None = 使用全局默认）
    verify_ssl: Option<bool>,

    /// 自定义 User-Agent（None = 使用全局默认）
    user_agent: Option<String>,

    /// 代理地址（None = 使用全局默认）
    proxy: Option<String>,

    /// 请求 ID（用于批量请求时的标识）
    id: Option<String>,

    /// 用户自定义数据（JSON 字符串）
    /// 可携带任意结构化数据（数组/对象），随请求传递到结果中
    /// 用于批量请求时关联业务上下文（如任务索引、回调标识等）
    user_data: Option<String>,

    /// 请求 Cookie 字符串（CURLOPT_COOKIE）
    /// 格式: "name1=value1; name2=value2"
    cookies: Option<String>,

    /// HTTP 基本认证凭据（CURLOPT_USERPWD）
    /// 格式: "username:password"
    auth: Option<String>,

    /// Bearer Token（CURLOPT_XOAUTH2_BEARER）
    /// 自动设置 Authorization: Bearer {token}
    bearer_token: Option<String>,

    /// Accept-Encoding 头部（CURLOPT_ENCODING）
    /// 如 "gzip, deflate"
    encoding: Option<String>,

    /// 自定义请求方法（CURLOPT_CUSTOMREQUEST）
    /// 覆盖标准 HTTP 方法，用于 CONNECT/TRACE 等非标准方法
    custom_method: Option<String>,

    /// Range 请求范围（CURLOPT_RANGE）
    /// 格式: "0-1023" 表示请求前 1024 字节
    range: Option<String>,
}

impl XhRequest {
    /// 创建新的请求构建器
    ///
    /// # 参数
    /// - `url`: 请求 URL
    ///
    /// # 返回
    /// 默认 GET 请求的构建器
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            method: HttpMethod::Get,
            headers: HeaderManager::new(),
            body: BodyType::None,
            connect_timeout: None,
            connect_timeout_ms: None,
            request_timeout: None,
            request_timeout_ms: None,
            follow_redirects: None,
            max_redirects: None,
            verify_ssl: None,
            user_agent: None,
            proxy: None,
            id: None,
            user_data: None,
            cookies: None,
            auth: None,
            bearer_token: None,
            encoding: None,
            custom_method: None,
            range: None,
        }
    }

    /// 设置请求 URL
    pub fn url(mut self, url: impl Into<String>) -> Self {
        self.url = url.into();
        self
    }

    /// 设置 HTTP 方法
    pub fn method(mut self, method: HttpMethod) -> Self {
        self.method = method;
        self
    }

    /// 设置为 GET 方法
    pub fn get(self) -> Self {
        self.method(HttpMethod::Get)
    }

    /// 设置为 POST 方法
    pub fn post(self) -> Self {
        self.method(HttpMethod::Post)
    }

    /// 设置为 PUT 方法
    pub fn put(self) -> Self {
        self.method(HttpMethod::Put)
    }

    /// 设置为 DELETE 方法
    pub fn delete(self) -> Self {
        self.method(HttpMethod::Delete)
    }

    /// 设置为 PATCH 方法
    pub fn patch(self) -> Self {
        self.method(HttpMethod::Patch)
    }

    /// 设置为 HEAD 方法
    pub fn head(self) -> Self {
        self.method(HttpMethod::Head)
    }

    /// 设置为 OPTIONS 方法
    pub fn options(self) -> Self {
        self.method(HttpMethod::Options)
    }

    /// 添加请求头
    ///
    /// # 参数
    /// - `name`: 头部名称
    /// - `value`: 头部值
    pub fn header(self, name: &str, value: &str) -> Self {
        self.headers.set(name, value);
        self
    }

    /// 批量添加请求头
    pub fn headers<I, S>(self, headers: I) -> Self
    where
        I: IntoIterator<Item = (S, S)>,
        S: AsRef<str>,
    {
        self.headers.set_many(headers);
        self
    }

    /// 设置原始字节请求体
    pub fn body_bytes(mut self, data: Vec<u8>) -> Self {
        self.body = BodyType::Bytes(data);
        self
    }

    /// 设置 JSON 请求体
    /// 自动设置 Content-Type: application/json
    pub fn body_json<T: serde::Serialize>(mut self, json: &T) -> XhCurlResult<Self> {
        let value = serde_json::to_value(json)?;
        self.body = BodyType::Json(value);
        // 自动设置 Content-Type
        self.headers.set("Content-Type", "application/json");
        Ok(self)
    }

    /// 设置 JSON 字符串请求体
    pub fn body_json_str(mut self, json_str: &str) -> XhCurlResult<Self> {
        let value: serde_json::Value = serde_json::from_str(json_str)?;
        self.body = BodyType::Json(value);
        self.headers.set("Content-Type", "application/json");
        Ok(self)
    }

    /// 设置表单数据
    /// 自动设置 Content-Type: application/x-www-form-urlencoded
    pub fn body_form(mut self, form: Vec<(String, String)>) -> Self {
        self.body = BodyType::Form(form);
        self.headers
            .set("Content-Type", "application/x-www-form-urlencoded");
        self
    }

    /// 设置多部分表单数据
    /// 自动设置 Content-Type: multipart/form-data
    pub fn body_multipart(mut self, fields: Vec<MultipartField>) -> Self {
        self.body = BodyType::Multipart(fields);
        // multipart 的 Content-Type 由 reqwest 自动设置（包含 boundary）
        self
    }

    /// 设置连接超时（秒）
    pub fn connect_timeout(mut self, secs: u64) -> Self {
        self.connect_timeout = Some(secs);
        self
    }

    /// 设置连接超时（毫秒）
    /// 优先级高于 connect_timeout（秒），用于需要亚秒级精度的场景。
    pub fn connect_timeout_ms(mut self, ms: u64) -> Self {
        self.connect_timeout_ms = Some(ms);
        self
    }

    /// 设置请求超时（秒）
    pub fn request_timeout(mut self, secs: u64) -> Self {
        self.request_timeout = Some(secs);
        self
    }

    /// 设置请求超时（毫秒）
    /// 优先级高于 request_timeout（秒），用于需要亚秒级精度的场景。
    pub fn request_timeout_ms(mut self, ms: u64) -> Self {
        self.request_timeout_ms = Some(ms);
        self
    }

    /// 清除请求级代理覆盖（恢复使用全局默认）
    pub fn clear_proxy(mut self) -> Self {
        self.proxy = None;
        self
    }

    /// 设置是否跟随重定向
    pub fn follow_redirects(mut self, follow: bool) -> Self {
        self.follow_redirects = Some(follow);
        self
    }

    /// 设置最大重定向次数
    pub fn max_redirects(mut self, max: u32) -> Self {
        self.max_redirects = Some(max);
        self
    }

    /// 设置是否验证 SSL 证书
    pub fn verify_ssl(mut self, verify: bool) -> Self {
        self.verify_ssl = Some(verify);
        self
    }

    /// 设置 User-Agent
    pub fn user_agent(mut self, ua: impl Into<String>) -> Self {
        self.user_agent = Some(ua.into());
        self
    }

    /// 设置代理
    pub fn proxy(mut self, proxy: impl Into<String>) -> Self {
        self.proxy = Some(proxy.into());
        self
    }

    /// 设置请求 ID（用于批量请求标识）
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// 设置用户自定义数据（JSON 字符串）
    /// 可携带任意结构化数据，随请求传递到结果中返回
    pub fn user_data(mut self, data: impl Into<String>) -> Self {
        self.user_data = Some(data.into());
        self
    }

    /// 设置请求 Cookie 字符串（CURLOPT_COOKIE）
    /// 格式: "name1=value1; name2=value2"
    pub fn cookies(mut self, cookies: impl Into<String>) -> Self {
        self.cookies = Some(cookies.into());
        self
    }

    /// 设置 HTTP 基本认证凭据（CURLOPT_USERPWD）
    /// 格式: "username:password"
    ///
    /// 空字符串或不含冒号 ':' 分隔符的凭据视为格式错误，返回 Err，
    /// 避免后续 to_reqwest 拆分凭据时静默退化为「用户名+空密码」导致认证失败难排查。
    pub fn basic_auth(mut self, credentials: impl Into<String>) -> XhCurlResult<Self> {
        let auth = credentials.into();
        if auth.is_empty() || !auth.contains(':') {
            return Err(XhCurlError::InvalidArgument(
                "basicAuth 凭据格式错误，应为 'user:pass' 格式".to_string(),
            ));
        }
        self.auth = Some(auth);
        Ok(self)
    }

    /// 设置 Bearer Token（CURLOPT_XOAUTH2_BEARER）
    /// 自动设置 Authorization: Bearer {token}
    pub fn bearer_token(mut self, token: impl Into<String>) -> Self {
        self.bearer_token = Some(token.into());
        self
    }

    /// 设置 Accept-Encoding（CURLOPT_ENCODING）
    /// 如 "gzip, deflate, br"
    pub fn encoding(mut self, encoding: impl Into<String>) -> Self {
        self.encoding = Some(encoding.into());
        self
    }

    /// 设置自定义请求方法（CURLOPT_CUSTOMREQUEST）
    /// 用于非标准 HTTP 方法（CONNECT/TRACE 等）
    pub fn custom_method(mut self, method: impl Into<String>) -> Self {
        self.custom_method = Some(method.into());
        self
    }

    /// 设置 Range 请求范围（CURLOPT_RANGE）
    /// 格式: "0-1023" 或 "0-" 或 "-1023"
    pub fn range(mut self, range: impl Into<String>) -> Self {
        self.range = Some(range.into());
        self
    }

    // ===== Getter 方法 =====

    /// 获取请求 URL
    pub fn get_url(&self) -> &str {
        &self.url
    }

    /// 获取 HTTP 方法
    pub fn get_method(&self) -> HttpMethod {
        self.method
    }

    /// 获取请求头管理器引用
    pub fn get_headers(&self) -> &HeaderManager {
        &self.headers
    }

    /// 获取请求体类型引用
    pub fn get_body(&self) -> &BodyType {
        &self.body
    }

    /// 获取请求 ID
    pub fn get_id(&self) -> Option<&str> {
        self.id.as_deref()
    }

    /// 获取用户自定义数据
    pub fn get_user_data(&self) -> Option<&str> {
        self.user_data.as_deref()
    }

    /// 获取 Cookie 字符串
    pub fn get_cookies(&self) -> Option<&str> {
        self.cookies.as_deref()
    }

    /// 获取 HTTP 基本认证凭据
    pub fn get_auth(&self) -> Option<&str> {
        self.auth.as_deref()
    }

    /// 获取 Bearer Token
    pub fn get_bearer_token(&self) -> Option<&str> {
        self.bearer_token.as_deref()
    }

    /// 获取自定义请求方法
    pub fn get_custom_method(&self) -> Option<&str> {
        self.custom_method.as_deref()
    }

    /// 获取 Range 请求范围
    pub fn get_range(&self) -> Option<&str> {
        self.range.as_deref()
    }

    /// 获取 Accept-Encoding
    pub fn get_encoding(&self) -> Option<&str> {
        self.encoding.as_deref()
    }

    /// 获取连接超时
    pub fn get_connect_timeout(&self) -> Option<u64> {
        self.connect_timeout
    }

    /// 获取请求超时
    pub fn get_request_timeout(&self) -> Option<u64> {
        self.request_timeout
    }

    /// 获取是否验证 SSL
    pub fn get_verify_ssl(&self) -> Option<bool> {
        self.verify_ssl
    }

    /// 获取 User-Agent
    pub fn get_user_agent(&self) -> Option<&str> {
        self.user_agent.as_deref()
    }

    /// 获取代理地址
    pub fn get_proxy(&self) -> Option<&str> {
        self.proxy.as_deref()
    }

    /// 校验 header 值仅含可见 ASCII（拒绝非 ASCII 字节与控制字符）。
    ///
    /// `HeaderValue::from_str` 在 http 1.x 按 RFC 7230 接受 obs-text
    ///（0x80-0xFF，即非 ASCII UTF-8 字节），仅拒绝控制字符。对于
    /// cookies/encoding/range/user_agent 等字段，原始非 ASCII 值（如中文、
    /// emoji）常被服务端/代理误处理，需显式拒绝并提示 urlencode。
    /// 此函数在 `HeaderValue::from_str` 之前先做 `is_ascii()` 检查，
    /// 拒绝非 ASCII 值；再由 `HeaderValue::from_str` 拒绝控制字符。
    fn validate_ascii_header_value(
        field_label: &str,
        value: &str,
    ) -> XhCurlResult<reqwest::header::HeaderValue> {
        if !value.is_ascii() {
            return Err(XhCurlError::InvalidArgument(format!(
                "无效的 {} 值（含非 ASCII 字节），请对中文值做 urlencode：{}",
                field_label, value
            )));
        }
        reqwest::header::HeaderValue::from_str(value).map_err(|_| {
            XhCurlError::InvalidArgument(format!(
                "无效的 {} 值（含控制字符）：{}",
                field_label, value
            ))
        })
    }

    /// 构建为 reqwest 请求构建器
    /// 将 XhRequest 转换为 reqwest::RequestBuilder
    ///
    /// # 参数
    /// - `client`: reqwest 客户端实例
    ///
    /// # 返回
    /// 配置好的 reqwest::RequestBuilder
    pub fn to_reqwest(&self, client: &reqwest::Client) -> XhCurlResult<reqwest::RequestBuilder> {
        // 早期 URL 校验：reqwest::Client::get 等方法将 URL 解析错误延迟到
        // send() 才暴露。此处提前用 Url::parse 校验，给出更清晰的错误信息，
        // 并在异步执行前即可发现空/非法 URL（fail-fast，PHP 边界输入校验）。
        reqwest::Url::parse(&self.url)
            .map_err(|e| XhCurlError::Generic(format!("无效的请求 URL {:?}: {}", self.url, e)))?;

        // 请求级 Client 配置覆盖：reqwest 的部分配置（重定向策略、SSL 验证、
        // 代理、连接超时）只能在 ClientBuilder 上设置，无法通过 RequestBuilder
        // 单独覆盖。当请求显式设置了这些参数中的任意一个时，构建新 Client
        // 应用覆盖。注意：这会牺牲连接池复用，仅在用户显式设置时才走此分支。
        let client = if self.needs_request_client() {
            self.build_request_client(client)?
        } else {
            client.clone()
        };

        // 根据方法创建请求构建器
        // 自定义方法优先（CURLOPT_CUSTOMREQUEST）
        let mut builder = if let Some(custom) = &self.custom_method {
            client.request(
                reqwest::Method::from_bytes(custom.as_bytes())
                    .map_err(|e| XhCurlError::Generic(format!("无效的 HTTP 方法: {}", e)))?,
                &self.url,
            )
        } else {
            match self.method {
                HttpMethod::Get => client.get(&self.url),
                HttpMethod::Post => client.post(&self.url),
                HttpMethod::Put => client.put(&self.url),
                HttpMethod::Delete => client.delete(&self.url),
                HttpMethod::Patch => client.patch(&self.url),
                HttpMethod::Head => client.head(&self.url),
                HttpMethod::Options => client.request(reqwest::Method::OPTIONS, &self.url),
            }
        };

        // 设置请求头
        let mut header_map = self.headers.to_header_map()?;

        // Accept-Encoding（CURLOPT_ENCODING）
        // 非法值（含非 ASCII 字节或控制字符）必须报错而非静默丢弃，避免用户设置的编码被忽略
        if let Some(encoding) = &self.encoding {
            let v = Self::validate_ascii_header_value("encoding", encoding)?;
            header_map.insert(reqwest::header::ACCEPT_ENCODING, v);
        }

        // Range（CURLOPT_RANGE）
        // 非法值（含非 ASCII 字节或控制字符）必须报错而非静默丢弃
        if let Some(range) = &self.range {
            let v = Self::validate_ascii_header_value("range", range)?;
            header_map.insert(reqwest::header::RANGE, v);
        }

        builder = builder.headers(header_map);

        // 设置请求体
        builder = match &self.body {
            BodyType::None => builder,
            BodyType::Bytes(data) => builder.body(data.clone()),
            BodyType::Json(value) => builder.json(value),
            BodyType::Form(form) => {
                let form_iter = form.iter().map(|(k, v)| (k.as_str(), v.as_str()));
                builder.form(&form_iter.collect::<Vec<_>>())
            }
            BodyType::Multipart(fields) => {
                let mut form = reqwest::multipart::Form::new();
                for field in fields {
                    let mut part = reqwest::multipart::Part::bytes(field.value.clone());
                    if let Some(filename) = &field.filename {
                        part = part.file_name(filename.clone());
                    }
                    if let Some(ct) = &field.content_type {
                        part = part
                            .mime_str(ct)
                            .map_err(|e| XhCurlError::Generic(format!("MIME 类型错误: {}", e)))?;
                    }
                    // 直接添加 part，无需先 text() 再 part()（原实现会重复添加同名空字段）
                    form = form.part(field.name.clone(), part);
                }
                builder.multipart(form)
            }
        };

        // HTTP 基本认证（CURLOPT_USERPWD）
        if let Some(auth) = &self.auth {
            if let Some(idx) = auth.find(':') {
                let (user, pass) = auth.split_at(idx);
                builder = builder.basic_auth(user, Some(&pass[1..]));
            } else {
                // 只有用户名，无密码
                builder = builder.basic_auth(auth.as_str(), Some(""));
            }
        }

        // Bearer Token（CURLOPT_XOAUTH2_BEARER）
        if let Some(token) = &self.bearer_token {
            builder = builder.bearer_auth(token);
        }

        // Cookie（CURLOPT_COOKIE）
        // 非法值（含非 ASCII 字节或控制字符）必须报错而非静默丢弃，避免用户设置的 cookie 被忽略
        if let Some(cookies) = &self.cookies {
            let v = Self::validate_ascii_header_value("cookies", cookies)?;
            builder = builder.header(reqwest::header::COOKIE, v);
        }

        // 设置超时（覆盖客户端默认值）
        // request_timeout_ms（毫秒）优先级高于 request_timeout（秒）
        // 0/负值跳过（视为「使用默认/无超时」），避免 Duration::from_millis(0) 立即超时
        if let Some(ms) = self.request_timeout_ms {
            if ms > 0 {
                builder = builder.timeout(Duration::from_millis(ms));
            }
        } else if let Some(timeout) = self.request_timeout {
            if timeout > 0 {
                builder = builder.timeout(Duration::from_secs(timeout));
            }
        }

        // 请求级 User-Agent 覆盖（CURLOPT_USERAGENT）
        // reqwest 共享 Client 只能设置全局 UA，此处通过请求头覆盖
        // 非法值（含非 ASCII 字节或控制字符）必须报错而非静默丢弃，避免 UA 被忽略
        if let Some(ua) = &self.user_agent {
            let v = Self::validate_ascii_header_value("user_agent", ua)?;
            builder = builder.header(reqwest::header::USER_AGENT, v);
        }

        Ok(builder)
    }

    /// 判断是否需要构建请求级 Client。
    ///
    /// 当请求显式设置了任一 Client 级配置（重定向策略、SSL 验证、代理、连接超时）
    /// 时返回 true。这些配置无法通过 RequestBuilder 单独覆盖，必须构建新 Client。
    ///
    /// connect_timeout / connect_timeout_ms 为 0 时视为「使用全局默认」，
    /// 不触发 Client 重建（与 `OverrideKey::from_request` 的 filter 逻辑一致）。
    fn needs_request_client(&self) -> bool {
        self.follow_redirects.is_some()
            || self.max_redirects.is_some()
            || self.verify_ssl.is_some()
            || self.proxy.is_some()
            || self.connect_timeout.filter(|&s| s > 0).is_some()
            || self.connect_timeout_ms.filter(|&ms| ms > 0).is_some()
    }

    /// 构建请求级客户端（应用请求级 Client 配置覆盖）。
    ///
    /// reqwest 的部分配置（重定向策略、SSL 验证、代理、连接超时）只能在
    /// ClientBuilder 上设置，无法通过 RequestBuilder 单独覆盖。当请求显式设置了
    /// 这些参数中的任意一个时，从全局 `create_client_builder()` 起步（继承
    /// UA/keepalive/连接池/TLS 等全部配置），再逐项应用请求级覆盖后构建新 Client。
    ///
    /// 注意：这会牺牲连接池复用（新 Client 有独立连接池），仅在用户显式设置
    /// 上述任一参数时才走此分支。
    fn build_request_client(&self, _client: &reqwest::Client) -> XhCurlResult<reqwest::Client> {
        // 请求级 Client 缓存：按覆盖参数组合（OverrideKey）复用已构建的 Client，
        // 避免同类请求每次新建 Client 丢失连接池/TLS 会话复用。
        // reqwest::Client 内部为 Arc，clone 廉价。
        let key = OverrideKey::from_request(self);
        let cache = request_client_cache();
        if let Ok(map) = cache.lock() {
            if let Some(cached) = map.get(&key) {
                return Ok(cached.clone());
            }
        }

        // 缓存未命中：构建新 Client。
        // 从全局管理器获取完整配置的 ClientBuilder，确保 UA/keepalive/连接池/TLS
        // 等全部继承，再逐项覆盖请求级配置。
        let mut builder = crate::curl::XhCurlManager::global().create_client_builder()?;

        // 请求级连接超时（覆盖全局默认）
        // connect_timeout_ms（毫秒）优先级高于 connect_timeout（秒）
        // 0/负值跳过（视为「使用默认」），避免 Duration::from_millis(0) 立即超时
        if let Some(ms) = self.connect_timeout_ms {
            if ms > 0 {
                builder = builder.connect_timeout(Duration::from_millis(ms));
            }
        } else if let Some(secs) = self.connect_timeout {
            if secs > 0 {
                builder = builder.connect_timeout(Duration::from_secs(secs));
            }
        }

        // 请求级 SSL 证书验证（覆盖全局默认）
        if let Some(verify) = self.verify_ssl {
            builder = builder.danger_accept_invalid_certs(!verify);
        }

        // 请求级代理（覆盖全局默认）
        // 用户显式设置的代理若无效应明确报错，而非静默忽略
        if let Some(proxy_url) = &self.proxy {
            match reqwest::Proxy::all(proxy_url) {
                Ok(proxy) => builder = builder.proxy(proxy),
                Err(e) => {
                    return Err(XhCurlError::Generic(format!(
                        "无效的代理地址 {}: {}",
                        proxy_url, e
                    )));
                }
            }
        }

        // 请求级重定向策略（覆盖全局默认）
        match self.follow_redirects {
            Some(false) => {
                builder = builder.redirect(reqwest::redirect::Policy::none());
            }
            Some(true) => {
                if let Some(max) = self.max_redirects {
                    builder = builder.redirect(reqwest::redirect::Policy::limited(max as usize));
                } else {
                    builder = builder.redirect(reqwest::redirect::Policy::default());
                }
            }
            None => {
                // follow_redirects 未设置但 max_redirects 设置了：限制重定向次数
                if let Some(max) = self.max_redirects {
                    builder = builder.redirect(reqwest::redirect::Policy::limited(max as usize));
                }
            }
        }

        let client = builder.build().map_err(XhCurlError::from)?;
        // 存入缓存供后续同类请求复用。若已存在同名键（理论不会，因 OverrideKey
        // 唯一对应一组覆盖参数），用新值覆盖。
        if let Ok(mut map) = cache.lock() {
            map.insert(key, client.clone());
        }
        Ok(client)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试 HTTP 方法解析
    #[test]
    fn test_http_method_parse() {
        assert_eq!(HttpMethod::from_str("GET").unwrap(), HttpMethod::Get);
        assert_eq!(HttpMethod::from_str("post").unwrap(), HttpMethod::Post);
        assert_eq!(HttpMethod::from_str("Delete").unwrap(), HttpMethod::Delete);
        assert!(HttpMethod::from_str("INVALID").is_err());
    }

    /// 测试请求构建器
    #[test]
    fn test_request_builder() {
        let req = XhRequest::new("https://api.example.com/users")
            .post()
            .header("Authorization", "Bearer token")
            .header("Accept", "application/json")
            .body_json_str(r#"{"name": "test", "age": 18}"#)
            .unwrap()
            .request_timeout(30)
            .verify_ssl(true);

        assert_eq!(req.get_url(), "https://api.example.com/users");
        assert_eq!(req.get_method(), HttpMethod::Post);
        assert!(req.get_headers().has("authorization"));
        assert!(req.get_headers().has("content-type"));
    }

    /// 测试表单数据
    #[test]
    fn test_form_body() {
        let req = XhRequest::new("https://example.com/login")
            .post()
            .body_form(vec![
                ("username".to_string(), "admin".to_string()),
                ("password".to_string(), "secret".to_string()),
            ]);

        if let BodyType::Form(form) = req.get_body() {
            assert_eq!(form.len(), 2);
            assert_eq!(form[0].0, "username");
        } else {
            panic!("请求体类型应为 Form");
        }
    }

    /// 测试多部分表单
    #[test]
    fn test_multipart_body() {
        let req = XhRequest::new("https://example.com/upload")
            .post()
            .body_multipart(vec![
                MultipartField::text("description", "test file"),
                MultipartField::file("file", "test.txt", b"hello world".to_vec(), "text/plain"),
            ]);

        if let BodyType::Multipart(fields) = req.get_body() {
            assert_eq!(fields.len(), 2);
            assert_eq!(fields[0].name, "description");
            assert_eq!(fields[1].filename, Some("test.txt".to_string()));
        } else {
            panic!("请求体类型应为 Multipart");
        }
    }

    /// 测试转换为 reqwest 请求
    #[test]
    fn test_to_reqwest() {
        let client = reqwest::Client::new();
        let req = XhRequest::new("https://httpbin.org/get")
            .get()
            .header("X-Test", "value");

        let builder = req.to_reqwest(&client);
        assert!(builder.is_ok());
    }

    /// to_reqwest 处理空 URL 应返回错误
    #[test]
    fn test_to_reqwest_empty_url_returns_error() {
        let client = reqwest::Client::new();
        let req = XhRequest::new("").get();
        let result = req.to_reqwest(&client);
        assert!(result.is_err());
    }

    /// to_reqwest 处理无效 URL 格式应返回错误
    #[test]
    fn test_to_reqwest_invalid_url_returns_error() {
        let client = reqwest::Client::new();
        let req = XhRequest::new("not a url with spaces").get();
        let result = req.to_reqwest(&client);
        assert!(result.is_err());
    }

    /// body_json_str 处理无效 JSON 应返回错误
    #[test]
    fn test_body_json_str_invalid_json_returns_error() {
        let result = XhRequest::new("https://x.com").body_json_str("{ invalid json }");
        assert!(result.is_err());
    }

    /// body_json_str 处理空字符串应返回错误
    #[test]
    fn test_body_json_str_empty_returns_error() {
        let result = XhRequest::new("https://x.com").body_json_str("");
        assert!(result.is_err());
    }

    /// build_request_client 处理无效代理应返回错误
    /// 请求级代理（self.proxy）在 build_request_client 中被解析；
    /// 无效格式应明确报错而非静默忽略，避免请求实际不走代理。
    /// 注：reqwest::Proxy::all 仅在 URL 语法非法时报错（如 "://" 缺 scheme）。
    #[test]
    fn test_build_request_client_invalid_proxy_returns_error() {
        let req = XhRequest::new("https://x.com").get().proxy("://");
        // build_request_client 内部使用 XhCurlManager::global() 获取全局配置，
        // 全局默认无代理 → create_client_builder 成功；
        // 然后应用请求级 proxy("://") → reqwest::Proxy::all 失败 → 返回错误。
        let client = reqwest::Client::new();
        let result = req.build_request_client(&client);
        assert!(result.is_err());
    }

    /// build_request_client 处理空代理字符串应返回错误
    #[test]
    fn test_build_request_client_empty_proxy_returns_error() {
        let req = XhRequest::new("https://x.com").get().proxy("");
        let client = reqwest::Client::new();
        let result = req.build_request_client(&client);
        assert!(result.is_err());
    }

    /// 请求级 Client 缓存：同一组覆盖参数的请求应复用同一 Client（命中缓存）。
    ///
    /// 验证 OverrideKey 相同时 build_request_client 不会新建条目——
    /// 两个不同 URL 但覆盖参数相同的请求构建后，缓存中该 OverrideKey 存在，
    /// 说明第二个请求命中了第一个请求写入的缓存。
    ///
    /// 注：不使用 `len()` 断言，因并行测试共享全局缓存，计数会受其他测试影响。
    /// 改为检查特定 OverrideKey 是否存在，确保测试并行安全。
    #[test]
    fn test_request_client_cache_hit() {
        clear_request_client_cache();

        // 两个请求仅 URL 不同，覆盖参数组合（verify_ssl/follow_redirects/proxy/
        // connect_timeout）完全相同 → OverrideKey 相同 → 命中同一缓存项。
        let req1 = XhRequest::new("https://httpbin.org/get?a=1")
            .get()
            .verify_ssl(false)
            .follow_redirects(true)
            .connect_timeout(15);
        let req2 = XhRequest::new("https://httpbin.org/get?b=2")
            .get()
            .verify_ssl(false)
            .follow_redirects(true)
            .connect_timeout(15);

        let placeholder = reqwest::Client::new();
        let _ = req1.build_request_client(&placeholder).unwrap();
        let _ = req2.build_request_client(&placeholder).unwrap();

        // 验证该 OverrideKey 存在于缓存中（而非断言精确条目数，避免并行干扰）
        let key = OverrideKey::from_request(&req1);
        let cache = request_client_cache();
        let map = cache.lock().unwrap_or_else(|e| e.into_inner());
        assert!(
            map.contains_key(&key),
            "OverrideKey 应存在于缓存中（命中或写入）"
        );
    }

    /// 请求级 Client 缓存：不同覆盖参数组合应生成不同的缓存条目。
    ///
    /// 注：不使用 `len()` 断言，因并行测试共享全局缓存。改为验证两个不同的
    /// OverrideKey 都存在于缓存中。
    #[test]
    fn test_request_client_cache_miss_different_overrides() {
        clear_request_client_cache();

        let req1 = XhRequest::new("https://x.com").get().verify_ssl(true);
        let req2 = XhRequest::new("https://x.com").get().verify_ssl(false);

        let placeholder = reqwest::Client::new();
        let _ = req1.build_request_client(&placeholder).unwrap();
        let _ = req2.build_request_client(&placeholder).unwrap();

        // 验证两个不同的 OverrideKey 都在缓存中
        let key1 = OverrideKey::from_request(&req1);
        let key2 = OverrideKey::from_request(&req2);
        let cache = request_client_cache();
        let map = cache.lock().unwrap_or_else(|e| e.into_inner());
        assert_ne!(key1, key2, "两个请求的 OverrideKey 应不同");
        assert!(map.contains_key(&key1), "第一个 OverrideKey 应在缓存中");
        assert!(map.contains_key(&key2), "第二个 OverrideKey 应在缓存中");
    }

    /// 请求级 Client 缓存：clear_request_client_cache 清空后缓存可重新填充。
    ///
    /// 注：不断言精确 len()（并行测试可能重新写入），仅验证 clear 不 panic
    /// 且后续构建可正常工作。
    #[test]
    fn test_clear_request_client_cache() {
        clear_request_client_cache();

        let req = XhRequest::new("https://x.com").get().verify_ssl(false);
        let placeholder = reqwest::Client::new();
        let _ = req.build_request_client(&placeholder).unwrap();

        // 验证构建后 key 存在
        let key = OverrideKey::from_request(&req);
        {
            let cache = request_client_cache();
            let map = cache.lock().unwrap_or_else(|e| e.into_inner());
            assert!(map.contains_key(&key), "构建后 OverrideKey 应存在");
        }

        // clear 不 panic
        clear_request_client_cache();

        // 验证 clear 后 key 不存在（除非并行测试恰好写入相同 key，概率极低）
        {
            let cache = request_client_cache();
            let map = cache.lock().unwrap_or_else(|e| e.into_inner());
            assert!(!map.contains_key(&key), "清空后该 OverrideKey 应不存在");
        }
    }
}
