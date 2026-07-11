# XHThreadPool 复用静默失效与 executeEach 超时缺口修复 Spec

## Why

第四轮 `align-threadpool-api-and-cookie-safety` 为 XHThreadPool 补齐了 `timeout()`/`maxResponseSize()`/`maxConcurrency()` 三个 setter，但站在 PHP 使用者角度审查发现两处真实可用性缺陷：

1. **pool 复用时配置静默失效**：`execute()` 中线程池只在 `pool.is_none()` 时创建并应用配置。同对象第二次 `execute()` 时 pool 已存在，用户在两次调用之间修改的 `maxConcurrency()`/`maxResponseSize()` **被静默忽略**——这正是前三轮一直在消除的"静默失败"反模式复发。用户从 `$pool->execute()` → `$pool->maxConcurrency(16)` → `$pool->execute()` 期望并发翻倍，实际毫无变化且无任何信号。

2. **executeEach 不强制 timeout**：XHMulti::executeEach 有完整的 deadline + `tokio::time::timeout(remaining, recv())` 超时机制（约第 1583-1616 行），但 XHThreadPool::executeEach 仅将 `timeout` 存入局部变量 `_timeout`（第 2121 行，下划线前缀显式未用）并注释"暂不强制生效"。同名方法、同字段、同 setter，在两个类上行为不一致——PHP 使用者设 `$pool->timeout(30)` 后调 `executeEach()` 会误以为有超时保护，实际可能无限挂起。

## What Changes

### P1：pool 复用时配置变更生效（或明确报错）
- **方案 A（推荐）**：`execute()`/`execute_each()` 检测到 pool 已存在但 `max_concurrency`/`max_response_size` 与 pool 当前配置不一致时，**重建线程池**（drop 旧的，用新配置创建）。代价是丢失旧 pool 的连接复用，但行为正确可预期。
- **方案 B**：配置变更时抛异常，提示"线程池已创建，配置变更需新建对象"。
- **选 A**：重建比报错更符合 PHP 使用者直觉（链式 setter 后 execute 应反映最新配置）。

### P1：XHThreadPool::executeEach 强制 timeout
- 用 `tokio::time::timeout(remaining, result_rx.recv())` 包裹 recv，与 XHMulti::executeEach 的 deadline 机制对齐。超时后 abort 剩余任务并返回错误（已处理数 + 超时提示）。
- 流式分支（streaming_enabled）的 select! 中加 `tokio::time::sleep_until(deadline)` 分支。

### P2：负值改为抛异常（与前三轮哲学一致）
- `timeout()`/`maxResponseSize()`/`maxConcurrency()` 传负值时抛异常，而非静默跳过。前三轮的核心主题就是消除静默失败，负值是明显的非法输入应报错。
- 0 保持"无超时/使用默认"语义（合法值）。

## Impact
- Affected specs: `align-threadpool-api-and-cookie-safety`（第四轮新增方法的语义修正）
- Affected code:
  - [php_ext.rs](file:///workspace/rust/src/php_ext.rs) — XHThreadPool::execute/execute_each 的 pool 复用逻辑、execute_each 超时、setter 负值校验
- 无破坏性变更：0 值语义不变，仅负值从"静默跳过"改为"抛异常"，符合前三轮既定方向

## ADDED Requirements

### Requirement: pool 复用时配置变更生效
XHThreadPool 在 `execute()`/`execute_each()` 时，若 pool 已存在且用户在两次调用之间修改了 `maxConcurrency`/`maxResponseSize`，系统 SHALL 重建线程池以应用新配置，而非静默沿用旧配置。

#### Scenario: 两次 execute 间修改 maxConcurrency
- **WHEN** user calls `$pool->execute()` then `$pool->maxConcurrency(16)` then `$pool->execute()`
- **THEN** the second execute uses 16 worker threads (not the original count)

#### Scenario: 未修改配置时复用 pool
- **WHEN** user calls `$pool->execute()` twice without changing config between calls
- **THEN** the pool is reused (no recreation overhead)

### Requirement: XHThreadPool::executeEach 强制 timeout
XHThreadPool::executeEach SHALL enforce the batch-level `timeout()` setting, symmetric to XHMulti::executeEach, aborting remaining tasks and returning an error when the deadline is exceeded.

#### Scenario: executeEach 超时中止
- **WHEN** user calls `$pool->timeout(2)->executeEach($cb)` and a request hangs
- **THEN** executeEach aborts after ~2 seconds and returns an error containing the timeout and completed count

### Requirement: 负值抛异常
`timeout()`/`maxResponseSize()`/`maxConcurrency()` on XHMulti and XHThreadPool SHALL throw on negative values, consistent with the "no silent failures" principle. 0 remains a valid "no limit/use default" value.

#### Scenario: 负值 timeout
- **WHEN** user calls `$pool->timeout(-5)`
- **THEN** a PHP exception is thrown with a message indicating timeout must be >= 0

## MODIFIED Requirements
（无破坏性修改，0 值语义不变）

## REMOVED Requirements
（无移除）
