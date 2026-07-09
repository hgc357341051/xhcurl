// +----------------------------------------------------------------------+
// | XHCurl 扩展 - HTTP 请求构建器                                         |
// | 使用 Builder 模式链式构建请求                                          |
// | 支持 GET/POST/PUT/DELETE/PATCH/HEAD/OPTIONS 方法                      |
// | 支持流式上传和下载                                                     |
// +----------------------------------------------------------------------+

use std::time::Duration;

use crate::error::{XhCurlError, XhCurlResult};
use crate::header::HeaderManager;

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

    /// 请求超时（秒，0 = 使用全局默认）
    request_timeout: Option<u64>,

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

    /// Cookie 文件路径（CURLOPT_COOKIEFILE）
    /// 读取该文件中的 cookie 加入请求
    cookie_file: Option<String>,

    /// Cookie 存储文件路径（CURLOPT_COOKIEJAR）
    /// 请求结束后将 cookie 保存到该文件
    cookie_jar: Option<String>,

    /// HTTP 基本认证凭据（CURLOPT_USERPWD）
    /// 格式: "username:password"
    auth: Option<String>,

    /// Bearer Token（CURLOPT_XOAUTH2_BEARER）
    /// 自动设置 Authorization: Bearer {token}
    bearer_token: Option<String>,

    /// 自定义 CA 证书路径（CURLOPT_CAINFO）
    ca_info: Option<String>,

    /// 客户端证书路径（CURLOPT_SSLCERT）
    ssl_cert: Option<String>,

    /// 客户端证书密钥路径（CURLOPT_SSLKEY）
    ssl_key: Option<String>,

    /// 客户端证书密钥密码（CURLOPT_SSLKEYPASSWD）
    ssl_key_password: Option<String>,

    /// Accept-Encoding 头部（CURLOPT_ENCODING）
    /// 如 "gzip, deflate"
    encoding: Option<String>,

    /// 自定义请求方法（CURLOPT_CUSTOMREQUEST）
    /// 覆盖标准 HTTP 方法，用于 CONNECT/TRACE 等非标准方法
    custom_method: Option<String>,

    /// Range 请求范围（CURLOPT_RANGE）
    /// 格式: "0-1023" 表示请求前 1024 字节
    range: Option<String>,

    /// 请求优先级（0 = 默认，数值越大优先级越高）
    /// 用于线程池模式下的任务调度
    priority: i32,

    /// 流式回调间隔（字节）
    /// 每接收指定大小数据触发一次回调
    /// 0 表示禁用流式回调
    stream_chunk_size: usize,
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
            request_timeout: None,
            follow_redirects: None,
            max_redirects: None,
            verify_ssl: None,
            user_agent: None,
            proxy: None,
            id: None,
            user_data: None,
            cookies: None,
            cookie_file: None,
            cookie_jar: None,
            auth: None,
            bearer_token: None,
            ca_info: None,
            ssl_cert: None,
            ssl_key: None,
            ssl_key_password: None,
            encoding: None,
            custom_method: None,
            range: None,
            priority: 0,
            stream_chunk_size: 0,
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
        let _ = self.headers.set(name, value);
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
        let _ = self.headers.set("Content-Type", "application/json");
        Ok(self)
    }

    /// 设置 JSON 字符串请求体
    pub fn body_json_str(mut self, json_str: &str) -> XhCurlResult<Self> {
        let value: serde_json::Value = serde_json::from_str(json_str)?;
        self.body = BodyType::Json(value);
        let _ = self.headers.set("Content-Type", "application/json");
        Ok(self)
    }

    /// 设置表单数据
    /// 自动设置 Content-Type: application/x-www-form-urlencoded
    pub fn body_form(mut self, form: Vec<(String, String)>) -> Self {
        self.body = BodyType::Form(form);
        let _ = self
            .headers
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

    /// 设置请求超时（秒）
    pub fn request_timeout(mut self, secs: u64) -> Self {
        self.request_timeout = Some(secs);
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

    /// 设置 Cookie 读取文件路径（CURLOPT_COOKIEFILE）
    pub fn cookie_file(mut self, path: impl Into<String>) -> Self {
        self.cookie_file = Some(path.into());
        self
    }

    /// 设置 Cookie 存储文件路径（CURLOPT_COOKIEJAR）
    pub fn cookie_jar(mut self, path: impl Into<String>) -> Self {
        self.cookie_jar = Some(path.into());
        self
    }

    /// 设置 HTTP 基本认证凭据（CURLOPT_USERPWD）
    /// 格式: "username:password"
    pub fn basic_auth(mut self, credentials: impl Into<String>) -> Self {
        self.auth = Some(credentials.into());
        self
    }

    /// 设置 Bearer Token（CURLOPT_XOAUTH2_BEARER）
    /// 自动设置 Authorization: Bearer {token}
    pub fn bearer_token(mut self, token: impl Into<String>) -> Self {
        self.bearer_token = Some(token.into());
        self
    }

    /// 设置自定义 CA 证书路径（CURLOPT_CAINFO）
    pub fn ca_info(mut self, path: impl Into<String>) -> Self {
        self.ca_info = Some(path.into());
        self
    }

    /// 设置客户端证书路径（CURLOPT_SSLCERT）
    pub fn ssl_cert(mut self, path: impl Into<String>) -> Self {
        self.ssl_cert = Some(path.into());
        self
    }

    /// 设置客户端证书密钥路径（CURLOPT_SSLKEY）
    pub fn ssl_key(mut self, path: impl Into<String>) -> Self {
        self.ssl_key = Some(path.into());
        self
    }

    /// 设置客户端证书密钥密码（CURLOPT_SSLKEYPASSWD）
    pub fn ssl_key_password(mut self, password: impl Into<String>) -> Self {
        self.ssl_key_password = Some(password.into());
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

    /// 设置请求优先级
    pub fn priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }

    /// 设置流式回调间隔（字节）
    /// 0 = 禁用流式回调
    pub fn stream_chunk_size(mut self, size: usize) -> Self {
        self.stream_chunk_size = size;
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

    /// 获取自定义 CA 证书路径
    pub fn get_ca_info(&self) -> Option<&str> {
        self.ca_info.as_deref()
    }

    /// 获取 Accept-Encoding
    pub fn get_encoding(&self) -> Option<&str> {
        self.encoding.as_deref()
    }

    /// 获取请求优先级
    pub fn get_priority(&self) -> i32 {
        self.priority
    }

    /// 获取流式回调间隔
    pub fn get_stream_chunk_size(&self) -> usize {
        self.stream_chunk_size
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

    /// 构建为 reqwest 请求构建器
    /// 将 XhRequest 转换为 reqwest::RequestBuilder
    ///
    /// # 参数
    /// - `client`: reqwest 客户端实例
    ///
    /// # 返回
    /// 配置好的 reqwest::RequestBuilder
    pub fn to_reqwest(&self, client: &reqwest::Client) -> XhCurlResult<reqwest::RequestBuilder> {
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
        let mut header_map = self.headers.to_header_map();

        // Accept-Encoding（CURLOPT_ENCODING）
        if let Some(encoding) = &self.encoding {
            if let Ok(v) = reqwest::header::HeaderValue::from_str(encoding) {
                header_map.insert(reqwest::header::ACCEPT_ENCODING, v);
            }
        }

        // Range（CURLOPT_RANGE）
        if let Some(range) = &self.range {
            if let Ok(v) = reqwest::header::HeaderValue::from_str(range) {
                header_map.insert(reqwest::header::RANGE, v);
            }
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
                    form = form
                        .text(field.name.clone(), String::new())
                        .part(field.name.clone(), part);
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
        if let Some(cookies) = &self.cookies {
            if let Ok(v) = reqwest::header::HeaderValue::from_str(cookies) {
                builder = builder.header(reqwest::header::COOKIE, v);
            }
        }

        // 设置超时（覆盖客户端默认值）
        if let Some(timeout) = self.request_timeout {
            builder = builder.timeout(Duration::from_secs(timeout));
        }

        Ok(builder)
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
}
