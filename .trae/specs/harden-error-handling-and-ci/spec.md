# 用户视角健壮性与体验完善 Spec

## Why

v1.0.6 发布后，从使用者角度对全部源码、文档、测试、CI 进行了第二轮审查，发现三类**直接影响用户**的问题：
1. **错误处理不一致导致用户踩坑**：`XHRequest::execute()` 在网络错误时抛 PHP 异常，而 `XHMulti::execute()` 返回 `success=false` 数组——README 教的 `if ($result['success'])` 模式对 `execute()` 不适用，新手极易踩坑。
2. **失败路径返回字段不完整**：`result_to_php_array` 失败分支只插入 `status/body/elapsed_ms/error`，缺 `headers/body_size/url` 等，README 声称"字段集一致"但实际不符，用户访问 `$result['headers']` 触发未定义索引警告。
3. **panic 路径让 PHP 进程崩溃**：`global_client()`/`global_runtime()` 用 `.expect()`，`curl.rs`/`header.rs` 的 `RwLock.unwrap()` 在锁中毒时 panic——FPM worker 一旦崩溃会持续重启。
4. **配置/文档脱节**：`http2_enabled` 配置项未在 README 文档化，`getConfig()` 在 `proxy` 为 None 时省略该键，`options()` 快捷方法缺失。
5. **CI 失效**：clippy/test 未启用 `--features php`，约 2400 行用户面代码（含 xhrun 安全转义）从未被 CI 检查；"验证扩展可加载"步骤用 `|| true` 永不失败。

本 spec 聚焦"让用户用得不崩溃、用得明白、CI 真正保障质量"，不做无关重构。

## What Changes

### 错误处理统一化（用户最直接感知）
- **`XHRequest::execute()` 统一返回结果数组**：网络/DNS/TLS 错误不再抛异常，改为包装成 `success=false` 的结果数组返回（与 `XHMulti`/fiber 路径一致）。**BREAKING**：原 `execute()` 抛异常的代码需改为检查 `$result['success']`（但这是 README 一直教的模式，实际是修正实现匹配文档）。
- **`global_client()`/`global_runtime()` 不再 panic**：改为返回 `Result`，错误以 PHP 异常形式抛出，让用户可 `try/catch`。
- **`RwLock.unwrap()` 统一为 `unwrap_or_else(|e| e.into_inner())`**：`curl.rs`（7 处）、`header.rs`（7 处）锁中毒时恢复而非 panic。
- **`fiber.rs` 的 `.expect("调度器未初始化")` 改为 `?` 传播错误**：5 处改为 `ok_or_else(|| "...".to_string())?`。
- **`PhpXhMulti::execute`/`execute_each` 的 `.expect("线程池已初始化")` 改为 `?`**：2 处。

### 失败路径字段完整性
- **`result_to_php_array` 失败分支补齐字段**：插入 `headers => []`、`body_size => 0`、`url => ""`，使失败路径与成功路径字段集完全一致（代码注释已声明此意图，实现需跟上）。
- **`getConfig()` 的 `proxy` 为 None 时插入 `null`**：与 `setConfig` 接受 `null` 的语义对称，避免用户 `$cfg['proxy']` 触发未定义索引。

### API 补全与文档对齐
- **新增 `XHRequest::options()` 快捷方法**：与 `get/post/put/delete/patch/head` 一致（`HttpMethod::Options` 已实现，PHP 层漏暴露）。
- **README 补 `http2_enabled` 配置项**：setConfig 示例和配置说明加入 `'http2_enabled' => true`。
- **README 说明 `execute()` 错误处理语义**：明确"所有 API 统一返回结果数组，检查 `$result['success']`"。
- **README 修正 `body()` 签名为 `body(string $data)`**：实现接受 `&Zval` 但文档写 `string`，统一为 `string`（非字符串类型 fallback 行为不稳定，不文档化）。
- **README 故障排查补充常见运行时问题**：请求超时、代理无效、响应体超限。

### CI 质量保障
- **CI clippy/test 启用 `--features php`**：让 `php_ext.rs`/`fiber.rs` 的用户面代码真正被检查。
- **移除"验证扩展可加载"的 `|| true`**：改为断言式验证，加载失败时 CI 红。
- **CI 增加 PHP 测试套件执行**：运行 `rust/tests/php_*.php`，让端到端测试真正保障质量。
- **CI 注释更新**：macOS PHP 版本注释改为 8.1~8.5（原写 8.1~8.3）。

### 测试改进
- **`test_drop_aborts_tasks` 改为真测试**：添加请求 + spawn_all 后 drop，验证任务被 abort（用原子计数器或检查任务未完成）。
- **`test_global_manager_config` 避免触碰全局单例**：改用独立实例，消除并行测试隐患。

## Impact

- **Affected specs**: user-centric-cleanup-and-consistency（v1.0.6 已完成），本 spec 为其后继
- **Affected code**:
  - `/workspace/rust/src/php_ext.rs` — execute() 错误处理、global_client/runtime、RwLock、expect→?、options()、getConfig proxy、result_to_php_array 失败字段
  - `/workspace/rust/src/curl.rs` — RwLock unwrap→unwrap_or_else（7 处）
  - `/workspace/rust/src/header.rs` — RwLock unwrap→unwrap_or_else（7 处）
  - `/workspace/rust/src/fiber.rs` — expect→?（5 处）
  - `/workspace/rust/src/multi.rs` — test_drop_aborts_tasks 改进
  - `/workspace/rust/tests/integration_test.rs` — test_global_manager_config 改用独立实例
  - `/workspace/README.md` — http2_enabled、execute() 错误处理说明、body() 签名、故障排查
  - `/workspace/.github/workflows/build-rust.yml` — clippy/test 启用 php feature、移除 || true、增加 PHP 测试
- **Breaking changes**: `XHRequest::execute()` 网络错误不再抛异常，改为返回 `success=false` 数组。这是修正实现匹配 README 一直文档化的模式，实际降低破坏性。

## ADDED Requirements

### Requirement: execute() 错误处理统一
`XHRequest::execute()` SHALL 在网络/DNS/TLS 等错误时返回 `success=false` 的结果数组（含 `status: 0`、`error` 字段），不得抛 PHP 异常。与 `XHMulti::execute()`、`XHCurl::await()` 行为一致。

#### Scenario: 网络错误返回 success=false
- **WHEN** 用户调用 `$req->execute()` 但目标 URL 不可达（DNS 失败/连接超时）
- **THEN** 返回 `['success' => false, 'status' => 0, 'error' => '...', ...]`，不抛异常

### Requirement: 全局初始化错误可恢复
`global_client()`/`global_runtime()` 初始化失败时 SHALL 返回错误信息，不得 panic 杀死 PHP 进程。用户修正配置后可重试。

#### Scenario: 无效代理不崩溃
- **WHEN** 用户 `setConfig(['proxy' => '://invalid'])` 后调用请求方法
- **THEN** 收到 PHP 异常（含错误信息），进程不崩溃，修正配置后可重试

### Requirement: 锁中毒自愈
`curl.rs`/`header.rs` 的 `RwLock` 中毒时 SHALL 恢复（`unwrap_or_else(|e| e.into_inner())`），不得 panic。保证 FPM worker 在偶发 panic 后可继续服务。

#### Scenario: 锁中毒后配置仍可读写
- **WHEN** 某次持锁操作 panic 导致 RwLock 中毒
- **THEN** 后续 `config()`/`set_config()`/`set()`/`get()` 等操作正常返回（取中毒时的数据）

### Requirement: 失败路径字段完整
`result_to_php_array` 失败分支 SHALL 插入 `headers => []`、`body_size => 0`、`url => ""`，使失败路径与成功路径字段集完全一致。

#### Scenario: 失败时访问 headers 不报未定义
- **WHEN** 请求失败，用户访问 `$result['headers']`
- **THEN** 得到空数组 `[]`，不触发 PHP 未定义索引警告

### Requirement: options() 快捷方法
`XHRequest` SHALL 提供 `options()` 方法，与 `get()/post()/put()` 等一致。

#### Scenario: 发送 OPTIONS 请求
- **WHEN** 用户调用 `XHCurl::createRequest($url)->options()->execute()`
- **THEN** 发送 HTTP OPTIONS 请求

### Requirement: getConfig proxy 键始终存在
`getConfig()` SHALL 始终返回 `proxy` 键，未设置时为 `null`（与 `setConfig` 接受 `null` 对称）。

### Requirement: CI 真正保障 php feature 代码质量
CI 的 clippy 和 test 步骤 SHALL 启用 `--features php`，确保 `php_ext.rs`/`fiber.rs` 的用户面代码被检查。

### Requirement: CI 扩展加载验证有效
"验证扩展可加载"步骤 SHALL 在加载失败时使 CI 失败，不得使用 `|| true`。

### Requirement: CI 执行 PHP 测试套件
CI SHALL 在编译扩展后运行 `rust/tests/php_*.php` 测试文件，失败时 CI 红。

## MODIFIED Requirements

### Requirement: README 文档完整性
README SHALL 列出 `http2_enabled` 配置项（含默认值 `true` 和 `false` 时强制 HTTP/1.1 的说明）。README SHALL 说明 `execute()` 的错误处理语义（统一返回结果数组）。README `body()` 签名 SHALL 为 `body(string $data)`。README 故障排查 SHALL 覆盖请求超时、代理无效、响应体超限。

## REMOVED Requirements

### Requirement: global_client/global_runtime panic on init failure
**Reason**: panic 杀死 PHP 进程，用户无法 try/catch，FPM worker 崩溃后持续重启。
**Migration**: 改为返回 `Result`，调用方传播错误为 PHP 异常。
