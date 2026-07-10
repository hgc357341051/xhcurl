// +----------------------------------------------------------------------+
// | XHCurl 扩展 - 错误类型定义                                           |
// | 使用 thiserror 派生宏自动实现 Display 和 Error trait                 |
// | 所有错误都通过 Result 类型传播，库代码永不 panic                      |
// +----------------------------------------------------------------------+

use thiserror::Error;

// +----------------------------------------------------------------------+
// | 公共常量（供 multi / threadpool / php_ext / curl 共用）              |
// +----------------------------------------------------------------------+

/// 单次批量请求的最大数量限制
/// 防止用户传入过多请求导致内存溢出
pub const MAX_REQUESTS_PER_BATCH: usize = 10000;

/// 默认最大响应体大小（10MB）
/// 超过此大小的响应体会被截断并返回错误
pub const DEFAULT_MAX_RESPONSE_SIZE: usize = 10 * 1024 * 1024;

/// XHCurl 错误类型枚举
/// 每个变体代表一类错误，包含上下文信息便于调试
#[derive(Debug, Error)]
pub enum XhCurlError {
    /// 网络请求错误（DNS 解析失败、连接超时、SSL 握手失败等）
    /// 包装 reqwest 的错误类型，保留原始错误链
    #[error("网络请求失败: {0}")]
    Request(#[from] reqwest::Error),

    /// JSON 序列化/反序列化错误
    /// 包装 serde_json 的错误类型
    #[error("JSON 处理错误: {0}")]
    Json(#[from] serde_json::Error),

    /// 参数验证错误（超时值为负、线程数超出范围等）
    /// 包含错误描述信息
    #[error("参数错误: {0}")]
    InvalidArgument(String),

    /// 内存分配失败（缓冲区超过最大限制、系统内存不足）
    /// 包含请求的大小和限制
    #[error("内存错误: {0}")]
    Memory(String),

    /// 线程池错误（线程创建失败、线程池已关闭）
    /// 包含错误描述
    #[error("线程池错误: {0}")]
    ThreadPool(String),

    /// 通用错误（不匹配任何特定类别的错误）
    /// 使用 String 类型保留完整错误信息
    #[error("{0}")]
    Generic(String),
}

/// XHCurl 结果类型别名
/// 所有可能失败的操作都返回此类型
/// 使用 `?` 运算符可以自动转换错误类型
pub type XhCurlResult<T> = Result<T, XhCurlError>;

/// 将字符串转换为通用错误的便捷宏
/// 用于快速创建 Generic 错误变体
#[macro_export]
macro_rules! generic_error {
    // 支持格式化字符串参数
    ($($arg:tt)*) => {
        $crate::error::XhCurlError::Generic(format!($($arg)*))
    };
}

/// 将字符串转换为参数错误的便捷宏
/// 用于参数验证失败时快速创建错误
#[macro_export]
macro_rules! invalid_arg {
    // 支持格式化字符串参数
    ($($arg:tt)*) => {
        $crate::error::XhCurlError::InvalidArgument(format!($($arg)*))
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试错误类型的 Display 实现
    #[test]
    fn test_error_display() {
        // 测试参数错误
        let err = XhCurlError::InvalidArgument("超时不能为负".to_string());
        assert!(err.to_string().contains("参数错误"));
        assert!(err.to_string().contains("超时不能为负"));

        // 测试通用错误
        let err = XhCurlError::Generic("测试错误".to_string());
        assert_eq!(err.to_string(), "测试错误");
    }

    /// 测试错误宏
    #[test]
    fn test_error_macros() {
        // 测试 generic_error 宏
        let err: XhCurlError = generic_error!("值 {} 超出范围 {}", 10, 5);
        assert!(err.to_string().contains("值 10 超出范围 5"));

        // 测试 invalid_arg 宏
        let err: XhCurlError = invalid_arg!("参数 {} 无效", "timeout");
        assert!(err.to_string().contains("参数 timeout 无效"));
    }

    /// 测试错误链转换
    #[test]
    fn test_error_from() {
        // 测试从 serde_json::Error 转换
        let json_err = serde_json::from_str::<serde_json::Value>("invalid json");
        assert!(json_err.is_err());

        // 转换为 XhCurlError
        let xhcurl_err: XhCurlError = json_err.unwrap_err().into();
        assert!(matches!(xhcurl_err, XhCurlError::Json(_)));
    }
}
