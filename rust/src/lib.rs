// ext-php-rs 在 Windows 上需要 abi_vectorcall 调用约定来生成 PHP 可调用的导出函数。
// 该调用约定是 nightly-only 特性，因此 Windows 构建必须使用 Rust nightly。
// 参考：https://docs.rs/ext-php-rs（官方示例同样在 crate 根添加此属性）
#![cfg_attr(all(windows, feature = "php"), feature(abi_vectorcall))]

// +----------------------------------------------------------------------+
// | XHCurl 扩展 - Rust 核心引擎入口                                       |
// |                                                                        |
// | 项目结构：                                                             |
// |   lib.rs       - 库入口，声明所有模块                                  |
// |   error.rs     - 错误类型定义                                          |
// |   buffer.rs    - 响应缓冲区                                            |
// |   header.rs    - HTTP 头部管理                                         |
// |   cookie.rs    - Cookie 管理                                           |
// |   curl.rs      - 全局管理器                                            |
// |   request.rs   - 请求构建器                                            |
// |   response.rs  - 响应对象                                              |
// |   multi.rs     - 异步批量执行器（tokio M:N 调度）                      |
// |   threadpool.rs - 线程池（channel 通信）                               |
// |   php_ext.rs   - PHP 扩展入口（ext-php-rs 绑定）                       |
// |                                                                        |
// | 架构优势：                                                             |
// |   1. 真正的 M:N 异步调度（类似 Golang goroutine）                      |
// |   2. 编译期线程安全（Send + Sync bounds）                              |
// |   3. 内存安全（所有权模型，无 double-free/use-after-free）             |
// |   4. 零成本抽象（泛型单态化，无运行时开销）                             |
// |   5. 统一 FPM/CLI 模式（tokio 自动适配）                                |
// +----------------------------------------------------------------------+

// 声明所有模块
// 模块顺序：基础模块在前，依赖它们的模块在后

/// 错误处理模块
/// 定义 XhCurlError 枚举和 XhCurlResult 类型别名
/// 使用 thiserror 派生宏自动实现 Display 和 Error trait
pub mod error;

/// 响应缓冲区模块
/// 提供线程安全的响应体存储，支持大小限制和分段读取
pub mod buffer;

/// HTTP 头部管理模块
/// 使用 HashMap 存储头部，支持大小写不敏感查找
pub mod header;

/// Cookie 管理模块
/// 支持会话级和持久化 Cookie，线程安全
pub mod cookie;

/// 全局管理器模块
/// 单例模式管理全局配置和共享状态
pub mod curl;

/// 请求构建器模块
/// 使用 Builder 模式链式构建 HTTP 请求
pub mod request;

/// 响应对象模块
/// 懒加载设计，响应体按需读取
pub mod response;

/// 异步批量执行器模块
/// 基于 tokio 的 M:N 调度，实现类似 goroutine 的并发
pub mod multi;

/// 线程池模块
/// 基于 tokio + channel 实现真正的多线程并发
pub mod threadpool;

/// 单请求执行器（公共逻辑）
/// 抽取 multi/threadpool 共用的请求执行逻辑，消除重复代码
pub mod executor;

/// PHP Fiber 协程桥接模块
/// 实现 PHP 端协程式 await 异步 HTTP 请求
#[cfg(feature = "php")]
pub mod fiber;

/// PHP 扩展入口模块
/// 使用 ext-php-rs 直接生成 PHP 扩展
#[cfg(feature = "php")]
pub mod php_ext;

// 导出常用类型，简化外部使用
// 使用 pub use 重导出，提供简洁的 API
pub use buffer::ResponseBuffer;
pub use cookie::{Cookie, CookieManager};
pub use curl::{GlobalConfig, XhCurlManager};
pub use error::{XhCurlError, XhCurlResult};
pub use header::HeaderManager;
pub use multi::{RequestResult, StreamEvent, XhMulti};
pub use request::{BodyType, HttpMethod, MultipartField, XhRequest};
pub use response::XhResponse;
pub use threadpool::{ResultMessage, TaskMessage, ThreadPoolConfig, XhThreadPool};

// +----------------------------------------------------------------------+
// | 库级别测试                                                            |
// +----------------------------------------------------------------------+

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试模块导出
    #[test]
    fn test_module_exports() {
        // 验证所有主要类型都可以被访问
        let _error: XhCurlError = XhCurlError::Generic("test".to_string());
        let _buffer: ResponseBuffer = ResponseBuffer::new(1024, 0);
        let _header: HeaderManager = HeaderManager::new();
        let _cookie: CookieManager = CookieManager::new();
        let _config: GlobalConfig = GlobalConfig::default();
        let _request: XhRequest = XhRequest::new("https://example.com");
        let _method: HttpMethod = HttpMethod::Get;
    }

    /// 测试错误宏
    #[test]
    fn test_error_macros() {
        let err: XhCurlError = generic_error!("测试错误 {}", 123);
        assert!(err.to_string().contains("测试错误 123"));

        let err: XhCurlError = invalid_arg!("参数无效");
        assert!(err.to_string().contains("参数无效"));
    }
}
