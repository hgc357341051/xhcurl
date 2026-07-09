// +----------------------------------------------------------------------+
// | XHCurl 扩展 - 线程池（XHThreadPool）                                   |
// | 基于 tokio + crossbeam-channel 实现真正的多线程并发                    |
// |                                                                        |
// | 核心改进（对比 C 版本）：                                              |
// | 1. 工作线程通过 channel 与主线程通信，解决"不能调用 PHP 函数"限制      |
// | 2. 使用 tokio 运行时，自动管理工作线程生命周期                         |
// | 3. 支持任务优先级调度                                                  |
// | 4. 编译期线程安全：Send + Sync bounds                                  |
// | 5. 统一 FPM/CLI 模式：FPM 使用单线程运行时，CLI 使用多线程运行时       |
// |                                                                        |
// | 工作模型：                                                             |
// |   主线程 (PHP) ──发送任务──> [任务队列] ──> 工作线程 1                 |
// |                                          ──> 工作线程 2                 |
// |                                          ──> 工作线程 N                 |
// |   主线程 (PHP) <──结果回调── [结果队列] <── 工作线程                    |
// +----------------------------------------------------------------------+

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::error::{XhCurlError, XhCurlResult};
use crate::multi::{RequestResult, StreamEvent};
use crate::request::XhRequest;
use crate::response::XhResponse;

/// 线程池任务消息
/// 主线程通过此消息类型向工作线程发送任务
#[derive(Debug)]
pub enum TaskMessage {
    /// 执行单个请求
    Request {
        /// 请求构建器
        request: XhRequest,
        /// reqwest 客户端（共享连接池）
        client: reqwest::Client,
        /// 流式回调发送端（可选）
        stream_tx: Option<mpsc::Sender<(String, StreamEvent)>>,
    },

    /// 关闭工作线程
    Shutdown,
}

/// 线程池结果消息
/// 工作线程通过此消息类型向主线程发送结果
#[derive(Debug)]
pub enum ResultMessage {
    /// 请求完成
    Completed(RequestResult),

    /// 工作线程已关闭
    WorkerShutdown,
}

/// 线程池配置
#[derive(Debug, Clone)]
pub struct ThreadPoolConfig {
    /// 工作线程数量
    /// 默认: CPU 核心数
    pub worker_count: usize,

    /// 任务队列容量
    /// 0 = 无界队列
    pub queue_capacity: usize,

    /// 空闲线程超时（秒）
    /// 超过此时间无任务则关闭线程
    /// 0 = 永不超时
    pub idle_timeout: u64,

    /// 是否启用任务优先级
    pub enable_priority: bool,

    /// 最大响应体大小（字节）
    /// 防止恶意服务器返回超大响应导致内存溢出
    /// 0 = 使用默认值 10MB
    pub max_response_size: usize,
}

/// 默认最大响应体大小：10MB
const DEFAULT_MAX_RESPONSE_SIZE: usize = 10 * 1024 * 1024;

impl Default for ThreadPoolConfig {
    fn default() -> Self {
        // 获取 CPU 核心数
        let cpu_count = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4);

        Self {
            worker_count: cpu_count,
            queue_capacity: 1000, // 有界队列防止内存溢出
            idle_timeout: 60,     // 60 秒空闲超时
            enable_priority: true,
            max_response_size: DEFAULT_MAX_RESPONSE_SIZE,
        }
    }
}

/// 线程池
/// 管理多个工作线程，并发执行 HTTP 请求
///
/// # 线程安全
/// - 线程池本身通过 Arc 共享
/// - 任务队列和结果队列使用 mpsc channel（线程安全）
/// - 工作线程间无共享状态，通过 channel 通信
///
/// # 对比 C 版本
/// - C 版本：手动创建 pthread，工作线程不能调用 PHP 函数
/// - Rust 版本：tokio 管理，通过 channel 回调，支持流式
pub struct XhThreadPool {
    /// 线程池配置
    config: ThreadPoolConfig,

    /// 任务发送端（主线程 → 工作线程）
    task_tx: Option<mpsc::Sender<TaskMessage>>,

    /// 结果接收端（工作线程 → 主线程）
    result_rx: Option<mpsc::Receiver<ResultMessage>>,

    /// 工作线程句柄
    workers: Vec<JoinHandle<()>>,

    /// reqwest 客户端（共享连接池）
    client: reqwest::Client,

    /// 是否已启动
    is_running: bool,
}

impl XhThreadPool {
    /// 创建新的线程池
    ///
    /// # 参数
    /// - `config`: 线程池配置
    /// - `client`: reqwest 客户端
    pub fn new(config: ThreadPoolConfig, client: reqwest::Client) -> Self {
        Self {
            config,
            task_tx: None,
            result_rx: None,
            workers: Vec::new(),
            client,
            is_running: false,
        }
    }

    /// 使用默认配置创建
    pub fn with_default_config(client: reqwest::Client) -> Self {
        Self::new(ThreadPoolConfig::default(), client)
    }

    /// 启动线程池
    /// 创建工作线程，开始接收任务
    pub fn start(&mut self) -> XhCurlResult<()> {
        if self.is_running {
            return Ok(()); // 已经启动
        }

        // 创建任务 channel（有界队列防止内存溢出）
        let (task_tx, task_rx) = mpsc::channel(if self.config.queue_capacity > 0 {
            self.config.queue_capacity
        } else {
            usize::MAX // 无界
        });

        // 创建结果 channel
        let (result_tx, result_rx) = mpsc::channel(self.config.queue_capacity.max(100));

        // 将 task_rx 和 result_tx 包装为 Arc<Mutex> 以便在多个工作线程间共享
        // 注意：mpsc::Receiver 不是 Sync，需要使用 tokio::sync::Mutex
        let task_rx = Arc::new(tokio::sync::Mutex::new(task_rx));

        // 创建工作线程
        for worker_id in 0..self.config.worker_count {
            let task_rx = Arc::clone(&task_rx);
            let result_tx = result_tx.clone();
            let client = self.client.clone();
            let idle_timeout = self.config.idle_timeout;
            let max_response_size = self.config.max_response_size;

            // 生成工作线程任务
            let handle = tokio::spawn(async move {
                Self::worker_loop(
                    worker_id,
                    task_rx,
                    result_tx,
                    client,
                    idle_timeout,
                    max_response_size,
                )
                .await;
            });

            self.workers.push(handle);
        }

        // 丢弃多余的 result_tx（保留一个用于发送）
        // 当所有 result_tx 都 drop 后，result_rx 会收到 None
        drop(result_tx);

        self.task_tx = Some(task_tx);
        self.result_rx = Some(result_rx);
        self.is_running = true;

        Ok(())
    }

    /// 工作线程主循环
    ///
    /// # 参数
    /// - `worker_id`: 工作线程 ID
    /// - `task_rx`: 任务接收端（共享）
    /// - `result_tx`: 结果发送端
    /// - `client`: reqwest 客户端
    /// - `idle_timeout`: 空闲超时（秒）
    async fn worker_loop(
        _worker_id: usize,
        task_rx: Arc<tokio::sync::Mutex<mpsc::Receiver<TaskMessage>>>,
        result_tx: mpsc::Sender<ResultMessage>,
        client: reqwest::Client,
        idle_timeout: u64,
        max_response_size: usize,
    ) {
        loop {
            // 获取任务（需要先获取锁再接收）
            // 使用 timeout 包裹整个 lock+recv，避免永久阻塞
            // 注意：recv() 在持有锁期间 await，多个工作线程串行竞争接收
            let task = if idle_timeout > 0 {
                // 有空闲超时：限制 lock+recv 的总耗时
                match tokio::time::timeout(Duration::from_secs(idle_timeout), async {
                    task_rx.lock().await.recv().await
                })
                .await
                {
                    Ok(task) => task,
                    Err(_) => {
                        // 空闲超时，退出工作线程
                        break;
                    }
                }
            } else {
                // 无超时，永久等待
                task_rx.lock().await.recv().await
            };

            match task {
                Some(TaskMessage::Request {
                    request,
                    client: _,
                    stream_tx,
                }) => {
                    // 执行请求
                    let request_id = request
                        .get_id()
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| request.get_url().to_string());

                    // 提取用户自定义数据（随结果原样带回）
                    let user_data = request.get_user_data().map(|s| s.to_string());

                    let start = Instant::now();

                    // 执行请求
                    let result = Self::execute_request(
                        client.clone(),
                        request,
                        request_id.clone(),
                        stream_tx,
                        max_response_size,
                    )
                    .await;

                    let elapsed = start.elapsed();

                    // 构建结果
                    let result_msg = match result {
                        Ok(response) => ResultMessage::Completed(RequestResult::success(
                            request_id, user_data, response, elapsed,
                        )),
                        Err(e) => ResultMessage::Completed(RequestResult::error(
                            request_id,
                            user_data,
                            e.to_string(),
                            elapsed,
                        )),
                    };

                    // 发送结果
                    if result_tx.send(result_msg).await.is_err() {
                        // 结果 channel 已关闭，退出工作线程
                        break;
                    }
                }
                Some(TaskMessage::Shutdown) | None => {
                    // 收到关闭信号或 channel 关闭
                    break;
                }
            }
        }

        // 通知主线程工作线程已关闭
        let _ = result_tx.send(ResultMessage::WorkerShutdown).await;
    }

    /// 执行单个 HTTP 请求
    /// 使用 from_parts 正确构建 XhResponse，保留完整的 status/url/headers 信息
    /// 使用流式读取响应体，带 max_response_size 限制，防止内存溢出
    async fn execute_request(
        client: reqwest::Client,
        request: XhRequest,
        request_id: String,
        stream_tx: Option<mpsc::Sender<(String, StreamEvent)>>,
        max_response_size: usize,
    ) -> XhCurlResult<XhResponse> {
        let start = Instant::now();

        // 转换请求
        let req_builder = request.to_reqwest(&client)?;

        // 发送请求
        let response = req_builder.send().await?;

        // 获取状态码
        let status = response.status().as_u16();

        // 收集响应头
        let mut headers_map = HashMap::new();
        for (name, value) in response.headers().iter() {
            if let Ok(value_str) = value.to_str() {
                headers_map.insert(name.as_str().to_string(), value_str.to_string());
            }
        }

        // 发送 Headers 事件（有界 channel，使用 await 实现背压）
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
        let version = match response.version() {
            reqwest::Version::HTTP_09 => Some("HTTP/0.9".to_string()),
            reqwest::Version::HTTP_10 => Some("HTTP/1.0".to_string()),
            reqwest::Version::HTTP_11 => Some("HTTP/1.1".to_string()),
            reqwest::Version::HTTP_2 => Some("HTTP/2".to_string()),
            reqwest::Version::HTTP_3 => Some("HTTP/3".to_string()),
            _ => None,
        };

        // 获取最终 URL
        let final_url = response.url().to_string();

        // 使用流式读取响应体，带大小限制（与 XhMulti 保持一致）
        // 防止恶意服务器返回超大响应导致内存溢出
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

            if new_size > max_response_size {
                // 超过限制：写入不超过 max_response_size 的部分后停止读取
                let remaining = max_response_size - body_size;
                if remaining > 0 {
                    body_data.extend_from_slice(&chunk[..remaining]);
                }
                body_size = max_response_size;
                size_exceeded = true;
                break;
            }

            body_data.extend_from_slice(&chunk);
            body_size = new_size;
        }

        // 发送 Chunk 事件（有界 channel，使用 await 实现背压）
        if let Some(tx) = &stream_tx {
            let _ = tx
                .send((
                    request_id.clone(),
                    StreamEvent::Chunk {
                        data: body_data.clone(),
                    },
                ))
                .await;
        }

        let elapsed = start.elapsed();

        // 如果响应体超过大小限制，返回错误（与 XhMulti 行为一致）
        if size_exceeded {
            return Err(XhCurlError::Memory(format!(
                "响应体大小超过限制 {} 字节",
                max_response_size
            )));
        }

        // 使用 from_parts 正确构建 XhResponse
        let xh_response = XhResponse::from_parts(
            status,
            final_url,
            headers_map,
            body_data,
            elapsed,
            remote_addr,
            version,
        );

        // 发送 Complete 事件（有界 channel，使用 await 实现背压）
        if let Some(tx) = &stream_tx {
            let _ = tx
                .send((request_id, StreamEvent::Complete { elapsed, body_size }))
                .await;
        }

        Ok(xh_response)
    }

    /// 提交单个请求到线程池
    ///
    /// # 参数
    /// - `request`: 请求构建器
    ///
    /// # 返回
    /// - `Ok(())`: 提交成功
    /// - `Err`: 线程池未启动或队列已满
    pub fn submit(&self, request: XhRequest) -> XhCurlResult<()> {
        let task_tx = self
            .task_tx
            .as_ref()
            .ok_or_else(|| XhCurlError::ThreadPool("线程池未启动".to_string()))?;

        // 使用 try_send 避免阻塞（如果队列满会立即返回错误）
        task_tx
            .try_send(TaskMessage::Request {
                request,
                client: self.client.clone(),
                stream_tx: None,
            })
            .map_err(|e| XhCurlError::ThreadPool(format!("提交任务失败: {}", e)))?;

        Ok(())
    }

    /// 提交单个请求（带流式回调）
    pub fn submit_with_stream(
        &self,
        request: XhRequest,
        stream_tx: mpsc::Sender<(String, StreamEvent)>,
    ) -> XhCurlResult<()> {
        let task_tx = self
            .task_tx
            .as_ref()
            .ok_or_else(|| XhCurlError::ThreadPool("线程池未启动".to_string()))?;

        task_tx
            .try_send(TaskMessage::Request {
                request,
                client: self.client.clone(),
                stream_tx: Some(stream_tx),
            })
            .map_err(|e| XhCurlError::ThreadPool(format!("提交任务失败: {}", e)))?;

        Ok(())
    }

    /// 批量提交请求并等待所有结果
    ///
    /// # 参数
    /// - `requests`: 请求列表
    ///
    /// # 返回
    /// 所有请求的结果列表
    pub async fn execute_all(
        &mut self,
        requests: Vec<XhRequest>,
    ) -> XhCurlResult<Vec<RequestResult>> {
        if !self.is_running {
            self.start()?;
        }

        let request_count = requests.len();
        if request_count == 0 {
            return Ok(Vec::new());
        }

        // 提交所有请求
        for request in requests {
            self.submit(request)?;
        }

        // 收集结果
        let mut results = Vec::with_capacity(request_count);
        let mut shutdown_count = 0;

        let result_rx = self
            .result_rx
            .as_mut()
            .ok_or_else(|| XhCurlError::ThreadPool("结果接收端不存在".to_string()))?;

        // 修复：原条件 `shutdown_count < worker_count && results.len() < request_count`
        // 用 && 导致 worker 提前关闭时即使结果未收齐也退出，丢失结果。
        // 正确逻辑：只要结果未收齐就继续等待，仅在所有 worker 关闭或 channel 关闭时退出。
        while results.len() < request_count {
            match result_rx.recv().await {
                Some(ResultMessage::Completed(result)) => {
                    results.push(result);
                }
                Some(ResultMessage::WorkerShutdown) => {
                    shutdown_count += 1;
                    // 所有工作线程都已关闭，无法再收到结果
                    if shutdown_count >= self.config.worker_count {
                        break;
                    }
                }
                None => {
                    // channel 关闭，无法再收到结果
                    break;
                }
            }
        }

        Ok(results)
    }

    /// 关闭线程池
    /// 发送关闭信号，等待所有工作线程退出
    pub async fn shutdown(&mut self) {
        // 发送关闭信号
        if let Some(task_tx) = &self.task_tx {
            for _ in 0..self.config.worker_count {
                let _ = task_tx.send(TaskMessage::Shutdown).await;
            }
        }

        // 等待所有工作线程退出
        for handle in self.workers.drain(..) {
            let _ = handle.await;
        }

        // 清理 channel
        self.task_tx = None;
        self.result_rx = None;
        self.is_running = false;
    }

    /// 获取工作线程数量
    pub fn worker_count(&self) -> usize {
        self.config.worker_count
    }

    /// 检查是否正在运行
    pub fn is_running(&self) -> bool {
        self.is_running
    }
}

impl Drop for XhThreadPool {
    fn drop(&mut self) {
        // 如果线程池还在运行，尝试关闭
        // 注意：在 drop 中不能 await，所以使用 try_send
        if self.is_running {
            if let Some(task_tx) = &self.task_tx {
                for _ in 0..self.config.worker_count {
                    let _ = task_tx.try_send(TaskMessage::Shutdown);
                }
            }
        }
    }
}

impl std::fmt::Debug for XhThreadPool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("XhThreadPool")
            .field("config", &self.config)
            .field("worker_count", &self.workers.len())
            .field("is_running", &self.is_running)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试线程池配置默认值
    #[test]
    fn test_default_config() {
        let config = ThreadPoolConfig::default();
        assert!(config.worker_count > 0);
        assert_eq!(config.queue_capacity, 1000);
        assert_eq!(config.idle_timeout, 60);
        assert!(config.enable_priority);
    }

    /// 测试线程池创建
    #[test]
    fn test_threadpool_creation() {
        let client = reqwest::Client::new();
        let pool = XhThreadPool::with_default_config(client);

        assert!(!pool.is_running());
        assert!(pool.worker_count() > 0);
    }

    /// 测试任务消息
    #[test]
    fn test_task_message() {
        let request = XhRequest::new("https://example.com");
        let client = reqwest::Client::new();

        let msg = TaskMessage::Request {
            request,
            client,
            stream_tx: None,
        };

        assert!(matches!(msg, TaskMessage::Request { .. }));

        let shutdown = TaskMessage::Shutdown;
        assert!(matches!(shutdown, TaskMessage::Shutdown));
    }

    /// 测试结果消息
    #[test]
    fn test_result_message() {
        let result = RequestResult::success(
            "req1".to_string(),
            None,
            XhResponse::from_error(
                "test".to_string(),
                "https://example.com".to_string(),
                Duration::from_secs(0),
            ),
            Duration::from_secs(1),
        );

        let msg = ResultMessage::Completed(result);
        assert!(matches!(msg, ResultMessage::Completed(_)));

        let shutdown = ResultMessage::WorkerShutdown;
        assert!(matches!(shutdown, ResultMessage::WorkerShutdown));
    }
}
