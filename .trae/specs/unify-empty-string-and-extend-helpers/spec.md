# 统一空字符串校验并扩展易用性 Helper Spec

## Why

第七轮将 `proxy('')`/`setConfig(['proxy'=>''])` 改为 fail-fast 后，链式 setter 中仍有 4 个（`userAgent`/`encoding`/`range`/`cookies`）接受空字符串但语义模糊——PHP 用户无法区分"清除"（`null`）和"设置空值"（`''`）。同时，PHP 用户期望的若干便捷方法（`referer()`、`cookie()` 增量添加、`jsonStr()` 预序列化 JSON、`getHeader(name)` 单值查询、`getMultipart()` 检视、`getBody()` 对 JSON/form 的回查）尚未提供，需通过 `header()` 拼装或无法实现。本轮完成 setter 校验一致性、补齐常用 helper、统一错误消息格式，并配套测试覆盖。

## What Changes

### 1. 空字符串 fail-fast 校验扩展（BREAKING，低风险）
- `userAgent('')` 抛异常，错误消息含"传 null 清除 User-Agent 覆盖"
- `encoding('')` 抛异常，错误消息含"传 null 清除 Accept-Encoding 覆盖"
- `range('')` 抛异常，错误消息含"传 null 清除 Range 覆盖"
- `cookies('')` 字符串路径抛异常，错误消息含"传 null 清除 Cookie 覆盖"
- 与第七轮 `proxy('')` 一致：null 仍为清除路径，空字符串明确报错

### 2. 错误消息格式统一（非 BREAKING）
- `bearerToken('')` 错误消息追加"传 null 清除 Bearer Token"提示，与 `proxy` 风格对齐
- `maxRedirects` 负值错误消息追加"0 = 不跟随重定向"语义提示，与其他数值 setter 一致
- `xhrun` 的 `timeout`/`max_output` 负值错误消息改为"0 = ..."格式，与其他 setter 一致

### 3. 新增便捷 setter（非 BREAKING）
- `referer(?string $referer): $this` —— 设置 Referer header，null 清除，空字符串抛异常，ASCII 校验
- `cookie(string $name, string $value): $this` —— 增量添加单个 cookie（追加到现有 Cookie 字符串，而非覆盖）
- `jsonStr(string $json): $this` —— 传入预序列化的 JSON 字符串作为请求体，自动设置 Content-Type: application/json；无效 JSON 抛异常

### 4. 新增 getter（非 BREAKING）
- `getHeader(string $name): ?string` —— 大小写不敏感查询单个 header 值
- `getMultipart(): ?array` —— 返回已设置的 multipart 字段数组（含 name/contents/filename 三键）；未设置或非 multipart body 返回 null
- `getReferer(): ?string` —— 返回 referer() 设置的值
- 扩展 `getBody(): ?string` —— 对 `BodyType::Json` 返回序列化后的 JSON 字符串，对 `BodyType::Form` 返回 `k1=v1&k2=v2` 格式字符串；`BodyType::Multipart` 仍返回 null（含二进制文件，无法安全序列化）；`BodyType::Bytes` 行为不变

### 5. maxRedirects(0) 文档化与测试（非 BREAKING）
- README 明确说明 `maxRedirects(0)` 等价于 `followRedirects(false)`，均表示不跟随重定向
- 新增测试验证 `maxRedirects(0)` 对重定向请求返回 3xx 而非跟随

### 6. 测试覆盖补齐（非 BREAKING）
- 新增 `php_unify_empty_and_helpers_test.php`，覆盖：
  - 空字符串 setter 抛异常（4 个 setter × 2 用例 = 8）
  - 新增便捷 setter 正常工作 + 边界（referer/cookie/jsonStr）
  - 新增 getter 返回正确值（getHeader/getMultipart/getReferer/getBody 对 JSON/form）
  - 错误消息格式一致性（bearerToken/maxRedirects/xhrun）
  - maxRedirects(0) 行为验证

## Impact

- **Affected specs**:
  - `unify-setter-getter-contract`（扩展 setter/getter 对称性契约）
  - `unify-field-and-safety`（扩展空字符串 fail-fast 一致性）
  - `harden-edges-and-add-getters`（扩展 getter 覆盖）
- **Affected code**:
  - `rust/src/php_ext.rs`：4 个 setter 加空校验、3 个错误消息修改、3 个新 setter、4 个新 getter、1 个 getter 扩展
  - `rust/src/request.rs`：新增 `referer` 字段、`cookie` 增量逻辑、`body_json_str` 暴露给 PHP、`get_multipart` 数据结构
  - `rust/Cargo.toml`：版本 1.1.0 → 1.2.0
  - `CHANGELOG.md`：新增 [1.2.0] 条目
  - `README.md`：新增方法文档、maxRedirects(0) 说明、getBody 行为说明
  - `rust/tests/php_unify_empty_and_helpers_test.php`：新增测试文件

## ADDED Requirements

### Requirement: 空字符串 fail-fast 校验扩展

所有支持 `null` 清除路径的字符串 setter SHALL 在传入空字符串 `""` 时抛 PHP 异常，错误消息 SHALL 包含字段名与"传 null 清除 X 覆盖"提示。已对齐的 setter（`proxy`/`bearerToken`/`basicAuth`）保持现状；本轮扩展 `userAgent`/`encoding`/`range`/`cookies` 至同一契约。

#### Scenario: userAgent 空字符串
- **WHEN** 调用 `->userAgent('')`
- **THEN** 抛 PHP 异常，消息含 `userAgent` 与 `传 null 清除 User-Agent 覆盖`

#### Scenario: userAgent null 清除
- **WHEN** 调用 `->userAgent(null)`
- **THEN** 不抛异常，清除请求级 User-Agent 覆盖（回退到全局配置）

#### Scenario: cookies 空字符串
- **WHEN** 调用 `->cookies('')`
- **THEN** 抛 PHP 异常，消息含 `cookies` 与 `传 null 清除 Cookie 覆盖`

### Requirement: 便捷 setter

XHRequest SHALL 提供以下便捷 setter，与现有链式 API 风格一致（返回 `$this` 或 `Result<$this, String>`）：

#### Scenario: referer 设置
- **WHEN** 调用 `->referer('https://example.com/page')`
- **THEN** 后续 execute() 请求头含 `Referer: https://example.com/page`

#### Scenario: referer 空字符串
- **WHEN** 调用 `->referer('')`
- **THEN** 抛 PHP 异常，消息含 `referer` 与 `传 null 清除 Referer 覆盖`

#### Scenario: cookie 增量添加
- **WHEN** 调用 `->cookie('session', 'abc')->cookie('token', 'xyz')`
- **THEN** 后续 execute() 请求头 Cookie 字段含 `session=abc; token=xyz`（增量追加，不覆盖）

#### Scenario: cookie 空名称
- **WHEN** 调用 `->cookie('', 'value')`
- **THEN** 抛 PHP 异常，消息含 `cookie` 与 `name 不能为空`

#### Scenario: jsonStr 正常
- **WHEN** 调用 `->jsonStr('{"name":"XHCurl"}')`
- **THEN** 后续 execute() 请求头含 `Content-Type: application/json`，body 为该 JSON 字符串

#### Scenario: jsonStr 无效 JSON
- **WHEN** 调用 `->jsonStr('{"name":}')`
- **THEN** 抛 PHP 异常，消息含 `jsonStr` 与 `无效 JSON`

### Requirement: 单值与复合 getter

XHRequest SHALL 提供以下 getter，与现有 getter 风格一致：

#### Scenario: getHeader 大小写不敏感
- **WHEN** 设置 `->header('Content-Type', 'application/json')` 后调用 `getHeader('content-type')`
- **THEN** 返回 `application/json`（小写键查询命中）

#### Scenario: getHeader 未设置
- **WHEN** 未设置某 header 调用 `getHeader('X-Not-Set')`
- **THEN** 返回 `null`

#### Scenario: getMultipart 已设置
- **WHEN** 设置 `->multipart([['name' => 'file', 'contents' => 'data', 'filename' => 'test.txt']])` 后调用 `getMultipart()`
- **THEN** 返回数组，每个元素含 `name`/`contents`/`filename` 三键

#### Scenario: getMultipart 未设置
- **WHEN** 未设置 multipart 调用 `getMultipart()`
- **THEN** 返回 `null`

#### Scenario: getBody 对 JSON
- **WHEN** 设置 `->json(['k' => 'v'])` 后调用 `getBody()`
- **THEN** 返回 JSON 序列化字符串 `{"k":"v"}`

#### Scenario: getBody 对 form
- **WHEN** 设置 `->form(['k' => 'v'])` 后调用 `getBody()`
- **THEN** 返回 `k=v` 格式字符串

#### Scenario: getBody 对 multipart
- **WHEN** 设置 multipart 后调用 `getBody()`
- **THEN** 返回 `null`（含二进制，无法安全序列化）

### Requirement: 错误消息格式一致

所有数值 setter 的负值错误消息 SHALL 包含"0 = ..."语义提示；所有支持 null 清除的字符串 setter 的空字符串错误消息 SHALL 包含"传 null 清除 X 覆盖"提示。

#### Scenario: bearerToken 空字符串消息
- **WHEN** 调用 `->bearerToken('')`
- **THEN** 异常消息含 `bearerToken 不能为空字符串，传 null 清除 Bearer Token`

#### Scenario: maxRedirects 负值消息
- **WHEN** 调用 `->maxRedirects(-1)`
- **THEN** 异常消息含 `maxRedirects 不能为负值，0 = 不跟随重定向`

#### Scenario: xhrun timeout 负值消息
- **WHEN** 调用 `xhrun('echo', [], [], -1)`
- **THEN** 异常消息含 `timeout 不能为负值，0 = 无超时`

## MODIFIED Requirements

### Requirement: getBody 返回范围

`getBody()` 返回通过 `body()`/`json()`/`form()` 设置的请求体内容。对 `body()` 设置的原始字节体返回字符串；对 `json()` 设置的数组返回序列化后的 JSON 字符串；对 `form()` 设置的数组返回 `k1=v1&k2=v2` 格式字符串；对 `multipart()` 设置的字段返回 `null`（含二进制文件内容，无法安全序列化）。

### Requirement: setter 空字符串契约

所有接受 `Option<String>` 参数且支持 null 清除路径的 setter SHALL 在空字符串时抛异常。本轮将 `userAgent`/`encoding`/`range`/`cookies`/`referer` 纳入契约（`referer` 为新增 setter，直接按契约实现）。

## REMOVED Requirements

无。
