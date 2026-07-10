// +----------------------------------------------------------------------+
// | XHCurl 扩展 - 异步批量执行器（XHMulti）                                |
// | 基于 tokio 异步运行时实现并发批量 HTTP 请求                            |
// |                                                                        |
// | 并发模型说明：                                                         |
// | - 核心库的 XhMulti 通过 tokio::spawn 创建 N 个异步任务                 |
// | - 实际调度模型由 PHP 绑定层（php_ext.rs）的运行时决定：                |
// |   * CLI 模式：多线程运行时，任务在多工作线程上并行（M:N）              |
// |   * FPM 模式：单线程运行时，任务协作式并发（M:1，类似 Node.js）        |
// | - 单请求超时由 XhRequest::request_timeout（reqwest 级别）处理          |
// |                                                                        |
// | 核心优势（对比 C 版本）：                                              |
// | 1. 非阻塞执行：主线程不阻塞，通过 channel 接收结果                     |
// | 2. 流式回调支持：通过 channel 实现工作线程到主线程的回调              |
// | 3. 编译期线程安全：Send + Sync bounds 保证无数据竞争                  |
// |                                                                        |
// | 安全改进（v2）：                                                       |
// | - 有界 channel 替代无界 channel，防止内存溢出                         |
// | - 流式读取响应体 + max_response_size 限制                              |
// | - 请求数量上限检查                                                     |
// +----------------------------------------------------------------------+

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::error::{XhCurlError, XhCurlResult, DEFAULT_MAX_RESPONSE_SIZE, MAX_REQUESTS_PER_BATCH};
use crate::request::XhRequest;
use crate::response::XhResponse;

// +----------------------------------------------------------------------+
// | 常量定义                                                              |
// +----------------------------------------------------------------------+

// MAX_REQUESTS_PER_BATCH 和 DEFAULT_MAX_RESPONSE_SIZE 定义在 error.rs，
// 供 multi / threadpool / php_ext / curl 共用，避免多处重复定义。

/// 流式事件 channel 的默认缓冲区大小
/// 限制积压事件数量，实现背压控制
const STREAM_CHANNEL_CAPACITY: usize = 1024;

/// 结果 channel 的默认缓冲区倍数
/// 相对于请求数量的倍数，确保不会因缓冲不足而阻塞
const RESULT_CHANNEL_MULTIPLIER: usize = 2;

// +----------------------------------------------------------------------+
// | 流式回调事件                                                          |
// +----------------------------------------------------------------------+

/// 流式回调事件
/// 工作线程通过 channel 发送给主线程的事件类型
/// 对应 C 版本的 onChunk / onHeader 回调
#[derive(Debug, Clone)]
pub enum StreamEvent {
    /// 接收到响应头
    /// 参数：状态码、头部 Map
    Headers {
        /// HTTP 状态码
        status: u16,
        /// 响应头键值对
        headers: HashMap<String, String>,
    },

    /// 接收到响应体数据块
    /// 参数：数据块
    Chunk {
        /// 响应体数据块
        data: Vec<u8>,
    },

    /// 请求完成
    /// 参数：总耗时、响应体大小
    Complete {
        /// 请求总耗时
        elapsed: Duration,
        /// 响应体总大小
        body_size: usize,
    },

    /// 请求出错
    /// 参数：错误信息
    Error {
        /// 错误描述
        message: String,
    },
}

// +----------------------------------------------------------------------+
// | 请求执行结果                                                          |
// +----------------------------------------------------------------------+

/// 单个请求的执行结果
#[derive(Debug)]
pub struct RequestResult {
    /// 请求 ID（用于关联请求和响应）
    pub id: String,

    /// 用户自定义数据（JSON 字符串，随请求原样带回）
    pub user_data: Option<String>,

    /// 响应对象（成功时）
    pub response: Option<XhResponse>,

    /// 错误信息（失败时）
    pub error: Option<String>,

    /// 请求耗时
    pub elapsed: Duration,
}

impl RequestResult {
    /// 创建成功结果
    pub fn success(
        id: String,
        user_data: Option<String>,
        response: XhResponse,
        elapsed: Duration,
    ) -> Self {
        Self {
            id,
            user_data,
            response: Some(response),
            error: None,
            elapsed,
        }
    }

    /// 创建失败结果
    pub fn error(id: String, user_data: Option<String>, error: String, elapsed: Duration) -> Self {
        Self {
            id,
            user_data,
            response: None,
            error: Some(error),
            elapsed,
        }
    }

    /// 检查是否成功
    pub fn is_success(&self) -> bool {
        self.response.is_some() && self.error.is_none()
    }
}

// +----------------------------------------------------------------------+
// | 异步批量执行器                                                        |
// +----------------------------------------------------------------------+

/// 异步批量执行器
/// 管理多个并发 HTTP 请求，基于 tokio 的 M:N 调度
///
/// # 工作原理
/// 1. 每个请求被封装为一个 tokio task（类似 goroutine）
/// 2. tokio 运行时自动在 M 个工作线程间调度 N 个 task
/// 3. 通过 mpsc channel 将结果发送回主线程
/// 4. 主线程通过 `recv_all()` 收集所有结果
///
/// # 对比 C 版本
/// - C 版本：curl_multi_perform 单线程事件循环，阻塞主线程
/// - Rust 版本：tokio M:N 调度，真正并行执行
///
/// # 线程安全
/// - XhMulti 本身不是线程安全的（管理任务状态）
/// - 但内部的异步任务可以跨线程执行（通过 channel 通信）
/// - 所有 channel 使用有界缓冲区，防止内存溢出
pub struct XhMulti {
    /// reqwest 客户端（共享连接池）
    client: reqwest::Client,

    /// 待执行的请求列表
    requests: Vec<XhRequest>,

    /// 最大并发数（0 = 无限制）
    max_concurrency: usize,

    /// 批量级全局超时
    /// 超时后 abort 未完成的任务并返回错误。
    /// 注意：单请求超时由 XhRequest::request_timeout（reqwest 级别）处理，
    /// 此超时是整个批量请求的总时限。
    timeout: Option<Duration>,

    /// 最大响应体大小（字节，0 = 使用默认值）
    /// 防止恶意服务器返回超大响应导致内存溢出
    max_response_size: usize,

    /// 流式回调 channel 发送端
    /// 用于将工作线程的事件发送到主线程
    /// Some 表示启用了流式回调
    /// 修复：改为有界 channel，实现背压控制
    stream_tx: Option<mpsc::Sender<(String, StreamEvent)>>,

    /// 已启动的异步任务句柄
    /// 任务结果通过 channel 收集，句柄仅用于等待任务完成
    tasks: Vec<JoinHandle<()>>,
}

impl XhMulti {
    /// 创建新的批量执行器
    ///
    /// # 参数
    /// - `client`: reqwest 客户端实例
    pub fn new(client: reqwest::Client) -> Self {
        Self {
            client,
            requests: Vec::new(),
            max_concurrency: 0, // 0 = 无限制
            timeout: None,
            max_response_size: DEFAULT_MAX_RESPONSE_SIZE,
            stream_tx: None,
            tasks: Vec::new(),
        }
    }

    /// 使用默认客户端创建
    pub fn with_default_client() -> XhCurlResult<Self> {
        let client = reqwest::Client::builder()
            .build()
            .map_err(XhCurlError::from)?;
        Ok(Self::new(client))
    }

    /// 添加请求到批量执行器
    /// 带数量上限检查，防止内存溢出
    ///
    /// # 参数
    /// - `request`: 请求构建器
    pub fn add(&mut self, request: XhRequest) -> XhCurlResult<&mut Self> {
        // 检查请求数量是否超过上限
        if self.requests.len() >= MAX_REQUESTS_PER_BATCH {
            return Err(XhCurlError::Memory(format!(
                "批量请求数量超过上限 {}，请分批执行",
                MAX_REQUESTS_PER_BATCH
            )));
        }
        self.requests.push(request);
        Ok(self)
    }

    /// 批量添加请求
    /// 带数量上限检查
    pub fn add_many(&mut self, requests: Vec<XhRequest>) -> XhCurlResult<&mut Self> {
        // 检查添加后是否超过上限
        let new_count = self.requests.len() + requests.len();
        if new_count > MAX_REQUESTS_PER_BATCH {
            return Err(XhCurlError::Memory(format!(
                "批量请求数量超过上限 {}（当前 {} + 新增 {}），请分批执行",
                MAX_REQUESTS_PER_BATCH,
                self.requests.len(),
                requests.len()
            )));
        }
        self.requests.extend(requests);
        Ok(self)
    }

    /// 设置最大并发数
    ///
    /// # 参数
    /// - `max`: 最大并发数（0 = 无限制）
    pub fn max_concurrency(mut self, max: usize) -> Self {
        self.max_concurrency = max;
        self
    }

    /// 设置全局超时
    ///
    /// # 参数
    /// - `secs`: 超时秒数（0 = 无超时）
    pub fn timeout(mut self, secs: u64) -> Self {
        self.timeout = if secs > 0 {
            Some(Duration::from_secs(secs))
        } else {
            None
        };
        self
    }

    /// 设置最大响应体大小
    /// 防止恶意服务器返回超大响应导致内存溢出
    ///
    /// # 参数
    /// - `max_size`: 最大字节数（0 = 使用默认值 10MB）
    pub fn max_response_size(mut self, max_size: usize) -> Self {
        self.max_response_size = if max_size > 0 {
            max_size
        } else {
            DEFAULT_MAX_RESPONSE_SIZE
        };
        self
    }

    /// 启用流式回调
    /// 返回接收端 channel，用于接收流式事件
    ///
    /// # 修复
    /// 改为有界 channel，限制积压事件数量，实现背压控制
    /// 当消费者处理速度跟不上生产者时，生产者会等待而非无限积压
    ///
    /// # 返回
    /// - `Receiver`: 流式事件接收端
    pub fn enable_streaming(&mut self) -> mpsc::Receiver<(String, StreamEvent)> {
        // 使用有界 channel 替代无界 channel
        // 缓冲区大小 = STREAM_CHANNEL_CAPACITY
        // 当缓冲区满时，发送端会等待（背压），而非无限积压导致内存溢出
        let (tx, rx) = mpsc::channel(STREAM_CHANNEL_CAPACITY);
        self.stream_tx = Some(tx);
        rx
    }

    /// 执行所有请求（异步）
    ///
    /// # 工作流程
    /// 1. 为每个请求创建 tokio task（类似 goroutine）
    /// 2. 使用 Semaphore 控制最大并发数
    /// 3. 所有 task 并发执行，tokio 自动调度
    /// 4. 通过 channel 收集结果
    ///
    /// # 返回
    /// 所有请求的结果列表
    ///
    /// # 结果顺序
    /// 结果按**完成顺序**排列（非提交顺序）。并发执行时，先完成的请求先入列。
    /// 如需按提交顺序获取结果，请使用每个结果的 `id` 字段（请求 ID 或 URL）
    /// 进行关联匹配，而非依赖数组索引。
    pub async fn execute(&mut self) -> XhCurlResult<Vec<RequestResult>> {
        // 如果没有请求，直接返回空结果
        if self.requests.is_empty() {
            return Ok(Vec::new());
        }

        // 启动所有请求任务（spawn 逻辑抽取到 spawn_all，供 execute / execute_each 共用）
        let (mut result_rx, expected) = self.spawn_all().await;

        // 收集所有结果
        let mut results = Vec::with_capacity(expected);

        match self.timeout {
            // 带批量级超时的结果收集
            Some(timeout_dur) => {
                let deadline = Instant::now() + timeout_dur;
                loop {
                    let now = Instant::now();
                    if now >= deadline {
                        // 批量超时：abort 剩余任务并返回错误，避免后续 handle.await 无限等待
                        self.abort_tasks();
                        return Err(XhCurlError::Generic(format!(
                            "批量请求超时（{} 秒），已完成 {}/{} 个",
                            timeout_dur.as_secs(),
                            results.len(),
                            expected
                        )));
                    }
                    let remaining = deadline - now;
                    match tokio::time::timeout(remaining, result_rx.recv()).await {
                        Ok(Some(result)) => results.push(result),
                        Ok(None) => break, // channel 关闭
                        Err(_) => {
                            // 批量超时：abort 剩余任务，避免任务泄漏
                            self.abort_tasks();
                            return Err(XhCurlError::Generic(format!(
                                "批量请求超时（{} 秒），已完成 {}/{} 个",
                                timeout_dur.as_secs(),
                                results.len(),
                                expected
                            )));
                        }
                    }
                }
            }
            // 无超时的结果收集
            None => {
                while let Some(result) = result_rx.recv().await {
                    results.push(result);
                }
            }
        }

        // 等待所有任务完成（确保没有任务泄漏）
        // 检测 task panic（JoinError），避免静默丢失结果
        self.join_tasks().await;

        // 完整性检查：task panic 会导致结果数量少于预期
        // 此时返回错误而非静默返回不完整结果
        if results.len() != expected {
            return Err(XhCurlError::Generic(format!(
                "部分任务异常退出：预期 {} 个结果，实际收到 {} 个",
                expected,
                results.len()
            )));
        }

        Ok(results)
    }

    /// 启动所有待执行请求的异步任务。
    ///
    /// 抽取自原 `execute()` 的内联 spawn 逻辑，供 `execute()`（累积返回）与
    /// `PhpXhMulti::execute_each`（流式回调）共用，消除 spawn 重复代码。
    ///
    /// - 排空 `self.requests`，为每个请求 spawn 一个 tokio task
    /// - 任务句柄存入 `self.tasks`（供调用方 abort / await）
    /// - 返回结果接收端与预期结果数（== 已 spawn 任务数）
    ///
    /// # 调用方职责
    /// 1. 消费返回的 `Receiver` 收集结果
    /// 2. 结束时调用 `abort_tasks()` / `join_tasks()` 处理残留任务，避免泄漏
    /// 3. 做完整性检查（结果数 == expected）
    pub async fn spawn_all(&mut self) -> (mpsc::Receiver<RequestResult>, usize) {
        // 创建结果 channel（有界，缓冲区 = 请求数 * 倍数）
        let channel_capacity = self
            .requests
            .len()
            .saturating_mul(RESULT_CHANNEL_MULTIPLIER)
            .max(16); // 最小 16 个缓冲位
        let (result_tx, result_rx) = mpsc::channel(channel_capacity);

        // 创建并发控制 Semaphore（如果设置了最大并发数）
        let semaphore = if self.max_concurrency > 0 {
            Some(Arc::new(tokio::sync::Semaphore::new(self.max_concurrency)))
        } else {
            None
        };

        // 获取最大响应体大小（用于流式读取时的截断检查）
        let max_response_size = self.max_response_size;

        // 为每个请求创建异步任务
        for request in self.requests.drain(..) {
            // 获取请求 ID（如果没有则使用 URL 作为 ID）
            let request_id = request
                .get_id()
                .map(|s| s.to_string())
                .unwrap_or_else(|| request.get_url().to_string());

            // 提取用户自定义数据（随结果原样带回）
            let user_data = request.get_user_data().map(|s| s.to_string());

            // 克隆共享资源
            let client = self.client.clone();
            let result_tx = result_tx.clone();
            let stream_tx = self.stream_tx.clone();
            let semaphore = semaphore.clone();

            // 生成异步任务（类似 go func() { ... }()）
            let handle: JoinHandle<()> = tokio::spawn(async move {
                // 如果有并发限制，获取 Semaphore 许可
                // _permit 在作用域结束时自动释放
                // Semaphore 关闭时 acquire 返回 Err（AcquireError），
                // 此时不执行请求，直接发送错误结果，避免结果数量不匹配
                let _permit = if let Some(sem) = &semaphore {
                    match sem.acquire().await {
                        Ok(permit) => Some(permit),
                        Err(_) => {
                            // Semaphore 已关闭，发送错误结果并退出
                            let elapsed = Instant::now().elapsed();
                            let result = RequestResult::error(
                                request_id,
                                user_data,
                                "并发信号量已关闭".to_string(),
                                elapsed,
                            );
                            let _ = result_tx.send(result).await;
                            return;
                        }
                    }
                } else {
                    None
                };

                // 记录开始时间
                let start = Instant::now();

                // 执行单个请求（带响应体大小限制）
                let result = Self::execute_single(
                    client,
                    request,
                    request_id.clone(),
                    stream_tx,
                    max_response_size,
                )
                .await;

                // 计算耗时
                let elapsed = start.elapsed();

                // 构建结果
                let result = match result {
                    Ok(response) => {
                        RequestResult::success(request_id, user_data, response, elapsed)
                    }
                    Err(e) => RequestResult::error(request_id, user_data, e.to_string(), elapsed),
                };

                // 通过 channel 发送结果
                // 如果发送失败，说明接收端已关闭（通常不会发生）
                // 结果统一从 channel 收集，无需再从任务返回值获取
                let _ = result_tx.send(result).await;
            });

            self.tasks.push(handle);
        }

        // 丢弃发送端（所有任务完成后 channel 自动关闭）
        drop(result_tx);

        // 预期结果数量（用于后续完整性检查）
        let expected = self.tasks.len();
        (result_rx, expected)
    }

    /// abort 并清空所有已 spawn 的任务句柄。
    /// 用于超时/回调异常等需要提前终止的清理路径，避免任务泄漏。
    pub fn abort_tasks(&mut self) {
        for handle in self.tasks.drain(..) {
            handle.abort();
        }
    }

    /// 等待所有已 spawn 的任务完成。
    /// 检测 task panic（JoinError），打印日志而非静默丢失。
    pub async fn join_tasks(&mut self) {
        for handle in self.tasks.drain(..) {
            if let Err(join_err) = handle.await {
                eprintln!("[XHMulti] 任务异常退出: {}", join_err);
            }
        }
    }

    /// 执行单个请求
    /// 使用流式读取 + max_response_size 限制，防止内存溢出
    ///
    /// 实际逻辑委托给 `crate::executor::execute_request`，
    /// 与 `XhThreadPool::execute_request`、`fiber::execute_http_task` 共用同一实现。
    ///
    /// # 参数
    /// - `client`: reqwest 客户端
    /// - `request`: 请求构建器
    /// - `request_id`: 请求 ID
    /// - `stream_tx`: 流式事件发送端（可选）
    /// - `max_response_size`: 最大响应体大小（字节）
    ///
    /// # 返回
    /// - `Ok(XhResponse)`: 请求成功
    /// - `Err(XhCurlError)`: 请求失败
    pub async fn execute_single(
        client: reqwest::Client,
        request: XhRequest,
        request_id: String,
        stream_tx: Option<mpsc::Sender<(String, StreamEvent)>>,
        max_response_size: usize,
    ) -> XhCurlResult<XhResponse> {
        crate::executor::execute_request(client, request, request_id, stream_tx, max_response_size)
            .await
    }

    /// 获取待执行请求数量
    pub fn len(&self) -> usize {
        self.requests.len()
    }

    /// 检查是否为空
    pub fn is_empty(&self) -> bool {
        self.requests.is_empty()
    }

    /// 清空所有待执行请求
    /// 同时 abort 运行中的任务，避免任务泄漏
    pub fn clear(&mut self) {
        self.requests.clear();
        // abort 运行中的任务（如果在 execute() 期间调用 clear，已 drain 的任务在此 abort）
        for handle in self.tasks.drain(..) {
            handle.abort();
        }
    }
}

impl std::fmt::Debug for XhMulti {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("XhMulti")
            .field("request_count", &self.requests.len())
            .field("max_concurrency", &self.max_concurrency)
            .field("max_response_size", &self.max_response_size)
            .field("has_streaming", &self.stream_tx.is_some())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试批量执行器创建
    #[test]
    fn test_multi_creation() {
        let multi = XhMulti::with_default_client().unwrap();
        assert!(multi.is_empty());
        assert_eq!(multi.max_concurrency, 0);
    }

    /// 测试添加请求
    #[test]
    fn test_add_requests() {
        let mut multi = XhMulti::with_default_client().unwrap();

        // 添加单个请求
        multi
            .add(XhRequest::new("https://httpbin.org/get"))
            .unwrap();
        multi
            .add(XhRequest::new("https://httpbin.org/status/200"))
            .unwrap();

        assert_eq!(multi.len(), 2);
    }

    /// 测试批量添加请求
    #[test]
    fn test_add_many_requests() {
        let mut multi = XhMulti::with_default_client().unwrap();

        let requests = vec![
            XhRequest::new("https://httpbin.org/get"),
            XhRequest::new("https://httpbin.org/post"),
        ];
        multi.add_many(requests).unwrap();

        assert_eq!(multi.len(), 2);
    }

    /// 测试请求数量上限
    #[test]
    fn test_request_limit() {
        let mut multi = XhMulti::with_default_client().unwrap();

        // 添加超过上限的请求应该失败
        let requests: Vec<XhRequest> = (0..=MAX_REQUESTS_PER_BATCH)
            .map(|i| XhRequest::new(format!("https://httpbin.org/get?id={}", i)))
            .collect();

        let result = multi.add_many(requests);
        assert!(result.is_err());
    }

    /// 测试流式回调启用
    #[test]
    fn test_enable_streaming() {
        let mut multi = XhMulti::with_default_client().unwrap();
        let _rx = multi.enable_streaming();
        assert!(multi.stream_tx.is_some());
    }

    /// 测试并发数设置
    #[test]
    fn test_max_concurrency() {
        let multi = XhMulti::with_default_client().unwrap().max_concurrency(10);

        assert_eq!(multi.max_concurrency, 10);
    }

    /// 测试超时设置
    #[test]
    fn test_timeout() {
        let multi = XhMulti::with_default_client().unwrap().timeout(30);

        assert_eq!(multi.timeout, Some(Duration::from_secs(30)));
    }

    /// 测试最大响应体大小设置
    #[test]
    fn test_max_response_size() {
        let multi = XhMulti::with_default_client()
            .unwrap()
            .max_response_size(1024 * 1024); // 1MB

        assert_eq!(multi.max_response_size, 1024 * 1024);
    }

    /// 测试清空请求
    #[test]
    fn test_clear() {
        let mut multi = XhMulti::with_default_client().unwrap();
        multi.add(XhRequest::new("https://example.com")).unwrap();
        assert_eq!(multi.len(), 1);

        multi.clear();
        assert!(multi.is_empty());
    }

    /// 测试流式事件
    #[test]
    fn test_stream_events() {
        use StreamEvent::*;

        // 测试 Headers 事件
        let event = Headers {
            status: 200,
            headers: HashMap::new(),
        };
        if let Headers { status, .. } = event {
            assert_eq!(status, 200);
        }

        // 测试 Chunk 事件
        let event = Chunk {
            data: b"hello".to_vec(),
        };
        if let Chunk { data } = event {
            assert_eq!(data, b"hello");
        }

        // 测试 Complete 事件
        let event = Complete {
            elapsed: Duration::from_secs(1),
            body_size: 100,
        };
        if let Complete { body_size, .. } = event {
            assert_eq!(body_size, 100);
        }

        // 测试 Error 事件
        let event = Error {
            message: "超时".to_string(),
        };
        if let Error { message } = event {
            assert_eq!(message, "超时");
        }
    }

    /// 测试请求结果
    #[test]
    fn test_request_result() {
        // 成功结果
        let response = XhResponse::from_error(
            "test".to_string(),
            "https://example.com".to_string(),
            Duration::from_secs(0),
        );
        let success =
            RequestResult::success("req1".to_string(), None, response, Duration::from_secs(1));
        assert!(success.is_success());
        assert!(success.response.is_some());
        assert!(success.error.is_none());

        // 失败结果
        let error = RequestResult::error(
            "req2".to_string(),
            None,
            "连接超时".to_string(),
            Duration::from_secs(30),
        );
        assert!(!error.is_success());
        assert!(error.response.is_none());
        assert!(error.error.is_some());
    }

    /// add 精确边界：填充至 MAX-1 后第 MAX 个成功，第 MAX+1 个失败。
    /// add 使用 `len() >= MAX` 判定（含等号），故达到 MAX 时下一次 add 必失败。
    #[test]
    fn test_add_exact_boundary() {
        let mut multi = XhMulti::with_default_client().unwrap();
        // 用 add_many 填充至 MAX-1（9999）
        let batch: Vec<XhRequest> = (0..MAX_REQUESTS_PER_BATCH - 1)
            .map(|i| XhRequest::new(format!("https://x.com/{}", i)))
            .collect();
        multi.add_many(batch).unwrap();
        assert_eq!(multi.len(), MAX_REQUESTS_PER_BATCH - 1);

        // 第 MAX 个（第 10000 个）：成功
        multi.add(XhRequest::new("https://x.com/last")).unwrap();
        assert_eq!(multi.len(), MAX_REQUESTS_PER_BATCH);

        // 第 MAX+1 个（第 10001 个）：失败
        let result = multi.add(XhRequest::new("https://x.com/over"));
        assert!(result.is_err());
        // 数量不应变化
        assert_eq!(multi.len(), MAX_REQUESTS_PER_BATCH);
    }

    /// add_many 精确边界：恰好 MAX 个成功，MAX+1 个失败。
    /// add_many 使用 `new_count > MAX` 判定（不含等号），故恰好 MAX 允许。
    #[test]
    fn test_add_many_exact_boundary() {
        let mut multi = XhMulti::with_default_client().unwrap();
        // 恰好 MAX 个：成功
        let batch: Vec<XhRequest> = (0..MAX_REQUESTS_PER_BATCH)
            .map(|i| XhRequest::new(format!("https://x.com/{}", i)))
            .collect();
        multi.add_many(batch).unwrap();
        assert_eq!(multi.len(), MAX_REQUESTS_PER_BATCH);

        // 再加 1 个：失败（MAX + 1 > MAX）
        let result = multi.add_many(vec![XhRequest::new("https://x.com/over")]);
        assert!(result.is_err());
    }

    /// max_response_size(0) 应归一化为默认值（10MB），避免 0 导致无限制
    #[test]
    fn test_max_response_size_zero_normalizes() {
        let multi = XhMulti::with_default_client().unwrap().max_response_size(0);
        assert_eq!(multi.max_response_size, DEFAULT_MAX_RESPONSE_SIZE);
    }

    /// timeout(0) 应清除超时（设为 None），0 即无超时
    #[test]
    fn test_timeout_zero_is_none() {
        let multi = XhMulti::with_default_client().unwrap().timeout(0);
        assert!(multi.timeout.is_none());
    }

    /// timeout(>0) 应设置对应时长
    #[test]
    fn test_timeout_positive_is_some() {
        let multi = XhMulti::with_default_client().unwrap().timeout(30);
        assert_eq!(multi.timeout, Some(Duration::from_secs(30)));
    }

    /// execute 空请求列表应直接返回空结果，不创建任何任务/channel
    #[tokio::test]
    async fn test_execute_empty_returns_empty() {
        let mut multi = XhMulti::with_default_client().unwrap();
        let results = multi.execute().await.unwrap();
        assert!(results.is_empty());
        // execute 会 drain requests，空列表执行后仍为空
        assert!(multi.is_empty());
    }
}
