# 统一 xhrun 字段集与文档同步 Spec

## Why
第五轮审查发现 `xhrun()` 成功路径与失败路径字段集不一致（成功缺 `error_type`/`error`/`command`，而 HTTP API 成功/失败字段集已统一），代码注释错误声称"与 execute() 一致"实际不一致；README 中 `error_type` 说明过时（称"成功路径不含此字段"，实际第四轮已改为插入空字符串）；`fiber_each` 空请求返回 Ok(0) 与 XHMulti/XHThreadPool executeEach 抛异常不一致；`to_info_map` 为死代码。这些问题让 PHP 用户在处理 HTTP 和 xhrun 结果时行为不一致，或被过时文档误导。

## What Changes
- **P2-1 xhrun 成功路径补字段**：在 xhrun 成功路径插入 `error_type=""`、`error=""`、`command` 三个字段，使成功/失败字段集一致，与 HTTP API 风格对齐；修正错误注释
- **P2-2 README 文档同步**：更新 README 中 `error_type` 说明（"成功时为空字符串"而非"不含此字段"）；同步 xhrun 字段表
- **P3-1 fiber_each 空请求抛异常**：将 `fiber_each` 空请求处理改为抛异常，与 XHMulti/XHThreadPool executeEach 对齐 **BREAKING**
- **P3-2 删除 to_info_map 死代码**：删除 `response.rs` 中未被使用的 `to_info_map` 方法及其测试
- **P3-3 URL 空字符串校验**：在 `create_request` 中增加 URL 空字符串校验（fail-fast）
- **版本升级**：Cargo.toml 1.0.7 → 1.0.8，CHANGELOG 新增 1.0.8 条目

## Impact
- Affected specs: fix-response-field-stability（字段稳定性延续）、unify-response-fields-and-config-safety（fiber 空请求延续）
- Affected code:
  - `rust/src/php_ext.rs`：xhrun 成功路径字段、fiber_each 空请求、create_request URL 校验
  - `rust/src/response.rs`：删除 to_info_map
  - `rust/Cargo.toml`：版本号
  - `README.md`：error_type 说明、xhrun 字段表
  - `CHANGELOG.md`：1.0.8 条目
  - 测试：fiber_each 空请求断言更新
- 受 BREAKING 影响的 PHP 用户代码：
  - `XHCurl::each([], $cb)` 现在抛异常（之前返回 0），迁移：先检查 `count($requests) > 0`

## ADDED Requirements
（无新增，仅修改）

## MODIFIED Requirements

### Requirement: xhrun 成功路径字段集
xhrun 成功路径 SHALL 插入 `error_type`（空字符串）、`error`（空字符串）、`command` 字段，使成功/失败路径字段集一致。

#### Scenario: 成功路径含 error_type
- **WHEN** xhrun 成功执行（exit_code == 0）
- **THEN** 返回数组含 `error_type` 字段，值为空字符串

#### Scenario: 成功路径含 error 和 command
- **WHEN** xhrun 成功执行
- **THEN** 返回数组含 `error`（空字符串）和 `command` 字段

### Requirement: fiber_each 空请求抛异常
fiber_each SHALL 对空请求列表抛异常，与 XHMulti/XHThreadPool executeEach 行为一致。

#### Scenario: 空请求抛异常
- **WHEN** 用户调用 `XHCurl::each([], $cb)`
- **THEN** 抛出 Exception，message 含"没有待执行请求"

### Requirement: URL 空字符串校验
create_request SHALL 对空字符串 URL 抛异常（fail-fast）。

#### Scenario: 空字符串 URL 抛异常
- **WHEN** 用户调用 `XHCurl::createRequest('')`
- **THEN** 抛出 Exception，message 含"url"和"空"

## 设计取舍说明

### 不修复的问题（评估后排除）
- **问题 1（XhResponse 不作为 PHP 类）**：设计选择，README 已说明响应统一以数组返回，保持现状
- **问题 2（无 createMulti/createThreadPool 工厂）**：风格层面，不影响功能
