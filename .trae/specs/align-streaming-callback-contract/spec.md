# 对齐流式回调行为契约 Spec

## Why

协程 `XHCurl::each()` 回调异常时中止剩余任务（`SchedulerGuard::drop` → abort 全部 task），`XHMulti::executeEach()` 回调异常时也调用 `abort_tasks()` 中止剩余任务。但 `XHThreadPool::executeEach()` 回调异常时仅 `break` 退出循环，**不中止剩余任务**——pool 被存回 `self.pool` 复用，已提交的请求继续在 worker 上执行，结果被丢弃/排空。三者行为不一致，PHP 使用者难以预期"回调抛异常后后台还在跑吗"。

此外，三种模式（协程 `each` / `XHMulti::executeEach` / `XHThreadPool::executeEach`）的**请求级流式回调**核心能力本已齐备（每完成一个请求就回调处理成功/失败结果，不累积，内存恒定），但文档未在一处明确说明三者都支持该能力且行为契约统一，导致使用者误以为"只有协程有 each，其他没有流式回调"。

## What Changes

- **修复 `XHThreadPool::execute_each` 回调异常时不中止剩余任务**：回调异常时不存回 pool（让 pool 在 `block_on` 闭包结束时被 drop，`Drop` 实现会 abort dispatcher + workers），与协程 `each` / `XHMulti::execute_each` 行为一致。下次调用时重建 pool。
- **README 文档澄清**：新增"请求级流式回调行为契约"小节，明确三种模式都支持请求级流式回调（`each` / `executeEach`），回调签名一致（`function(array $result): void`），回调异常都会中止剩余任务并向上传播。
- **测试验证**：新增测试验证 XHThreadPool 回调异常后剩余任务不触发回调，且三者行为一致。

## Impact

- 受影响文件：
  - `rust/src/php_ext.rs`（`PhpXhThreadPool::execute_each` 回调异常分支改为返回 `None` 让 pool 被 drop）
  - `README.md`（新增"请求级流式回调行为契约"小节）
  - `rust/tests/php_streaming_test.php`（新增回调异常中止剩余任务的测试）
- 不改动：`XHMulti`、协程 `fiber.rs`、`threadpool.rs` 核心逻辑、channel、executor、`StreamEvent`。
- 不影响现有 API：`executeEach` 签名不变，正常路径行为不变（pool 仍存回复用）。仅在回调异常路径行为变化（中止剩余任务而非继续后台执行）。

## ADDED Requirements

### Requirement: XHThreadPool 回调异常中止剩余任务

`XHThreadPool::executeEach` 的 `$onResult` 回调抛异常时，系统 SHALL 中止剩余已提交的任务（通过 drop pool → `Drop` 实现 abort dispatcher + workers），与协程 `each()` 和 `XHMulti::executeEach()` 行为一致。

#### Scenario: 回调异常后剩余任务不触发回调
- **WHEN** 用户提交 5 个请求，第 2 个请求的回调抛异常
- **THEN** 第 1、2 个请求触发回调（第 2 个回调抛异常）
- **AND** 第 3、4、5 个请求不再触发回调
- **AND** 后台 worker 被 abort（已 in-flight 的 HTTP 请求被中止）
- **AND** 方法返回错误（异常 message）
- **AND** 下次调用 `executeEach` 时重建线程池（pool 为 None）

#### Scenario: 正常路径行为不变
- **WHEN** 回调不抛异常
- **THEN** 所有请求正常触发回调
- **AND** pool 被存回复用（行为不变，性能不退化）

## MODIFIED Requirements

### Requirement: 三种执行模式的请求级流式回调行为契约

三种模式（协程 `each` / `XHMulti::executeEach` / `XHThreadPool::executeEach`）的请求级流式回调 SHALL 遵循统一行为契约：
- 每完成一个请求（成功或失败）立即调用回调
- 回调收到的 `$result` 数组字段一致（由共享 `result_to_php_array` 生成：`id/success/status/body/headers/elapsed_ms/body_size/url/error?/user_data?`）
- 失败请求也触发回调（`success=false, status=0, body="", error=...`）
- 不累积结果（内存恒定）
- 回调异常时中止剩余任务并向上传播异常（不静默吞异常，不让后台继续跑）
- 返回处理的结果总数（`int`）

## REMOVED Requirements

无。本 spec 仅修复行为不一致和补充文档，不删除现有功能。
