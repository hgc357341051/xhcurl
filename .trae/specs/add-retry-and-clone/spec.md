# 请求重试与克隆 Spec

## Why

站在 PHP 使用者角度，生产环境调用第三方 API 时经常遇到偶发性网络抖动（DNS 失败、连接拒绝、超时），
当前必须自己在 PHP 层写 `for` 循环 + `try/catch` + `usleep` 实现重试，代码冗余且易错；
批量场景下「相同配置、仅 URL 不同」的请求需重复配置，缺少便捷的克隆机制。
Guzzle/axios/Symfony HTTP Client 均内置 retry 与 clone 能力，XHCurl 应补齐这两个高频功能。

## What Changes

- 新增 `XHRequest::retry(int $times, int $delay_ms = 0): $this`：设置失败重试次数与重试间隔
  - `times = 0`（默认）：不重试（保持现有行为）
  - `times > 0`：失败时最多重试 N 次（总尝试次数 = N + 1）
  - `delay_ms`：重试间隔（毫秒），0 = 立即重试；负值抛异常（fail-fast）
  - **重试条件**：仅重试**网络错误**（`status == 0`，即请求未到达服务器：DNS/连接/超时/SSL）。
    HTTP 错误（4xx/5xx）不重试（服务器已响应，属业务逻辑），与 Guzzle 默认行为一致。
  - 结果数组新增 `attempts` 字段（int，1 = 首次未重试，2 = 重试 1 次，依此类推）
  - 影响 `execute()` 与 `executeJson()`（后者内部调前者）
  - 不影响 `XHMulti`/`XHThreadPool`/协程路径（本轮聚焦单请求级）
- 新增 `XHRequest::__clone()` 魔术方法：支持 PHP `clone $req` 安全深拷贝
  - Rust 侧 `XhRequest` 已 derive `Clone`，`__clone()` 直接克隆内部 `request` 字段
  - 未实现时 PHP `clone` 行为未定义（可能浅拷贝导致双重释放），需显式实现
  - 典型场景：批量调用不同 URL，配置相同 → `clone $req` 后仅改 URL
- mock_server 新增 `/flaky` 端点：前 N 次返回 503（网络错误模拟），第 N+1 次返回 200
  - 通过 `?fail=N` 控制失败次数，用文件计数器模拟状态
- mock_server 新增 `/echo-attempts` 端点：回显请求 `X-Attempt` header，用于验证重试次数

## Impact

- Affected specs: 无（新独立主题）
- Affected code:
  - [rust/src/request.rs](file:///workspace/rust/src/request.rs)：XhRequest 新增 `retry_times: u32` + `retry_delay_ms: u64` 字段，withOptions 支持 `retry` key
  - [rust/src/php_ext.rs](file:///workspace/rust/src/php_ext.rs)：PhpXhRequest 新增 `retry()` setter、`__clone()` 魔术方法、`execute()` 内部重试循环、`getRetry()` getter、结果数组新增 `attempts` 字段
  - [rust/src/executor.rs](file:///workspace/rust/src/executor.rs)：execute_request 返回值需区分「网络错误」vs「HTTP 错误」（status 字段）
  - [rust/tests/mock_server.php](file:///workspace/rust/tests/mock_server.php)：新增 `/flaky` 与 `/echo-attempts` 端点
  - [README.md](file:///workspace/README.md)：新增 retry()/clone 说明、attempts 字段说明
  - [rust/Cargo.toml](file:///workspace/rust/Cargo.toml)：1.5.0 → 1.6.0
  - [CHANGELOG.md](file:///workspace/CHANGELOG.md)：新增 [1.6.0] 条目

## ADDED Requirements

### Requirement: 请求重试

系统 SHALL 提供 `XHRequest::retry(int $times, int $delay_ms = 0): $this` 方法，允许在请求失败时自动重试。

#### Scenario: 默认不重试（times=0）

- **WHEN** 用户未调用 `retry()` 或调用 `retry(0)`
- **THEN** `execute()` 行为与 v1.5.0 完全一致，结果数组 `attempts = 1`

#### Scenario: 网络错误重试成功

- **WHEN** 用户调用 `retry(2, 100)`，首次请求网络错误（status=0），第二次成功
- **THEN** `execute()` 返回 `success=true`，`attempts=2`，耗时含 100ms 延迟

#### Scenario: 重试次数耗尽仍失败

- **WHEN** 用户调用 `retry(2)`，三次尝试均网络错误
- **THEN** `execute()` 返回 `success=false`，`attempts=3`，error 含最后一次错误信息

#### Scenario: HTTP 错误不重试

- **WHEN** 用户调用 `retry(3)`，服务器返回 500
- **THEN** `execute()` 返回 `success=false`，`attempts=1`（HTTP 错误不触发重试）

#### Scenario: 4xx 不重试

- **WHEN** 用户调用 `retry(3)`，服务器返回 404
- **THEN** `execute()` 返回 `success=false`，`attempts=1`

#### Scenario: 负值抛异常

- **WHEN** 用户调用 `retry(-1)` 或 `retry(1, -100)`
- **THEN** setter 抛异常（fail-fast，与 timeout 负值一致）

#### Scenario: executeJson 受 retry 影响

- **WHEN** 用户调用 `retry(2)` 后调用 `executeJson()`
- **THEN** 内部 execute() 触发重试，最终失败抛异常时含 attempts 信息

### Requirement: 请求克隆

系统 SHALL 支持 PHP `clone $req` 语法，通过 `__clone()` 魔术方法安全深拷贝 XHRequest 对象。

#### Scenario: 克隆后独立修改

- **WHEN** 用户执行 `$req2 = clone $req1; $req2->url('...')`
- **THEN** `$req1` 与 `$req2` 完全独立，修改 `$req2` 不影响 `$req1`

#### Scenario: 克隆保留所有配置

- **WHEN** 用户配置 `$req`（headers/body/timeout/retry 等）后 `clone $req`
- **THEN** 克隆对象保留所有配置，可继续链式调用

## MODIFIED Requirements

### Requirement: 结果数组字段

结果数组新增 `attempts` 字段（int），所有 HTTP 响应路径（execute/XHMulti/XHThreadPool/协程）均含此字段。
非重试场景 `attempts = 1`。字段集从 11 扩展到 12。

### Requirement: withOptions 支持的选项

`withOptions()` 新增支持 `retry` key（关联数组 `['times' => int, 'delay_ms' => int]`），
对应 `retry()` setter。其他 18 个 key 不变。

## REMOVED Requirements

无。
