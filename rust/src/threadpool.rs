// +----------------------------------------------------------------------+
// | XHCurl 扩展 - 线程池（XHThreadPool）                                   |
// | 基于 tokio + channel 实现真正的多线程并发                              |
// |                                                                        |
// | 核心改进（对比 C 版本）：                                              |
// | 1. 工作线程通过 channel 与主线程通信，解决"不能调用 PHP 函数"限制      |
// | 2. 使用 tokio 运行时，自动管理工作线程生命周期                         |
// | 3. 编译期线程安全：Send + Sync bounds                                  |
// | 4. 统一 FPM/CLI 模式：FPM 使用单线程运行时，CLI 使用多线程运行时       |
// |                                                                        |
// | 工作模型：                                                             |
// |   主线程 (PHP) ──发送任务──> [任务队列] ──> 工作线程 1                 |
// |                                          ──> 工作线程 2                 |
// |                                          ──> 工作线程 N                 |
// |   主线程 (PHP) <──结果回调── [结果队列] <── 工作线程                    |
// +----------------------------------------------------------------------+

use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::error::{XhCurlError, XhCurlResult, DEFAULT_MAX_RESPONSE_SIZE};
use crate::multi::{RequestResult, StreamEvent};
use crate::request::XhRequest;
use crate::response::XhResponse;

/// 线程池任务消息
/// 主线程通过此消息类型向工作线程发送任务
#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum TaskMessage {
    /// 执行单个请求
    Request {
        /// 请求构建器
        request: XhRequest,
        /// 流式回调发送端（可选）
        stream_tx: Option<mpsc::Sender<(String, StreamEvent)>>,
    },

    /// 关闭工作线程
    Shutdown,
}

/// 线程池结果消息
/// 工作线程通过此消息类型向主线程发送结果
#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
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
    /// 0 = 无界队列（使用 unbounded_channel）
    pub queue_capacity: usize,

    /// 空闲线程超时（秒）
    /// 超过此时间无任务则关闭线程
    /// 0 = 永不超时（默认，线程池保持工作线程存活以复用连接）
    ///
    /// 注意：设置非零值可能导致 worker 在请求间隔较长时退出，
    /// 若此时仍有待处理任务，结果可能丢失。建议保持默认 0。
    pub idle_timeout: u64,

    /// 最大响应体大小（字节）
    /// 防止恶意服务器返回超大响应导致内存溢出
    /// 0 = 使用默认值 10MB
    pub max_response_size: usize,
}

impl Default for ThreadPoolConfig {
    fn default() -> Self {
        // 获取 CPU 核心数
        let cpu_count = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4);

        Self {
            worker_count: cpu_count,
            queue_capacity: 1000, // 有界队列防止内存溢出
            idle_timeout: 0,      // 永不超时：线程池应保持 worker 存活以复用连接
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
    /// 有界 channel（queue_capacity > 0）或无界 channel（queue_capacity == 0）
    task_tx: Option<mpsc::Sender<TaskMessage>>,

    /// 无界任务发送端（仅 queue_capacity == 0 时使用）
    task_tx_unbounded: Option<mpsc::UnboundedSender<TaskMessage>>,

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
            task_tx_unbounded: None,
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

        // 创建任务 channel
        // queue_capacity == 0 时使用无界 channel（避免 usize::MAX 分配极大内存的 bug）
        let (task_tx, task_rx) = if self.config.queue_capacity > 0 {
            let (tx, rx) = mpsc::channel(self.config.queue_capacity);
            (Some(tx), Some(rx))
        } else {
            (None, None)
        };

        // 无界 channel 用于 queue_capacity == 0 的场景
        let (task_tx_ub, task_rx_ub) = mpsc::unbounded_channel();
        self.task_tx_unbounded = Some(task_tx_ub);

        // 创建结果 channel
        let (result_tx, result_rx) = mpsc::channel(self.config.queue_capacity.max(100));

        // 将 task_rx 包装为 Arc<Mutex> 以便在多个工作线程间共享
        // 注意：mpsc::Receiver 不是 Sync，需要使用 tokio::sync::Mutex
        let task_rx = task_rx.map(|rx| Arc::new(tokio::sync::Mutex::new(rx)));
        let task_rx_ub = Arc::new(tokio::sync::Mutex::new(task_rx_ub));

        // 创建工作线程
        for worker_id in 0..self.config.worker_count {
            let task_rx = task_rx.clone();
            let task_rx_ub = Arc::clone(&task_rx_ub);
            let result_tx = result_tx.clone();
            let client = self.client.clone();
            let idle_timeout = self.config.idle_timeout;
            let max_response_size = self.config.max_response_size;
            let use_bounded = task_rx.is_some();

            // 生成工作线程任务
            let handle = tokio::spawn(async move {
                Self::worker_loop(
                    worker_id,
                    task_rx,
                    task_rx_ub,
                    use_bounded,
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

        self.task_tx = task_tx;
        self.result_rx = Some(result_rx);
        self.is_running = true;

        Ok(())
    }

    /// 工作线程主循环
    ///
    /// # 参数
    /// - `worker_id`: 工作线程 ID
    /// - `task_rx`: 有界任务接收端（共享，可选）
    /// - `task_rx_ub`: 无界任务接收端（共享）
    /// - `use_bounded`: 是否使用有界 channel
    /// - `result_tx`: 结果发送端
    /// - `client`: reqwest 客户端（worker 闭包捕获，复用连接池）
    /// - `idle_timeout`: 空闲超时（秒），0 = 永不超时
    /// - `max_response_size`: 最大响应体大小
    #[allow(clippy::too_many_arguments)]
    async fn worker_loop(
        _worker_id: usize,
        task_rx: Option<Arc<tokio::sync::Mutex<mpsc::Receiver<TaskMessage>>>>,
        task_rx_ub: Arc<tokio::sync::Mutex<mpsc::UnboundedReceiver<TaskMessage>>>,
        use_bounded: bool,
        result_tx: mpsc::Sender<ResultMessage>,
        client: reqwest::Client,
        idle_timeout: u64,
        max_response_size: usize,
    ) {
        loop {
            // 获取任务
            // idle_timeout == 0 时永久等待（默认行为，线程池保持 worker 存活）
            // idle_timeout > 0 时空闲超时退出（用户显式设置，可能导致结果丢失）
            let task = if idle_timeout > 0 {
                // 有空闲超时：限制 lock+recv 的总耗时
                let timeout_result = if use_bounded {
                    let rx = task_rx.as_ref().unwrap();
                    tokio::time::timeout(
                        Duration::from_secs(idle_timeout),
                        Box::pin(async { rx.lock().await.recv().await }),
                    )
                    .await
                } else {
                    tokio::time::timeout(
                        Duration::from_secs(idle_timeout),
                        Box::pin(async { task_rx_ub.lock().await.recv().await }),
                    )
                    .await
                };

                match timeout_result {
                    Ok(task) => task,
                    Err(_) => {
                        // 空闲超时，退出工作线程
                        break;
                    }
                }
            } else {
                // 无超时，永久等待
                if use_bounded {
                    task_rx.as_ref().unwrap().lock().await.recv().await
                } else {
                    task_rx_ub.lock().await.recv().await
                }
            };

            match task {
                Some(TaskMessage::Request { request, stream_tx }) => {
                    // 执行请求（复用 worker 闭包捕获的 client，无需每次 clone）
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
    /// 委托给 `crate::executor::execute_request`，与 XhMulti、fiber 共用同一实现
    async fn execute_request(
        client: reqwest::Client,
        request: XhRequest,
        request_id: String,
        stream_tx: Option<mpsc::Sender<(String, StreamEvent)>>,
        max_response_size: usize,
    ) -> XhCurlResult<XhResponse> {
        crate::executor::execute_request(client, request, request_id, stream_tx, max_response_size)
            .await
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
        let task = TaskMessage::Request {
            request,
            stream_tx: None,
        };

        if let Some(task_tx) = &self.task_tx {
            // 有界 channel
            task_tx
                .try_send(task)
                .map_err(|e| XhCurlError::ThreadPool(format!("提交任务失败: {}", e)))?;
        } else if let Some(task_tx) = &self.task_tx_unbounded {
            // 无界 channel
            task_tx
                .send(task)
                .map_err(|e| XhCurlError::ThreadPool(format!("提交任务失败: {}", e)))?;
        } else {
            return Err(XhCurlError::ThreadPool("线程池未启动".to_string()));
        }

        Ok(())
    }

    /// 提交单个请求（带流式回调）
    pub fn submit_with_stream(
        &self,
        request: XhRequest,
        stream_tx: mpsc::Sender<(String, StreamEvent)>,
    ) -> XhCurlResult<()> {
        let task = TaskMessage::Request {
            request,
            stream_tx: Some(stream_tx),
        };

        if let Some(task_tx) = &self.task_tx {
            task_tx
                .try_send(task)
                .map_err(|e| XhCurlError::ThreadPool(format!("提交任务失败: {}", e)))?;
        } else if let Some(task_tx) = &self.task_tx_unbounded {
            task_tx
                .send(task)
                .map_err(|e| XhCurlError::ThreadPool(format!("提交任务失败: {}", e)))?;
        } else {
            return Err(XhCurlError::ThreadPool("线程池未启动".to_string()));
        }

        Ok(())
    }

    /// 批量提交请求并等待所有结果
    ///
    /// # 参数
    /// - `requests`: 请求列表
    ///
    /// # 返回
    /// 所有请求的结果列表（数量等于成功提交的请求数）
    pub async fn execute_all(
        &mut self,
        requests: Vec<XhRequest>,
    ) -> XhCurlResult<Vec<RequestResult>> {
        if !self.is_running {
            self.start()?;
        }

        if requests.is_empty() {
            return Ok(Vec::new());
        }

        // 提交所有请求，记录成功提交的数量
        // 若中途 submit 失败（队列满），已提交的请求仍需收集结果
        let mut submitted_count = 0;
        for request in requests {
            match self.submit(request) {
                Ok(()) => submitted_count += 1,
                Err(e) => {
                    // 提交失败，但已提交的请求需要继续收集结果
                    // 不立即返回错误，避免已提交请求的结果丢失
                    eprintln!("警告: 提交任务失败: {}", e);
                }
            }
        }

        if submitted_count == 0 {
            return Err(XhCurlError::ThreadPool("所有请求提交失败".to_string()));
        }

        // 收集结果（数量等于成功提交的请求数）
        let mut results = Vec::with_capacity(submitted_count);
        let mut shutdown_count = 0;

        let result_rx = self
            .result_rx
            .as_mut()
            .ok_or_else(|| XhCurlError::ThreadPool("结果接收端不存在".to_string()))?;

        // 只要结果未收齐就继续等待
        // 仅在所有 worker 关闭或 channel 关闭时退出
        while results.len() < submitted_count {
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

        // 完整性检查：worker panic 或提前退出可能导致结果数量不足
        // 此时返回错误而非静默返回不完整结果
        if results.len() != submitted_count {
            return Err(XhCurlError::ThreadPool(format!(
                "部分任务异常退出：预期 {} 个结果，实际收到 {} 个",
                submitted_count,
                results.len()
            )));
        }

        Ok(results)
    }

    /// 关闭线程池
    /// 发送关闭信号，等待所有工作线程退出
    pub async fn shutdown(&mut self) {
        // 发送关闭信号
        let shutdown_count = self.config.worker_count;
        if let Some(task_tx) = &self.task_tx {
            for _ in 0..shutdown_count {
                let _ = task_tx.send(TaskMessage::Shutdown).await;
            }
        } else if let Some(task_tx) = &self.task_tx_unbounded {
            for _ in 0..shutdown_count {
                let _ = task_tx.send(TaskMessage::Shutdown);
            }
        }

        // 等待所有工作线程退出
        for handle in self.workers.drain(..) {
            let _ = handle.await;
        }

        // 清理 channel
        self.task_tx = None;
        self.task_tx_unbounded = None;
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
        // 注意：在 drop 中不能 await，所以使用 try_send / send（无界）
        if self.is_running {
            if let Some(task_tx) = &self.task_tx {
                for _ in 0..self.config.worker_count {
                    let _ = task_tx.try_send(TaskMessage::Shutdown);
                }
            } else if let Some(task_tx) = &self.task_tx_unbounded {
                for _ in 0..self.config.worker_count {
                    let _ = task_tx.send(TaskMessage::Shutdown);
                }
            }
        }

        // abort 所有仍在运行的 worker task，避免任务泄漏
        // drop 中不能 await JoinHandle，但可以 abort（立即取消 task）
        for handle in self.workers.drain(..) {
            handle.abort();
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
        assert_eq!(config.idle_timeout, 0); // 默认永不超时
        assert_eq!(config.max_response_size, DEFAULT_MAX_RESPONSE_SIZE);
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

        let msg = TaskMessage::Request {
            request,
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
