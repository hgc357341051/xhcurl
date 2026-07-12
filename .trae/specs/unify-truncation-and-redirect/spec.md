# HTTP 响应超限分类与测试覆盖补齐 Spec

## Why

第八轮审计发现 1 项 P1 文档错误与多项 P2 测试盲点。README 第 1208 行声称"响应体超限时 `success` 仍为 `true`、`body` 不完整"，但实现实际返回 `success=false`、`body=""`、`body_size=0`，PHP 用户若按文档判断会被误导。同时 HTTP 响应超限的 `error_type` 被归类为 `"unknown"`，与 `xhrun` 的 `output_too_large` 风格不一致；HTTP 结果数组无 `truncated` 字段，无法程序化区分"超限失败"与"网络错误失败"。此外 `mock_server.php` 缺 `/redirect` 端点，导致 `maxRedirects(0)` 测试是在非重定向端点上验证"等价性"——伪测试。本轮修正文档、补齐 error_type 分类与 truncated 字段、扩展 mock_server 端点、补齐测试覆盖。

## What Changes

### 1. 修正 README 截断行为描述（P1，非 BREAKING）
- 第 1208 行附近，将"截断时 `success` 仍为 `true`，但 `body` 不完整"修正为实际行为：
  响应体超过 `max_response_size` 时，请求被视为失败（`success=false`、`body=""`、`body_size=0`、`error` 含"超过最大限制"、`error_type="response_too_large"`、`truncated=true`）
- 同步修正字段表与 note 块（如有）

### 2. HTTP 响应超限专属 error_type（P2，非 BREAKING）
- `classify_error_type` 函数（或 `result_to_php_array` 失败路径）增加对 Memory 错误的识别
- 错误消息含"超过最大限制"或"响应体"时，`error_type = "response_too_large"`
- 与 `xhrun` 的 `error_type="output_too_large"` 风格对齐
- 值集扩展为 `dns/timeout/ssl/connection/response_too_large/unknown`

### 3. HTTP 结果数组新增 truncated 字段（P2，非 BREAKING）
- HTTP 结果数组新增 `truncated` 布尔字段，默认 `false`
- 仅当响应体超限时设为 `true`
- 成功路径、其他失败路径均为 `false`
- 字段集从 10 扩展到 11（remote_addr/version/.../truncated）
- 与 `xhrun` 的 `truncated` 字段对齐
- 所有响应构建路径（execute/XHMulti/XHThreadPool/fiber_await/gather/each）同步增加此字段

### 4. 扩展 mock_server 端点（P2，非 BREAKING）
- 新增 `/redirect?n=N` 端点：返回 302 重定向到 `/redirect?n=N-1`，n=0 时返回 200 + JSON
- 新增 `/large?size=N` 端点：返回指定大小的响应体（用于触发 max_response_size 截断）
- 现有端点不变

### 5. 补齐测试覆盖（P2，非 BREAKING）
- 新增 `php_unify_truncation_and_redirect_test.php`，覆盖：
  - HTTP 响应体超限：`success=false`/`body=""`/`error_type="response_too_large"`/`truncated=true`
  - HTTP 成功路径：`truncated=false`
  - `maxRedirects(0)` 对 `/redirect?n=1` 返回 302 不跟随
  - `maxRedirects(5)` 对 `/redirect?n=3` 跟随到 200
  - `followRedirects(false)` 对 `/redirect?n=1` 返回 302 不跟随
  - `followRedirects(true)->maxRedirects(5)` 跟随到 200
  - error_type 值集：dns/timeout/connection/response_too_large/""（成功）
  - body_size 与 strlen(body) 一致性断言

## Impact

- **Affected specs**:
  - `unify-field-and-safety`（扩展 error_type 分类与字段集）
  - `unify-xhrun-fields-and-upgrade`（HTTP truncated 字段对齐 xhrun）
  - `fix-response-field-stability`（扩展字段集）
- **Affected code**:
  - `rust/src/php_ext.rs`：`classify_error_type`/`result_to_php_array`/`response_to_php_array` 增加 response_too_large 分类与 truncated 字段
  - `rust/src/executor.rs`：在响应超限时通过错误类型或额外标志传递 truncated 信息（或保持现有错误消息，由 PHP 边界按错误消息分类）
  - `rust/tests/mock_server.php`：新增 `/redirect` 与 `/large` 端点
  - `rust/tests/php_unify_truncation_and_redirect_test.php`：新增测试文件
  - `README.md`：修正截断行为描述、error_type 值集、字段表新增 truncated
  - `rust/Cargo.toml`：版本 1.2.0 → 1.3.0
  - `CHANGELOG.md`：新增 [1.3.0] 条目

## ADDED Requirements

### Requirement: HTTP 响应超限专属 error_type

系统 SHALL 在 HTTP 响应体超过 `max_response_size` 时，将 `error_type` 字段设为 `"response_too_large"`，与 `xhrun` 的 `"output_too_large"` 风格对齐。

#### Scenario: 响应体超限
- **WHEN** 服务器返回超过 `max_response_size` 的响应体
- **THEN** 结果数组 `error_type = "response_too_large"`，`error` 含"超过最大限制"

#### Scenario: 普通网络错误
- **WHEN** 请求因 DNS/连接/超时等失败
- **THEN** 结果数组 `error_type` 仍为 `dns`/`timeout`/`ssl`/`connection`/`unknown` 之一，与 `response_too_large` 区分

### Requirement: HTTP 结果数组 truncated 字段

系统 SHALL 在所有 HTTP 结果数组（execute/XHMulti/XHThreadPool/fiber_await/gather/each）中包含 `truncated` 布尔字段。

#### Scenario: 响应体超限
- **WHEN** 响应体超过 `max_response_size`
- **THEN** `truncated = true`，`success = false`，`body = ""`

#### Scenario: 正常响应
- **WHEN** 响应体未超过限制
- **THEN** `truncated = false`（无论成功或失败）

### Requirement: mock_server 新增重定向与大响应端点

mock_server SHALL 提供以下端点供测试使用：

#### Scenario: /redirect 端点
- **WHEN** 请求 `/redirect?n=N`
- **THEN** 若 N>0 返回 302 + Location: /redirect?n=N-1；若 N=0 返回 200 + JSON `{"redirected":true}`

#### Scenario: /large 端点
- **WHEN** 请求 `/large?size=N`
- **THEN** 返回 200 + 指定 N 字节的响应体（用于触发 max_response_size 截断）

## MODIFIED Requirements

### Requirement: HTTP 结果数组字段集

HTTP 结果数组字段集从 10 扩展到 11：新增 `truncated` 布尔字段。所有路径（成功/失败、execute/XHMulti/XHThreadPool/fiber_await/gather/each/xhrun）字段集保持一致（xhrun 已有 truncated 字段，HTTP 现补齐）。

### Requirement: error_type 值集

HTTP `error_type` 值集扩展为 `dns/timeout/ssl/connection/response_too_large/unknown`，成功路径仍为 `""`。`response_too_large` 专门标识响应体超限场景。

### Requirement: README 截断行为描述

README 中关于响应体超限的描述 SHALL 准确反映实现行为：`success=false`、`body=""`、`body_size=0`、`error` 含"超过最大限制"、`error_type="response_too_large"`、`truncated=true`。原描述"截断时 `success` 仍为 `true`，但 `body` 不完整"已被删除。

## REMOVED Requirements

无。
