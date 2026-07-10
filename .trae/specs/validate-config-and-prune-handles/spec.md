# 配置类型校验与任务句柄回收 Spec

## Why

全量代码审计发现两个遗留问题：(1) `set_config` 对类型不匹配的配置项（如 `'connect_timeout' => '60'` 字符串、`'verify_ssl' => 1` 整数）静默跳过——ext-php-rs 的 `Zval::long()` 仅接受 `DataType::Long`、`Zval::bool()` 仅接受 `DataType::True/False`，不做自动类型转换，用户误传类型时配置不生效且无任何反馈；(2) `fiber.rs` 的 `Scheduler.task_handles` 在单次 `run()` 内无界增长——每次 `await`/`gather`/`each` spawn 的 tokio task 的 `JoinHandle` 仅在 `drop_scheduler`（run 结束）时统一清理，长轮询场景（`while(true) { await($req); }`）下已完成任务的句柄持续累积。

## What Changes

- `set_config` 收集所有"值存在但类型提取失败"的配置项名，若存在则返回错误信息列出这些项，让用户知晓哪些配置未生效。
- `run_event_loop` 在每次处理完一个任务结果（resume + 异常检查后）调用 `task_handles.retain(|h| !h.is_finished())` 回收已完成的 `JoinHandle`，避免单次 run() 内句柄无界增长。

## Impact

- 受影响文件：
  - `rust/src/php_ext.rs`（set_config 类型校验）
  - `rust/src/fiber.rs`（run_event_loop 句柄回收）
- 不影响现有 API 签名
- 修复后：类型不匹配的配置项会返回明确错误（而非静默忽略）；长轮询 run() 场景下已完成任务的 JoinHandle 被及时回收，内存占用恒定
- 现有测试均使用正确类型，不受影响

## ADDED Requirements

### Requirement: set_config 反馈类型不匹配项

`set_config` SHALL 收集所有"键存在但值类型与期望不符"的配置项名，若存在则返回错误信息列出这些项，使用户能发现配置未生效的原因。

#### Scenario: 字符串传给数值配置项
- **WHEN** 用户调用 `setConfig(['connect_timeout' => '60'])`（字符串而非整数）
- **THEN** 返回错误信息，列出 `connect_timeout` 类型不匹配
- **AND** 不修改任何配置

#### Scenario: 正确类型正常应用
- **WHEN** 用户调用 `setConfig(['connect_timeout' => 60])`（整数）
- **THEN** 配置正常应用，返回 Ok

### Requirement: run_event_loop 回收已完成任务句柄

`run_event_loop` SHALL 在每次处理完一个任务结果后，用 `JoinHandle::is_finished()` 过滤 `task_handles`，移除已完成任务的句柄，避免单次 `run()` 内 `JoinHandle` 无界增长。

#### Scenario: 长轮询 await 句柄回收
- **WHEN** 在 `run()` 回调内循环调用 `await` 数千次
- **THEN** 已完成任务的 `JoinHandle` 在下次事件泵迭代时被回收
- **AND** `task_handles` 长度不随已完成任务数无限增长
