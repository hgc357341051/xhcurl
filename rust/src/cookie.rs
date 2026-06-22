// +----------------------------------------------------------------------+
// | XHCurl 扩展 - Cookie 管理                                            |
// | 支持会话级和持久化 Cookie，线程安全                                   |
// | 使用 Arc<RwLock<HashMap>> 实现多线程安全共享                          |
// +----------------------------------------------------------------------+

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// 单个 Cookie 的表示
/// 对应 C 版本的 xhcurl_cookie_t 结构体
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cookie {
    /// Cookie 名称
    pub name: String,
    /// Cookie 值
    pub value: String,
    /// 过期时间（Unix 时间戳，0 表示会话级 Cookie）
    pub expires: u64,
    /// 域名
    pub domain: String,
    /// 路径
    pub path: String,
    /// 是否为安全 Cookie（仅 HTTPS 传输）
    pub secure: bool,
    /// 是否为 HttpOnly（JavaScript 不可访问）
    pub http_only: bool,
}

impl Cookie {
    /// 创建新的 Cookie 实例
    ///
    /// # 参数
    /// - `name`: Cookie 名称
    /// - `value`: Cookie 值
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
            expires: 0, // 默认会话级
            domain: String::new(),
            path: "/".to_string(), // 默认路径
            secure: false,
            http_only: false,
        }
    }

    /// 设置过期时间
    pub fn with_expires(mut self, expires: u64) -> Self {
        self.expires = expires;
        self
    }

    /// 设置域名
    pub fn with_domain(mut self, domain: impl Into<String>) -> Self {
        self.domain = domain.into();
        self
    }

    /// 设置路径
    pub fn with_path(mut self, path: impl Into<String>) -> Self {
        self.path = path.into();
        self
    }

    /// 设置安全标志
    pub fn with_secure(mut self, secure: bool) -> Self {
        self.secure = secure;
        self
    }

    /// 设置 HttpOnly 标志
    pub fn with_http_only(mut self, http_only: bool) -> Self {
        self.http_only = http_only;
        self
    }

    /// 检查 Cookie 是否已过期
    ///
    /// # 参数
    /// - `current_time`: 当前 Unix 时间戳
    ///
    /// # 返回
    /// - `true`: 已过期（expires > 0 且 current_time > expires）
    /// - `false`: 未过期或会话级 Cookie
    pub fn is_expired(&self, current_time: u64) -> bool {
        // expires == 0 表示会话级 Cookie，永不过期
        self.expires > 0 && current_time > self.expires
    }

    /// 将 Cookie 转换为 HTTP 请求头格式字符串
    /// 格式: "name=value; "
    pub fn to_header_string(&self) -> String {
        format!("{}={}", self.name, self.value)
    }
}

/// Cookie 管理器
/// 线程安全的 Cookie 存储，支持按域名和路径组织
///
/// # 线程安全
/// 使用 Arc<RwLock<HashMap>> 实现多线程共享
/// 对应 C 版本的 cookie jar 功能
#[derive(Debug, Clone)]
pub struct CookieManager {
    /// Cookie 存储
    /// 键格式: "domain|name"（使用域名+名称作为唯一键）
    /// 值: Cookie 实例
    cookies: Arc<RwLock<HashMap<String, Cookie>>>,
}

impl CookieManager {
    /// 创建新的 Cookie 管理器
    pub fn new() -> Self {
        Self {
            cookies: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 生成 Cookie 存储键
    /// 格式: "domain|name"
    fn make_key(domain: &str, name: &str) -> String {
        format!("{}|{}", domain.to_lowercase(), name)
    }

    /// 添加 Cookie
    ///
    /// # 参数
    /// - `cookie`: Cookie 实例
    pub fn set(&self, cookie: Cookie) {
        let key = Self::make_key(&cookie.domain, &cookie.name);
        let mut cookies = self.cookies.write().unwrap();
        cookies.insert(key, cookie);
    }

    /// 简化版添加 Cookie（仅名称和值）
    ///
    /// # 参数
    /// - `name`: Cookie 名称
    /// - `value`: Cookie 值
    pub fn set_simple(&self, name: &str, value: &str) {
        let cookie = Cookie::new(name, value);
        self.set(cookie);
    }

    /// 获取指定域名和名称的 Cookie
    ///
    /// # 参数
    /// - `domain`: 域名
    /// - `name`: Cookie 名称
    ///
    /// # 返回
    /// - `Some(Cookie)`: Cookie 存在
    /// - `None`: Cookie 不存在
    pub fn get(&self, domain: &str, name: &str) -> Option<Cookie> {
        let key = Self::make_key(domain, name);
        let cookies = self.cookies.read().unwrap();
        cookies.get(&key).cloned()
    }

    /// 获取指定域名的所有 Cookie
    ///
    /// # 参数
    /// - `domain`: 域名
    ///
    /// # 返回
    /// 匹配域名的所有 Cookie 列表
    pub fn get_by_domain(&self, domain: &str) -> Vec<Cookie> {
        let cookies = self.cookies.read().unwrap();
        let domain_lower = domain.to_lowercase();

        cookies
            .values()
            .filter(|c| c.domain.to_lowercase() == domain_lower)
            .cloned()
            .collect()
    }

    /// 移除指定域名和名称的 Cookie
    ///
    /// # 参数
    /// - `domain`: 域名
    /// - `name`: Cookie 名称
    pub fn remove(&self, domain: &str, name: &str) -> Option<Cookie> {
        let key = Self::make_key(domain, name);
        let mut cookies = self.cookies.write().unwrap();
        cookies.remove(&key)
    }

    /// 清空所有 Cookie
    pub fn clear(&self) {
        let mut cookies = self.cookies.write().unwrap();
        cookies.clear();
    }

    /// 获取 Cookie 数量
    pub fn len(&self) -> usize {
        let cookies = self.cookies.read().unwrap();
        cookies.len()
    }

    /// 检查是否为空
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// 清理过期 Cookie
    ///
    /// # 参数
    /// - `current_time`: 当前 Unix 时间戳
    pub fn clean_expired(&self, current_time: u64) {
        let mut cookies = self.cookies.write().unwrap();
        // retain 方法保留返回 true 的元素
        cookies.retain(|_, cookie| !cookie.is_expired(current_time));
    }

    /// 将指定域名的 Cookie 转换为请求头字符串
    /// 格式: "name1=value1; name2=value2"
    ///
    /// # 参数
    /// - `domain`: 域名
    pub fn to_header_string(&self, domain: &str) -> String {
        let cookies = self.get_by_domain(domain);
        if cookies.is_empty() {
            return String::new();
        }

        // 将所有 Cookie 拼接为 "name=value; name=value" 格式
        cookies
            .iter()
            .map(|c| c.to_header_string())
            .collect::<Vec<_>>()
            .join("; ")
    }

    /// 从 Set-Cookie 响应头解析 Cookie
    ///
    /// # 参数
    /// - `header_value`: Set-Cookie 头部的值
    /// - `default_domain`: 默认域名（如果 Cookie 未指定域名）
    ///
    /// # 返回
    /// 解析后的 Cookie 实例
    pub fn parse_set_cookie(header_value: &str, default_domain: &str) -> Option<Cookie> {
        // Set-Cookie 格式: name=value; Expires=...; Path=/; Domain=...; Secure; HttpOnly

        let mut parts = header_value.split(';');
        // 第一部分是 name=value
        let first_part = parts.next()?;
        let mut nv = first_part.split('=');
        let name = nv.next()?.trim();
        let value = nv.next()?.trim();

        if name.is_empty() {
            return None;
        }

        let mut cookie = Cookie::new(name, value)
            .with_domain(default_domain.to_string());

        // 解析属性
        for part in parts {
            let part = part.trim();
            let mut attr = part.split('=');
            let attr_name = attr.next()?.trim().to_lowercase();
            let attr_value = attr.next().map(|v| v.trim()).unwrap_or("");

            match attr_name.as_str() {
                "expires" => {
                    // 尝试解析过期时间（简化处理，实际应解析日期）
                    // 这里仅作为示例，实际实现需要日期解析
                }
                "path" => {
                    cookie.path = attr_value.to_string();
                }
                "domain" => {
                    cookie.domain = attr_value.to_string();
                }
                "secure" => {
                    cookie.secure = true;
                }
                "httponly" => {
                    cookie.http_only = true;
                }
                _ => {}
            }
        }

        Some(cookie)
    }
}

impl Default for CookieManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试 Cookie 基本操作
    #[test]
    fn test_cookie_basic() {
        let cm = CookieManager::new();

        // 添加 Cookie
        let cookie = Cookie::new("session_id", "abc123")
            .with_domain("example.com")
            .with_path("/")
            .with_secure(true)
            .with_http_only(true);
        cm.set(cookie);

        // 获取 Cookie
        let retrieved = cm.get("example.com", "session_id").unwrap();
        assert_eq!(retrieved.name, "session_id");
        assert_eq!(retrieved.value, "abc123");
        assert_eq!(retrieved.domain, "example.com");
        assert!(retrieved.secure);
        assert!(retrieved.http_only);

        // 获取不存在的 Cookie
        assert!(cm.get("example.com", "nonexistent").is_none());
    }

    /// 测试按域名获取 Cookie
    #[test]
    fn test_cookie_by_domain() {
        let cm = CookieManager::new();

        cm.set(Cookie::new("token1", "val1").with_domain("example.com"));
        cm.set(Cookie::new("token2", "val2").with_domain("example.com"));
        cm.set(Cookie::new("token3", "val3").with_domain("other.com"));

        let cookies = cm.get_by_domain("example.com");
        assert_eq!(cookies.len(), 2);

        let cookies = cm.get_by_domain("other.com");
        assert_eq!(cookies.len(), 1);
    }

    /// 测试 Cookie 头部字符串生成
    #[test]
    fn test_cookie_header_string() {
        let cm = CookieManager::new();
        cm.set(Cookie::new("name1", "value1").with_domain("example.com"));
        cm.set(Cookie::new("name2", "value2").with_domain("example.com"));

        let header = cm.to_header_string("example.com");
        // Cookie 顺序可能不同，检查包含关系
        assert!(header.contains("name1=value1"));
        assert!(header.contains("name2=value2"));
        assert!(header.contains("; "));
    }

    /// 测试 Cookie 过期检查
    #[test]
    fn test_cookie_expiration() {
        // 会话级 Cookie（expires=0）永不过期
        let session_cookie = Cookie::new("session", "val");
        assert!(!session_cookie.is_expired(9999999999));

        // 过期 Cookie
        let expired_cookie = Cookie::new("temp", "val").with_expires(1000);
        assert!(expired_cookie.is_expired(2000));
        assert!(!expired_cookie.is_expired(500));
    }

    /// 测试清理过期 Cookie
    #[test]
    fn test_clean_expired() {
        let cm = CookieManager::new();
        cm.set(Cookie::new("session", "val").with_domain("example.com")); // 会话级
        cm.set(Cookie::new("temp", "val").with_domain("example.com").with_expires(1000)); // 已过期

        assert_eq!(cm.len(), 2);

        // 清理过期 Cookie（当前时间 2000 > 过期时间 1000）
        cm.clean_expired(2000);

        assert_eq!(cm.len(), 1);
        assert!(cm.get("example.com", "session").is_some());
        assert!(cm.get("example.com", "temp").is_none());
    }

    /// 测试解析 Set-Cookie 头
    #[test]
    fn test_parse_set_cookie() {
        let header = "session_id=abc123; Path=/; Domain=example.com; Secure; HttpOnly";
        let cookie = CookieManager::parse_set_cookie(header, "default.com").unwrap();

        assert_eq!(cookie.name, "session_id");
        assert_eq!(cookie.value, "abc123");
        assert_eq!(cookie.path, "/");
        assert_eq!(cookie.domain, "example.com");
        assert!(cookie.secure);
        assert!(cookie.http_only);
    }
}
