# FPM 模式保护与超时/负数校验修复 Spec

## Why

全面审查发现 4 个实质性问题：(1) `fiber_run` 未检查 SAPI，在 FPM 模式下用单线程运行时，`recv_timeout` 阻塞 PHP 线程导致 spawn 的 tokio 任务无法执行，陷入无限循环（`XHThreadPool` 有 `sapi_is_cli()` 检查但 Fiber 路径缺失）；(2) `XhMulti::execute()` 超时 `break` 路径未 abort 任务，之后 `handle.await` 无限等待，超时形同虚设；(3) PHP 入口方法将 `i64` 负值直接 `as usize`/`as u64`，产生非预期的巨大数值（如 `Semaphore::new(usize::MAX)`）；(4) `create_fiber` 的 `__construct` 调用后未检查 PHP 异常。

## What Changes

### P0:Fiber 路径添加 SAPI 检查（防 FPM 无限循环）

- 在 `fiber_run` 入口添加 `sapi_is_cli()` 检查，非 CLI 模式返回明确错误。
- 与 `XHThreadPool::execute`/`execute_each` 的 SAPI 检查保持一致。

### P1:修复 XhMulti::execute 超时 break 路径（防无限阻塞）

- 在 `multi.rs` 超时循环的 `now >= deadline` break 前添加 abort + 返回错误，与 `execute_each` 的处理一致。
- 确保 `execute` 超时后不无限 `await` 未完成任务。

### P2:PHP 入口方法添加负数校验

- 为接受 `i64` 数值参数的方法添加负数检查：`timeout`/`connect_timeout`/`max_redirects`/`max_concurrency`/`max_response_size`/`XHThreadPool::__construct(workers)`。
- 负值返回错误或 clamp 到 0，避免转为巨大 usize/u64。

### P3:create_fiber 的 __construct 后检查 PHP 异常

- 在 `create_fiber` 调用 `__construct` 后、返回前调用 `take_php_exception()`，防止构造异常被静默吞掉。

## Impact

- 受影响文件：
  - `rust/src/fiber.rs`（P0: fiber_run SAPI 检查；P3: create_fiber 异常检查）
  - `rust/src/multi.rs`（P1: execute 超时 break 路径 abort）
  - `rust/src/php_ext.rs`（P2: 负数校验）
  - `rust/tests/`（P0-P3 新增测试）
- 不影响现有 API 签名
- P0 修复后：FPM 模式下 `run()` 返回明确错误而非无限循环
- P1 修复后：`execute` 超时后立即返回错误而非无限阻塞
- P2 修复后：负值参数返回错误而非产生巨大数值
- P3 修复后：Fiber 构造异常正确传播

## ADDED Requirements

### Requirement: Fiber 路径 SAPI 检查

`fiber_run` SHALL 在入口检查 SAPI，非 CLI 模式返回错误，避免在 FPM 单线程运行时下因 `recv_timeout` 阻塞导致 tokio 任务无法执行的无限循环。

#### Scenario: FPM 模式下 run 返回错误
- **WHEN** 在 FPM 模式（SAPI 非 cli）下调用 `XHCurl::run()`
- **THEN** 返回错误信息说明仅 CLI 模式可用
- **AND** 不进入事件泵循环（避免无限循环）

### Requirement: XhMulti::execute 超时 break 路径 abort

`XhMulti::execute` 超时收集循环在 `now >= deadline` 退出时 SHALL abort 所有未完成任务并返回错误，而非跳到 `handle.await` 无限等待。

#### Scenario: 超时后立即返回错误
- **WHEN** `execute` 设置了批量超时
- **AND** 超时时刻部分任务未完成
- **THEN** abort 所有未完成的 JoinHandle
- **AND** 返回超时错误信息（含已完成数量）

### Requirement: PHP 入口方法负数校验

接受 `i64` 数值参数的 PHP 方法 SHALL 校验负值，负值时返回错误或 clamp 到 0，避免 `as usize`/`as u64` 产生非预期的巨大数值。

#### Scenario: 负数超时参数返回错误
- **WHEN** 用户调用 `timeout(-1)`
- **THEN** 返回错误（不设置巨大 u64）

## MODIFIED Requirements

### Requirement: create_fiber 异常检查

`create_fiber` 在调用 `Fiber::__construct` 后 SHALL 调用 `take_php_exception()` 检查 PHP 异常，防止构造失败被静默吞掉导致后续 `start` 在未正确构造的 Fiber 上调用。
