# 修复响应数组字段稳定性与 xhrun 失败路径完整性 Spec

## Why
第三轮 PHP 使用者视角审查发现多个响应数组字段稳定性问题：`remote_addr`/`version` 条件性插入导致 PHP 端 `Undefined index` warning；`xhrun()` 的 `failure_result` 辅助函数和 `exit_error` 分支缺少 `error_type`/`command`/`error` 字段，与主路径不一致；`XHRequest::execute()` 失败路径 `elapsed_ms` 恒为 0；XHMulti 缺少 `clear()` 方法。这些问题让 PHP 用户必须写防御性 `isset()` 代码，或在日志/调试场景下得到不完整的字段集。

## What Changes
- **P2-1 响应数组字段稳定性**：`fill_response_fields` 中 `remote_addr`/`version` 的 `None` 分支插入 `null`（与 `body` 空时插入空字符串的处理对齐），保证字段集在所有路径下稳定存在
- **P2-2 xhrun failure_result 补 error_type**：`failure_result` 辅助函数补充 `error_type` 字段（白/黑名单用 `"denied"`，启动失败用 `"spawn_failed"`），与主路径的 timeout/output_too_large/exit_error 枚举对齐
- **P2-3 xhrun exit_error 路径补字段**：`exit_code != 0` 分支补充 `command` 和 `error` 字段，与 timeout/truncated 路径一致；同时在函数顶部统一插入 `command`（成功失败都包含）
- **P2-4 execute 失败路径 elapsed_ms**：`XHRequest::execute()` 失败时用 `start.elapsed()` 而非 `Duration::from_secs(0)`，记录真实失败耗时
- **P2-5 XHMulti 暴露 clear()**：在 `PhpXhMulti` 暴露 `clear()` 方法委托给 `XhMulti::clear()`，支持显式重置请求列表
- **P3-1 错误措辞统一**：xhrun 的 `"不能为负数"` 改为 `"不能为负值"`，与其他 setter 一致
- **P3-2 body 文档修正**：将 `execute()` 文档注释中 `body: string（UTF-8 文本）` 改为 `body: string（二进制安全，可能非 UTF-8）`

## Impact
- Affected specs: align-php-user-api-consistency（xhrun error_type）、unify-setter-getter-contract（错误措辞）
- Affected code:
  - `rust/src/php_ext.rs`：fill_response_fields、failure_result、xhrun exit_error 分支、execute 失败路径、PhpXhMulti 新增 clear、文档注释
  - 无新增依赖
- 受影响测试：可能需更新断言（如原断言 `!isset($r['error_type'])` 现在会 isset）

## ADDED Requirements

### Requirement: XHMulti clear 方法
XHMulti SHALL 暴露 `clear()` 方法清空待执行请求列表，支持复用同一对象。

#### Scenario: clear 后 count 为 0
- **WHEN** 用户调用 `$multi->add($req1)->add($req2)->clear()`
- **THEN** `$multi->count()` 返回 0

## MODIFIED Requirements

### Requirement: 响应数组字段集稳定性
fill_response_fields SHALL 保证 `remote_addr` 和 `version` 字段在所有响应中存在，None 时插入 null。

#### Scenario: remote_addr 为 None 时字段存在
- **WHEN** 响应的 remote_addr 为 None（如代理场景）
- **THEN** `$resp['remote_addr']` 存在且为 null（不触发 Undefined index warning）

### Requirement: xhrun failure_result 字段完整性
failure_result SHALL 插入 `error_type` 字段，与主路径的 error_type 枚举对齐。

#### Scenario: 白名单拒绝返回 error_type=denied
- **WHEN** 命令不在白名单中
- **THEN** 返回数组含 `error_type` 字段，值为 `"denied"`

#### Scenario: 启动失败返回 error_type=spawn_failed
- **WHEN** 命令启动失败（如不存在）
- **THEN** 返回数组含 `error_type` 字段，值为 `"spawn_failed"`

### Requirement: xhrun exit_error 路径字段完整性
xhrun 的 exit_error 分支 SHALL 插入 `command` 和 `error` 字段，与 timeout/truncated 路径一致。

#### Scenario: exit_error 路径含 command 字段
- **WHEN** 子进程退出码非 0
- **THEN** 返回数组含 `command` 字段（值为执行的命令）

### Requirement: XHRequest execute 失败路径耗时记录
XHRequest::execute() 失败时 SHALL 记录真实失败耗时到 `elapsed_ms` 字段。

#### Scenario: DNS 失败时 elapsed_ms > 0
- **WHEN** 请求因 DNS 解析失败而失败
- **THEN** `$resp['elapsed_ms']` 反映真实失败耗时（非 0）

### Requirement: 错误措辞统一
xhrun 的 timeout 负值错误消息 SHALL 使用"不能为负值"，与其他 setter 一致。

### Requirement: body 字段文档准确性
execute() 文档注释 SHALL 标注 body 为二进制安全（可能非 UTF-8）。

## 设计取舍说明

### 不修复的问题（评估后排除）
- **问题 3（构造函数不对称）**：XHMulti 无参 + setter vs XHThreadPool 带参，属风格差异，不构成功能缺陷，强行统一会破坏现有 API
- **问题 5（状态分类字段）**：PHP 用户可用 `$resp['status']` 自行判断，新增字段属增强非修复
- **问题 6（工厂方法不统一）**：风格层面，无功能影响
- **问题 9（空列表处理不一致）**：fiber gather/each 返回空数组/0 是合理的数据流语义，XHMulti/XHThreadPool 抛异常是状态机语义，两者不同属设计差异，建议文档化
- **问题 11（fiber 缺流式回调）**：增强功能，非修复
- **问题 14/15（setConfig 负值/部分应用）**：setConfig 是全局配置入口，改为抛异常破坏面大；部分应用问题改为两阶段校验复杂度高。建议后续单独评估
- **问题 2（XHThreadPool shutdown 等）**：shutdown 是 async，PHP 侧需 block_on 包装，复杂度较高，建议后续单独处理
