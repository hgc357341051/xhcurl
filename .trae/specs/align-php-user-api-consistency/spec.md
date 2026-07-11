# PHP 使用者视角 API 一致性优化 Spec

## Why

第六轮「站在 PHP 使用者角度审查代码」发现 5 个 P1 一致性问题，均影响 PHP 用户对 API 的可预期性：

1. **`cookies()` 数组整数键静默跳过**（php_ext.rs:948-952），与同文件 `headers()` 整数键抛异常（php_ext.rs:585-591）行为不一致——用户传错形式时一个报错一个静默丢失项。
2. **`XHThreadPool::maxConcurrency(0)` 文档说"无限制"，实现却是"使用 CPU 核心数"**（php_ext.rs:1910 注释 + 2034-2037 `if max_concurrency > 0 { config.worker_count = ... }`，未设置时使用 `ThreadPoolConfig::default().worker_count = available_parallelism()`）——用户以为 0 = 不限并发，实际拿到 CPU 核心数。
3. **构造函数 `new XHThreadPool(-4)` 静默 clamp**（php_ext.rs:1856-1864），而 setter `maxConcurrency(-1)` 抛异常（php_ext.rs:1916-1918）——同一参数负值在不同入口行为分裂，PHP 用户难以预期。
4. **多个 setter 缺对应 getter**：`basicAuth`/`bearerToken`/`followRedirects`/`maxRedirects`/`encoding`/`range` 都有 setter 但无 getter（仅 `auth`/`bearer_token`/`follow_redirects`/`max_redirects`/`encoding`/`range` 字段存在）——用户无法回查已设置的值。
5. **`xhrun` 失败路径无 `error_type` 枚举**（php_ext.rs:3314-3323），而 `execute()`/`XHMulti`/`XHThreadPool` 失败路径都有 `error_type`（dns/timeout/ssl/connection/unknown，php_ext.rs:2561-2565）——同一扩展失败字段集分裂，用户不能复用同一套错误分类逻辑。

## What Changes

### P1 一致性修复（必做）

- **P1-1: `cookies()` 数组整数键改为抛异常**
  - `php_ext.rs:948-952` 的 `if key.is_long() { return Ok(()); }` 改为 `return Err(...)`，错误信息与 `headers()` 对齐（"cookies() 不支持列表数组（整数键），请使用关联数组 ['name' => 'value'] 形式"）。
  - **BREAKING**：原静默跳过整数键的代码会抛异常。但原行为属于"静默失败"已被多轮 spec 列为反模式，破坏性可接受。

- **P1-2: `maxConcurrency(0)` 语义文档对齐实现**
  - 修改 `php_ext.rs:1910` 注释、`maxConcurrency` setter 文档字符串，明确"0 = 使用默认（CPU 核心数）"。
  - README 同步修正表述（若存在"0 = 无限制"措辞）。
  - 实现保持不变（0 时使用 `ThreadPoolConfig::default().worker_count`）。**选择文档对齐实现而非实现追加"真无限"**：真无限并发可能压垮目标服务器与本机资源，不符合 PHP 用户稳健预期。

- **P1-3: `XHThreadPool::__construct` 负值改为抛异常**
  - `php_ext.rs:1856-1864` 中 `match workers { Some(n) if n >= 0 => n as usize, _ => 0 }` 改为：负值返回 `Err`（与 setter 一致）。`__construct` 通过 `try_from` 或在 `__construct` 中校验后抛 PHP 异常（ext-php-rs 把 `Err(String)` 转 PHP 异常）。
  - 错误信息：`"XHThreadPool workers 不能为负值，0 = 使用默认（CPU 核心数）"`。
  - **BREAKING**：原 `new XHThreadPool(-4)` 静默使用默认值的代码会抛异常。但与 `maxConcurrency(-1)` 抛异常对齐后行为可预期，破坏性可接受。
  - 现有测试 `test_threadpool_negative_workers_clamped` 需更新为 `try/catch` 断言抛异常。

- **P1-4: 补全 6 个缺失 getter**
  - `php_ext.rs` 的 `PhpXhRequest impl` 新增：
    - `getBasicAuth(): ?string` → 返回 `request.auth`（`user:pass` 格式字符串或 null）
    - `getBearerToken(): ?string` → 返回 `request.bearer_token`
    - `getFollowRedirects(): ?bool` → 返回 `request.follow_redirects`
    - `getMaxRedirects(): ?int` → 返回 `request.max_redirects`（u32 → i64）
    - `getEncoding(): ?string` → 返回 `request.encoding`
    - `getRange(): ?string` → 返回 `request.range`
  - 命名与前序 getter 一致（`getXxx` PHP camelCase）。
  - **非破坏性**：纯新增方法。
  - **不补 `getBody`/`getJson`/`getForm`/`getMultipart`**：这 4 个 setter 互相覆盖（同一请求体字段），单一 getter 语义不明；如未来有需求再单独设计。

- **P1-5: `xhrun` 失败路径补 `error_type` 枚举**
  - `php_ext.rs:3314-3323` 失败分支按错误来源插入 `error_type`：
    - 超时 → `"timeout"`
    - 输出截断 → `"output_too_large"`
    - 退出码非 0 但非超时/截断 → `"exit_error"`
  - 成功路径不插入 `error_type`（与 `execute()` 一致）。
  - README xhrun 返回字段表同步补充 `error_type` 行。
  - **非破坏性**：纯新增字段，成功路径不变。

### P2 改进（同批落地，低风险）

- **P2-1: `bearerToken("")` 空值校验**
  - `php_ext.rs:920-926` `bearer_token` 改为返回 `Result<_, String>`，空字符串抛异常 `"bearerToken 不能为空字符串"`。
  - 与 `basicAuth` 空值校验对齐（php_ext.rs:901-911）。
  - **BREAKING（轻微）**：原 `bearerToken("")` 静默设置空 token 的代码会抛异常。但空 token 必然导致 401，提前抛异常更友好。

- **P2-2: `xhrun` 超时 kill 进程组（防孙进程泄漏）**
  - `php_ext.rs` 中 `Command::new(...)` 在 Unix 平台调用 `.process_group(0)`（rust-lang/libc 提供，等价 `setpgid(0, 0)`），让子进程成为新进程组 leader。
  - 超时 kill 时改用 `killpg(pgid, SIGKILL)`（或 `nix::unistd::killpg`），杀整个进程组（含 shell 派生的孙进程）。
  - 仅 Unix 实现（Windows 行为不同，文档注明）。
  - **非破坏性**：仅增强超时清理能力，正常路径不变。

- **P2-3: `executeEach` 空请求与 `execute` 行为对齐**
  - `XHMulti::execute_each`（php_ext.rs:1538-1541）和 `XHThreadPool::execute_each` 的 `if self.requests.is_empty() { return Ok(0); }` 改为 `return Err(...)`，错误信息与对应 `execute()` 一致。
  - **BREAKING（轻微）**：原 `executeEach([])` 返回 `Ok(0)` 的代码会抛异常。但与 `execute()` 行为对齐后用户可预期。

- **P2-4: `classify_error_type` 子串匹配改为优先级明确的顺序**
  - 当前实现（php_ext.rs:2497-2521）已按 dns → timeout → ssl → connection → unknown 顺序，但缺少注释说明顺序敏感性。
  - 补充文档注释说明"按优先级从高到低匹配，新增类型时需谨慎放置顺序"。
  - 同时增加单元测试覆盖组合场景（如 "dns lookup timeout" 应分类为 dns 而非 timeout）。
  - **非破坏性**：仅文档 + 测试增强。

- **P2-5: `format_request_error_message` 仅特判 timeout**
  - 检查该函数实现，若仅特判 timeout 而忽略其他 error_type，统一通过 `classify_error_type` 分类后格式化。
  - 详细修改依据实际代码而定（实施时 sub-agent 自行判断）。
  - **非破坏性**：仅错误消息文案优化。

- **P2-6: README `error_type` 措辞修正**
  - README:592 "成功路径不含此字段（或为空字符串）" 修正为 "成功路径不含此字段"（实现从未在成功路径插入空字符串）。
  - **非破坏性**：纯文档。

- **P2-7: README "HTTP 0xx" 说法修正**
  - README:597 "某些边缘场景下成功路径的状态码也可能为 0（如 HTTP 0xx）" 修正——HTTP 状态码规范最小为 100（1xx Informational），不存在"HTTP 0xx"。删除该误导性括号注释。
  - **非破坏性**：纯文档。

## Impact

- **Affected specs**: 补全前序 spec 未覆盖的 PHP 使用者视角一致性 gaps：
  - `align-threadpool-api-and-cookie-safety`（第四轮）— 本轮补 cookies 整数键 + maxConcurrency 0 语义
  - `fix-threadpool-reuse-and-each-timeout`（第五轮）— 本轮补构造函数负值与 setter 对齐
  - `harden-edges-and-add-getters`（第三轮）— 本轮补 6 个缺失 getter
  - `harden-error-handling-and-ci`（早期）— 本轮补 xhrun error_type

- **Affected code**:
  - `/workspace/rust/src/php_ext.rs` — cookies 整数键、maxConcurrency 文档、构造函数负值、6 个 getter、xhrun error_type、bearerToken 校验、executeEach 空请求、classify_error_type 注释、xhrun 进程组
  - `/workspace/rust/src/request.rs` — 可能需暴露字段（如 `auth` 已是 pub？需 sub-agent 实施时确认）
  - `/workspace/README.md` — maxConcurrency 0 语义、xhrun error_type 字段、error_type 措辞、HTTP 0xx 修正
  - `/workspace/rust/tests/php_*.php` — 更新 `test_threadpool_negative_workers_clamped` 等受影响测试，新增覆盖新行为的测试

- **Breaking changes**:
  - P1-1: `cookies(['foo', 'bar'])` 列表数组现抛异常（原静默丢项）
  - P1-3: `new XHThreadPool(-1)` 现抛异常（原静默使用默认）
  - P2-1: `bearerToken('')` 现抛异常（原静默设置空 token）
  - P2-3: `$multi->executeEach([])` 现抛异常（原返回 0）
  - 均为"原静默失败改为显式失败"，符合多轮 spec 已确立的修复方向，破坏性可接受。

## ADDED Requirements

### Requirement: cookies 数组形式校验一致性

`XHRequest::cookies()` 数组形式 SHALL 对整数键抛异常，错误信息 SHALL 提示使用关联数组形式，与 `headers()` 行为一致。

#### Scenario: cookies 传列表数组抛异常
- **WHEN** 用户调用 `->cookies(['foo', 'bar'])`
- **THEN** 抛 PHP 异常，message 含 "cookies" 和 "整数键"/"关联数组" 关键词

#### Scenario: cookies 传关联数组正常
- **WHEN** 用户调用 `->cookies(['name' => 'value'])`
- **THEN** 不抛异常，cookie 正确设置

### Requirement: XHThreadPool maxConcurrency 0 语义文档化

`XHThreadPool::maxConcurrency(0)` SHALL 表示"使用默认值（CPU 核心数）"，**不是**"无限制"。文档（注释 + README）SHALL 明确此语义。

#### Scenario: maxConcurrency(0) 使用 CPU 核心数
- **WHEN** 用户调用 `new XHThreadPool()` 或 `->maxConcurrency(0)`
- **THEN** 实际 worker 数为 `available_parallelism()`（CPU 核心数）

### Requirement: XHThreadPool 构造函数负值校验

`XHThreadPool::__construct(workers)` 接收负值 SHALL 抛异常，与 `maxConcurrency` setter 行为一致。0 和正值 SHALL 正常处理。

#### Scenario: 构造函数传负值抛异常
- **WHEN** 用户调用 `new XHThreadPool(-1)`
- **THEN** 抛 PHP 异常，message 含 "workers" 和 "负值" 关键词

#### Scenario: 构造函数传 0 使用默认
- **WHEN** 用户调用 `new XHThreadPool(0)` 或 `new XHThreadPool()`
- **THEN** 不抛异常，worker 数使用 CPU 核心数

### Requirement: XHRequest getter 完整性

`PhpXhRequest` SHALL 为以下 setter 提供对应 getter（返回 `Option`，未设置时 PHP 端返回 `null`）：
- `basicAuth()` ↔ `getBasicAuth(): ?string`
- `bearerToken()` ↔ `getBearerToken(): ?string`
- `followRedirects()` ↔ `getFollowRedirects(): ?bool`
- `maxRedirects()` ↔ `getMaxRedirects(): ?int`
- `encoding()` ↔ `getEncoding(): ?string`
- `range()` ↔ `getRange(): ?string`

#### Scenario: getter 返回已设置的值
- **WHEN** 用户调用 `->bearerToken('abc')->getBearerToken()`
- **THEN** 返回 `'abc'`

#### Scenario: getter 未设置返回 null
- **WHEN** 用户调用 `->getBearerToken()` 未先调用 `bearerToken()`
- **THEN** 返回 `null`

### Requirement: xhrun 失败路径 error_type 字段

`XHCurl::xhrun()` 失败路径（`success=false`）SHALL 在返回数组中插入 `error_type` 字段，取值枚举：
- `"timeout"` — 命令执行超时
- `"output_too_large"` — 输出超过 max_output 被截断
- `"exit_error"` — 退出码非 0（非超时/截断）

成功路径 SHALL 不插入 `error_type` 字段（与 `execute()` 一致）。

#### Scenario: xhrun 超时返回 error_type=timeout
- **WHEN** 用户调用 `XHCurl::xhrun('sleep 10', ['timeout' => 1])`
- **THEN** 返回数组 `success=false`、`timed_out=true`、`error_type='timeout'`

#### Scenario: xhrun 退出码非 0 返回 error_type=exit_error
- **WHEN** 用户调用 `XHCurl::xhrun('exit 1')`
- **THEN** 返回数组 `success=false`、`exit_code=1`、`error_type='exit_error'`

#### Scenario: xhrun 成功不含 error_type
- **WHEN** 用户调用 `XHCurl::xhrun('echo hello')`
- **THEN** 返回数组 `success=true`，**不含** `error_type` 字段

### Requirement: bearerToken 空值校验

`XHRequest::bearerToken(token)` 接收空字符串 SHALL 抛异常，与 `basicAuth` 空值校验一致。

#### Scenario: bearerToken 空字符串抛异常
- **WHEN** 用户调用 `->bearerToken('')`
- **THEN** 抛 PHP 异常，message 含 "bearerToken" 和 "空" 关键词

### Requirement: xhrun 超时清理进程组

`xhrun` shell 模式下（`shell => true`）超时 SHALL 杀整个进程组（含 shell 派生的孙进程），仅 Unix 平台。

#### Scenario: shell 模式超时不留孙进程
- **WHEN** 用户调用 `XHCurl::xhrun('sleep 60', ['shell' => true, 'timeout' => 1])`
- **THEN** 1 秒后返回 `success=false`、`timed_out=true`，且 `sleep 60` 进程已终止（无残留孙进程）

### Requirement: executeEach 空请求一致性

`XHMulti::executeEach` 和 `XHThreadPool::executeEach` 空请求列表 SHALL 抛异常（与对应 `execute()` 一致），不再返回 `Ok(0)`。

#### Scenario: XHMulti executeEach 空请求抛异常
- **WHEN** 用户调用 `(new XHMulti())->executeEach(fn() => null)`
- **THEN** 抛 PHP 异常，message 含 "没有待执行请求"

## MODIFIED Requirements

### Requirement: classify_error_type 优先级顺序

`classify_error_type` SHALL 按以下优先级从高到低匹配（先匹配先返回）：
1. `dns` — DNS 解析失败
2. `timeout` — 超时
3. `ssl` — SSL/TLS 错误
4. `connection` — 连接错误
5. `unknown` — 其他

新增类型时 SHALL 在函数文档注释中说明顺序敏感性。

### Requirement: README 失败路径字段说明准确性

README 失败路径字段说明 SHALL 与实现一致：
- `error_type` 成功路径不含此字段（**非"或为空字符串"**）。
- 不再提及"HTTP 0xx"——HTTP 状态码规范最小为 100。

## REMOVED Requirements

无（本 spec 全部为修改/新增，不删除现有功能）。
