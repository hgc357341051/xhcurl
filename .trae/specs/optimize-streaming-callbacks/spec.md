# 流式回调代码审查优化 Spec

## Why

刚实现了三条流式回调路径（`XHCurl::each`、`XhMulti::executeEach`、`XHThreadPool::executeEach`）和 P0 异常传播修复。审查发现 3 个实际问题：(1) 回调异常错误处理逻辑重复 3 次违反 DRY；(2) `XhMulti::executeEach` 回调抛异常时剩余 tokio 任务未被 abort，造成任务泄漏；(3) `XHThreadPool::executeEach` 缺少完整性检查，worker 提前退出时会静默返回不完整结果。

## What Changes

### P1: 提取回调调用辅助函数（消除重复）

- 在 `php_ext.rs` 新增私有辅助函数 `invoke_streaming_callback`，封装 `callback_callable.try_call` + `map_err`（匹配 `Error::Exception` 提取 message、其他错误格式化）逻辑。
- `XhMulti::execute_each` 的超时分支、无超时分支，以及 `XHThreadPool::execute_each` 的回调调用，统一替换为调用此辅助函数。

### P2: 修复 XhMulti::executeEach 回调异常时的任务泄漏

- 回调抛异常时（`?` 提前返回），剩余 `tasks: Vec<JoinHandle>` 被 drop 但未 abort，任务在 tokio 运行时上继续执行（HTTP 请求继续发送），造成资源浪费。
- 修复：在 `?` 返回前，先 `for handle in tasks.drain(..) { handle.abort(); }` 中止剩余任务。
- 对超时分支和无超时分支均需处理。

### P3: 补全 XHThreadPool::executeEach 完整性检查

- `execute_all`（threadpool.rs:537）有完整性检查 `results.len() != submitted_count → Err`，但 `execute_each` 没有。
- 当 WorkerShutdown 或 channel 关闭提前 break 循环时，`count` 可能小于 `submitted`，当前静默返回不完整结果。
- 修复：循环结束后，若 `count < submitted`，返回 `Err`（与 `execute_all` 一致）。

### P4: 补充测试覆盖

- `php_multi_each_test.php`：新增回调抛异常终止测试（验证回调异常后返回错误、剩余请求不回调）。
- `php_threadpool_each_test.php`：新增回调抛异常终止测试。
- `php_multi_each_test.php`：新增超时测试（设置短超时 + 慢响应端点，验证超时返回错误并 abort 任务）。

## Impact

- 受影响文件：
  - `rust/src/php_ext.rs`（P1 辅助函数 + 替换 3 处调用；P2 任务 abort；P3 完整性检查）
  - `rust/tests/php_multi_each_test.php`（P4 新增回调异常 + 超时测试）
  - `rust/tests/php_threadpool_each_test.php`（P4 新增回调异常测试）
- 不改动：fiber.rs（fiber_each 的任务取消需要重构架构，属过度工程化，保持已知限制）、threadpool.rs、multi.rs
- 不影响现有 API 签名和行为（P2/P3 是修复缺陷，使错误路径行为正确）

## ADDED Requirements

### Requirement: 回调调用辅助函数

系统 SHALL 在 `php_ext.rs` 提供私有辅助函数 `invoke_streaming_callback`，封装流式回调的 `try_call` + 异常 message 提取逻辑，供 `XhMulti::execute_each` 和 `XHThreadPool::execute_each` 共用，消除重复代码。

#### Scenario: 回调正常返回
- **WHEN** 用户回调正常执行（无异常）
- **THEN** 辅助函数返回 `Ok(())`
- **AND** 调用方继续处理下一个结果

#### Scenario: 回调抛 PHP 异常
- **WHEN** 用户回调抛出 PHP 异常
- **THEN** 辅助函数提取异常 message 返回 `Err(message)`
- **AND** 异常 message 不含 NUL 字节（使用 `extract_exception_message` 而非 Debug 格式化）

### Requirement: XhMulti::executeEach 回调异常时中止剩余任务

`XhMulti::execute_each` 在回调抛异常提前返回时，SHALL abort 所有尚未完成的 tokio 任务，避免任务泄漏。

#### Scenario: 回调异常后任务被中止
- **WHEN** 10 个请求并发执行，第 2 个回调抛异常
- **THEN** 剩余未完成的 tokio 任务被 abort（不再继续发 HTTP 请求）
- **AND** `execute_each` 返回 `Err`（含异常 message）
- **AND** 已完成但未回调的结果被丢弃

### Requirement: XHThreadPool::executeEach 完整性检查

`XHThreadPool::execute_each` 在收集循环结束后，SHALL 检查已处理结果数是否等于已提交请求数，不等时返回错误（与 `execute_all` 行为一致）。

#### Scenario: Worker 提前退出
- **WHEN** worker 线程 panic 导致提前退出
- **AND** 已收集结果数 < 已提交请求数
- **THEN** `execute_each` 返回 `Err`，错误信息说明预期与实际数量

#### Scenario: 正常完成
- **WHEN** 所有提交请求都收到结果
- **THEN** 返回 `Ok(count)`，count == submitted

## MODIFIED Requirements

### Requirement: XhMulti::execute_each 回调调用

`XhMulti::execute_each` 的超时分支和无超时分支 SHALL 通过 `invoke_streaming_callback` 辅助函数调用用户回调，而非内联 `try_call` + `map_err`。回调异常时 SHALL 先 abort 剩余任务再返回错误。

### Requirement: XHThreadPool::execute_each 结果收集

`XHThreadPool::execute_each` 的结果收集循环 SHALL 在退出后检查 `count == submitted`，不等时返回错误。
