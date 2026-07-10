// +----------------------------------------------------------------------+
// | XHCurl 扩展 - executor::execute_request 异步集成测试                  |
// |                                                                      |
// | 使用进程内 TcpListener 模拟 HTTP 服务器，零外部依赖（无需 httpbin）。 |
// | 覆盖 executor.rs 中此前完全未测的异步路径：                            |
// |   1. 成功请求 → 状态码/响应体正确                                    |
// |   2. max_response_size 截断 → 返回 Memory 错误                       |
// |   3. 流式事件序列 → Headers → Chunk → Complete                       |
// +----------------------------------------------------------------------+

use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use xhcurl::executor::execute_request;
use xhcurl::{StreamEvent, XhRequest};

/// 启动一个一次性的进程内 HTTP 服务器，返回 (地址, 句柄)。
///
/// 接受一个连接，读取并丢弃请求行/头部，回复固定状态码 + 响应体。
/// `chunked` 控制是否用 Transfer-Encoding: chunked 分块发送（测试流式 chunk 路径）。
async fn start_test_server(
    status: u16,
    body: Vec<u8>,
    chunked: bool,
) -> (std::net::SocketAddr, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let handle = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();

        // 读取并丢弃 HTTP 请求（读到空行或超时即可）
        let mut buf = vec![0u8; 4096];
        let _ = tokio::time::timeout(Duration::from_secs(1), stream.read(&mut buf)).await;

        // 构造响应
        let resp = if chunked {
            format!(
                "HTTP/1.1 {} OK\r\nTransfer-Encoding: chunked\r\nContent-Type: text/plain\r\n\r\n",
                status
            )
        } else {
            format!(
                "HTTP/1.1 {} OK\r\nContent-Length: {}\r\nContent-Type: text/plain\r\n\r\n",
                status,
                body.len()
            )
        };
        stream.write_all(resp.as_bytes()).await.unwrap();

        if chunked {
            // 分块发送：每块 <hex-size>\r\n<data>\r\n，末尾 0\r\n\r\n
            let mut start = 0;
            let chunk_size = 16; // 小块，触发多个 Chunk 事件
            while start < body.len() {
                let end = (start + chunk_size).min(body.len());
                let size_line = format!("{:X}\r\n", end - start);
                stream.write_all(size_line.as_bytes()).await.unwrap();
                stream.write_all(&body[start..end]).await.unwrap();
                stream.write_all(b"\r\n").await.unwrap();
                start = end;
            }
            stream.write_all(b"0\r\n\r\n").await.unwrap();
        } else {
            stream.write_all(&body).await.unwrap();
        }
        let _ = stream.flush().await;
    });

    (addr, handle)
}

/// execute_request 成功路径：返回正确状态码与响应体
#[tokio::test]
async fn test_execute_request_success() {
    let body = b"hello xhcurl".to_vec();
    let (addr, _handle) = start_test_server(200, body.clone(), false).await;

    let client = reqwest::Client::new();
    let request = XhRequest::new(format!("http://{}/", addr)).get();
    let response = execute_request(client, request, "req-1".to_string(), None, 10 * 1024 * 1024)
        .await
        .expect("请求应成功");

    assert_eq!(response.status(), 200);
    assert!(response.is_success());
    assert_eq!(response.body_text().unwrap(), "hello xhcurl");
}

/// execute_request max_response_size 截断：响应体超过限制应返回 Memory 错误
#[tokio::test]
async fn test_execute_request_size_exceeded() {
    // 100 字节响应体，限制 5 字节
    let body = vec![b'A'; 100];
    let (addr, _handle) = start_test_server(200, body.clone(), false).await;

    let client = reqwest::Client::new();
    let request = XhRequest::new(format!("http://{}/", addr)).get();
    let result = execute_request(client, request, "req-2".to_string(), None, 5).await;

    assert!(result.is_err(), "响应体超过 max_response_size 应返回错误");
    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("最大限制"),
        "错误信息应提及大小限制，实际: {}",
        err
    );
}

/// execute_request 流式事件序列：Headers → Chunk(s) → Complete
#[tokio::test]
async fn test_execute_request_streaming_events() {
    let body = vec![b'X'; 40]; // 40 字节，chunked 16/16/8 → 3 个 Chunk 事件
    let (addr, _handle) = start_test_server(200, body.clone(), true).await;

    let (tx, mut rx) = mpsc::channel::<(String, StreamEvent)>(64);
    let client = reqwest::Client::new();
    let request = XhRequest::new(format!("http://{}/", addr)).get();

    let response = execute_request(
        client,
        request,
        "req-3".to_string(),
        Some(tx),
        10 * 1024 * 1024,
    )
    .await
    .expect("流式请求应成功");

    // 响应体应完整
    assert_eq!(response.status(), 200);
    assert_eq!(response.body_text().unwrap().len(), 40);

    // 收集所有流式事件
    let mut events = Vec::new();
    while let Ok(Some((id, e))) = tokio::time::timeout(Duration::from_secs(2), rx.recv()).await {
        events.push((id, e));
    }

    // 验证事件序列
    let kinds: Vec<&str> = events
        .iter()
        .map(|(id, e)| {
            assert_eq!(id, "req-3", "事件 request_id 应一致");
            match e {
                StreamEvent::Headers { .. } => "Headers",
                StreamEvent::Chunk { .. } => "Chunk",
                StreamEvent::Complete { .. } => "Complete",
                StreamEvent::Error { .. } => "Error",
            }
        })
        .collect();

    // 必须以 Headers 开头、Complete 结尾，中间至少一个 Chunk
    assert!(
        kinds.first().is_some_and(|&k| k == "Headers"),
        "首个事件应为 Headers: {:?}",
        kinds
    );
    assert!(
        kinds.last().is_some_and(|&k| k == "Complete"),
        "末个事件应为 Complete: {:?}",
        kinds
    );
    assert!(
        kinds.contains(&"Chunk"),
        "应至少有一个 Chunk 事件: {:?}",
        kinds
    );
    assert!(!kinds.contains(&"Error"), "不应有 Error 事件: {:?}", kinds);
}

/// execute_request 错误路径的流式事件：请求失败时应发送 Error 事件
#[tokio::test]
async fn test_execute_request_error_sends_error_event() {
    let (addr, _handle) = start_test_server(200, b"ok".to_vec(), false).await;

    // 使用无效的 max_response_size=0 触发截断错误路径
    // （DEFAULT 归一化只在 XhMulti setter 层，execute_request 直接受 0）
    // 此处改用连接到已关闭端口触发网络错误
    let client = reqwest::Client::new();
    // 绑定一个立即关闭的端口，保证连接失败
    let drop_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let closed_addr = drop_listener.local_addr().unwrap();
    drop(drop_listener);

    let (tx, mut rx) = mpsc::channel::<(String, StreamEvent)>(16);
    let request = XhRequest::new(format!("http://{}", closed_addr)).get();
    let _ = addr; // 抑制未使用警告

    let result = execute_request(
        client,
        request,
        "req-err".to_string(),
        Some(tx),
        10 * 1024 * 1024,
    )
    .await;
    assert!(result.is_err(), "连接已关闭端口应失败");

    // 应收到 Error 事件
    let mut got_error = false;
    while let Ok(Some((_, e))) = tokio::time::timeout(Duration::from_secs(1), rx.recv()).await {
        if let StreamEvent::Error { .. } = e {
            got_error = true;
            break;
        }
    }
    assert!(got_error, "失败路径应发送 StreamEvent::Error");
}
