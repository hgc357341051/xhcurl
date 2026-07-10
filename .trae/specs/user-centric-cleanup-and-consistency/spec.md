# 用户视角代码优化与一致性完善 Spec

## Why

经过对全部 11 个 Rust 源文件、README、CHANGELOG、CI 工作流的全面审查，发现多处从**使用者角度**影响简易实用性的问题：README 漏列 4 个已实现方法、FPM/CLI 能力表与实现冲突、链式 setter 返回类型分裂导致链式断裂、`set` 前缀风格不统一、`fiber_each` 硬编码 64 导致用户配置不生效、`http2_enabled`/`use_multi_thread` 死配置字段让用户误以为生效、`XhMulti` 缺 Drop 有任务泄漏风险、负值/零值语义在不同字段间反复横跳。本 spec 聚焦"让用户用得简单、用得明白"，不做无关重构。

## What Changes

### 文档对齐实现（用户最先看到）
- **README 补全 4 个漏列方法**：`XHCurl::each()`、`XHMulti::timeout()`、`XHMulti::executeEach()`、`XHThreadPool::executeEach()`，含签名、回调签名、返回值、示例。
- **修正 README FPM/CLI 能力表**：协程 `run/await/gather/each` 实际仅 CLI 可用（fiber.rs:474-476 显式拒绝 FPM），README 表格（行 660-663）声称 FPM 支持协程是错误的，需更正并在协程章节顶部加 CLI-only 警告。
- **README 响应数组字段表区分成功/失败**：失败时（`success=false`）实际无 `status/body_size/headers/url/remote_addr/version` 字段（php_ext.rs:1496-1504），需在表中明确标注。
- **README `id` 字段默认值说明修正**：同步 `execute()` 默认为 URL，fiber 路径默认为 `"task-{N}"`（fiber.rs:677-680），需说明两套默认。
- **README 补 executeEach/each 回调签名**：`function(array $result): void`，返回值为处理结果总数（int）。

### API 一致性（链式调用体验）
- **链式 setter 统一返回 `&mut Self`**：当前 `method()`/`json()`/`form()`/`multipart()`/`setUserData()` 返回 `Result<&mut Self, String>` 破坏链式调用。改为内部处理错误（无效输入时跳过并记录，或对不会失败的方法直接返回 `&mut Self`），让用户能写 `createRequest($url)->get()->json([...])->timeout(10)->execute()`。
  - `form()` 实现本就不会失败，直接改 `&mut Self`。
  - `method()`/`json()`/`multipart()`/`setUserData()` 内部已用 `Result`，改为失败时跳过该次设置（与 `header()` 风格对齐前需统一错误策略，见下条）。
- **`set` 前缀统一移除**：`setId` → `id`、`setUserData` → `userData`，与其余 18 个无 `set` 前缀的链式 setter 风格一致。保留旧名为别名（PHP 端通过 `#[php(name=...)]`）避免破坏性变更。
- **负值处理统一为"跳过+返回错误信息"**：当前 `setConfig` 跳过、`XHRequest::timeout` clamp 到 0、`XHMulti::timeout` 置 0 表示无超时、`maxRedirects(-1)` clamp 到 0 表示不跟随。统一为：负值跳过（保留原值）并在错误信息中提示，与 `setConfig` 现有行为对齐。

### Bug 修复（用户可感知的行为差异）
- **`fiber_each` 并发上限读取配置**（P2 bug）：`fiber.rs:370` 硬编码 `total.min(64)`，而 `fiber_gather`（fiber.rs:258-261）读取 `fiber_max_concurrency` 配置。用户 `setConfig(['fiber_max_concurrency'=>128])` 后 gather 生效但 each 仍是 64。统一读取配置。
- **`XhMulti` 实现 Drop**（P2 资源泄漏）：`XhMulti` 持有 `tasks: Vec<JoinHandle<()>>`（multi.rs:197）但无 Drop，若 `spawn_all` 后 panic/早期返回，任务在后台继续运行泄漏连接。参考 `XhThreadPool`（threadpool.rs:598-612）的 Drop 实现，为 `XhMulti` 加 Drop 调用 `abort_tasks`。
- **`id` 字段默认值统一**：fiber 路径 `await/gather/each` 默认 `"task-{N}"` 与同步 `execute()` 默认 URL 不一致（README:455 声称默认 URL）。统一为：未设置 `setId()` 时默认为请求 URL（与文档一致），fiber 路径用 URL 作为 fallback。

### 死配置字段清理
- **`http2_enabled`**：curl.rs:43 字段存在但 `create_client_builder` 从不读取（注释说 reqwest 默认协商）。用户通过 `setConfig`/`getConfig` 以为可配置实则无效。处理：在 `create_client_builder` 中实际读取（reqwest 默认已支持 HTTP/2 协商，该字段用于 `.http2_prior_knowledge()` 场景或明确禁用），或从 `set_config`/`get_config` 移除并标注 deprecated。**选择实际读取**：`http2_enabled=true` 时保持默认协商（当前行为），`false` 时显式 `.http1_only()` 禁用 HTTP/2。
- **`use_multi_thread`**：curl.rs:70 字段仅 `default()` 和测试出现，`set_config`/`get_config` 未暴露，`create_client_builder` 不读。**处理：移除该字段**（运行时类型由 `sapi_is_cli()` 决定，php_ext.rs:88-92，此字段无实际作用）。

### 响应字段一致性
- **失败路径补齐 `body` 字段为空字符串**（已实现，php_ext.rs:1500），README 需说明。
- **`result_to_php_array` 失败路径补 `status: 0`**：当前失败时不写 status 字段，用户访问 `$r['status']` 会得到未定义警告。补 `status => 0`（0 是哨兵值，非合法 HTTP 状态码），让字段集在成功/失败时一致，用户可安全访问 `$r['status'] ?? 0`。

## Impact

- **Affected specs**: 无直接依赖的前序 spec，但补全了前序 spec（fix-code-audit-findings、harden-inputs-and-cleanup、validate-config-and-prune-handles）未覆盖的用户视角问题。
- **Affected code**:
  - `/workspace/rust/src/php_ext.rs` — 链式 setter 返回类型、`set` 前缀别名、`http2_enabled` 实际读取、负值处理、`status: 0` 补齐
  - `/workspace/rust/src/fiber.rs` — `fiber_each` 读取配置、`id` 默认值统一为 URL
  - `/workspace/rust/src/multi.rs` — `XhMulti` 实现 Drop
  - `/workspace/rust/src/curl.rs` — 移除 `use_multi_thread` 字段、`http2_enabled` 在 `create_client_builder` 中读取
  - `/workspace/rust/src/request.rs` — 负值处理统一
  - `/workspace/README.md` — 补全 4 方法、修正 FPM/CLI 表、响应字段区分成功/失败、`id` 默认值说明
- **Breaking changes**: `setId`/`setUserData` 保留为别名（通过 `#[php(name=...)]`），新增 `id`/`userData` 方法，不破坏旧代码。链式 setter 返回类型从 `Result` 改为 `&mut Self` 对 PHP 端无影响（PHP 不感知 Rust 的 Result，ext-php-rs 会把 Err 转为抛异常或返回 false）。

## ADDED Requirements

### Requirement: 链式 setter 一致性
所有 `XHRequest` 的链式 setter 方法 SHALL 返回 `&mut Self`（PHP 端返回 `$this`），不得返回 `Result`。内部错误 SHALL 通过跳过本次设置 + 记录的方式处理，不中断链式调用。

#### Scenario: 链式调用不被 Result 打断
- **WHEN** 用户调用 `XHCurl::createRequest($url)->get()->json(['k'=>'v'])->timeout(10)->execute()`
- **THEN** 链式调用正常完成，无需 `?` 或 `unwrap()`

#### Scenario: 无效 JSON 跳过设置
- **WHEN** 用户调用 `->json("{ invalid")` 传入无效 JSON
- **THEN** 该次设置被跳过，链式调用继续，`execute()` 时 body 为空

### Requirement: 方法命名一致性
所有 `XHRequest` 链式 setter SHALL 不使用 `set` 前缀。原 `setId`/`setUserData` SHALL 同时提供 `id`/`userData` 新名与旧名别名，保证向后兼容。

#### Scenario: 新名调用
- **WHEN** 用户调用 `->id('foo')->userData(['k'=>'v'])`
- **THEN** 等价于 `->setId('foo')->setUserData(['k'=>'v'])`

### Requirement: fiber_each 并发配置生效
`XHCurl::each()` 的并发上限 SHALL 读取 `GlobalConfig.fiber_max_concurrency`，与 `gather()` 行为一致。

#### Scenario: setConfig 调整 each 并发
- **WHEN** 用户 `setConfig(['fiber_max_concurrency' => 128])` 后调用 `each($requests, $cb)`
- **THEN** 实际并发上限为 128（而非硬编码 64）

### Requirement: XhMulti 资源安全
`XhMulti` SHALL 实现 `Drop`，在 drop 时 `abort` 所有未完成的 tokio 任务，避免任务泄漏。

#### Scenario: spawn_all 后 panic 不泄漏任务
- **WHEN** `spawn_all()` 被调用后，`XhMulti` 因 panic 被 drop
- **THEN** 所有 `tasks` 中的 JoinHandle 被abort，后台任务不再运行

### Requirement: id 字段默认值统一
未调用 `setId()` 时，所有执行路径（同步 `execute()`、fiber `await/gather/each`、multi、threadpool）返回的 `id` 字段 SHALL 默认为请求 URL。

#### Scenario: fiber 路径 id 默认为 URL
- **WHEN** 用户不调用 `setId()`，执行 `XHCurl::await($req)`
- **THEN** 返回的 `id` 字段为请求 URL（而非 `"task-N"`）

### Requirement: 死配置字段清理
- `http2_enabled` SHALL 在 `create_client_builder` 中被实际读取：`false` 时禁用 HTTP/2（`.http1_only()`），`true` 时保持默认协商。
- `use_multi_thread` 字段 SHALL 被移除（运行时类型由 SAPI 决定，此字段无作用）。

### Requirement: 失败响应字段一致性
请求失败时（`success=false`），返回数组 SHALL 包含 `status => 0`（哨兵值）和 `body => ""`（空字符串），确保字段集与成功路径一致，用户可安全访问 `$r['status']` 无需 `??` 兜底。

#### Scenario: 失败时访问 status 不报未定义
- **WHEN** 请求失败，用户访问 `$r['status']`
- **THEN** 得到 `0`，不触发 PHP 未定义索引警告

## MODIFIED Requirements

### Requirement: README 文档完整性
README SHALL 列出所有公开 PHP 方法，含 `XHCurl::each`、`XHMulti::timeout`、`XHMulti::executeEach`、`XHThreadPool::executeEach`，含签名、参数、返回值、示例。

### Requirement: README FPM/CLI 能力表准确性
README 的 FPM/CLI 能力表 SHALL 反映实现：协程 `run/await/gather/each` 仅 CLI 可用，FPM 下调用返回错误。协程章节顶部 SHALL 加 CLI-only 警告。

### Requirement: 负值处理一致性
所有数值配置项（`setConfig` 的数值项、`XHRequest::timeout/connectTimeout/maxRedirects`、`XHMulti::timeout/maxConcurrency/maxResponseSize`、`XHThreadPool::__construct`）SHALL 对负值统一处理：跳过本次设置（保留原值）。不再 clamp 到 0 或置 0。

## REMOVED Requirements

### Requirement: use_multi_thread 配置字段
**Reason**: 该字段从未被任何代码读取，运行时类型由 `sapi_is_cli()` 决定（php_ext.rs:88-92），字段存在仅增加 `GlobalConfig` 宽度且让维护者误以为有作用。
**Migration**: 无外部影响（从未通过 `setConfig`/`getConfig` 暴露），直接删除字段及 `default()` 中的初始化。
