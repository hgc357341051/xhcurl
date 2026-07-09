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

/// 测试 Cookie 管理流程
#[test]
fn test_cookie_management_flow() {
    let cm = CookieManager::new();

    // 添加多个域名的 Cookie
    cm.set(
        Cookie::new("session_id", "abc123")
            .with_domain("example.com")
            .with_secure(true)
            .with_http_only(true),
    );

    cm.set(
        Cookie::new("token", "xyz789")
            .with_domain("api.example.com")
            .with_path("/v1"),
    );

    cm.set(
        Cookie::new("tracking", "track123")
            .with_domain("example.com")
            .with_expires(1000),
    ); // 已过期

    // 验证
    assert_eq!(cm.len(), 3);

    // 获取指定域名的 Cookie
    let cookies = cm.get_by_domain("example.com");
    assert_eq!(cookies.len(), 2);

    // 清理过期 Cookie
    cm.clean_expired(2000);
    assert_eq!(cm.len(), 2);

    // 生成请求头
    let header = cm.to_header_string("example.com");
    assert!(header.contains("session_id=abc123"));
}

/// 测试头部管理
#[test]
fn test_header_management() {
    let hm = HeaderManager::new();

    // 添加头部
    hm.set("Content-Type", "application/json").unwrap();
    hm.set("Authorization", "Bearer token").unwrap();
    hm.set("X-Request-ID", "req-12345").unwrap();

    // 验证大小写不敏感
    assert!(hm.has("content-type"));
    assert!(hm.has("CONTENT-TYPE"));
    assert_eq!(hm.get("content-type"), Some("application/json".to_string()));

    // 转换为 reqwest HeaderMap
    let header_map = hm.to_header_map();
    assert_eq!(header_map.len(), 3);
}

/// 测试缓冲区操作
#[test]
fn test_buffer_operations() {
    let mut buf = ResponseBuffer::new(4096, 1024); // 最大 1KB

    // 写入数据
    buf.write(b"Hello").unwrap();
    buf.write(b" World").unwrap();

    assert_eq!(buf.len(), 11);
    assert_eq!(buf.as_slice(), b"Hello World");

    // 分段读取
    assert_eq!(buf.chunk(0, 5), b"Hello");
    assert_eq!(buf.chunk(6, 5), b"World");

    // 测试大小限制
    let large_data = vec![b'x'; 2000];
    let result = buf.write(&large_data);
    assert!(result.is_err()); // 超过 1KB 限制
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
    multi.add(XhRequest::new("https://httpbin.org/get").id("req1"));
    multi.add(XhRequest::new("https://httpbin.org/get").id("req2"));
    multi.add(XhRequest::new("https://httpbin.org/get").id("req3"));

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
        enable_priority: true,
        max_response_size: 10 * 1024 * 1024,
    };

    let client = reqwest::Client::new();
    let pool = XhThreadPool::new(config, client);

    assert_eq!(pool.worker_count(), 8);
    assert!(!pool.is_running());
}
