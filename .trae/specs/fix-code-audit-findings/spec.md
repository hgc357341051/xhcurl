# 代码审查发现修复 Spec

## Why

第二轮全面代码审查(覆盖 error.rs / header.rs / php_ext.rs / fiber.rs / curl.rs / request.rs / response.rs / multi.rs / threadpool.rs / executor.rs)发现多个合理性问题和优化点,涉及静默丢数据、逻辑 bug、API 不一致、二进制安全遗漏等。本 spec 规划这些问题的修复,提升代码健壮性和一致性。

## 审查范围说明

本轮审查**不包括** `fiber_gather` 的 `MAX_REQUESTS_PER_BATCH` 数量检查(DoS 漏洞)——用户已决定忽略该问题,让用户自行分组执行。

## What Changes

### 高优先级(静默丢数据 / 逻辑 bug)

- **修复 `header.rs` `to_header_map()` 静默丢头部** — 非法头部名/值被 `if let Ok` 静默跳过,可能导致鉴权/Content-Type 丢失。改为返回 `Result` 并报错。
- **修复 `php_ext.rs` 错误响应缺 `body` 字段** — body 为 None 时不插入 `body` 键,PHP 端 `$resp['body']` 触发 Undefined index。改为插入空字符串。
- **修复 `fiber.rs` `fiber_gather` 信号量获取失败仍执行请求** — `.ok()` 吞掉 `AcquireError` 后继续执行,绕过并发上限。改为发送错误结果并 return。
- **修复 `php_ext.rs` `php_array_to_form` 非二进制安全** — value 只用 `string()`,非 UTF-8 字节静默丢弃表单项。改为 `binary::<u8>()` 优先。

### 中优先级(API 一致性 / 冗余)

- **修复 `php_ext.rs` `set_config`/`get_config` 字段不对称** — `get_config` 返回 `http2_enabled`/`tcp_keepalive`/`max_connections` 但 `set_config` 无法设置。补齐 set 分支。
- **修复 `request.rs:755` `Generic` 包装丢失类型链** — `builder.build()` 返回 `reqwest::Error` 应走 `Request` 变体(`.map_err(XhCurlError::from)`),而非 `Generic(format!())`。
- **修复 `php_ext.rs` `elapsed_ms`/`error` 字段双重插入冗余** — `result_to_php_array` 和 `fill_response_fields` 都插入这两个字段,后者覆盖前者。统一为一处负责。
- **修复 `fiber.rs` 嵌套 `run()` 破坏调度器** — 内层 `drop_scheduler` 清空外层调度器导致 panic。改为引用计数或拒绝嵌套。

### 暂不处理(需用户决策或低优先级)

- `fiber.rs` FPM 模式协程死循环(current-thread 运行时不被驱动)— 需用户决策修复方向(加 FPM 守卫 vs 改造运行时驱动),本 spec 不包含。
- `header.rs` 锁中毒处理不一致 — 低风险,可后续改用 `parking_lot::RwLock` 统一。
- `error.rs` `Memory` 变体语义误用 — 低风险,涉及错误类型重构。
- `php_ext.rs` `global_client` OnceLock 固化后 `set_config` 失效 — 需架构决策(reloadClient vs 文档约束)。
- `php_ext.rs` `xhrun` shell 注入 — 已有文档警告,低风险。
- 各模块测试覆盖补充 — 持续改进,不阻塞本轮。

## Impact

- 受影响文件:
  - `rust/src/header.rs`(`to_header_map` 签名变更 + 测试)
  - `rust/src/php_ext.rs`(body 字段、set_config、form 二进制安全、elapsed/error 冗余)
  - `rust/src/fiber.rs`(信号量错误处理、嵌套 run 防护)
  - `rust/src/request.rs`(build_request_client 错误类型)
  - `rust/src/error.rs`(可能新增 From 实现)
- **潜在 BREAKING**:`to_header_map()` 签名从 `-> HeaderMap` 变为 `-> XhCurlResult<HeaderMap>`,调用方需处理 Result。但该方法是内部方法,不影响 PHP API。

## ADDED Requirements

### Requirement: 非法头部明确报错

`HeaderManager::to_header_map()` SHALL 对非法头部名或值返回 `XhCurlError::InvalidArgument` 错误,而非静默跳过。

#### Scenario: 非法头部名
- **WHEN** 用户设置了头部名为 `"X Test"`(含空格,非法 HTTP header name token 字符)
- **AND** 调用 `to_header_map()`
- **THEN** 返回 `Err(XhCurlError::InvalidArgument(...))`,错误信息包含具体哪个头部非法

#### Scenario: 合法头部不受影响
- **WHEN** 用户设置了头部 `"Content-Type: application/json"`
- **AND** 调用 `to_header_map()`
- **THEN** 返回 `Ok(HeaderMap)`,包含该头部

### Requirement: 响应 body 字段始终存在

`fill_response_fields()` SHALL 在 body 为 None 时插入空字符串 `""`,确保 PHP 端 `$resp['body']` 始终可访问,不触发 Undefined index。

#### Scenario: 请求失败无响应体
- **WHEN** 请求失败(如连接超时)
- **AND** PHP 端访问 `$resp['body']`
- **THEN** 返回空字符串 `""`,不触发 Undefined index

### Requirement: 信号量获取失败不执行请求

`fiber_gather` 中信号量 `acquire()` 失败时 SHALL 发送 `RequestResult::error(...)` 到 channel 并跳过请求执行,不绕过并发限制。

#### Scenario: 信号量关闭
- **WHEN** `semaphore.acquire()` 返回 `Err(AcquireError)`
- **THEN** 发送错误结果到 channel
- **AND** 不执行 `execute_http_task`

### Requirement: 表单值二进制安全

`php_array_to_form()` SHALL 优先使用二进制安全方式读取表单值,非 UTF-8 字节不被丢弃。

#### Scenario: 二进制表单值
- **WHEN** PHP 传入表单值含非 UTF-8 字节
- **THEN** 该键值对保留原始字节,不被静默丢弃

## MODIFIED Requirements

### Requirement: set_config/get_config 字段对称

`set_config` SHALL 支持与 `get_config` 返回字段一致的所有可配置项,包括 `http2_enabled`、`tcp_keepalive`、`max_connections`、`tcp_keepalive_interval`。

#### Scenario: 设置 http2_enabled
- **WHEN** 用户调用 `setConfig(['http2_enabled' => false])`
- **THEN** 全局配置的 `http2_enabled` 被设为 false
- **AND** `getConfig()` 返回 `http2_enabled = false`

### Requirement: build_request_client 错误类型保留

`build_request_client()` 中 `ClientBuilder::build()` 失败时 SHALL 使用 `XhCurlError::from(reqwest::Error)` 走 `Request` 变体,保留错误类型链,而非用 `Generic(format!())` 丢弃类型。

#### Scenario: 客户端构建失败
- **WHEN** `builder.build()` 返回 `Err(reqwest::Error)`
- **THEN** 返回 `Err(XhCurlError::Request(...))`
- **AND** 原始 `reqwest::Error` 可通过 `Error::source()` 取回

### Requirement: elapsed_ms/error 字段单次写入

`result_to_php_array` 和 `fill_response_fields` SHALL 由单处负责写入 `elapsed_ms` 和 `error` 字段,避免双重插入覆盖歧义。

#### Scenario: 请求成功
- **WHEN** 请求成功完成
- **THEN** `elapsed_ms` 由 `result_to_php_array` 顶层写入一次
- **AND** `fill_response_fields` 不再写入 `elapsed_ms`

### Requirement: 嵌套 run 防护

`fiber_run` SHALL 检测嵌套调用并返回明确错误,而非破坏外层调度器状态导致 panic。

#### Scenario: 嵌套 run 调用
- **WHEN** 在 `run()` 的回调内再次调用 `run()`
- **THEN** 内层 `run()` 返回 `Err("不支持嵌套调用 XHCurl::run")`
- **AND** 外层调度器状态不受影响
