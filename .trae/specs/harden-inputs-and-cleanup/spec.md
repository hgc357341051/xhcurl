# 输入校验加固与配置一致性修复 Spec

## Why

在前 8 个 spec 修复后，对全部 11 个源文件做了一次全新的全量代码审计，发现 4 个低风险、高价值的遗留问题：`extract_requests` 先克隆全部元素再检查批量上限（大数组场景内存浪费/OOM 风险）；`fiber_await` 在 spawn tokio task 后才校验 Fiber 上下文，校验失败会留下已 spawn 的孤立任务；`get_config` 缺少 `tcp_keepalive_interval` 字段导致配置无法回读；`spawn_output_reader` 在 `limit = usize::MAX` 时 `buf.len() + n` 存在整数溢出风险。

## What Changes

- `extract_requests` 在克隆前先用 `requests.len()` 检查批量上限，提前返回错误（与 `XhMulti::add`/`XhThreadPool::add` 的"检查先于操作"模式一致）。
- `fiber_await` 将 `Fiber::getCurrent` 调用前置到 `runtime.spawn` 之前，校验失败时直接返回错误，不产生孤立 tokio task。
- `get_config` 补充 `tcp_keepalive_interval` 字段，使 `setConfig` 设置的值可被 `getConfig` 完整回读。
- `spawn_output_reader` 用 `checked_add` 替代 `buf.len() + n`，防止 `limit = usize::MAX` 时的整数溢出。

## Impact

- 受影响文件：
  - `rust/src/php_ext.rs`（extract_requests 提前检查、get_config 补字段、spawn_output_reader checked_add）
  - `rust/src/fiber.rs`（fiber_await getCurrent 前置）
- 不影响现有 API 签名
- 修复后：超大数组传入 gather/each 时在克隆前即被拒绝（内存安全）；await 在非 Fiber 上下文调用不残留 tokio task；配置可完整回读；xhrun 大输出不触发整数溢出。

## ADDED Requirements

### Requirement: extract_requests 提前检查批量上限

`extract_requests` SHALL 在遍历克隆数组元素前，先用 `requests.len()` 检查是否超过 `MAX_REQUESTS_PER_BATCH`，超过则直接返回错误，避免先克隆全部元素再拒绝导致的内存浪费。

#### Scenario: 超大数组提前拒绝
- **WHEN** 用户向 `gather`/`each` 传入超过 `MAX_REQUESTS_PER_BATCH` 个元素的数组
- **THEN** 在克隆任何 `XhRequest` 之前即返回错误
- **AND** 错误信息含上限说明

### Requirement: fiber_await 先校验 Fiber 上下文再 spawn

`fiber_await` SHALL 在 `runtime.spawn` 之前调用 `Fiber::getCurrent` 并校验返回值，校验失败时直接返回错误，不 spawn tokio task，避免产生无对应 pending 表项的孤立任务。

#### Scenario: 非 Fiber 上下文调用 await
- **WHEN** 在 `run()` 回调外、非 Fiber 上下文中调用 `await`
- **THEN** 返回错误信息说明需在 Fiber 内调用
- **AND** 不 spawn 任何 tokio task

### Requirement: get_config 包含 tcp_keepalive_interval 字段

`get_config` SHALL 返回 `tcp_keepalive_interval` 字段，使其与 `set_config` 可设置的配置项一一对应，支持配置完整回读校验。

#### Scenario: setConfig 后 getConfig 可回读 tcp_keepalive_interval
- **WHEN** 用户 `setConfig(['tcp_keepalive_interval' => 120])` 后调用 `getConfig()`
- **THEN** 返回数组包含 `tcp_keepalive_interval` 键且值为 120

### Requirement: spawn_output_reader 防整数溢出

`spawn_output_reader` SHALL 使用 `checked_add` 计算 `buf.len() + n`，溢出时停止读取并标记 `exceeded = true`，避免 `limit = usize::MAX` 时回绕导致检查失效。

#### Scenario: 超大输出不溢出
- **WHEN** xhrun 设置 `max_output = 0`（无限制，limit = usize::MAX）且子进程输出大量数据
- **THEN** `buf.len() + n` 用 checked_add 计算，不发生整数回绕
- **AND** 不会因回绕使大小检查错误通过
