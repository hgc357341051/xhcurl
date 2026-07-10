// +----------------------------------------------------------------------+
// | XHCurl 扩展 - 集成测试                                                |
// | 测试核心功能的端到端流程                                               |
// +----------------------------------------------------------------------+

use xhcurl::*;

/// 测试完整的请求构建流程
#[test]
fn test_request_build_flow() {
    // 构建一个复杂的请求
    let request = XhRequest::new("https://httpbin.org/post")
        .post()
        .header("Authorization", "Bearer token123")
        .header("Accept", "application/json")
        .header("X-Custom", "custom-value")
        .body_json_str(r#"{"key": "value"}"#)
        .unwrap()
        .request_timeout(30)
        .verify_ssl(true)
        .id("test-request-001");

    // 验证请求配置
    assert_eq!(request.get_url(), "https://httpbin.org/post");
    assert_eq!(request.get_method(), HttpMethod::Post);
    assert_eq!(request.get_id(), Some("test-request-001"));
    assert!(request.get_headers().has("authorization"));
    assert!(request.get_headers().has("accept"));
    assert!(request.get_headers().has("x-custom"));
    assert!(request.get_headers().has("content-type"));
}

/// 测试全局管理器配置
#[test]
fn test_global_manager_config() {
    let manager = XhCurlManager::global();

    // 修改配置
    manager.modify_config(|c| {
        c.connect_timeout = 60;
        c.request_timeout = 120;
        c.max_response_size = 20 * 1024 * 1024;
        c.follow_redirects = false;
        c.verify_ssl = false;
    });

    // 验证配置
    let config = manager.config();
    assert_eq!(config.connect_timeout, 60);
    assert_eq!(config.request_timeout, 120);
    assert_eq!(config.max_response_size, 20 * 1024 * 1024);
    assert!(!config.follow_redirects);
    assert!(!config.verify_ssl);

    // 恢复默认配置
    manager.set_config(GlobalConfig::default());
}

/// 测试头部管理
#[test]
fn test_header_management() {
    let hm = HeaderManager::new();

    // 添加头部
    hm.set("Content-Type", "application/json");
    hm.set("Authorization", "Bearer token");
    hm.set("X-Request-ID", "req-12345");

    // 验证大小写不敏感
    assert!(hm.has("content-type"));
    assert!(hm.has("CONTENT-TYPE"));
    assert_eq!(hm.get("content-type"), Some("application/json".to_string()));

    // 转换为 reqwest HeaderMap
    let header_map = hm.to_header_map();
    assert_eq!(header_map.len(), 3);
}

/// 测试错误处理
#[test]
fn test_error_handling() {
    // 测试错误转换
    let json_err = serde_json::from_str::<serde_json::Value>("invalid");
    assert!(json_err.is_err());

    let xhcurl_err: XhCurlError = json_err.unwrap_err().into();
    assert!(matches!(xhcurl_err, XhCurlError::Json(_)));

    // 测试错误宏
    let err: XhCurlError = generic_error!("测试错误: {}", "参数无效");
    assert!(err.to_string().contains("测试错误"));

    let err: XhCurlError = invalid_arg!("参数 {} 超出范围 {}", 10, 5);
    assert!(err.to_string().contains("参数 10 超出范围 5"));
}

/// 测试 HTTP 方法
#[test]
fn test_http_methods() {
    // 测试方法解析
    assert_eq!(HttpMethod::from_str("GET").unwrap(), HttpMethod::Get);
    assert_eq!(HttpMethod::from_str("post").unwrap(), HttpMethod::Post);
    assert_eq!(HttpMethod::from_str("DELETE").unwrap(), HttpMethod::Delete);
    assert!(HttpMethod::from_str("INVALID").is_err());

    // 测试方法字符串
    assert_eq!(HttpMethod::Get.to_string(), "GET");
    assert_eq!(HttpMethod::Post.to_string(), "POST");
}

/// 测试批量执行器创建
#[test]
fn test_multi_executor() {
    let client = reqwest::Client::new();
    let mut multi = XhMulti::new(client).max_concurrency(10).timeout(30);

    // 添加请求
    let _ = multi.add(XhRequest::new("https://httpbin.org/get").id("req1"));
    let _ = multi.add(XhRequest::new("https://httpbin.org/get").id("req2"));
    let _ = multi.add(XhRequest::new("https://httpbin.org/get").id("req3"));

    assert_eq!(multi.len(), 3);

    // 启用流式回调
    let _rx = multi.enable_streaming();
}

/// 测试线程池配置
#[test]
fn test_threadpool_config() {
    let config = ThreadPoolConfig {
        worker_count: 8,
        queue_capacity: 500,
        idle_timeout: 30,
        max_response_size: 10 * 1024 * 1024,
    };

    let client = reqwest::Client::new();
    let pool = XhThreadPool::new(config, client);

    assert_eq!(pool.worker_count(), 8);
    assert!(!pool.is_running());
}
