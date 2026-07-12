# 新增 HTTP 便捷方法与 JSON 解析 Spec

## Why

XHCurl 作为 HTTP 客户端扩展，缺少现代 HTTP 客户端（Guzzle、Symfony HTTP Client、Axios）的标准便捷方法。PHP 使用者在处理 JSON API、构建查询参数、设置 Accept/Content-Type 时需要手动拼接字符串或重复调用 `header()`，代码冗长且易错。本轮聚焦补齐这四类高频场景的便捷方法，提升日常开发体验。

## What Changes

### 新增方法（均无 BREAKING，向后兼容）

- **`XHRequest::query(array $params): $this`**：增量追加 URL 查询参数，与已有 URL 查询参数合并。值自动 URL 编码。
- **`XHRequest::accept(string $type): $this`**：设置 `Accept` header（`header('Accept', $type)` 的语义化别名）。
- **`XHRequest::contentType(string $type): $this`**：设置 `Content-Type` header（`header('Content-Type', $type)` 的语义化别名）。
- **`XHRequest::executeJson(): mixed`**：执行请求并自动 `json_decode` 响应体。Content-Type 非 `application/json` 时抛异常；解析失败抛异常（fail-fast）。

### 不包含的范围（留给后续轮次）

- retry 重试机制（中等难度，需修改 executor）
- CA 证书/客户端证书支持（中等难度，需 reqwest Identity 配置）
- 调试模式/耗时细分（中等难度，需新增 timing 字段）
- 结果顺序选项（中等难度，需 XhMulti 内部索引映射）

## Impact

- Affected specs: 无（新增功能，不修改现有行为）
- Affected code:
  - `rust/src/request.rs`：新增 `query_params` 字段、`query()` 方法、`to_reqwest()` 中查询参数合并逻辑
  - `rust/src/php_ext.rs`：新增 `accept()`/`contentType()`/`executeJson()` 三个 PHP 绑定方法
  - `rust/tests/mock_server.php`：新增 `/echo-query` 与 `/echo-json` 端点（用于测试 query 合并与 JSON 解析）
  - `rust/tests/php_add_http_helpers_test.php`：新增测试文件
  - `README.md`：方法表新增 4 个方法行
  - `rust/Cargo.toml`：版本 1.3.0 → 1.4.0
  - `CHANGELOG.md`：新增 [1.4.0] 条目

## ADDED Requirements

### Requirement: URL 查询参数构建

系统 SHALL 提供 `XHRequest::query(array $params): $this` 方法，将数组参数增量追加为 URL 查询参数。

- 参数键值自动 URL 编码（防注入与特殊字符问题）
- 与已有 URL 查询参数合并（非覆盖）
- 多次调用 `query()` 累加参数（非覆盖）
- 支持 int/float/bool/null 标量值（自动转字符串）
- 空数组 `query([])` 不抛异常，返回 `$this`（无操作）
- 数组元素非标量（如嵌套数组/对象）抛异常（fail-fast）

#### Scenario: 追加查询参数到无参数 URL

- **WHEN** 用户调用 `createRequest('http://example.com/api')->get()->query(['page' => 1, 'limit' => 10])`
- **THEN** 实际请求 URL 为 `http://example.com/api?page=1&limit=10`

#### Scenario: 合并已有查询参数

- **WHEN** 用户调用 `createRequest('http://example.com/api?existing=1')->get()->query(['page' => 2])`
- **THEN** 实际请求 URL 为 `http://example.com/api?existing=1&page=2`（已有参数保留，新参数追加）

#### Scenario: 多次调用累加

- **WHEN** 用户调用 `->query(['a' => 1])->query(['b' => 2])`
- **THEN** URL 含 `a=1&b=2` 两个参数

#### Scenario: 标量值自动转换

- **WHEN** 用户调用 `->query(['active' => true, 'count' => 5, 'rate' => 1.5, 'name' => null])`
- **THEN** 参数值为 `active=1&count=5&rate=1.5&name=`（bool→1/0，null→空字符串）

### Requirement: Accept Header 便捷方法

系统 SHALL 提供 `XHRequest::accept(string $type): $this` 方法，设置 `Accept` header。

- 等价于 `header('Accept', $type)`
- 空字符串 `$type` 抛异常（fail-fast，与 `userAgent('')` 等一致）
- 多次调用覆盖（与 `header()` 行为一致）

#### Scenario: 设置 Accept

- **WHEN** 用户调用 `->accept('application/json')`
- **THEN** 请求头含 `Accept: application/json`

### Requirement: Content-Type Header 便捷方法

系统 SHALL 提供 `XHRequest::contentType(string $type): $this` 方法，设置 `Content-Type` header。

- 等价于 `header('Content-Type', $type)`
- 空字符串 `$type` 抛异常（fail-fast）
- 多次调用覆盖
- 与 `json()`/`form()`/`multipart()` 的自动 Content-Type 设置不冲突（后调用者覆盖前者）

#### Scenario: 设置 Content-Type

- **WHEN** 用户调用 `->contentType('application/xml')->body('<xml/>')`
- **THEN** 请求头含 `Content-Type: application/xml`

### Requirement: JSON 响应自动解析

系统 SHALL 提供 `XHRequest::executeJson(): mixed` 方法，执行请求并自动解析 JSON 响应体。

- 内部调用 `execute()`，对 `body` 字段执行 `json_decode($body, true)`（关联数组形式）
- Content-Type 不含 `application/json` 时抛异常（提示"期望 application/json 响应，实际为 {type}"）
- `json_decode` 失败（返回 null 且 `json_last_error()` 非 0）时抛异常（含 `json_last_error_msg()`）
- 成功路径返回解析后的 PHP 值（数组/标量/null）
- 失败请求（`success=false`）抛异常（含 error 字段）
- 不影响 `execute()` 的返回数组结构

#### Scenario: 成功解析 JSON 响应

- **WHEN** 用户调用 `->get()->executeJson()`，响应 Content-Type 为 `application/json`，body 为 `{"name":"test","age":30}`
- **THEN** 返回 `['name' => 'test', 'age' => 30]` 关联数组

#### Scenario: 非 JSON Content-Type 抛异常

- **WHEN** 用户调用 `->get()->executeJson()`，响应 Content-Type 为 `text/html`
- **THEN** 抛异常，消息含"期望 application/json"与"实际为 text/html"

#### Scenario: JSON 解析失败抛异常

- **WHEN** 用户调用 `->get()->executeJson()`，响应 body 为 `{"invalid":}`（无效 JSON）
- **THEN** 抛异常，消息含 `json_last_error_msg()` 的内容

#### Scenario: 请求失败抛异常

- **WHEN** 用户调用 `->get()->executeJson()`，请求失败（`success=false`）
- **THEN** 抛异常，消息含 `error` 字段内容
