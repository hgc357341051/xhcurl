# Tasks

## Phase 1: 核心层适配（最小改动）

- [x] Task 1: 检查 `threadpool.rs` 的 `stream_tx` 暴露路径
  - [x] SubTask 1.1: 确认 `XhThreadPool` 是否有类似 `enable_streaming()` 的方法，或 `execute_all`/`execute_each` 是否接受 stream_rx
  - [x] SubTask 1.2: 如缺失，在 `XhThreadPool` 新增 `enable_streaming()` 返回 `mpsc::Receiver<(String, StreamEvent)>`（参照 `XhMulti::enable_streaming`）
  - [x] SubTask 1.3: 确认 `XhThreadPool::execute_all`/`execute_each` 的 `execute_request` 调用传递了 `stream_tx`（executor.rs 已支持）

## Phase 2: PHP 层暴露 onChunk/onHeaders（XHMulti）

- [x] Task 2: 修改 `PhpXhMulti::execute_each` 签名与实现
  - [x] SubTask 2.1: 签名改为 `execute_each(&mut self, callback: &Zval, on_chunk: Option<&Zval>, on_headers: Option<&Zval>) -> Result<i64, String>`
  - [x] SubTask 2.2: 当 `on_chunk` 或 `on_headers` 非 None 时，调用 `multi.enable_streaming()` 获取 `stream_rx`
  - [x] SubTask 2.3: 收集循环改用 `tokio::select!` 同时接收 `result_rx` 和 `stream_rx`（无超时分支）
  - [x] SubTask 2.4: 超时分支同样用 `select!`（`tokio::time::timeout` 包裹 select）
  - [x] SubTask 2.5: `StreamEvent::Headers` → 调用 `on_headers` 回调（`$requestId, $status, $headers`）
  - [x] SubTask 2.6: `StreamEvent::Chunk` → 调用 `on_chunk` 回调（`$requestId, $chunk` 二进制安全）
  - [x] SubTask 2.7: `StreamEvent::Complete`/`Error` → 不单独调回调（结果回调已覆盖）
  - [x] SubTask 2.8: 回调异常时 `abort_tasks()` 并向上传播（与现有 result 回调异常处理一致）

- [ ] Task 3: 修改 `PhpXhMulti::execute` 也支持流式（可选，评估为不必要）
  - [x] SubTask 3.1: 评估结论 — execute 累积全部结果，流式 chunk 意义有限，不实现
  - [x] SubTask 3.2: 文档已说明"响应体分块级流式仅 executeEach 支持"

## Phase 3: PHP 层暴露 onChunk/onHeaders（XHThreadPool）

- [x] Task 4: 修改 `PhpXhThreadPool::execute_each` 签名与实现
  - [x] SubTask 4.1: 签名改为 `execute_each(&mut self, callback: &Zval, on_chunk: Option<&Zval>, on_headers: Option<&Zval>) -> Result<i64, String>`
  - [x] SubTask 4.2: 当 `on_chunk` 或 `on_headers` 非 None 时，用 `submit_with_stream` 提交请求并创建 stream channel
  - [x] SubTask 4.3: 收集循环改用 `select!` 同时接收结果 channel 和流式 channel
  - [x] SubTask 4.4: StreamEvent 分发逻辑与 XHMulti 一致（复用辅助函数）

## Phase 4: 辅助函数与复用

- [x] Task 5: 新增 `dispatch_stream_event` 辅助函数
  - [x] SubTask 5.1: `php_ext.rs` 新增私有函数，将 `StreamEvent` 转为 PHP 回调参数（status、headers hashtable、chunk bytes）
  - [x] SubTask 5.2: headers 转换复用现有逻辑（无重复代码）
  - [x] SubTask 5.3: chunk 作为二进制安全字符串传递（PHP string 可含任意字节）

## Phase 5: 文档与测试

- [x] Task 6: README 文档更新
  - [x] SubTask 6.1: 特性表新增"流式回调"行（请求级 + 响应体分块级）
  - [x] SubTask 6.2: 新增"流式回调类型"小节，区分请求级（onResult）与分块级（onChunk/onHeaders）
  - [x] SubTask 6.3: 更新 `executeEach` 签名表（XHMulti + XHThreadPool）
  - [x] SubTask 6.4: 补充 onChunk/onHeaders 使用示例（流式下载、SSE 场景）
  - [x] SubTask 6.5: 故障排查补充"流式回调不触发"条目

- [x] Task 7: 新增 `php_streaming_test.php`
  - [x] SubTask 7.1: 测试 `onChunk` 回调触发，chunk 拼接后等于完整 body
  - [x] SubTask 7.2: 测试 `onHeaders` 回调触发，status/headers 正确
  - [x] SubTask 7.3: 测试 XHMulti 和 XHThreadPool 两条路径
  - [x] SubTask 7.4: 测试不传可选参数时行为不变（回归）
  - [x] SubTask 7.5: 测试回调异常时中止剩余任务

- [x] Task 8: 更新 `mock_server.php` 支持流式测试
  - [x] SubTask 8.1: 新增大响应体端点（`/stream?n=20&size=1024`，分块返回，flush 确保分段发送）
  - [x] SubTask 8.2: 确保响应体足够大以触发多个 chunk（>8KB 通常会分块）

## Phase 6: 验证与提交

- [x] Task 9: 运行完整验证流水线
  - [x] SubTask 9.1: `cargo fmt --check`
  - [x] SubTask 9.2: `cargo clippy --features php -- -D warnings`
  - [x] SubTask 9.3: `cargo test --lib --features php`（95 passed）
  - [x] SubTask 9.4: 编译 PHP 扩展（`cargo build --features php`）
  - [x] SubTask 9.5: 启动 mock 服务器，运行全部 `php_*.php` 测试（181 tests, 0 failures）
  - [x] SubTask 9.6: 从 PHP 使用者角度验证：签名直觉性（$onResult + 两个可选）、null 处理、错误传播清晰

- [x] Task 10: CHANGELOG + 版本 + 提交
  - [x] SubTask 10.1: CHANGELOG 新增 `[1.0.8]` 条目
  - [x] SubTask 10.2: Cargo.toml 版本 → 1.0.8
  - [x] SubTask 10.3: `cargo fmt` + git commit + tag v1.0.8

# Task Dependencies
- Task 1（threadpool stream_tx 检查）独立，先行确认是否需改动核心层
- Task 2（XHMulti execute_each）依赖 Task 5（辅助函数）建议先做 Task 5
- Task 4（XHThreadPool execute_each）依赖 Task 1 + Task 5
- Task 3（execute 也支持流式）可选，评估后决定
- Task 6（README）独立，可与 Task 2/4 并行
- Task 7（PHP 测试）依赖 Task 2 + Task 4 + Task 8
- Task 8（mock_server）独立，可与 Task 2/4 并行
- Task 9（验证）依赖所有前序任务
- Task 10（提交）依赖 Task 9 通过
