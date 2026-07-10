// +----------------------------------------------------------------------+
// | XHCurl 扩展 - 单请求执行器（公共逻辑）                                |
// |                                                                        |
// | 抽取 multi.rs::execute_single 和 threadpool.rs::execute_request 的     |
// | 公共逻辑，消除约 130 行重复代码。                                      |
// |                                                                        |
// | 功能：                                                                 |
// | 1. 将 XhRequest 转为 reqwest 请求并发送                                |
// | 2. 流式读取响应体，带 max_response_size 限制（防内存溢出）             |
// | 3. 可选发送流式事件（Headers/Chunk/Complete）到 channel               |
// | 4. 构建 XhResponse 返回                                                |
// |                                                                        |
// | 流式 Chunk 事件：逐块发送（每收到一个 chunk 发一个事件），             |
// | 而非累积完整 body 后一次性发送。                                       |
// +----------------------------------------------------------------------+

use std::collections::HashMap;
use std::time::Instant;

use tokio::sync::mpsc;

use crate::error::{XhCurlError, XhCurlResult};
use crate::multi::StreamEvent;
use crate::request::XhRequest;
use crate::response::XhResponse;

/// 执行单个 HTTP 请求（核心公共逻辑）。
///
/// 供 `XhMulti::execute_single`、`XHThreadPool::execute_request`、
/// `fiber::execute_http_task` 共用，确保三处行为完全一致。
///
/// # 参数
/// - `client`: reqwest 客户端（共享连接池）
/// - `request`: 请求构建器
/// - `request_id`: 请求标识（用于流式事件关联）
/// - `stream_tx`: 可选的流式事件发送端（None 表示不启用流式回调）
/// - `max_response_size`: 响应体最大字节数（超过则截断并返回错误）
///
/// # 返回
/// - `Ok(XhResponse)`: 请求成功，包含完整响应
/// - `Err(XhCurlError)`: 请求失败（网络错误、响应体超限等）
///
/// # 流式事件完整性
/// 无论成功还是失败，只要 `stream_tx` 存在，都会发送一个终结事件：
/// - 成功 → `StreamEvent::Complete`
/// - 失败 → `StreamEvent::Error`
///
/// 这确保流式消费端不会因请求失败而永久等待终结信号。
pub async fn execute_request(
    client: reqwest::Client,
    request: XhRequest,
    request_id: String,
    stream_tx: Option<mpsc::Sender<(String, StreamEvent)>>,
    max_response_size: usize,
) -> XhCurlResult<XhResponse> {
    let start = Instant::now();

    // 将实际执行委托给 inner 函数，外层统一处理错误路径的流式事件。
    // 这样无论 inner 内部在哪一步失败（请求构建/发送/流式读取/超限），
    // 都能保证发送 StreamEvent::Error，避免消费端永久等待。
    let result = execute_request_inner(
        client,
        request,
        request_id.clone(),
        stream_tx.clone(),
        max_response_size,
        start,
    )
    .await;

    match result {
        Ok(resp) => Ok(resp),
        Err(e) => {
            // 错误路径：发送 Error 事件，确保流式消费端收到终结信号
            if let Some(tx) = &stream_tx {
                let _ = tx
                    .send((
                        request_id,
                        StreamEvent::Error {
                            message: e.to_string(),
                        },
                    ))
                    .await;
            }
            Err(e)
        }
    }
}

/// 实际执行单个 HTTP 请求的内部逻辑。
///
/// 由 `execute_request` 调用，错误时由外层负责发送 `StreamEvent::Error`。
async fn execute_request_inner(
    client: reqwest::Client,
    request: XhRequest,
    request_id: String,
    stream_tx: Option<mpsc::Sender<(String, StreamEvent)>>,
    max_response_size: usize,
    start: Instant,
) -> XhCurlResult<XhResponse> {
    // 将 XhRequest 转换为 reqwest 请求
    let req_builder = request.to_reqwest(&client)?;

    // 发送请求
    let response = req_builder.send().await?;

    // 获取状态码和响应头
    let status = response.status().as_u16();

    // 收集响应头
    // 注意：HeaderValue::to_str() 仅在值为可见 ASCII 时成功，遇到含非 ASCII
    // 字节的响应头（如带 UTF-8 文件名的 Content-Disposition）会整条丢弃。
    // 改用 from_utf8_lossy 保留这类响应头（仅替换真正无效的字节），
    // 对下载文件名解析等场景更友好。
    //
    // 重复头处理：HTTP 响应中同一头部名可能出现多次（如 Set-Cookie）。
    // 按 RFC 7230 §3.2.2，同名多值头可合并为逗号分隔的单个值。
    let mut headers_map: HashMap<String, String> = HashMap::new();
    for (name, value) in response.headers().iter() {
        let value_str = String::from_utf8_lossy(value.as_bytes()).into_owned();
        headers_map
            .entry(name.as_str().to_string())
            .and_modify(|existing| {
                existing.push_str(", ");
                existing.push_str(&value_str);
            })
            .or_insert(value_str);
    }

    // 如果启用了流式回调，发送 Headers 事件
    if let Some(tx) = &stream_tx {
        let _ = tx
            .send((
                request_id.clone(),
                StreamEvent::Headers {
                    status,
                    headers: headers_map.clone(),
                },
            ))
            .await;
    }

    // 获取远程地址
    let remote_addr = response.remote_addr().map(|addr| addr.to_string());

    // 获取 HTTP 版本
    let version = http_version_string(response.version());

    // 获取最终 URL（可能因重定向而变化）
    let final_url = response.url().to_string();

    // 流式读取响应体，带大小限制
    // 每收到一个 chunk 即发送流式事件（真正的流式语义）
    let mut body_data = Vec::new();
    let mut body_size: usize = 0;
    let mut size_exceeded = false;

    let mut stream = response;
    while let Some(chunk) = stream.chunk().await? {
        let chunk_len = chunk.len();

        // 使用 checked_add 防止整数溢出
        let new_size = body_size
            .checked_add(chunk_len)
            .ok_or_else(|| XhCurlError::Memory("响应体大小溢出".to_string()))?;

        // max_response_size == 0 表示无限制（与 GlobalConfig.max_response_size 注释一致）
        // 仅当 max_response_size > 0 时执行大小限制检查
        if max_response_size > 0 && new_size > max_response_size {
            // 超过限制：写入不超过 max_response_size 的部分后停止读取
            let remaining = max_response_size - body_size;
            if remaining > 0 {
                body_data.extend_from_slice(&chunk[..remaining]);
                // 发送最后一块的流式事件
                if let Some(tx) = &stream_tx {
                    let _ = tx
                        .send((
                            request_id.clone(),
                            StreamEvent::Chunk {
                                data: chunk[..remaining].to_vec(),
                            },
                        ))
                        .await;
                }
            }
            body_size = max_response_size;
            size_exceeded = true;
            break;
        }

        // 未超限：写入完整 chunk
        // 发送逐块流式事件（真正的流式语义，而非累积后一次性发送）
        if let Some(tx) = &stream_tx {
            let _ = tx
                .send((
                    request_id.clone(),
                    StreamEvent::Chunk {
                        data: chunk.to_vec(),
                    },
                ))
                .await;
        }
        body_data.extend_from_slice(&chunk);
        body_size = new_size;
    }

    // 计算耗时
    let elapsed = start.elapsed();

    // 如果响应体超过大小限制，返回错误
    if size_exceeded {
        return Err(XhCurlError::Memory(format!(
            "响应体超过最大限制 {} 字节",
            max_response_size
        )));
    }

    // 构建 XhResponse
    let xh_response = XhResponse::from_parts(
        status,
        final_url,
        headers_map,
        body_data,
        elapsed,
        remote_addr,
        version,
    );

    // 发送 Complete 事件
    if let Some(tx) = &stream_tx {
        let _ = tx
            .send((request_id, StreamEvent::Complete { elapsed, body_size }))
            .await;
    }

    Ok(xh_response)
}

/// 将 reqwest HTTP 版本枚举转为字符串
/// 统一此逻辑，避免在多处重复 match
pub fn http_version_string(version: reqwest::Version) -> Option<String> {
    match version {
        reqwest::Version::HTTP_09 => Some("HTTP/0.9".to_string()),
        reqwest::Version::HTTP_10 => Some("HTTP/1.0".to_string()),
        reqwest::Version::HTTP_11 => Some("HTTP/1.1".to_string()),
        reqwest::Version::HTTP_2 => Some("HTTP/2".to_string()),
        reqwest::Version::HTTP_3 => Some("HTTP/3".to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_http_version_string_http_09() {
        assert_eq!(
            http_version_string(reqwest::Version::HTTP_09),
            Some("HTTP/0.9".to_string())
        );
    }

    #[test]
    fn test_http_version_string_http_10() {
        assert_eq!(
            http_version_string(reqwest::Version::HTTP_10),
            Some("HTTP/1.0".to_string())
        );
    }

    #[test]
    fn test_http_version_string_http_11() {
        assert_eq!(
            http_version_string(reqwest::Version::HTTP_11),
            Some("HTTP/1.1".to_string())
        );
    }

    #[test]
    fn test_http_version_string_http_2() {
        assert_eq!(
            http_version_string(reqwest::Version::HTTP_2),
            Some("HTTP/2".to_string())
        );
    }

    #[test]
    fn test_http_version_string_http_3() {
        assert_eq!(
            http_version_string(reqwest::Version::HTTP_3),
            Some("HTTP/3".to_string())
        );
    }
}
