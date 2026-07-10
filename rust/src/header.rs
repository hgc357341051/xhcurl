// +----------------------------------------------------------------------+
// | XHCurl 扩展 - HTTP 头部管理                                          |
// | 使用 HashMap 存储头部，支持大小写不敏感查找                           |
// | 使用 Arc<RwLock<>> 实现多线程安全共享（读多写少场景）                 |
// +----------------------------------------------------------------------+

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// HTTP 头部管理器
/// 线程安全的头部存储，支持大小写不敏感的键查找
///
/// # 线程安全
/// 使用 Arc<RwLock<HashMap>> 实现多线程共享：
/// - RwLock 允许多个读操作并行，写操作独占
/// - Arc 允许跨线程共享所有权
/// - 对应 C 版本的 xhcurl_header_t 结构体
#[derive(Debug, Clone)]
pub struct HeaderManager {
    /// 头部存储（内部可变性通过 RwLock 实现）
    /// 使用 Arc 包装以便跨线程共享
    ///
    /// 注意：HTTP/2 要求头部名称小写，我们在插入时统一转为小写
    headers: Arc<RwLock<HashMap<String, String>>>,
}

impl HeaderManager {
    /// 创建新的头部管理器
    ///
    /// # 返回
    /// 空的头部管理器实例
    pub fn new() -> Self {
        Self {
            headers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 添加头部（如果键已存在则覆盖）
    ///
    /// # 参数
    /// - `name`: 头部名称（自动转为小写，符合 HTTP/2 规范）
    /// - `value`: 头部值
    pub fn set(&self, name: &str, value: &str) {
        // 获取写锁（独占访问）
        // unwrap 安全：只有当 RwLock 被 poison（持有者 panic）时才会失败
        let mut headers = self.headers.write().unwrap();

        // 将键转为小写（HTTP/2 要求头部名称小写）
        // 值保持原样
        headers.insert(name.to_lowercase(), value.to_string());
    }

    /// 批量添加头部
    ///
    /// # 参数
    /// - `pairs`: 头部键值对迭代器
    pub fn set_many<I, S>(&self, pairs: I)
    where
        I: IntoIterator<Item = (S, S)>,
        S: AsRef<str>,
    {
        let mut headers = self.headers.write().unwrap();
        for (name, value) in pairs {
            headers.insert(name.as_ref().to_lowercase(), value.as_ref().to_string());
        }
    }

    /// 获取头部值（大小写不敏感）
    ///
    /// # 参数
    /// - `name`: 头部名称
    ///
    /// # 返回
    /// - `Some(value)`: 头部存在
    /// - `None`: 头部不存在
    pub fn get(&self, name: &str) -> Option<String> {
        // 获取读锁（共享访问，多个读操作可以并行）
        let headers = self.headers.read().ok()?;
        // 查找时也转为小写，实现大小写不敏感
        headers.get(&name.to_lowercase()).cloned()
    }

    /// 检查头部是否存在
    ///
    /// # 参数
    /// - `name`: 头部名称
    ///
    /// # 返回
    /// - `true`: 头部存在
    /// - `false`: 头部不存在
    pub fn has(&self, name: &str) -> bool {
        let headers = self.headers.read().unwrap();
        headers.contains_key(&name.to_lowercase())
    }

    /// 移除头部
    ///
    /// # 参数
    /// - `name`: 要移除的头部名称
    ///
    /// # 返回
    /// - `Some(value)`: 被移除的头部值
    /// - `None`: 头部不存在
    pub fn remove(&self, name: &str) -> Option<String> {
        let mut headers = self.headers.write().unwrap();
        headers.remove(&name.to_lowercase())
    }

    /// 获取所有头部（克隆所有数据）
    ///
    /// # 返回
    /// 包含所有头部的 HashMap
    pub fn all(&self) -> HashMap<String, String> {
        let headers = self.headers.read().unwrap();
        headers.clone()
    }

    /// 清空所有头部
    pub fn clear(&self) {
        let mut headers = self.headers.write().unwrap();
        headers.clear();
    }

    /// 获取头部数量
    pub fn len(&self) -> usize {
        let headers = self.headers.read().unwrap();
        headers.len()
    }

    /// 检查是否为空
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// 转换为 reqwest 的 HeaderMap 格式
    /// 用于实际发送 HTTP 请求时使用
    ///
    /// # 返回
    /// reqwest::header::HeaderMap 实例
    pub fn to_header_map(&self) -> reqwest::header::HeaderMap {
        let headers = self.headers.read().unwrap();
        let mut map = reqwest::header::HeaderMap::new();

        for (name, value) in headers.iter() {
            // 解析头部名称（处理无效字符）
            if let Ok(header_name) = reqwest::header::HeaderName::from_bytes(name.as_bytes()) {
                // 解析头部值
                if let Ok(header_value) = reqwest::header::HeaderValue::from_str(value) {
                    map.insert(header_name, header_value);
                }
            }
        }

        map
    }
}

/// 为 HeaderManager 实现默认 trait
/// 允许使用 HeaderManager::default() 创建实例
impl Default for HeaderManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试头部的基本增删改查
    #[test]
    fn test_header_basic_operations() {
        let hm = HeaderManager::new();

        // 添加头部
        hm.set("Content-Type", "application/json");
        hm.set("Authorization", "Bearer token123");

        // 验证存在性
        assert!(hm.has("Content-Type"));
        assert!(hm.has("content-type")); // 大小写不敏感
        assert!(!hm.has("X-Custom"));

        // 获取值
        assert_eq!(hm.get("Content-Type"), Some("application/json".to_string()));
        assert_eq!(hm.get("CONTENT-TYPE"), Some("application/json".to_string())); // 大小写不敏感
        assert_eq!(hm.get("X-Custom"), None);

        // 验证数量
        assert_eq!(hm.len(), 2);

        // 移除头部
        let removed = hm.remove("Content-Type");
        assert_eq!(removed, Some("application/json".to_string()));
        assert!(!hm.has("Content-Type"));
        assert_eq!(hm.len(), 1);
    }

    /// 测试批量添加头部
    #[test]
    fn test_header_set_many() {
        let hm = HeaderManager::new();
        let pairs = vec![
            ("Accept", "application/json"),
            ("Accept-Encoding", "gzip"),
            ("User-Agent", "XHCurl/2.0"),
        ];
        hm.set_many(pairs);

        assert_eq!(hm.len(), 3);
        assert_eq!(hm.get("accept"), Some("application/json".to_string()));
    }

    /// 测试多线程安全
    #[test]
    fn test_header_thread_safety() {
        use std::thread;

        let hm = Arc::new(HeaderManager::new());
        let mut handles = vec![];

        // 启动 10 个线程并发写入
        for i in 0..10 {
            let hm_clone = Arc::clone(&hm);
            let handle = thread::spawn(move || {
                hm_clone.set(&format!("X-Thread-{}", i), &format!("value-{}", i));
            });
            handles.push(handle);
        }

        // 等待所有线程完成
        for handle in handles {
            handle.join().unwrap();
        }

        // 验证所有头部都已写入
        assert_eq!(hm.len(), 10);
        for i in 0..10 {
            assert_eq!(
                hm.get(&format!("x-thread-{}", i)),
                Some(format!("value-{}", i))
            );
        }
    }
}
