// +----------------------------------------------------------------------+
// | XHCurl 扩展 - 全局管理器                                              |
// | 管理全局配置、共享状态、运行时初始化                                   |
// | 使用 OnceCell 实现线程安全的单例初始化                                 |
// | 使用 Arc<RwLock<>> 共享可变配置                                        |
// +----------------------------------------------------------------------+

use std::sync::{Arc, OnceLock, RwLock};
use std::time::Duration;

use crate::cookie::CookieManager;
use crate::error::{XhCurlError, XhCurlResult};

/// 全局配置结构体
/// 对应 C 版本的 xhcurl_globals 结构体
/// 存储所有全局可配置参数
#[derive(Debug, Clone)]
pub struct GlobalConfig {
    /// 全局默认连接超时（秒）
    /// 0 表示无超时
    pub connect_timeout: u64,

    /// 全局默认请求超时（秒）
    /// 0 表示无超时
    pub request_timeout: u64,

    /// 最大响应体大小（字节）
    /// 0 表示无限制
    /// 防止恶意服务器返回超大响应导致内存溢出
    pub max_response_size: usize,

    /// 是否启用重定向跟随
    pub follow_redirects: bool,

    /// 最大重定向次数
    /// 防止重定向循环
    pub max_redirects: u32,

    /// 是否验证 SSL 证书
    /// 生产环境应启用，开发环境可禁用
    pub verify_ssl: bool,

    /// 是否启用 HTTP/2
    pub http2_enabled: bool,

    /// 默认 User-Agent
    pub user_agent: String,

    /// 全局默认代理地址
    /// 格式: "http://host:port" 或 "socks5://host:port"
    pub proxy: Option<String>,

    /// 是否启用 TCP Keep-Alive
    pub tcp_keepalive: bool,

    /// TCP Keep-Alive 间隔（秒）
    pub tcp_keepalive_interval: u64,

    /// 默认并发连接数限制
    /// 对应 curl_multi 的 maxconnects
    pub max_connections: usize,

    /// 是否在 CLI 模式下使用多线程运行时
    /// FPM 模式始终使用单线程运行时
    pub use_multi_thread: bool,
}

impl Default for GlobalConfig {
    /// 默认配置
    /// 提供合理的默认值，兼顾安全性和性能
    fn default() -> Self {
        Self {
            // 连接超时 30 秒（对应 C 版本的默认 30s）
            connect_timeout: 30,
            // 请求超时 60 秒
            request_timeout: 60,
            // 最大响应体 10MB（防止内存溢出）
            max_response_size: 10 * 1024 * 1024,
            // 默认启用重定向跟随
            follow_redirects: true,
            // 最大重定向 10 次（防止循环）
            max_redirects: 10,
            // 默认验证 SSL 证书（安全优先）
            verify_ssl: true,
            // 默认启用 HTTP/2（性能优先）
            http2_enabled: true,
            // 默认 User-Agent
            user_agent: format!("XHCurl/{}", env!("CARGO_PKG_VERSION")),
            // 默认无代理
            proxy: None,
            // 默认启用 TCP Keep-Alive
            tcp_keepalive: true,
            // Keep-Alive 间隔 60 秒
            tcp_keepalive_interval: 60,
            // 默认最大连接数 100
            max_connections: 100,
            // CLI 模式默认使用多线程运行时
            use_multi_thread: true,
        }
    }
}

/// XHCurl 全局管理器
/// 单例模式，管理全局配置和共享状态
///
/// # 线程安全
/// - 使用 OnceLock 保证单例的线程安全初始化
/// - 使用 Arc<RwLock<>> 共享可变配置
/// - 对应 C 版本的 xhcurl_globals 全局变量
pub struct XhCurlManager {
    /// 全局配置（读写锁保护，允许多读单写）
    config: RwLock<GlobalConfig>,

    /// 全局 Cookie 管理器
    /// 使用 Arc 共享所有权，可在多个请求间共享
    pub cookie_manager: Arc<CookieManager>,

    /// 是否已初始化
    initialized: RwLock<bool>,
}

impl XhCurlManager {
    /// 创建新的管理器实例
    ///
    /// # 参数
    /// - `config`: 初始配置
    fn new(config: GlobalConfig) -> Self {
        Self {
            config: RwLock::new(config),
            cookie_manager: Arc::new(CookieManager::new()),
            initialized: RwLock::new(false),
        }
    }

    /// 获取全局单例实例
    ///
    /// 使用 OnceLock 保证只初始化一次
    /// 首次调用时使用默认配置初始化
    ///
    /// # 返回
    /// 全局管理器的引用
    pub fn global() -> &'static XhCurlManager {
        // OnceLock 保证线程安全的延迟初始化
        // 第一次调用时初始化，后续调用直接返回已有实例
        static INSTANCE: OnceLock<XhCurlManager> = OnceLock::new();

        INSTANCE.get_or_init(|| {
            // 使用默认配置初始化
            XhCurlManager::new(GlobalConfig::default())
        })
    }

    /// 获取配置的只读快照
    ///
    /// # 返回
    /// 当前配置的克隆（避免长时间持有锁）
    pub fn config(&self) -> GlobalConfig {
        let config = self.config.read().unwrap();
        config.clone()
    }

    /// 更新全局配置
    ///
    /// # 参数
    /// - `new_config`: 新的配置
    pub fn set_config(&self, new_config: GlobalConfig) {
        let mut config = self.config.write().unwrap();
        *config = new_config;
    }

    /// 修改配置（使用闭包，避免手动管理锁）
    ///
    /// # 参数
    /// - `f`: 配置修改闭包
    ///
    /// # 示例
    /// ```
    /// use xhcurl::XhCurlManager;
    /// let manager = XhCurlManager::global();
    /// manager.modify_config(|c| {
    ///     c.connect_timeout = 60;
    ///     c.verify_ssl = false;
    /// });
    /// ```
    pub fn modify_config<F>(&self, f: F)
    where
        F: FnOnce(&mut GlobalConfig),
    {
        let mut config = self.config.write().unwrap();
        f(&mut config);
    }

    /// 初始化管理器
    /// 在 PHP 扩展的 MINIT 阶段调用
    pub fn initialize(&self) -> XhCurlResult<()> {
        let mut initialized = self.initialized.write().unwrap();
        if *initialized {
            // 已经初始化，直接返回成功
            return Ok(());
        }

        // TODO: 在此初始化 tokio 运行时
        // 运行时在首次请求时按需创建

        *initialized = true;
        Ok(())
    }

    /// 清理管理器
    /// 在 PHP 扩展的 MSHUTDOWN 阶段调用
    pub fn shutdown(&self) {
        let mut initialized = self.initialized.write().unwrap();
        *initialized = false;

        // 清理 Cookie
        self.cookie_manager.clear();
    }

    /// 创建配置好的 reqwest 客户端构建器
    /// 根据全局配置设置客户端参数
    ///
    /// # 返回
    /// 配置好的 ClientBuilder，调用方可进一步自定义后构建
    pub fn create_client_builder(&self) -> reqwest::ClientBuilder {
        let config = self.config();

        // 创建客户端构建器
        let mut builder = reqwest::Client::builder()
            // 设置连接超时
            .connect_timeout(Duration::from_secs(config.connect_timeout))
            // 设置请求超时
            .timeout(Duration::from_secs(config.request_timeout))
            // 设置重定向策略
            .redirect(if config.follow_redirects {
                reqwest::redirect::Policy::limited(config.max_redirects as usize)
            } else {
                reqwest::redirect::Policy::none()
            })
            // SSL 证书验证
            .danger_accept_invalid_certs(!config.verify_ssl)
            // HTTP/2 支持：reqwest 默认协商升级 HTTP/2，无需显式配置
            // TCP Keep-Alive
            .tcp_keepalive(if config.tcp_keepalive {
                Some(Duration::from_secs(config.tcp_keepalive_interval))
            } else {
                None
            })
            // 连接池大小
            .pool_max_idle_per_host(config.max_connections)
            // 默认 User-Agent
            .user_agent(&config.user_agent);

        // 设置代理
        if let Some(proxy_url) = &config.proxy {
            if let Ok(proxy) = reqwest::Proxy::all(proxy_url) {
                builder = builder.proxy(proxy);
            }
        }

        builder
    }

    /// 创建共享的 reqwest 客户端
    /// 客户端内部管理连接池，应尽量复用
    ///
    /// # 返回
    /// 配置好的 reqwest::Client
    pub fn create_client(&self) -> XhCurlResult<reqwest::Client> {
        self.create_client_builder()
            .build()
            .map_err(XhCurlError::from)
    }

    /// 检查是否在 CLI 模式下运行
    /// 对应 C 版本的 xhcurl_is_cli_mode 函数
    ///
    /// # 返回
    /// - `true`: CLI 模式（可使用多线程运行时）
    /// - `false`: FPM/Web 模式（使用单线程运行时）
    pub fn is_cli_mode() -> bool {
        // 在 PHP 扩展中，需要检查 sapi_module.name
        // 这里通过 ext-php-rs 的 API 获取
        // 暂时返回 true，实际实现需要调用 PHP SAPI 检查
        // TODO: 通过 ext-php-rs 获取 PHP_SAPI 全局变量
        true
    }

    /// 创建建议的 tokio 运行时
    ///
    /// # 返回
    /// - CLI 模式: 多线程运行时（利用所有 CPU 核心）
    /// - FPM 模式: 单线程运行时（避免线程安全问题）
    pub fn create_runtime() -> XhCurlResult<tokio::runtime::Runtime> {
        if Self::is_cli_mode() {
            // CLI 模式：多线程运行时
            // 类似 goroutine 的 M:N 调度
            // tokio 的工作线程数默认等于 CPU 核心数
            tokio::runtime::Builder::new_multi_thread()
                .enable_all() // 启用 IO、时间、信号等
                .thread_name("xhcurl-worker") // 线程名称，便于调试
                .build()
                .map_err(|e| XhCurlError::Generic(format!("创建多线程运行时失败: {}", e)))
        } else {
            // FPM 模式：单线程运行时
            // 避免多线程与 PHP 内存管理器冲突
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|e| XhCurlError::Generic(format!("创建单线程运行时失败: {}", e)))
        }
    }
}

impl std::fmt::Debug for XhCurlManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let config = self.config.read().unwrap();
        let initialized = self.initialized.read().unwrap();
        f.debug_struct("XhCurlManager")
            .field("config", &*config)
            .field("initialized", &*initialized)
            .field("cookie_count", &self.cookie_manager.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试默认配置
    #[test]
    fn test_default_config() {
        let config = GlobalConfig::default();
        assert_eq!(config.connect_timeout, 30);
        assert_eq!(config.request_timeout, 60);
        assert_eq!(config.max_response_size, 10 * 1024 * 1024);
        assert!(config.follow_redirects);
        assert!(config.verify_ssl);
        assert!(config.http2_enabled);
        assert!(config.tcp_keepalive);
    }

    /// 测试配置修改
    #[test]
    fn test_modify_config() {
        let manager = XhCurlManager::new(GlobalConfig::default());

        // 修改配置
        manager.modify_config(|c| {
            c.connect_timeout = 60;
            c.verify_ssl = false;
        });

        // 验证修改
        let config = manager.config();
        assert_eq!(config.connect_timeout, 60);
        assert!(!config.verify_ssl);
    }

    /// 测试全局单例
    #[test]
    fn test_global_singleton() {
        // 获取全局实例
        let manager1 = XhCurlManager::global();
        let manager2 = XhCurlManager::global();

        // 验证是同一个实例（指针相等）
        assert!(std::ptr::eq(manager1, manager2));
    }

    /// 测试客户端构建
    #[test]
    fn test_create_client() {
        let manager = XhCurlManager::new(GlobalConfig::default());
        let client = manager.create_client();
        assert!(client.is_ok());
    }

    /// 测试初始化和关闭
    #[test]
    fn test_initialize_shutdown() {
        let manager = XhCurlManager::new(GlobalConfig::default());

        // 初始化
        assert!(manager.initialize().is_ok());
        assert!(manager.initialize().is_ok()); // 重复初始化不报错

        // 关闭
        manager.shutdown();

        // 重新初始化
        assert!(manager.initialize().is_ok());
    }
}
