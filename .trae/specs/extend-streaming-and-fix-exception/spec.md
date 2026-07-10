# 流式回调扩展 + 异常传播修复 Spec

## Why

存在两个问题:(1) 事件泵 `run_event_loop` 调用 `Fiber::resume()` 后,PHP Fiber 内抛出的异常被 ext-php-rs 的 `try_call_method` 静默吞掉(不检查 `ExecutorGlobals::take_exception`),导致事件泵报"空闲"超时而非传播原始异常,gather/each 均受影响;(2) `XHThreadPool::execute` 和 `XhMulti::execute` 都是全量收集结果后一次性返回,大量请求时内存累积,缺少"先请求成功的先处理"流式回调模式。

## What Changes

### P0:修复事件泵异常传播(高优先级)

- 在 `fiber.rs` 的 `run_event_loop` 中,resume 调用后检查 `ExecutorGlobals::take_exception()`,若存在异常则提取 message 并返回 `Err`。
- 对 `start` 和 `getReturn` 调用也加异常检查,形成完整防御。
- 使用 `ext_php_rs::zend::ExecutorGlobals` API(已确认存在于 ext-php-rs 0.15.15)。

### P1:XhMulti::executeEach 流式回调(中优先级)

- 新增 PHP 方法 `XHMulti::executeEach(callable $callback): int`。
- 复用 XhMulti 的 spawn + channel 机制,在 `block_on` 内循环 recv,每收到一个结果就调用回调,不累积到 Vec。
- 回调签名与 `XHCurl::each` 一致:`function(array $result): void`。
- 回调收到的 `$result` 字段复用 `result_to_php_array`,与 execute/each 一致。

### P2:XHThreadPool::executeEach 流式回调(中优先级)

- 新增 PHP 方法 `XHThreadPool::executeEach(callable $callback): int`。
- 复用 ThreadPool 的 dispatcher/worker 机制,改造收集循环为"recv 一个调一次回调"。
- 回调签名与上面一致。

## Impact

- 受影响文件:
  - `rust/src/fiber.rs`(P0:run_event_loop 异常检查,约 20 行)
  - `rust/src/php_ext.rs`(P1:PhpXhMulti::executeEach + P2:PhpXhThreadPool::executeEach)
  - `rust/src/threadpool.rs`(P2:可能新增 execute_each Rust 方法或直接在 php_ext.rs 层处理)
- 不改动:executor.rs、request.rs、response.rs、multi.rs(保持纯 Rust,PHP 层在 php_ext.rs 处理回调)
- 不影响现有 API:gather/each/execute 行为不变(P0 是修复缺陷,使异常正确传播)
- **P0 修复后**:gather/each 在回调或用户代码抛异常时,异常会正确传播而非超时报"空闲"

## ADDED Requirements

### Requirement: 事件泵异常传播

`run_event_loop` 在调用 `Fiber::resume()` / `Fiber::start()` / `Fiber::getReturn()` 后,SHALL 检查 `ExecutorGlobals::take_exception()`,若存在异常则提取其 message 并返回 `Err`,确保 PHP Fiber 内抛出的异常能正确传播到 `run()` 调用者,而非被静默吞掉导致超时。

#### Scenario: 回调抛异常
- **WHEN** 用户在 `XHCurl::each` 的回调内抛出异常
- **THEN** `run()` 立即返回 `Err`,错误信息包含异常的 message
- **AND** 不出现"事件泵空闲但主 Fiber 未终止"超时错误
- **AND** 后续请求不再触发回调

#### Scenario: gather 后用户代码抛异常
- **WHEN** 用户在 `XHCurl::run` 的回调内,gather 返回后抛出异常
- **THEN** `run()` 立即返回 `Err`,错误信息包含异常的 message
- **AND** 不出现"事件泵空闲"超时

#### Scenario: 正常执行无异常
- **WHEN** Fiber 内无异常抛出
- **THEN** `ExecutorGlobals::take_exception()` 返回 `None`
- **AND** 事件泵正常循环,行为不变

### Requirement: XhMulti::executeEach 流式回调

系统 SHALL 提供 `XHMulti::executeEach(callable $callback): int` 方法,并发执行请求,每完成一个就立即调用回调,不累积全部结果。

#### Scenario: 正常流式处理
- **WHEN** 用户调用 `$multi->executeEach(function($result) { writeToDb($result); })`
- **AND** 有 50 个请求
- **THEN** 请求并发执行(受 maxConcurrency 限制)
- **AND** 每完成一个请求立即调用回调,传入结果数组(字段与 execute 返回元素一致)
- **AND** 回调返回后结果释放,不累积到内存
- **AND** 返回处理总数(= 50)

#### Scenario: 失败请求也触发回调
- **WHEN** 某请求失败
- **THEN** 回调仍被调用,`$result['success'] === false`

#### Scenario: 空请求列表
- **WHEN** 未 add 任何请求就调用 executeEach
- **THEN** 不执行任何请求,返回 0

### Requirement: XHThreadPool::executeEach 流式回调

系统 SHALL 提供 `XHThreadPool::executeEach(callable $callback): int` 方法,并发执行请求,每完成一个就立即调用回调,不累积全部结果。

#### Scenario: 正常流式处理
- **WHEN** 用户调用 `$pool->executeEach(function($result) { saveToDb($result); })`
- **AND** 有 100 个请求
- **THEN** 请求并发执行(受 worker 数限制)
- **AND** 每完成一个立即调用回调
- **AND** 返回处理总数(= 100)

#### Scenario: 仅 CLI 可用
- **WHEN** 在 FPM 模式下调用 executeEach
- **THEN** 返回错误"XHThreadPool 仅在 CLI 模式可用"(与 execute 一致)

## MODIFIED Requirements

### Requirement: 事件泵 run_event_loop 异常处理

`run_event_loop` 在每次调用 PHP Fiber 方法(resume/start/getReturn)后,SHALL 检查 `ExecutorGlobals::take_exception()` 并在存在异常时返回 `Err`。此修复确保 gather/each/await 在用户代码抛异常时行为正确。
