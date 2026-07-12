# 统一 Setter/Getter 契约与校验时机 Spec

## Why
上一轮 align-php-user-api-consistency 修复了 6 个 getter 和若干校验问题后，新一轮 PHP 使用者视角审查发现仍存在 7 处 API 不一致：1 个 P1（`customMethod()` 设置后 `getMethod()` 返回与实际请求不符的值，且无 `getCustomMethod()` 可查），3 个 P2（负值处理策略分裂、`timeout(0)` 语义分裂、ASCII 校验时机分裂），3 个 P3（`proxy(null)` 独有 clear 语义、body 系列无 getter、URL 不可变更）。这些问题会让 PHP 用户在调试与请求复用场景下得到与实际网络行为不一致的读取值，或遇到错误反馈点远离根因的体验。

## What Changes
- **P1-1 修复 `getMethod()` 误导**：暴露 `getCustomMethod(): ?string`；并在设置了 `customMethod` 时让 `getMethod()` 返回自定义方法名（与实际 `to_reqwest` 行为一致）
- **P2-1 统一负值处理**：XHRequest 的 `timeout`/`timeoutMs`/`connectTimeout`/`connectTimeoutMs`/`maxRedirects` 对负值改为抛异常（与 XHMulti/XHThreadPool 的 `maxConcurrency`/`maxResponseSize`/`timeout` 一致） **BREAKING**
- **P2-2 `timeout(0)` 语义文档化**：XHRequest.timeout(0) 意为"用全局默认"，XHMulti/XHThreadPool.timeout(0) 意为"无批量超时"——两者语义不同属设计取舍，在 README 与 PHP doc 中显式说明差异，不强行统一
- **P2-3 ASCII 校验前置**：`cookies`/`encoding`/`range`/`userAgent` 的非 ASCII 校验从 execute 阶段提前到 setter 阶段（与 `header`/`basicAuth`/`bearerToken` 的 fail-fast 一致） **BREAKING**
- **P3-1 null-clear 语义对齐**：为 `basicAuth`/`bearerToken`/`userAgent`/`encoding`/`range`/`cookies` 增加 `null` 参数的清除语义（与 `proxy(null)` 一致）；新增 `clearXxx()` 显式方法作为备选
- **P3-2 暴露 `getBody(): ?string`**：Rust 侧 `get_body()` 已存在，桥接到 PHP
- **P3-3 暴露 `url()` setter**：Rust 侧 `url()` 已存在，桥接到 PHP（支持请求模板复用不同 URL）

## Impact
- Affected specs: align-php-user-api-consistency（前置）、harden-edges-and-add-getters（getter 基础）、align-threadpool-api-and-cookie-safety（负值校验前置）
- Affected code:
  - `rust/src/php_ext.rs`：XHRequest 的 timeout/timeoutMs/connectTimeout/connectTimeoutMs/maxRedirects setter、customMethod getter、body getter、url setter、null-clear 语义
  - `rust/src/request.rs`：ASCII 校验前置到 builder 方法（已有 `validate_ascii_header_value`，需在 setter 时调用而非 to_reqwest 时）
  - `README.md`：timeout(0) 语义差异说明、新增 getter/setter 文档
- 受 BREAKING 影响的 PHP 用户代码：
  - 传负值给 timeout/timeoutMs/connectTimeout/connectTimeoutMs/maxRedirects 的代码（应改为传 0 或正数）
  - 传非 ASCII 给 cookies/encoding/range/userAgent 且依赖延迟到 execute 报错的代码（现在会立即抛异常）

## ADDED Requirements

### Requirement: 自定义方法读取对称性
XHRequest SHALL 暴露 `getCustomMethod(): ?string` 返回 `customMethod()` 设置的值，未设置返回 null。

#### Scenario: 设置后能取回
- **WHEN** 用户调用 `$req->customMethod('PROPFIND')`
- **THEN** `$req->getCustomMethod()` 返回 `'PROPFIND'`

#### Scenario: 未设置返回 null
- **WHEN** 用户未调用 `customMethod()`
- **THEN** `$req->getCustomMethod()` 返回 `null`

### Requirement: getMethod 反映实际请求方法
XHRequest::getMethod() SHALL 返回实际将使用的 HTTP 方法名：当设置了 `customMethod` 时返回自定义方法名，否则返回标准方法枚举名。

#### Scenario: 设置 customMethod 后 getMethod 返回自定义值
- **WHEN** 用户调用 `$req->customMethod('PROPFIND')` 后调用 `getMethod()`
- **THEN** 返回 `'PROPFIND'`（与 `to_reqwest` 实际使用的方法一致）

#### Scenario: 未设置 customMethod 时 getMethod 返回标准方法
- **WHEN** 用户调用 `$req->get()` 后调用 `getMethod()`
- **THEN** 返回 `'GET'`

### Requirement: 请求体 getter
XHRequest SHALL 暴露 `getBody(): ?string` 返回 `body()`/`json()`/`form()` 设置的请求体字符串，未设置返回 null。

#### Scenario: body 设置后能取回
- **WHEN** 用户调用 `$req->body('raw text')`
- **THEN** `$req->getBody()` 返回 `'raw text'`

#### Scenario: 未设置返回 null
- **WHEN** 用户未设置 body
- **THEN** `$req->getBody()` 返回 `null`

### Requirement: URL setter
XHRequest SHALL 暴露 `url(string $url): $self_` 链式 setter，允许在构造后变更请求 URL。

#### Scenario: 链式变更 URL
- **WHEN** 用户调用 `XHCurl::createRequest($u1)->get()->url($u2)`
- **THEN** `getUrl()` 返回 `$u2`，且链式调用不被打断

### Requirement: null-clear 语义
XHRequest 的 `basicAuth`/`bearerToken`/`userAgent`/`encoding`/`range`/`cookies` setter SHALL 接受 `null` 参数以清除该字段（与 `proxy(null)` 一致）。

#### Scenario: null 清除已设值
- **WHEN** 用户调用 `$req->userAgent('X')->userAgent(null)`
- **THEN** `$req->getUserAgent()` 返回 `null`

## MODIFIED Requirements

### Requirement: 数值 setter 负值处理
XHRequest 的 `timeout`/`timeoutMs`/`connectTimeout`/`connectTimeoutMs`/`maxRedirects` setter SHALL 对负值抛异常（与 XHMulti/XHThreadPool 的 maxConcurrency/maxResponseSize/timeout 一致），错误信息含字段名与"负值"关键词。

#### Scenario: 负值抛异常
- **WHEN** 用户调用 `$req->timeout(-1)`
- **THEN** 抛出 Exception，message 含 `timeout` 与 `负值`

#### Scenario: 零值不抛异常
- **WHEN** 用户调用 `$req->timeout(0)`
- **THEN** 不抛异常（0 = 使用全局默认，与现有行为一致）

### Requirement: ASCII 校验时机
XHRequest 的 `cookies`/`encoding`/`range`/`userAgent` setter SHALL 在 setter 调用时立即校验非 ASCII 字符（与 `header`/`basicAuth`/`bearerToken` 一致），而非延迟到 execute。

#### Scenario: setter 时抛异常
- **WHEN** 用户调用 `$req->userAgent('Client 😀')`
- **THEN** 立即抛出 Exception，message 含 `userAgent` 与非 ASCII 相关说明

#### Scenario: 合法 ASCII 不抛异常
- **WHEN** 用户调用 `$req->userAgent('MyClient/1.0')`
- **THEN** 不抛异常

## REMOVED Requirements
（无移除，仅修改与新增）

## 设计取舍说明

### timeout(0) 语义不强行统一（P2-2）
XHRequest.timeout(0) = "用全局默认超时"（单请求级，0 表示不覆盖全局配置）
XHMulti/XHThreadPool.timeout(0) = "无批量超时"（批量级，0 表示不限制批量总时长）

两者语义不同属合理设计：单请求的 0 是"不覆盖"，批量的 0 是"不限制"。强行统一会破坏现有行为。本 spec 选择**文档化差异**而非强行统一。
