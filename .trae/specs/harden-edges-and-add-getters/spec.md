# 边界值统一、Getter 对称与提交失败反馈 Spec

## Why

前两轮消除了 setter 链上的静默丢弃，但仍有三类残留问题影响 PHP 使用者：① **0 值语义危险不统一**——`timeout(0)`/`timeoutMs(0)`/`connectTimeout(0)` 会触发 reqwest `Duration::from_secs(0)` 即**立即超时**（而非使用者直觉的"无超时"），且 `connectTimeout(0)` 还会无谓重建 Client；② **getter 严重不对称**——Rust 端有完整 getter，PHP 端仅暴露 `getUrl`/`getMethod`，使用者无法在 execute 前检查/日志已设置的配置；③ **批量提交失败静默丢失**——`XHThreadPool` 队列满时 `submit` 失败仅 `eprintln`，PHP 端收到短于预期的结果数组且无任何错误信号（`MAX_REQUESTS_PER_BATCH=10000` 远大于默认队列 `1000`）。本轮延续"消除静默失败"主题，补齐这些边界与对称性缺口。

## What Changes

### P0：提交失败不再静默
- **`XHThreadPool::execute_all`/`execute_each` 提交失败转为错误**：当 `submitted_count < requests.len()` 时返回 `Err`（含失败数量），而非 `Ok(部分结果)` + eprintln，避免大批量场景下结果静默丢失

### P1：0 值语义统一与边界校验
- **`timeout(0)`/`timeoutMs(0)` 不再立即超时**：在 `to_reqwest()` 中对 `request_timeout`/`request_timeout_ms`/`connect_timeout`/`connect_timeout_ms` 增加 `if > 0` 判断（0/负值跳过设置 = 使用全局默认）
- **全局 `connect_timeout=0`/`request_timeout=0` 同样跳过**：在 `curl.rs::create_client` 中增加 `if > 0` 判断
- **`connectTimeout(0)` 不触发 Client 重建**：在 `needs_request_client()` 中对 `connect_timeout` 用 `.filter(|&s| s > 0)` 过滤
- **`cookies()` 数组形式对齐 `form()` 类型转换**：整型/浮点/布尔值转为字符串（`true→"1"`、`false→"0"`、`123→"123"`），数组/对象/资源抛异常（当前仅处理字符串值，其余静默跳过）
- **`multipart()` 空 name 校验**：字段解析后校验 `!name.is_empty()`，为空抛异常；非数组元素抛异常（当前 `continue` 静默跳过）
- **`body()` 非字符串输入抛异常**：null/int/float/bool/array/object 在 `binary()`/`string()` 均返回 None 时，抛异常 `"body 参数必须是字符串"`（当前静默返回空 Vec）
- **`headers()` 列表数组校验**：检测所有键为整数（纯列表数组）时抛异常，提示需关联数组 `['name' => 'value']`
- **`xhrun` env 非字符串值转换**：整型/浮点/布尔转为字符串，而非静默跳过

### P1：Getter 对称
- **XHRequest 补充 getter**：`getTimeout()`/`getConnectTimeout()`/`getHeaders()`/`getCookies()`/`getProxy()`/`getVerifySsl()`/`getUserAgent()`/`getId()`/`getUserData()`（均为一行委托 Rust 端已有 getter）
- **XHMulti/XHThreadPool 补充 `count()`/`isEmpty()`**：委托 `len()`/`is_empty()`

### P2：文档
- README 补充：onHeaders 回调 headers 键名小写说明、timeout 与 batchTimeout 语义区分说明、0 值语义说明
- 链式 setter 校验说明更新（cookies 类型转换、body/multipart/headers 校验）

## Impact
- Affected specs: `fix-silent-setters-and-config-lifecycle`（延续静默失败主题）、`fix-silent-failures-and-usability`（边界统一）
- Affected code:
  - [request.rs](file:///workspace/rust/src/request.rs) — to_reqwest 的 0 值判断、needs_request_client 的 filter
  - [curl.rs](file:///workspace/rust/src/curl.rs) — create_client 的 0 值判断
  - [threadpool.rs](file:///workspace/rust/src/threadpool.rs) — execute_all/execute_each 提交失败返回 Err
  - [php_ext.rs](file:///workspace/rust/src/php_ext.rs) — cookies 类型转换、multipart/body/headers 校验、xhrun env、新增 getter、count/isEmpty
  - [README.md](file:///README.md) — getter 文档、0 值语义、headers 小写说明

## ADDED Requirements

### Requirement: XHRequest getter 对称
The system SHALL expose PHP getter methods for all request-level configuration: `getTimeout()`/`getConnectTimeout()`/`getHeaders()`/`getCookies()`/`getProxy()`/`getVerifySsl()`/`getUserAgent()`/`getId()`/`getUserData()`, delegating to existing Rust getters.

#### Scenario: 检查已设置的配置
- **WHEN** user calls `$req->timeout(30)->getTimeout()`
- **THEN** returns 30 (the configured value)

#### Scenario: 未设置时返回默认值或 null
- **WHEN** user calls `$req->getProxy()` without setting a proxy
- **THEN** returns null (no request-level proxy override)

### Requirement: XHMulti/XHThreadPool count/isEmpty
The system SHALL expose `count(): int` and `isEmpty(): bool` on XHMulti and XHThreadPool, returning the number of pending requests.

#### Scenario: 检查待执行请求数
- **WHEN** user calls `$multi->add($req1)->add($req2)->count()`
- **THEN** returns 2

## MODIFIED Requirements

### Requirement: timeout 类配置 0 值统一为"跳过/使用默认"
All timeout-related setters (`timeout`/`timeoutMs`/`connectTimeout`/`connectTimeoutMs`) and global config (`connect_timeout`/`request_timeout`) SHALL treat 0 as "skip setting / use global default", NOT as "immediate timeout". Negative values SHALL also be skipped.

#### Scenario: timeout(0) 不立即超时
- **WHEN** user calls `$req->timeout(0)->get()->execute()`
- **THEN** the request uses the global default request timeout (or no timeout if unset), NOT an immediate timeout

#### Scenario: connectTimeoutMs(0) 不触发 Client 重建
- **WHEN** user calls `$req->connectTimeoutMs(0)`
- **THEN** no request-level Client is built (uses global Client), no unnecessary connection pool reset

### Requirement: XHThreadPool 提交失败反馈
`XHThreadPool::execute_all` and `execute_each` SHALL return an error (not partial results) when some requests fail to submit due to queue full, with an error message containing the failed count.

#### Scenario: 队列满时 execute 抛异常
- **WHEN** user adds 5000 requests to XHThreadPool with default queue capacity 1000 and calls `execute()`
- **THEN** an exception is thrown with message indicating N requests failed to submit (NOT silently returning 1000 results)

### Requirement: cookies() 数组形式支持标量值
`cookies(array $cookies)` SHALL convert scalar values (int/float/bool) to strings consistently with `form()`: bool true→"1", false→"0", int/float→string representation. Array/object/resource values SHALL throw an exception.

#### Scenario: cookies 整型值
- **WHEN** user calls `$req->cookies(['session' => 12345, 'flag' => true])`
- **THEN** cookie header contains `session=12345; flag=1`

#### Scenario: cookies 数组值
- **WHEN** user calls `$req->cookies(['tags' => ['a', 'b']])`
- **THEN** an exception is thrown

### Requirement: multipart() 字段校验
`multipart()` SHALL validate each field: `name` must be non-empty (throw exception if empty); non-array elements SHALL throw an exception (not silently `continue`).

#### Scenario: multipart 空 name
- **WHEN** user calls `$req->multipart([['value' => 'data']])` (missing name)
- **THEN** an exception is thrown with "multipart 字段缺少 name"

### Requirement: body() 类型校验
`body(mixed $body)` SHALL throw an exception when the input is not a string (null/int/float/bool/array/object), instead of silently returning an empty body.

#### Scenario: body 数组
- **WHEN** user calls `$req->post()->body(['key' => 'val'])`
- **THEN** an exception is thrown with "body 参数必须是字符串"

### Requirement: headers() 列表数组校验
`headers(array $headers)` SHALL throw an exception when the input is a list array (all integer keys), suggesting the expected associative format `['name' => 'value']`.

#### Scenario: headers 列表数组
- **WHEN** user calls `$req->headers(['Content-Type', 'application/json'])`
- **THEN** an exception is thrown with "headers() 需要关联数组"

### Requirement: xhrun env 标量值转换
`xhrun` env option SHALL convert scalar values (int/float/bool) to strings, not silently skip them.

#### Scenario: env 整型值
- **WHEN** user calls `xhrun('cmd', [], ['env' => ['VERBOSE' => 1]])`
- **THEN** the VERBOSE=1 env var is set for the subprocess

## REMOVED Requirements
（无移除）
