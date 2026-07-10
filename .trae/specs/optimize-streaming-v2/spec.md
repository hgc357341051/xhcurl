# 流式回调资源管理优化 Spec（v2）

## Why

二次审查发现流式回调路径存在 2 个高危资源管理缺陷：(1) `fiber_run` 失败路径未清理 thread_local 调度器，导致一次失败后进程永久无法再次调用 `run()`（误报"不支持嵌套调用"）；(2) `fiber_await`/`gather`/`each` spawn 的 tokio 任务句柄被丢弃，主 Fiber 异常退出时无法 abort，任务残留全局运行时持续消耗资源。此外发现 `create_fiber` 的 refcount 管理不规范（set_object 已 inc_count，注释却写"不增加 refcount"）。

## What Changes

### P0:修复 fiber_run 调度器泄漏（用 RAII guard）

- 引入 `SchedulerGuard` 结构体，`Drop` 时调用 `drop_scheduler()`。
- `fiber_run` 在 `init_scheduler()` 后构造 guard，确保任何提前返回（`create_fiber`/`start`/`take_php_exception` 失败、`run_event_loop` 返回 Err）都触发清理。
- 修复后：单次 `run()` 失败不影响后续 `run()` 调用。

### P1:修复 fiber 路径 tokio 任务泄漏（保存 + abort JoinHandle）

- `Scheduler` 新增 `task_handles: Vec<tokio::task::JoinHandle<()>>` 字段。
- `fiber_await`/`fiber_gather`/`fiber_each` 的 `runtime.spawn(...)` 返回的 JoinHandle 存入 `task_handles`。
- `drop_scheduler()` 先 abort 所有 handle，再 drop Scheduler（与 `XhMulti::execute_each` 的 abort 模式一致）。
- 修复后：主 Fiber 异常退出时，残留 HTTP 任务被中止，不持续占用运行时资源。

### P2:修正 create_fiber 的 refcount 注释与处理

- `set_object(&mut obj)` 实际调用 `val.inc_count()`（ext-php-rs 0.15 源码 zval.rs:1024 确认）。
- 当前代码让 `obj`（ZBox）正常 Drop，Drop 调用 `zend_object_release`（dec_count），净效果：set_object +1，obj drop -1，zv 持有的 refcount 为 1（正确）。
- 修正注释：从"set_object 不会增加 refcount（移交所有权）"改为"set_object 会 inc_count，obj 随后 Drop 时 dec_count，净 refcount=1（zv 独占引用）"。
- 不改变运行时行为（当前 refcount 计算正确），仅修正误导性注释。

## Impact

- 受影响文件：
  - `rust/src/fiber.rs`（P0: SchedulerGuard + fiber_run 重构；P1: Scheduler.task_handles 字段 + spawn 后保存 + drop_scheduler abort；P2: create_fiber 注释修正）
- 不改动：php_ext.rs、threadpool.rs、multi.rs（XhMulti/XHThreadPool 路径已在上个 spec 修复 abort）
- 不影响现有 API 签名和行为（P0/P1 是修复资源泄漏，使错误路径行为正确；P2 仅注释）
- P0 修复后：`run()` 失败后可再次调用（之前需重启进程）
- P1 修复后：异常退出后无残留 tokio 任务（之前会持续执行 HTTP 请求直到完成）

## ADDED Requirements

### Requirement: fiber_run 调度器 RAII 清理

`fiber_run` SHALL 使用 RAII guard（`SchedulerGuard`）确保在任何返回路径（成功或失败）都清理 thread_local 调度器，避免单次失败后永久无法再次调用 `run()`。

#### Scenario: run() 失败后可再次调用
- **WHEN** 用户调用 `XHCurl::run()` 且 Fiber 内抛异常
- **THEN** `run()` 返回 `Err`
- **AND** 调度器被清理（thread_local 重置为 None）
- **AND** 用户可立即再次调用 `run()` 而不报"不支持嵌套调用"

#### Scenario: run() 成功后调度器清理
- **WHEN** `run()` 正常完成
- **THEN** 调度器被清理
- **AND** thread_local 重置为 None

### Requirement: fiber 路径 tokio 任务 abort

`fiber_await`/`fiber_gather`/`fiber_each` spawn 的 tokio 任务句柄 SHALL 被保存，在调度器清理时（`drop_scheduler`）全部 abort，避免主 Fiber 异常退出后任务残留。

#### Scenario: 主 Fiber 异常退出后任务被中止
- **WHEN** `fiber_gather` spawn 了 10 个 HTTP 任务
- **AND** 主 Fiber 抛异常导致 `run_event_loop` 返回 Err
- **THEN** `drop_scheduler` abort 所有未完成的 JoinHandle
- **AND** 残留 HTTP 任务不再继续执行（被 abort）

### Requirement: create_fiber refcount 注释准确性

`create_fiber` 的注释 SHALL 准确描述 `set_object` 的 refcount 语义（inc_count + obj Drop dec_count，净 refcount=1），而非错误的"不增加 refcount（移交所有权）"。

## MODIFIED Requirements

### Requirement: Scheduler 清理

`drop_scheduler` 在 drop Scheduler 前 SHALL 先 abort 所有保存的 `task_handles`，确保无残留 tokio 任务。

### Requirement: fiber_run 错误路径

`fiber_run` 的所有错误返回路径（`create_fiber` 失败、`start` 失败、`take_php_exception` 失败、`run_event_loop` 返回 Err）SHALL 触发调度器清理，通过 RAII guard 保证。
