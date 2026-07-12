# 统一响应字段稳定性与 executeEach 资源安全 Spec

## Why
第四轮 PHP 使用者视角审查发现响应数组字段集在成功/失败路径仍不完全一致（`error_type`/`error` 条件插入），`XHThreadPool::executeEach` 在校验回调前已 take requests 导致无效回调时请求列表丢失（数据丢失），`setConfig` 负值静默跳过与请求级 setter 抛异常的行为分裂。这些问题让 PHP 用户必须写防御性 `isset()` 代码，或因传入无效回调而永久丢失已添加的请求。

## What Changes
- **P1-1 修复 XHThreadPool::executeEach take 顺序**：将 `std::mem::take(&mut self.requests)` 移到回调校验（含 on_chunk/on_headers）之后，与 `XHMulti::execute_each` 的顺序对齐，避免无效回调导致 requests 丢失
- **P2-1 响应字段 error_type 稳定插入**：`result_to_php_array` 成功路径也插入 `error_type`（空字符串 `""`），使字段集在成功/失败路径一致
- **P2-2 响应字段 error 稳定插入**：`fill_response_fields` 中 `error` 改为无条件插入（None 时为空字符串），与 `remote_addr`/`version` 处理一致
- **P2-3 setConfig 负值抛异常**：将 7 处 `if v >= 0 { apply }` 改为 `else { 负值错误收集 }`，与请求级 setter 行为对齐 **BREAKING**
- **P3-1 setConfig 部分应用修复**：先全量校验（类型 + 负值），全部通过后再统一应用，避免"部分应用+返回 Err"

## Impact
- Affected specs: fix-response-field-stability（响应字段稳定性延续）、unify-setter-getter-contract（负值校验延续）
- Affected code:
  - `rust/src/php_ext.rs`：XHThreadPool::execute_each take 顺序、result_to_php_array 成功路径、fill_response_fields error 字段、set_config 负值校验与两阶段应用
  - 无新增依赖
- 受 BREAKING 影响的 PHP 用户代码：
  - `XHCurl::setConfig(['connect_timeout' => -5])` 现在抛异常（之前静默跳过），迁移：传 0 或正数

## ADDED Requirements
（无新增，仅修改）

## MODIFIED Requirements

### Requirement: XHThreadPool executeEach 资源安全
XHThreadPool::executeEach SHALL 在校验回调（callback/on_chunk/on_headers）通过后才 take requests，避免无效回调导致请求列表丢失。

#### Scenario: 无效回调时 requests 不丢失
- **WHEN** 用户调用 `$pool->add($req1)->add($req2)->executeEach($invalidCallback)`
- **THEN** 抛出异常后 `$pool->count()` 仍为 2（requests 未丢失）

### Requirement: 响应字段 error_type 稳定存在
result_to_php_array SHALL 在成功路径也插入 `error_type` 字段（空字符串），使字段集在成功/失败路径一致。

#### Scenario: 成功响应含 error_type 字段
- **WHEN** 请求成功返回
- **THEN** `$resp['error_type']` 存在且为空字符串（不触发 Undefined index）

### Requirement: 响应字段 error 稳定存在
fill_response_fields SHALL 无条件插入 `error` 字段（None 时为空字符串），与 remote_addr/version 处理一致。

#### Scenario: 成功响应含 error 字段
- **WHEN** 请求成功返回（无错误）
- **THEN** `$resp['error']` 存在且为空字符串

### Requirement: setConfig 负值校验
setConfig SHALL 对所有数值型配置项的负值抛异常，与请求级 setter 行为一致。

#### Scenario: 负值抛异常
- **WHEN** 用户调用 `XHCurl::setConfig(['connect_timeout' => -5])`
- **THEN** 抛出 Exception，message 含字段名与"负值"

### Requirement: setConfig 原子性应用
setConfig SHALL 先校验全部配置项（类型 + 负值），全部通过后才统一应用，避免部分应用。

#### Scenario: 类型错误时不应用任何配置
- **WHEN** 用户调用 `XHCurl::setConfig(['connect_timeout' => 30, 'verify_ssl' => 'invalid'])`
- **THEN** 抛出异常，且 `connect_timeout` 未被应用（全局配置保持原值）

## 设计取舍说明

### 不修复的问题（评估后排除）
- **问题 7（Fiber gather/each 缺流式回调）**：增强功能，需改 fiber.rs 事件泵与 execute_http_task 签名，复杂度高，建议后续单独处理
- **问题 8（异常类型无区分）**：注册 PHP 异常类需改 #[php_module] 与所有 Result 返回类型，破坏面大，建议后续单独评估
- **问题 9（全局配置缺毫秒级/部分项无请求级覆盖）**：增强功能，非修复
- **问题 10（fiber recv_timeout 空转）**：性能优化，无正确性问题，优先级低
