# 流式回调 each API Spec

## Why

现有 `XHCurl::gather()` 并发执行多个请求后一次性返回全部结果,在请求数量大时(如 10 万)会导致内存累积溢出。用户需要一种"先请求成功的先处理,不用等任务全部结束"的模式:每完成一个请求就立即调用用户回调处理并释放,内存恒定。现有 API 无此能力(gather 累积、await 串行、StreamEvent 是响应体流式非请求级)。

## What Changes

- 新增 PHP 方法 `XHCurl::each(array $requests, callable $callback): int` — 并发提交请求,每完成一个就同步调用回调,不累积结果。
- 新增 Rust 函数 `fiber_each` — 镜像 `fiber_gather`,复用同一套 spawn/pending/suspend/事件泵骨架,仅循环体改为"调回调"而非"插入 results"。
- 事件泵 `run_event_loop` 零改动 — 泵只负责 resume(fiber, result),Fiber 内部决定累积或回调。
- 回调异常立即终止 each 并向上抛出(与 gather 行为一致,不静默吞异常)。
- 复用 `result_to_php_array` — 回调收到的 `$result` 字段与 gather 返回元素完全一致(id/success/status/body/headers/elapsed_ms/user_data/error)。
- 复用 gather 的 `Semaphore::new(min(N, 64))` 并发上限与 `bounded(256)` channel 背压机制。

## Impact

- 受影响文件:
  - `rust/src/fiber.rs`(新增 `fiber_each` 函数,约 80 行)
  - `rust/src/php_ext.rs`(新增 `coroutine_each` PHP 方法 + 模块注册)
- 不改动:事件泵、channel、Scheduler、gather、await、executor、multi、threadpool、request、response。
- 不影响现有 API:gather/await/execute 行为不变。
- 新 API 仅在 `XHCurl::run()` 内 + Fiber 上下文中可用,与 gather/await 一致。

## ADDED Requirements

### Requirement: 流式回调 each API

系统 SHALL 提供 `XHCurl::each(array $requests, callable $callback): int` 方法,并发执行请求,每完成一个就立即调用回调处理,不累积全部结果。

#### Scenario: 正常流式处理
- **WHEN** 用户在 `XHCurl::run()` 内调用 `XHCurl::each($requests, function($result) { writeToDb($result); })`
- **AND** 有 100 个请求
- **THEN** 请求并发执行(上限 64)
- **AND** 每完成一个请求,立即调用回调,传入该请求的结果数组(字段与 gather 元素一致)
- **AND** 回调返回后,该结果立即释放,不累积到内存
- **AND** 方法返回处理的总数(= 输入请求数 100)

#### Scenario: 结果按完成顺序触发(非提交顺序)
- **WHEN** 提交 3 个请求,第 1 个最慢
- **THEN** 先完成的请求先触发回调(可能是第 2、3 个)
- **AND** 回调可通过 `$result['id']` 关联提交顺序

#### Scenario: 空请求列表
- **WHEN** 用户调用 `XHCurl::each([], $callback)`
- **THEN** 不 spawn 任何任务,不 suspend
- **AND** 返回 0

#### Scenario: 单个请求
- **WHEN** 用户调用 `XHCurl::each([$req], $callback)`
- **THEN** 该请求执行完成后调用回调一次
- **AND** 返回 1

#### Scenario: 回调抛异常
- **WHEN** 回调内抛出异常
- **THEN** each 立即终止,异常向上传播到 `run()`
- **AND** 已 spawn 的 tokio task 自然结束(结果 channel 在 run 结束后丢弃,不泄漏)
- **AND** 后续请求不再触发回调

#### Scenario: 失败请求也触发回调
- **WHEN** 某请求失败(如连接超时)
- **THEN** 回调仍被调用,`$result['success'] === false`
- **AND** `$result['error']` 含错误信息
- **AND** `$result['body']` 为空字符串

#### Scenario: 内存恒定(不累积)
- **WHEN** 处理 1000 个请求,每个响应体 1MB
- **THEN** 峰值内存约为 单个响应体 + channel 积压(256 × 单结果)
- **AND** 不随请求总数线性增长(对比 gather 会累积 1GB)

#### Scenario: 必须在 run() 内调用
- **WHEN** 用户在 `XHCurl::run()` 外直接调用 `XHCurl::each(...)`
- **THEN** 返回错误 "必须在 XHCurl::run() 协程内调用"

#### Scenario: 背压控制
- **WHEN** 回调处理慢,tokio 完成的请求结果堆积超过 256 条
- **THEN** tokio send 阻塞,新请求的 execute_http_task 等待
- **AND** 并发自动降速,不内存爆炸
- **AND** 回调完成后立即消化队列

## MODIFIED Requirements

无。本 spec 仅新增 API,不修改现有 gather/await/execute 行为。
