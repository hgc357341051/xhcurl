# 修复静默失败与 PHP 易用性优化 Spec

## Why

站在 PHP 程序员使用者角度审查发现：`json()`/`setUserData()`/`multipart()`/`method()` 在输入无效时**静默跳过**（链式调用继续不中断），导致请求以空 body 或错误方法发出，使用者完全无感知——这是最危险的数据丢失陷阱。此外 `proxy()` 无法清除代理、`timeout()` 无毫秒精度、`cookies()` 只接受原始字符串等易用性缺口影响实际业务场景。

## What Changes

### P0: 静默失败改为抛异常（5 处）

- **`json()` 序列化失败抛异常**：`php_array_to_json` 失败或 `body_json_str` 失败时抛 PHP 异常（而非静默跳过）
- **`setUserData()` / `userData()` 序列化失败抛异常**：同上
- **`multipart()` 遇到非数组字段跳过该字段（而非整个设置）**：单个字段格式错误时跳过该字段继续处理其余字段，而非 `return self_` 退出整个方法
- **`method()` 无效方法名抛异常**：非标准 HTTP 方法名时抛异常（`customMethod()` 才是用于非标准方法的）
- **文档与实现一致**：README 明确说明配置类错误（如无效代理）会抛异常，请求级失败返回 `success=false` 数组

### P1: 易用性优化（4 项）

- **`proxy()` 接受 `?string`**：传 `null` 时清除请求级代理覆盖（与 `setConfig(['proxy' => null])` 对称）
- **新增 `timeoutMs(int $ms)`**：毫秒级超时精度（`timeout()` 保持秒级不变）
- **`cookies()` 接受数组**：新增重载，接受关联数组 `['name' => 'value', ...]`，内部自动拼接为 `"name=value; name2=value2"` 格式
- **结果数组新增 `error_type` 字段**：失败时返回错误类型枚举（`dns`/`timeout`/`ssl`/`connection`/`http`/`config`），便于程序化区分错误

## Impact

- 受影响文件：
  - `rust/src/php_ext.rs`（5 处静默失败修复 + 4 项易用性优化 + `result_to_php_array` 新增 `error_type`）
  - `rust/src/response.rs` 或 `rust/src/error.rs`（错误类型分类逻辑）
  - `README.md`（文档更新：错误处理说明、新方法签名、`error_type` 字段说明）
  - `rust/tests/php_*.php`（新增测试验证异常抛出 + 新功能）
- **不向后兼容**：`json()`/`setUserData()`/`method()` 静默失败改为抛异常——已有代码若依赖静默跳过行为（极不可能），需加 try/catch。这是正确的破坏性变更（静默失败本身是 bug）
- 不改动：核心层（multi.rs/threadpool.rs/executor.rs）、流式回调逻辑、协程架构

## ADDED Requirements

### Requirement: 链式 setter 输入无效时抛异常

`json()`/`setUserData()`/`userData()` 在序列化失败时 SHALL 抛出 PHP 异常（而非静默跳过），异常 message 说明序列化失败原因。`method()` 在方法名无效时 SHALL 抛异常。

#### Scenario: json() 序列化失败抛异常
- **WHEN** 用户调用 `->post()->json($data)->execute()`，`$data` 含无法序列化的内容（如资源）
- **THEN** `json()` 立即抛出 PHP 异常，message 含"JSON 序列化失败"
- **AND** 异常 message 含原始序列化错误原因

#### Scenario: method() 无效方法名抛异常
- **WHEN** 用户调用 `->method('PUTT')`（拼写错误）
- **THEN** `method()` 立即抛出 PHP 异常，message 含"无效的 HTTP 方法"
- **AND** 提示使用 `customMethod()` 设置非标准方法

#### Scenario: multipart() 单字段错误跳过该字段
- **WHEN** 用户调用 `->multipart([['name'=>'file','contents'=>'data'], 'invalid_field'])`
- **THEN** 第一个字段正常处理
- **AND** 第二个非数组字段被跳过（不影响其余字段）
- **AND** 不抛异常（容忍单个字段错误，继续处理有效字段）

### Requirement: proxy() 接受 null 清除

`XHRequest::proxy()` SHALL 接受 `?string $proxy` 参数，传 `null` 时清除请求级代理覆盖。

#### Scenario: 清除请求级代理
- **WHEN** 全局设置了代理，用户调用 `$req->proxy(null)->execute()`
- **THEN** 该请求不走代理（直接连接）

### Requirement: timeoutMs() 毫秒级超时

系统 SHALL 提供 `XHRequest::timeoutMs(int $ms)` 方法，设置毫秒级超时精度。

#### Scenario: 毫秒级超时
- **WHEN** 用户调用 `->timeoutMs(500)->execute()` 访问慢响应端点
- **THEN** 500ms 后请求超时返回 `success=false`

### Requirement: cookies() 接受数组

`XHRequest::cookies()` SHALL 接受关联数组 `['name' => 'value', ...]`，内部自动拼接为 cookie 字符串。

#### Scenario: 数组形式 cookies
- **WHEN** 用户调用 `->cookies(['session' => 'abc123', 'lang' => 'zh'])->execute()`
- **THEN** 请求头含 `Cookie: session=abc123; lang=zh`

### Requirement: 结果数组 error_type 字段

结果数组在失败时 SHALL 包含 `error_type` 字段，值为错误类型枚举字符串。

#### Scenario: DNS 失败错误类型
- **WHEN** 请求因 DNS 解析失败而失败
- **THEN** `$result['error_type'] === 'dns'`

#### Scenario: 超时错误类型
- **WHEN** 请求因超时而失败
- **THEN** `$result['error_type'] === 'timeout'`

#### Scenario: 成功路径无 error_type
- **WHEN** 请求成功
- **THEN** `$result` 不含 `error_type` 字段（或为空字符串）

## MODIFIED Requirements

### Requirement: 链式调用错误处理

`json()`/`setUserData()`/`userData()`/`method()` 在输入无效时 SHALL 抛出 PHP 异常而非静默跳过。链式调用中断，使用者需 try/catch 捕获。其他链式 setter（`timeout`/`header`/`form` 等）的负值跳过行为不变（负值非无效输入，是"不设置"语义）。

### Requirement: result_to_php_array 失败路径字段

`result_to_php_array` 失败路径 SHALL 在现有字段（`status: 0, body: "", headers: [], error: ..., success: false`）基础上新增 `error_type` 字段。成功路径不含 `error_type`（或为空字符串）。

## REMOVED Requirements

无。
