# 暴露响应体分块流式回调给 PHP Spec

## Why

XHCurl 核心层（`multi.rs`）已实现响应体分块流式能力：`XhMulti::enable_streaming()` 返回
`mpsc::Receiver<(String, StreamEvent)>`，`executor.rs` 在请求过程中发送 `Headers`/`Chunk`/
`Complete`/`Error` 事件（对应 C 版本的 `onChunk`/`onHeader` 回调）。但该能力**未通过
`php_ext.rs` 暴露给 PHP 用户**，PHP 用户只能拿到每个请求完整结束后的整体结果，无法做
响应体实时分块处理（如大文件流式下载、SSE、NDJSON 流式解析）。

同时，README 文档未明确区分两种"流式"概念（请求级 vs 响应体分块级），且特性表未点出
`each`/`executeEach` 流式回调能力，导致用户误以为"只有协程有 each"。

## What Changes

### 1. 新增 `onChunk`/`onHeaders` 回调到 XHMulti（PHP 用户最常用路径）

- **MODIFIED** `XHMulti::executeEach(callable $onResult, ?callable $onChunk = null, ?callable $onHeaders = null): int`
  - 新增两个可选参数 `$onChunk`、`$onHeaders`（向后兼容，已有代码不传这两个参数不受影响）
  - 当传入 `$onChunk` 或 `$onHeaders` 时，内部调用 `XhMulti::enable_streaming()` 启用流式
  - `block_on` 收集循环改用 `tokio::select!` 同时接收 `result_rx` 和 `stream_rx`
  - `$onHeaders` 回调签名：`function(string $requestId, int $status, array $headers): void`
  - `$onChunk` 回调签名：`function(string $requestId, string $chunk): void`（**二进制安全**）
  - 超时分支同样支持 `select!`（用 `tokio::time::timeout` 包裹 select）

### 2. 新增 `onChunk`/`onHeaders` 回调到 XHThreadPool

- **MODIFIED** `XHThreadPool::executeEach(callable $onResult, ?callable $onChunk = null, ?callable $onHeaders = null): int`
  - 同 XHMulti 的新增参数语义
  - 线程池的 `execute_request` 已支持 `stream_tx`，只需在 PHP 层启用并接收
  - `block_on` 收集循环改用 `select!` 同时接收结果 channel 和流式 channel

### 3. StreamEvent → PHP 值转换辅助函数

- **ADDED** `stream_event_to_callback_args(&StreamEvent) -> (status, headers_ht)` 等
  - `Headers` 事件 → `int $status` + `array $headers`（复用 `headers_to_php_array`）
  - `Chunk` 事件 → `string $chunk`（二进制安全，直接传字节）
  - `Complete`/`Error` 事件 → 不单独调回调（结果回调 `onResult` 已覆盖终结语义）

### 4. README 文档澄清

- 特性表新增"流式回调"行，列出 `each`/`executeEach`（请求级）和 `onChunk`/`onHeaders`（响应体分块级）
- 新增"流式回调类型"小节，明确区分：
  - **请求级流式**（已有）：每完成一个请求触发回调（`each`/`executeEach` 的 `$onResult`）
  - **响应体分块级流式**（新增）：请求过程中每收到一块数据触发回调（`$onChunk`/`$onHeaders`）
- 补充 `executeEach` 的新参数签名和示例
- 故障排查补充"流式回调不触发"条目

### 5. PHP 测试验证

- 新增 `php_streaming_test.php`：
  - `onChunk` 回调触发且 chunk 拼接后等于完整 body
  - `onHeaders` 回调触发且 status/headers 正确
  - XHMulti 和 XHThreadPool 两条路径都验证
  - 空请求/无流式回调时行为不变（回归）
- 现有 `php_each_test.php` 等不受影响（新参数可选，向后兼容）

## Impact

- Affected specs: `add-each-streaming-api`、`extend-streaming-and-fix-exception`（executeEach 签名扩展，向后兼容）
- Affected code:
  - `rust/src/php_ext.rs` — `PhpXhMulti::execute_each`、`PhpXhThreadPool::execute_each` 签名 + 实现
  - `rust/src/php_ext.rs` — 新增 `stream_event_to_php_args` 辅助函数
  - `rust/src/multi.rs` — 无改动（核心层已就绪，仅 PHP 层调用 `enable_streaming()`）
  - `rust/src/threadpool.rs` — 可能需暴露 `stream_rx` 给 PHP 层（检查现有接口）
  - `rust/tests/mock_server.php` — 新增大响应体端点（供 onChunk 测试用）
  - `rust/tests/php_streaming_test.php` — 新增
  - `README.md` — 文档更新

## ADDED Requirements

### Requirement: 响应体分块流式回调

系统 SHALL 在 `XHMulti::executeEach` 和 `XHThreadPool::executeEach` 中提供可选的
`$onChunk` 和 `$onHeaders` 回调参数，使 PHP 用户能在 HTTP 请求过程中实时处理
响应体数据块和响应头，无需等待整个请求完成。

#### Scenario: onChunk 回调在下载过程中触发

- **WHEN** 用户调用 `$multi->executeEach($onResult, $onChunk)` 执行批量请求
- **AND** 服务器返回响应体（可能分多个 chunk 传输）
- **THEN** 每收到一个响应体数据块时，`$onChunk($requestId, $chunk)` 被调用
- **AND** 所有 chunk 拼接后等于完整响应体
- **AND** 请求完成后 `$onResult($result)` 仍被调用（含完整 body）

#### Scenario: onHeaders 回调在收到响应头时触发

- **WHEN** 用户调用 `$multi->executeEach($onResult, null, $onHeaders)` 执行批量请求
- **AND** 服务器返回响应头
- **THEN** 收到响应头时 `$onHeaders($requestId, $status, $headers)` 被调用一次
- **AND** `$status` 为 HTTP 状态码（int），`$headers` 为头部关联数组

#### Scenario: 不传可选参数时行为不变（向后兼容）

- **WHEN** 用户调用 `$multi->executeEach($onResult)`（仅传一个参数）
- **THEN** 行为与变更前完全一致（不启用流式，仅触发结果回调）
- **AND** 不产生额外开销（enable_streaming 不被调用）

#### Scenario: 线程安全

- **WHEN** tokio 工作线程执行 HTTP 请求并发送 StreamEvent 到 channel
- **THEN** StreamEvent 通过线程安全 channel 传递到 PHP 线程
- **AND** PHP 回调仅在 PHP 线程上调用（`block_on` 当前线程）
- **AND** tokio 工作线程不触碰 PHP API/zval

## MODIFIED Requirements

### Requirement: XHMulti::executeEach 签名

原有：`public XHMulti::executeEach(callable $callback): int`
修改为：`public XHMulti::executeEach(callable $onResult, ?callable $onChunk = null, ?callable $onHeaders = null): int`

`$onResult` 参数名从 `$callback` 改为 `$onResult`（语义更清晰，但 PHP 端按位置传参不受影响）。
当 `$onChunk` 或 `$onHeaders` 非 null 时，内部启用 `enable_streaming()` 并用 `select!` 分发。

### Requirement: XHThreadPool::executeEach 签名

同上，新增 `$onChunk`/`$onHeaders` 可选参数。
