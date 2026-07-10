// +----------------------------------------------------------------------+
// | XHCurl 扩展 - HTTP 响应对象                                           |
// | 懒加载设计：响应体按需读取，避免大响应体占用内存                       |
// | 支持流式读取和分段访问                                                 |
// +----------------------------------------------------------------------+

use std::collections::HashMap;
use std::time::Duration;

use crate::error::{XhCurlError, XhCurlResult};
use crate::header::HeaderManager;

/// HTTP 响应对象
/// 封装 reqwest 的响应，提供懒加载和便捷访问方法
///
/// # 设计理念
/// - 响应头在请求完成时立即解析
/// - 响应体按需读取（懒加载），避免大响应体占用内存
/// - 支持流式读取，适合处理大文件下载
///
/// # 线程安全
/// 响应对象在请求完成后转移到调用方，不再被多线程访问
/// 对应 C 版本的 xhcurl_response_t 结构体
#[derive(Debug)]
pub struct XhResponse {
    /// HTTP 状态码（如 200、404、500）
    status: u16,

    /// 响应头管理器
    headers: HeaderManager,

    /// 响应体数据（懒加载）
    /// None 表示尚未读取响应体
    body: Option<Vec<u8>>,

    /// 最终 URL（可能因重定向而与请求 URL 不同）
    final_url: String,

    /// 请求耗时（从发送到接收完成）
    elapsed: Duration,

    /// 响应体大小（字节）
    /// 在流式读取时用于跟踪已接收的数据量
    body_size: usize,

    /// 远程服务器地址（IP:Port）
    remote_addr: Option<String>,

    /// HTTP 协议版本（如 "HTTP/1.1"、"HTTP/2"）
    version: Option<String>,

    /// 是否成功（2xx 状态码）
    is_success: bool,

    /// 错误信息（如果请求失败）
    error: Option<String>,
}

impl XhResponse {
    /// 从已解析的响应元数据创建 XhResponse
    ///
    /// 流式读取（chunk()）消费 reqwest::Response 后，无法再调用其 API，
    /// 使用此方法用已收集的元数据手动构建 XhResponse。
    ///
    /// # 参数
    /// - `status`: HTTP 状态码
    /// - `url`: 最终 URL
    /// - `headers`: 响应头 HashMap
    /// - `body`: 响应体数据
    /// - `elapsed`: 请求耗时
    /// - `remote_addr`: 远程服务器地址（可选）
    /// - `version`: HTTP 协议版本（可选）
    pub fn from_parts(
        status: u16,
        url: String,
        headers: HashMap<String, String>,
        body: Vec<u8>,
        elapsed: Duration,
        remote_addr: Option<String>,
        version: Option<String>,
    ) -> Self {
        // 构建 HeaderManager
        let header_mgr = HeaderManager::new();
        for (name, value) in &headers {
            header_mgr.set(name, value);
        }

        // 判断是否成功（2xx）
        let is_success = (200..300).contains(&status);

        // 响应体大小
        let body_size = body.len();

        Self {
            status,
            headers: header_mgr,
            body: Some(body),
            final_url: url,
            elapsed,
            body_size,
            remote_addr,
            version,
            is_success,
            error: None,
        }
    }

    /// 创建错误响应（请求失败时使用）
    ///
    /// # 参数
    /// - `error`: 错误信息
    /// - `url`: 请求 URL
    /// - `elapsed`: 耗时
    pub fn from_error(error: String, url: String, elapsed: Duration) -> Self {
        Self {
            status: 0,
            headers: HeaderManager::new(),
            body: None,
            final_url: url,
            elapsed,
            body_size: 0,
            remote_addr: None,
            version: None,
            is_success: false,
            error: Some(error),
        }
    }

    /// 直接设置响应体数据（用于流式读取后设置）
    ///
    /// # 参数
    /// - `data`: 响应体数据
    pub fn set_body(&mut self, data: Vec<u8>) {
        self.body_size = data.len();
        self.body = Some(data);
    }

    // ===== 状态码相关方法 =====

    /// 获取 HTTP 状态码
    pub fn status(&self) -> u16 {
        self.status
    }

    /// 检查是否成功（2xx）
    pub fn is_success(&self) -> bool {
        self.is_success
    }

    /// 检查是否为客户端错误（4xx）
    pub fn is_client_error(&self) -> bool {
        self.status >= 400 && self.status < 500
    }

    /// 检查是否为服务器错误（5xx）
    pub fn is_server_error(&self) -> bool {
        self.status >= 500 && self.status < 600
    }

    /// 检查是否为重定向（3xx）
    pub fn is_redirect(&self) -> bool {
        self.status >= 300 && self.status < 400
    }

    // ===== 响应头相关方法 =====

    /// 获取响应头管理器引用
    pub fn headers(&self) -> &HeaderManager {
        &self.headers
    }

    /// 获取指定响应头
    pub fn header(&self, name: &str) -> Option<String> {
        self.headers.get(name)
    }

    /// 获取 Content-Type 响应头
    pub fn content_type(&self) -> Option<String> {
        self.headers.get("content-type")
    }

    /// 获取 Content-Length 响应头
    pub fn content_length(&self) -> Option<usize> {
        self.headers
            .get("content-length")
            .and_then(|s| s.parse().ok())
    }

    // ===== 响应体相关方法 =====

    /// 获取响应体数据（如果已读取）
    pub fn body(&self) -> Option<&[u8]> {
        self.body.as_deref()
    }

    /// 获取响应体大小
    pub fn body_size(&self) -> usize {
        self.body_size
    }

    /// 将响应体作为 UTF-8 字符串获取
    ///
    /// # 返回
    /// - `Ok(String)`: 转换成功
    /// - `Err`: 响应体未读取或不是有效 UTF-8
    pub fn body_text(&self) -> XhCurlResult<String> {
        let body = self
            .body
            .as_ref()
            .ok_or_else(|| XhCurlError::Generic("响应体尚未读取".to_string()))?;
        String::from_utf8(body.clone())
            .map_err(|e| XhCurlError::Generic(format!("UTF-8 转换失败: {}", e)))
    }

    /// 将响应体作为 JSON 解析
    ///
    /// # 返回
    /// - `Ok(serde_json::Value)`: 解析成功
    /// - `Err`: 响应体未读取或 JSON 格式错误
    pub fn body_json(&self) -> XhCurlResult<serde_json::Value> {
        let body = self
            .body
            .as_ref()
            .ok_or_else(|| XhCurlError::Generic("响应体尚未读取".to_string()))?;
        serde_json::from_slice(body).map_err(XhCurlError::from)
    }

    // ===== 元数据方法 =====

    /// 获取最终 URL（可能因重定向而变化）
    pub fn url(&self) -> &str {
        &self.final_url
    }

    /// 获取请求耗时
    pub fn elapsed(&self) -> Duration {
        self.elapsed
    }

    /// 获取远程服务器地址
    pub fn remote_addr(&self) -> Option<&str> {
        self.remote_addr.as_deref()
    }

    /// 获取 HTTP 协议版本
    pub fn version(&self) -> Option<&str> {
        self.version.as_deref()
    }

    /// 获取错误信息（如果有）
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// 检查是否有错误
    pub fn has_error(&self) -> bool {
        self.error.is_some()
    }

    /// 将响应信息转换为 HashMap（用于 PHP 数组转换）
    pub fn to_info_map(&self) -> HashMap<String, String> {
        let mut map = HashMap::new();
        map.insert("status".to_string(), self.status.to_string());
        map.insert("url".to_string(), self.final_url.clone());
        map.insert("elapsed".to_string(), format!("{:?}", self.elapsed));
        map.insert("body_size".to_string(), self.body_size.to_string());
        map.insert("is_success".to_string(), self.is_success.to_string());

        if let Some(addr) = &self.remote_addr {
            map.insert("remote_addr".to_string(), addr.clone());
        }
        if let Some(version) = &self.version {
            map.insert("version".to_string(), version.clone());
        }
        if let Some(error) = &self.error {
            map.insert("error".to_string(), error.clone());
        }

        map
    }
}

impl Clone for XhResponse {
    fn clone(&self) -> Self {
        Self {
            status: self.status,
            headers: self.headers.clone(),
            body: self.body.clone(),
            final_url: self.final_url.clone(),
            elapsed: self.elapsed,
            body_size: self.body_size,
            remote_addr: self.remote_addr.clone(),
            version: self.version.clone(),
            is_success: self.is_success,
            error: self.error.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试错误响应创建
    #[test]
    fn test_error_response() {
        let response = XhResponse::from_error(
            "连接超时".to_string(),
            "https://example.com".to_string(),
            Duration::from_secs(30),
        );

        assert_eq!(response.status(), 0);
        assert!(!response.is_success());
        assert!(response.has_error());
        assert_eq!(response.error(), Some("连接超时"));
        assert_eq!(response.url(), "https://example.com");
    }

    /// 测试响应体操作
    #[test]
    fn test_body_operations() {
        let mut response = XhResponse::from_error(
            "test".to_string(),
            "https://example.com".to_string(),
            Duration::from_secs(0),
        );

        // 设置响应体
        response.set_body(b"hello world".to_vec());

        // 验证
        assert_eq!(response.body_size(), 11);
        assert_eq!(response.body(), Some(b"hello world".as_ref()));
        assert_eq!(response.body_text().unwrap(), "hello world");
    }

    /// 测试 JSON 响应体
    #[test]
    fn test_json_body() {
        let mut response = XhResponse::from_error(
            "test".to_string(),
            "https://example.com".to_string(),
            Duration::from_secs(0),
        );

        response.set_body(br#"{"name": "test", "age": 18}"#.to_vec());

        let json = response.body_json().unwrap();
        assert_eq!(json["name"], "test");
        assert_eq!(json["age"], 18);
    }

    /// 测试状态码判断
    #[test]
    fn test_status_checks() {
        let mut response = XhResponse::from_error(
            "test".to_string(),
            "https://example.com".to_string(),
            Duration::from_secs(0),
        );

        // 模拟 200 响应
        response.status = 200;
        response.is_success = true;
        assert!(response.is_success());
        assert!(!response.is_client_error());
        assert!(!response.is_server_error());

        // 模拟 404 响应
        response.status = 404;
        response.is_success = false;
        assert!(!response.is_success());
        assert!(response.is_client_error());

        // 模拟 500 响应
        response.status = 500;
        assert!(response.is_server_error());
    }

    /// 测试信息 Map 转换
    #[test]
    fn test_to_info_map() {
        let response = XhResponse::from_error(
            "连接失败".to_string(),
            "https://example.com".to_string(),
            Duration::from_secs(5),
        );

        let map = response.to_info_map();
        assert_eq!(map.get("status"), Some(&"0".to_string()));
        assert_eq!(map.get("url"), Some(&"https://example.com".to_string()));
        assert_eq!(map.get("error"), Some(&"连接失败".to_string()));
    }

    // ===== TDD 新增测试：from_parts 构造函数 + 状态码边界 =====

    /// from_parts 正常 2xx 响应应标记为成功
    #[test]
    fn test_from_parts_success_2xx() {
        let resp = XhResponse::from_parts(
            200,
            "https://example.com".to_string(),
            HashMap::new(),
            b"ok".to_vec(),
            Duration::from_millis(100),
            None,
            None,
        );
        assert!(resp.is_success());
        assert_eq!(resp.status(), 200);
        assert_eq!(resp.body(), Some(b"ok".as_ref()));
        assert_eq!(resp.body_size(), 2);
        assert!(resp.error().is_none());
    }

    /// 状态码 199 不应是 success（下边界）
    #[test]
    fn test_status_boundary_199_not_success() {
        let resp = XhResponse::from_parts(
            199,
            "https://x".to_string(),
            HashMap::new(),
            Vec::new(),
            Duration::from_secs(0),
            None,
            None,
        );
        assert!(!resp.is_success());
    }

    /// 状态码 200 是 success 下边界
    #[test]
    fn test_status_boundary_200_success() {
        let resp = XhResponse::from_parts(
            200,
            "https://x".to_string(),
            HashMap::new(),
            Vec::new(),
            Duration::from_secs(0),
            None,
            None,
        );
        assert!(resp.is_success());
    }

    /// 状态码 299 是 success 上边界
    #[test]
    fn test_status_boundary_299_success() {
        let resp = XhResponse::from_parts(
            299,
            "https://x".to_string(),
            HashMap::new(),
            Vec::new(),
            Duration::from_secs(0),
            None,
            None,
        );
        assert!(resp.is_success());
    }

    /// 状态码 300 不是 success，是 redirect
    #[test]
    fn test_status_boundary_300_redirect() {
        let resp = XhResponse::from_parts(
            300,
            "https://x".to_string(),
            HashMap::new(),
            Vec::new(),
            Duration::from_secs(0),
            None,
            None,
        );
        assert!(!resp.is_success());
        assert!(resp.is_redirect());
    }

    /// 状态码 399 是 redirect 上边界
    #[test]
    fn test_status_boundary_399_redirect() {
        let resp = XhResponse::from_parts(
            399,
            "https://x".to_string(),
            HashMap::new(),
            Vec::new(),
            Duration::from_secs(0),
            None,
            None,
        );
        assert!(resp.is_redirect());
    }

    /// 状态码 400 是 client_error 下边界
    #[test]
    fn test_status_boundary_400_client_error() {
        let resp = XhResponse::from_parts(
            400,
            "https://x".to_string(),
            HashMap::new(),
            Vec::new(),
            Duration::from_secs(0),
            None,
            None,
        );
        assert!(!resp.is_success());
        assert!(resp.is_client_error());
        assert!(!resp.is_server_error());
    }

    /// 状态码 499 是 client_error 上边界
    #[test]
    fn test_status_boundary_499_client_error() {
        let resp = XhResponse::from_parts(
            499,
            "https://x".to_string(),
            HashMap::new(),
            Vec::new(),
            Duration::from_secs(0),
            None,
            None,
        );
        assert!(resp.is_client_error());
    }

    /// 状态码 500 是 server_error 下边界
    #[test]
    fn test_status_boundary_500_server_error() {
        let resp = XhResponse::from_parts(
            500,
            "https://x".to_string(),
            HashMap::new(),
            Vec::new(),
            Duration::from_secs(0),
            None,
            None,
        );
        assert!(resp.is_server_error());
        assert!(!resp.is_client_error());
    }

    /// 状态码 599 是 server_error 上边界
    #[test]
    fn test_status_boundary_599_server_error() {
        let resp = XhResponse::from_parts(
            599,
            "https://x".to_string(),
            HashMap::new(),
            Vec::new(),
            Duration::from_secs(0),
            None,
            None,
        );
        assert!(resp.is_server_error());
    }

    /// 状态码 600 不是 server_error
    #[test]
    fn test_status_boundary_600_not_server_error() {
        let resp = XhResponse::from_parts(
            600,
            "https://x".to_string(),
            HashMap::new(),
            Vec::new(),
            Duration::from_secs(0),
            None,
            None,
        );
        assert!(!resp.is_server_error());
        assert!(!resp.is_redirect());
        assert!(!resp.is_client_error());
        assert!(!resp.is_success());
    }

    /// from_parts 应正确设置 headers
    #[test]
    fn test_from_parts_with_headers() {
        let mut headers = HashMap::new();
        headers.insert("content-type".to_string(), "application/json".to_string());
        headers.insert("content-length".to_string(), "42".to_string());

        let resp = XhResponse::from_parts(
            200,
            "https://x".to_string(),
            headers,
            b"{}".to_vec(),
            Duration::from_secs(0),
            Some("1.2.3.4:443".to_string()),
            Some("HTTP/2".to_string()),
        );

        assert_eq!(
            resp.header("content-type"),
            Some("application/json".to_string())
        );
        assert_eq!(resp.content_length(), Some(42));
        assert_eq!(resp.content_type(), Some("application/json".to_string()));
        assert_eq!(resp.remote_addr(), Some("1.2.3.4:443"));
        assert_eq!(resp.version(), Some("HTTP/2"));
    }

    // ===== TDD 新增测试：body_text / body_json 错误路径 =====

    /// body_text 当 body 为 None 时应返回错误
    #[test]
    fn test_body_text_none_returns_error() {
        let resp = XhResponse::from_error(
            "err".to_string(),
            "https://x".to_string(),
            Duration::from_secs(0),
        );
        let result = resp.body_text();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("响应体尚未读取"));
    }

    /// body_text 当 body 包含无效 UTF-8 字节时应返回错误
    #[test]
    fn test_body_text_invalid_utf8_returns_error() {
        let mut resp = XhResponse::from_error(
            "err".to_string(),
            "https://x".to_string(),
            Duration::from_secs(0),
        );
        resp.set_body(vec![0xFF, 0xFE, 0xFD]);
        let result = resp.body_text();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("UTF-8"));
    }

    /// body_json 当 body 为 None 时应返回错误
    #[test]
    fn test_body_json_none_returns_error() {
        let resp = XhResponse::from_error(
            "err".to_string(),
            "https://x".to_string(),
            Duration::from_secs(0),
        );
        let result = resp.body_json();
        assert!(result.is_err());
    }

    /// body_json 当 body 为无效 JSON 时应返回错误
    #[test]
    fn test_body_json_invalid_json_returns_error() {
        let mut resp = XhResponse::from_error(
            "err".to_string(),
            "https://x".to_string(),
            Duration::from_secs(0),
        );
        resp.set_body(b"not json {".to_vec());
        let result = resp.body_json();
        assert!(result.is_err());
    }

    /// XhResponse Clone 应完整复制所有字段
    #[test]
    fn test_response_clone_preserves_fields() {
        let mut headers = HashMap::new();
        headers.insert("x-custom".to_string(), "val".to_string());
        let resp = XhResponse::from_parts(
            201,
            "https://x".to_string(),
            headers,
            b"body".to_vec(),
            Duration::from_millis(42),
            Some("1.2.3.4".to_string()),
            Some("HTTP/1.1".to_string()),
        );
        let cloned = resp.clone();
        assert_eq!(cloned.status(), 201);
        assert_eq!(cloned.body(), Some(b"body".as_ref()));
        assert_eq!(cloned.body_size(), 4);
        assert_eq!(cloned.header("x-custom"), Some("val".to_string()));
        assert_eq!(cloned.remote_addr(), Some("1.2.3.4"));
        assert_eq!(cloned.version(), Some("HTTP/1.1"));
        assert_eq!(cloned.elapsed(), Duration::from_millis(42));
    }

    /// content_length 当头不存在时应返回 None
    #[test]
    fn test_content_length_missing_returns_none() {
        let resp = XhResponse::from_parts(
            200,
            "https://x".to_string(),
            HashMap::new(),
            Vec::new(),
            Duration::from_secs(0),
            None,
            None,
        );
        assert_eq!(resp.content_length(), None);
    }

    /// content_length 当头为非数字时应返回 None
    #[test]
    fn test_content_length_non_numeric_returns_none() {
        let mut headers = HashMap::new();
        headers.insert("content-length".to_string(), "abc".to_string());
        let resp = XhResponse::from_parts(
            200,
            "https://x".to_string(),
            headers,
            Vec::new(),
            Duration::from_secs(0),
            None,
            None,
        );
        assert_eq!(resp.content_length(), None);
    }
}
