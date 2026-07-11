# Checklist

## 核心层适配
- [x] `threadpool.rs` 有 `enable_streaming()` 或等价方法暴露 stream_rx 给调用方
- [x] `XhThreadPool::execute_all`/`execute_each` 的 `execute_request` 调用传递了 stream_tx
- [x] 核心层（multi.rs/threadpool.rs）无破坏性改动

## XHMulti onChunk/onHeaders
- [x] `PhpXhMulti::execute_each` 签名含 `on_chunk: Option<&Zval>` 和 `on_headers: Option<&Zval>`
- [x] 当 on_chunk/on_headers 非 None 时调用 `multi.enable_streaming()`
- [x] 收集循环用 `tokio::select!` 同时接收 result_rx 和 stream_rx
- [x] 超时分支也用 select!（timeout 包裹）
- [x] StreamEvent::Headers → 调 on_headers 回调（requestId, status, headers array）
- [x] StreamEvent::Chunk → 调 on_chunk 回调（requestId, chunk 二进制安全）
- [x] 回调异常时 abort_tasks 并向上传播
- [x] 不传可选参数时行为不变（向后兼容）
- [x] 主循环结束后 drain 残留 stream channel 事件（避免尾部 chunk 丢失）
- [x] null 参数正确处理（PHP null 视为未传）

## XHThreadPool onChunk/onHeaders
- [x] `PhpXhThreadPool::execute_each` 签名含 `on_chunk` 和 `on_headers` 可选参数
- [x] 当非 None 时用 `submit_with_stream` 提交请求并创建 stream channel
- [x] 收集循环用 select! 同时接收结果 channel 和流式 channel
- [x] StreamEvent 分发逻辑与 XHMulti 一致（复用辅助函数）
- [x] 主循环结束后 drain 残留 stream channel 事件

## 辅助函数
- [x] `dispatch_stream_event` 将 StreamEvent 转为 PHP 回调参数
- [x] `extract_callback_error` 统一错误提取（复用 fiber::extract_exception_message）
- [x] headers 转换复用现有逻辑（无重复代码）
- [x] chunk 作为二进制安全字符串传递

## 线程安全
- [x] StreamEvent 通过线程安全 channel 传递到 PHP 线程
- [x] PHP 回调仅在 PHP 线程调用（block_on 当前线程）
- [x] tokio 工作线程不触碰 PHP API/zval

## 文档
- [x] 特性表新增"流式回调"行
- [x] 新增"流式回调类型"小节区分请求级 vs 分块级
- [x] executeEach 签名表更新（XHMulti + XHThreadPool）
- [x] onChunk/onHeaders 使用示例
- [x] each() 章节澄清协程仅支持请求级流式
- [x] 故障排查补充"流式回调不触发"条目

## PHP 测试
- [x] `php_streaming_test.php` 验证 onChunk 回调触发
- [x] chunk 拼接后等于完整 body
- [x] onHeaders 回调触发且 status/headers 正确
- [x] XHMulti 和 XHThreadPool 两条路径都验证
- [x] 不传可选参数时行为不变（回归）
- [x] 回调异常时中止剩余任务
- [x] mock_server.php 支持分块响应（/stream 端点，flush 分段发送）

## 验证
- [x] cargo fmt --check 通过
- [x] cargo clippy --features php -- -D warnings 通过
- [x] cargo test --lib --features php 全部通过（95 passed）
- [x] PHP 扩展编译成功（cargo build --features php）
- [x] 全部 php_*.php 测试通过（7 文件，181 tests，0 failures）
- [x] 从 PHP 使用者角度评估 API 直觉性和可用性
- [x] CHANGELOG 新增 1.0.8 条目
- [x] Cargo.toml 版本 → 1.0.8
