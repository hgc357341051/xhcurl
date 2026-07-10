# 清理死代码与防止陈旧结果 Spec

## Why

全量代码审计剩余项中，3 个问题值得本轮处理：(1) `PhpXhResponse` 类注册但从未被实例化——`XHRequest::execute()` 返回数组而非对象，`PhpXhResponse.response` 永远为 `None`，所有方法返回默认值，约 104 行死代码误导用户；(2) `XhThreadPool` 的 `result_rx` 跨 `execute`/`execute_each` 调用复用，上一次调用若因 `WorkerShutdown` 提前 break，残留消息会被下一次调用当作新结果读入（默认 `idle_timeout=0` 不触发，但自定义超时场景存在）；(3) 3 处手动 `for _ in 0..len` 迭代未复用已有的 `for_each_kv` 辅助函数，风格不一致。

## What Changes

- 移除 `PhpXhResponse` 类定义（struct + impl，约 104 行）及其在 `#[php_module]` 中的 `.class::<PhpXhResponse>()` 注册。该类从未被实例化、无测试引用、所有方法因 `response: None` 返回默认值，移除不影响任何现有功能。
- `XhThreadPool::execute_all` 在开始收集结果前，drain `result_rx` 中残留的陈旧消息（`WorkerShutdown` 等），防止上一次调用的残留消息污染当前结果。
- 将 `extract_requests`、`opt_string_vec` 中的手动 `for _ in 0..len` 迭代替换为 `for_each_kv` 调用，统一迭代风格。

## Impact

- 受影响文件：
  - `rust/src/php_ext.rs`（移除 PhpXhResponse、extract_requests 改用 for_each_kv、opt_string_vec 改用 for_each_kv）
  - `rust/src/threadpool.rs`（execute_all drain result_rx）
- **BREAKING**: PHP 端 `class_exists('XHResponse')` 将返回 `false`。但该类从未可用（无构造路径、所有方法返回默认值），不影响任何正常使用的代码。
- 修复后：减少 104 行死代码；自定义 `idle_timeout > 0` 场景下结果不再错位；迭代风格统一。

## ADDED Requirements

### Requirement: execute_all drain 陈旧结果

`XhThreadPool::execute_all` SHALL 在开始收集结果前，先 drain `result_rx` 中残留的消息（`WorkerShutdown` 等），防止上一次调用提前 break 时残留的陈旧消息污染当前调用的结果集。

#### Scenario: 前次调用残留不污染当前结果
- **WHEN** 上一次 execute_all 因 WorkerShutdown 提前 break，channel 中残留 WorkerShutdown 消息
- **AND** 下一次 execute_all 开始收集结果
- **THEN** 先 drain 残留消息，不将陈旧消息计入当前结果集

## REMOVED Requirements

### Requirement: PhpXhResponse 类
**Reason**: 该类通过 `#[php_class]` 注册为 PHP 类 `XHResponse`，但无任何构造路径——`XHRequest::execute()` 返回数组而非 `XHResponse` 对象，`PhpXhResponse.response` 字段永远为 `None`，所有方法（`status()`/`body()`/`headers()` 等）返回默认值（0/空/Err）。约 104 行死代码误导用户以为存在对象式响应 API。无测试引用、无代码实例化。
**Migration**: 无需迁移。该类从未可用，PHP 端使用 `execute()` 返回的关联数组即可获取所有响应字段（status/body/headers/url/elapsed_ms 等）。
