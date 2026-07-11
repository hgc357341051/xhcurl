# XHThreadPool API 对称、Cookie 安全与 Getter 补全 Spec

## Why

前三轮消除了 setter 链上的静默失败与边界陷阱，但 XHMulti 与 XHThreadPool 的 API 仍严重不对称——XHMulti 有 `timeout()`/`maxResponseSize()`/`maxConcurrency()`，XHThreadPool 三个都缺，大批量任务无超时保护有资源占用风险；同时 `cookies(array)` 直接 `format!("{}={}", k, v)` 拼接不对 value 转义，含 `;`/`=` 的 value 会破坏 Cookie 头格式甚至注入伪造 cookie（如 `['user' => 'a; admin=1']` 会拼成 `user=a; admin=1`），与 PHP 原生 `setcookie()` 默认 URL 编码的行为不一致；此外 getter 体系在毫秒级超时和多线程配置上仍有缺口。本轮补齐这些对称性与安全缺口。

## What Changes

### P1：XHThreadPool API 对称（与 XHMulti 对齐）
- **新增 `XHThreadPool::timeout(int $secs): $this`**：批量级总时限，与 `XHMulti::timeout` 语义一致（超时后 abort 未完成任务）。0 = 无超时
- **新增 `XHThreadPool::maxResponseSize(int $bytes): $this`**：单响应大小限制，透传给 `ThreadPoolConfig`
- **新增 `XHThreadPool::maxConcurrency(int $max): $this`**：并发数上限（线程池已有 `max_concurrency` 字段，仅打通 PHP 绑定）

### P1：Cookie 安全（防注入）
- **`cookies(array)` 对 value 做 URL 编码**：与 PHP `setcookie()` 默认行为对齐，防止含 `;`/`=`/`,` 的 value 破坏 Cookie 格式或注入伪造 cookie。key 不编码（cookie name 通常为字母数字）

### P2：Getter 对称补全
- **新增 `getTimeoutMs()`/`getConnectTimeoutMs()`**：与 `timeoutMs`/`connectTimeoutMs` setter 对称（当前 getter 只有秒级）
- **新增 XHMulti/XHThreadPool 的 `getMaxConcurrency()`/`getMaxResponseSize()`/`getTimeout()`**：introspect 批次配置

### P2：文档
- README 补充：XHThreadPool 新增方法说明、cookie 数组形式 URL 编码说明、迁移自 Guzzle/curl 的注意事项（timeout 语义、headers 小写）
- 错误处理示例补充 try/catch + if success 完整示例

## Impact
- Affected specs: `harden-edges-and-add-getters`（getter 对称延续）、`fix-silent-setters-and-config-lifecycle`（cookie 安全校验）
- Affected code:
  - [php_ext.rs](file:///workspace/rust/src/php_ext.rs) — XHThreadPool 新增 3 setter、cookie URL 编码、新增 getter
  - [threadpool.rs](file:///workspace/rust/src/threadpool.rs) — execute 路径透传 timeout/maxResponseSize/maxConcurrency
  - [README.md](file:///README.md) — XHThreadPool 方法说明、cookie 编码说明、迁移注意事项

## ADDED Requirements

### Requirement: XHThreadPool 批量级配置
The system SHALL expose `timeout(int $secs)`, `maxResponseSize(int $bytes)`, `maxConcurrency(int $max)` on XHThreadPool, symmetric to XHMulti, to allow per-batch configuration of overall timeout, response size limit, and concurrency cap.

#### Scenario: XHThreadPool 批量超时
- **WHEN** user calls `$pool->timeout(30)->add($req1)->add($req2)->execute()` and one request hangs
- **THEN** execute aborts remaining tasks after 30 seconds and returns results collected so far (with error on the hanging request)

#### Scenario: XHThreadPool maxResponseSize
- **WHEN** user calls `$pool->maxResponseSize(5_000_000)->execute()` and a response exceeds 5MB
- **THEN** that request's result contains `success=false` with error indicating size exceeded

### Requirement: Cookie URL 编码（防注入）
`cookies(array $cookies)` SHALL URL-encode cookie values before joining into the `name=value; name2=value2` string, preventing `;`/`=`/`,` in values from breaking the Cookie header format or injecting forged cookies. Cookie names are NOT encoded (expected to be alphanumeric).

#### Scenario: cookie value 含分号
- **WHEN** user calls `$req->cookies(['user' => 'a; admin=1'])`
- **THEN** the Cookie header is `user=a%3B+admin%3D1` (URL-encoded), NOT `user=a; admin=1` (which would inject a second cookie)

#### Scenario: 正常 cookie 值
- **WHEN** user calls `$req->cookies(['session' => 'abc123', 'count' => 42])`
- **THEN** the Cookie header is `session=abc123; count=42` (alphanumeric values unaffected by URL encoding)

### Requirement: 毫秒级超时 getter
The system SHALL expose `getTimeoutMs(): ?int` and `getConnectTimeoutMs(): ?int` on XHRequest, returning the millisecond-level timeout set by `timeoutMs()`/`connectTimeoutMs()`, symmetric to the setters.

#### Scenario: 获取毫秒超时
- **WHEN** user calls `$req->timeoutMs(500)->getTimeoutMs()`
- **THEN** returns 500

### Requirement: XHMulti/XHThreadPool 批次配置 getter
The system SHALL expose `getMaxConcurrency()`, `getMaxResponseSize()`, `getTimeout()` on XHMulti and XHThreadPool, returning the configured batch-level values (0 = unset/use default).

#### Scenario: 获取批次配置
- **WHEN** user calls `$multi->maxConcurrency(10)->timeout(30)->getMaxConcurrency()`
- **THEN** returns 10; `getTimeout()` returns 30

## MODIFIED Requirements
（无破坏性修改，新增方法与 getter 均为增量）

## REMOVED Requirements
（无移除）
