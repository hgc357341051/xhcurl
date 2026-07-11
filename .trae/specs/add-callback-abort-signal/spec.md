# 流式回调返回值控制中止 Spec

## Why

当前三种流式回调（`XHCurl::each()`、`XHMulti::executeEach()`、`XHThreadPool::executeEach()`）的中止决策**只能通过回调内抛异常实现**（throw = 中止剩余任务）。这与 PHP 使用者的直觉不符：使用者希望"根据自己业务情况在回调里判断是否任务异常，然后**显式决定**是继续后续任务还是中断剩余任务"，而非用异常控制流程。用异常控制流程是 PHP 反模式，且无法区分"业务异常"和"意外系统异常"。

## What Changes

- **回调返回值控制中止**：三种流式回调的 `$onResult` 回调返回值 SHALL 被检查：
  - 返回 `false`（严格 `=== false`）→ 中止剩余任务，方法返回已处理的结果数（不视为错误）
  - 返回 `true` / `null` / `void` / 其他值 → 继续处理剩余任务（向后兼容）
- **抛异常仍中止**（保持现状）：回调抛 PHP 异常时仍中止剩余任务并返回错误（向后兼容，不破坏现有代码）
- **辅助函数扩展**：`invoke_streaming_callback` 返回值改为 `Result<bool, String>`，`Ok(true)` = 继续，`Ok(false)` = 使用者请求中止，`Err(msg)` = 异常中止
- **fiber_each 同步扩展**：协程 `each()` 的回调调用也检查返回值
- **README 文档**：补充"回调返回值控制中止"说明与示例

## Impact

- 受影响文件：
  - `rust/src/php_ext.rs`（`invoke_streaming_callback` 返回 `Result<bool, String>`；`PhpXhMulti::execute_each` 和 `PhpXhThreadPool::execute_each` 的回调后检查返回值，`false` 时 break 并返回 `Ok(count)`）
  - `rust/src/fiber.rs`（`fiber_each` 的回调调用检查返回值，`false` 时提前退出循环返回 `Ok(count)`）
  - `README.md`（新增"回调返回值控制中止"小节 + 示例）
  - `rust/tests/php_callback_abort_test.php` 或新测试文件（新增返回 `false` 中止的测试）
- 不改动：`threadpool.rs`、`multi.rs`、`executor.rs`、`request.rs`、`response.rs`、`StreamEvent`、`onChunk`/`onHeaders` 回调（仅 `$onResult` 支持返回值控制）
- **向后兼容**：现有回调返回 `void`/`null` → 继续处理（行为不变）；现有回调抛异常 → 中止（行为不变）

## ADDED Requirements

### Requirement: 流式回调返回值控制中止

三种流式回调的 `$onResult` 回调 SHALL 支持通过返回值控制是否中止剩余任务：返回 `false`（严格 `=== false`）中止剩余任务，返回其他值（`true`/`null`/`void`/任意非 false 值）继续处理。

#### Scenario: 回调返回 false 中止剩余任务
- **WHEN** 用户提交 5 个请求，`$onResult` 回调在第 2 次返回 `false`
- **THEN** 第 1、2 个请求触发回调
- **AND** 第 3、4、5 个请求不再触发回调
- **AND** 后台剩余任务被中止（与抛异常一致：协程 abort task / XhMulti abort_tasks / XHThreadPool drop pool）
- **AND** 方法返回 `int`（已处理的结果数 2，**不视为错误**）

#### Scenario: 回调返回 true 继续处理（向后兼容）
- **WHEN** 回调返回 `true` / `null` / 不返回（void）
- **THEN** 所有请求正常触发回调
- **AND** 方法返回处理总数（与当前行为一致）

#### Scenario: 回调返回非 bool 值继续处理
- **WHEN** 回调返回整数 `0`、空字符串 `''`、空数组 `[]` 等
- **THEN** 视为"继续"（仅严格 `=== false` 才中止）
- **AND** 避免与 PHP 弱类型陷阱（如 `0 == false` 但 `0 !== false`）

#### Scenario: 回调抛异常仍中止（向后兼容）
- **WHEN** 回调抛 PHP 异常
- **THEN** 中止剩余任务并返回错误（行为不变，与当前一致）

#### Scenario: 三种模式行为一致
- **WHEN** 协程 `each()` / `XHMulti::executeEach()` / `XHThreadPool::executeEach()` 的回调返回 `false`
- **THEN** 三者都中止剩余任务并返回已处理数（`int`，非错误）

## MODIFIED Requirements

### Requirement: invoke_streaming_callback 辅助函数

`invoke_streaming_callback` SHALL 返回 `Result<bool, String>`：
- `Ok(true)`：回调正常返回，且返回值非严格 `false` → 继续处理
- `Ok(false)`：回调正常返回，且返回值严格 `=== false` → 使用者请求中止
- `Err(msg)`：回调抛异常 → 异常中止

调用方（`PhpXhMulti::execute_each` / `PhpXhThreadPool::execute_each`）SHALL 根据 `Ok(false)` 退出循环并返回 `Ok(count)`（已处理数，不视为错误），同时执行与异常中止相同的清理（abort 剩余任务 / drop pool）。

### Requirement: fiber_each 回调调用

`fiber_each` 的回调调用 SHALL 检查返回值：回调返回严格 `false` 时提前退出循环并返回 `Ok(count)`（已处理数）。回调抛异常时仍中止（行为不变）。

## REMOVED Requirements

无。本 spec 仅新增返回值控制能力，不删除现有异常中止机制。
