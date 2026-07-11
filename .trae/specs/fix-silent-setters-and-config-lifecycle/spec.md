# 消除残留静默失败与配置生效生命周期 Spec

## Why

上一轮（`fix-silent-failures-and-usability`）只覆盖了 `json()/method()/multipart()/setUserData()` 四处静默失败，但 `cookies()/encoding()/range()/userAgent()/form()/header()` 仍存在"设置非法值时静默丢弃"的同模式缺陷；更严重的是 `setConfig()` 修改全局 `proxy`/`verify_ssl` 等配置后，无覆盖请求仍使用旧的全局 Client（`OnceLock` 永不重建），导致代理隐私失效、SSL 验证未关闭等数据安全问题。本轮聚焦"消除残留静默失败"与"配置变更生效"两个主题，延续上一轮的修复目标。

## What Changes

### P0：静默丢弃与配置失效（数据安全）
- **修复全局配置不生效**：`setConfig()` 成功后重建全局 Client，使 `proxy`/`verify_ssl`/`user_agent`/`http2_enabled`/`tcp_keepalive`/`max_connections` 等影响 Client 构建的配置对**无覆盖请求**立即生效
- **修复 `cookies()/encoding()/range()/userAgent()` 静默丢弃**：`HeaderValue::from_str` 失败时改为抛 PHP 异常，错误信息含字段名和原始值
- **修复 `XHThreadPool::execute()` 资源丢失**：调整执行顺序为"先 `global_client()` → 再 `take` requests/pool"，与 `XHMulti::execute()` 一致

### P1：残留静默失败与对称性
- **修复 `form()` 静默丢弃非标量值**：遇到数组/对象/资源时抛异常，提示用 `multipart()` 或 `json()`
- **`header()` 改为 fail-fast**：调用时即用 `HeaderValue::from_str` 校验，错误信息含 header 名和值
- **新增 `connectTimeoutMs(int $ms)`**：与 `timeoutMs` 对称，支持亚秒级连接超时
- **暴露 `headers(array $headers): $this`**：批量设置请求头（核心库已有 `headers()`，PHP 层未暴露）
- **`execute()` 空请求列表抛异常**：`XHMulti::execute()`/`XHThreadPool::execute()` 遇到空 requests 时抛异常（当前静默返回空数组）

### P2：低成本改进
- **HEAD 请求跳过 body 读取**：按 RFC 7231，HEAD 响应无 body，跳过 `stream.chunk()` 循环
- **`basicAuth()` 空值校验**：空字符串/无冒号格式时抛异常并给出修复建议

## Impact
- Affected specs: `fix-silent-failures-and-usability`（延续主题）、`align-streaming-callback-contract`（执行路径相关）
- Affected code:
  - [php_ext.rs](file:///workspace/rust/src/php_ext.rs) — setter 校验、execute 顺序、headers 暴露、空请求报错
  - [request.rs](file:///workspace/rust/src/request.rs) — cookies/encoding/range/user_agent 校验抛错、connect_timeout_ms 字段、basicAuth 校验
  - [curl.rs](file:///workspace/rust/src/curl.rs) — 全局 Client 重建机制
  - [executor.rs](file:///workspace/rust/src/executor.rs) — HEAD 跳过 body
  - [README.md](file:///README.md) — 新增 connectTimeoutMs/headers、配置生效说明、空请求报错说明

## ADDED Requirements

### Requirement: 全局配置变更立即生效
The system SHALL rebuild the global reqwest Client when `XHCurl::setConfig()` succeeds, so that `proxy`/`verify_ssl`/`user_agent`/`http2_enabled`/`tcp_keepalive`/`max_connections` changes take effect for requests without request-level overrides.

#### Scenario: setConfig 关闭 SSL 验证后立即生效
- **WHEN** user calls `XHCurl::setConfig(['verify_ssl' => false])` after the global Client was already initialized with `verify_ssl=true`
- **THEN** the next `execute()` on a request without request-level override uses a Client with `verify_ssl=false`

#### Scenario: setConfig 切换代理后立即生效
- **WHEN** user calls `XHCurl::setConfig(['proxy' => 'http://new-proxy:8080'])` after global Client was initialized with a different proxy
- **THEN** subsequent requests without request-level proxy use the new proxy

### Requirement: `connectTimeoutMs()` 毫秒级连接超时
The system SHALL provide `XHRequest::connectTimeoutMs(int $ms): $this` to set connect timeout at millisecond precision, symmetric to `timeoutMs()`.

#### Scenario: 设置 500ms 连接超时
- **WHEN** user calls `$req->connectTimeoutMs(500)->get()->execute()`
- **THEN** the connect phase times out after 500ms if the server is unreachable

### Requirement: `headers()` 批量设置请求头
The system SHALL expose `XHRequest::headers(array $headers): $this` to set multiple headers in one call, each validated immediately (fail-fast).

#### Scenario: 批量设置多个 header
- **WHEN** user calls `$req->headers(['X-A'=>'1','X-B'=>'2'])`
- **THEN** both headers are set; if any value is invalid, an exception is thrown with the header name and value

## MODIFIED Requirements

### Requirement: 链式 setter 输入校验（fail-fast）
All chainable setters that accept string values convertible to `HeaderValue` (`cookies`/`encoding`/`range`/`userAgent`/`header`/`form`) SHALL validate input at call time and throw a PHP exception on invalid input, with an error message containing the field name and original value. Setters SHALL NOT silently discard invalid values.

#### Scenario: cookies 含非 ASCII 字符
- **WHEN** user calls `$req->cookies(['nickname' => '张三'])`
- **THEN** an exception is thrown with message containing "cookies" and "张三", suggesting urlencode

#### Scenario: header 含 NUL 字节
- **WHEN** user calls `$req->header('X-Bad', "value\x00null")`
- **THEN** an exception is thrown immediately (not deferred to execute), with message containing header name "X-Bad" and the value

#### Scenario: form 含数组值
- **WHEN** user calls `$req->form(['roles' => ['admin', 'user']])`
- **THEN** an exception is thrown suggesting use of `multipart()` or `json()`

### Requirement: `XHThreadPool::execute()` 资源保留
`XHThreadPool::execute()` SHALL obtain the global client before taking `requests` and `pool` from `self`, so that a client initialization failure does not lose the user's request list.

#### Scenario: 全局代理无效时 requests 不丢失
- **WHEN** user adds 100 requests to `XHThreadPool` and calls `execute()` while global proxy config is invalid
- **THEN** an exception is thrown AND the 100 requests remain in the object (re-callable after fixing config)

### Requirement: 空请求列表 execute 抛异常
`XHMulti::execute()` and `XHThreadPool::execute()` SHALL throw an exception when the request list is empty, instead of silently returning an empty array.

#### Scenario: 空请求 execute
- **WHEN** user calls `$multi->execute()` without adding any request
- **THEN** an exception is thrown with a clear message

### Requirement: HEAD 请求不读取响应体
`execute_request_inner` SHALL skip body reading when the request method is HEAD, per RFC 7231 §4.3.2.

#### Scenario: HEAD 请求
- **WHEN** user calls `$req->method('HEAD')->execute()`
- **THEN** no body is read from the response stream (empty body, status and headers still returned)

### Requirement: `basicAuth()` 空值校验
`basicAuth(string $credentials)` SHALL reject empty strings and credentials without a colon separator, throwing an exception with a usage hint.

#### Scenario: 空字符串凭据
- **WHEN** user calls `$req->basicAuth('')`
- **THEN** an exception is thrown suggesting format `user:pass`

## REMOVED Requirements
（无移除）
