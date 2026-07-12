# 新增 withOptions 批量设置与全局 base_uri/base_headers Spec

## Why

XHCurl 在微服务与生产环境场景下配置灵活性不足：
1. PHP 使用者设置请求时需链式调用多次 `->header()->timeout()->retry()` 等，代码冗长
2. 微服务架构下每个请求都要写完整 URL（含重复的 host），环境切换需修改每个请求
3. 认证 Token、TraceID 等公共 header 无法全局默认，每个请求都要重复 `->header()` 设置

本轮聚焦补齐**请求级批量配置**（`withOptions()`）与**全局基础配置**（`base_uri`/`base_headers`），提升微服务与多环境部署的开发效率。

## What Changes

### 新增方法（均无 BREAKING，向后兼容）

- **`XHRequest::withOptions(array $options): $this`**：请求级批量设置多个选项，内部按 key 分发到对应 setter。支持 timeout/timeoutMs/connectTimeout/headers/header/query/accept/contentType/retry 等常见选项。
- **全局 `base_uri` 配置**：`setConfig(['base_uri' => 'https://user-svc.internal'])`，请求 URL 以 `/` 开头时自动拼接 base_uri 为完整 URL。
- **全局 `base_headers` 配置**：`setConfig(['base_headers' => ['Authorization' => 'Bearer xxx']])`，所有请求自动携带这些公共 header，请求级同名 header 覆盖全局。

### 不包含的范围（留给后续轮次）

- 自动重试机制 retry（中等难度，需修改 executor，单独一轮）
- 耗时细分 timing（中等难度，需多阶段计时，单独一轮）
- 请求取消 cancellation（高难度，需 CancellationToken 跨线程共享）
- 客户端证书/mTLS（中高难度，需 reqwest Identity 配置）
- debug 日志与错误上下文重构（涉及 BREAKING，单独评估）

## Impact

- Affected specs: 无（新增功能，不修改现有行为）
- Affected code:
  - `rust/src/curl.rs`：GlobalConfig 新增 `base_uri`/`base_headers` 字段，配置指纹比对包含新字段
  - `rust/src/request.rs`：XhRequest 新增 `with_options()` 方法，`to_reqwest()` 中 URL 拼接与 base_headers 合并逻辑
  - `rust/src/php_ext.rs`：新增 `withOptions()` PHP 绑定方法，`setConfig()` 校验与处理 base_uri/base_headers
  - `rust/tests/mock_server.php`：新增 `/base-test` 端点（用于测试 base_uri 拼接）
  - `rust/tests/php_add_withoptions_and_base_config_test.php`：新增测试文件
  - `README.md`：方法表新增 withOptions 行，配置项表新增 base_uri/base_headers 行，新增微服务场景示例
  - `rust/Cargo.toml`：版本 1.4.0 → 1.5.0
  - `CHANGELOG.md`：新增 [1.5.0] 条目

## ADDED Requirements

### Requirement: 请求级批量配置方法 withOptions

系统 SHALL 提供 `XHRequest::withOptions(array $options): $this` 方法，一次性设置多个请求选项，内部按 key 分发到对应 setter。

#### 支持的选项 key

| key | 类型 | 对应 setter | 说明 |
|-----|------|-------------|------|
| `timeout` | int | `timeout()` | 请求总超时（秒） |
| `timeout_ms` | int | `timeoutMs()` | 请求总超时（毫秒） |
| `connect_timeout` | int | `connectTimeout()` | 连接超时（秒） |
| `headers` | array | 多次 `header()` | 关联数组，key=header名，value=header值 |
| `query` | array | `query()` | URL 查询参数 |
| `accept` | string | `accept()` | Accept header |
| `content_type` | string | `contentType()` | Content-Type header |
| `body` | string | `body()` | 请求体 |
| `json` | array | `json()` | JSON 请求体（自动序列化） |
| `form` | array | `form()` | 表单请求体 |
| `user_agent` | string | `userAgent()` | User-Agent header |
| `referer` | string | `referer()` | Referer header |
| `encoding` | string | `encoding()` | Accept-Encoding header |
| `range` | string | `range()` | Range header |
| `proxy` | string | `proxy()` | 代理 |
| `verify_ssl` | bool | `verifySsl()` | SSL 验证 |
| `follow_redirects` | bool | `followRedirects()` | 跟随重定向 |
| `max_redirects` | int | `maxRedirects()` | 最大重定向次数 |

#### 行为约定

- 未知 key 抛异常（fail-fast，避免拼写错误静默忽略）
- 选项值为 null 时跳过该选项（不调用对应 setter，等同于不设置）
- 选项值类型不匹配抛异常（如 `timeout` 传字符串）
- `headers` 数组中 header 值为 null 时跳过该 header
- 多次调用 `withOptions()` 累加（非覆盖），后调用覆盖同名选项
- 与链式 setter 混用正常工作（withOptions 之后的 setter 覆盖 withOptions 设置）

#### Scenario: 批量设置多个选项

- **WHEN** 用户调用 `->get()->withOptions(['timeout' => 30, 'headers' => ['Authorization' => 'Bearer xxx'], 'query' => ['page' => 1]])`
- **THEN** 等价于 `->get()->timeout(30)->header('Authorization', 'Bearer xxx')->query(['page' => 1])`

#### Scenario: 未知 key 抛异常

- **WHEN** 用户调用 `->withOptions(['timedout' => 30])`（拼写错误）
- **THEN** 抛异常，消息含"不支持的选项 key: timedout"

#### Scenario: null 值跳过

- **WHEN** 用户调用 `->withOptions(['timeout' => 30, 'proxy' => null])`
- **THEN** 仅设置 timeout=30，proxy 不调用（保持原值）

### Requirement: 全局 base_uri 配置

系统 SHALL 支持通过 `setConfig(['base_uri' => 'https://example.com'])` 设置全局基础 URI。

#### 行为约定

- 请求 URL 以 `/` 开头时，与 base_uri 拼接为完整 URL
- 请求 URL 以 `http://` 或 `https://` 开头时（绝对 URL），忽略 base_uri
- base_uri 末尾的 `/` 自动处理（避免双斜杠）
- base_uri 为空字符串或 null 时清除（不设置 base_uri）
- base_uri 非法（如 `://invalid`）在 setConfig 阶段抛异常（fail-fast，两阶段校验）

#### Scenario: 相对 URL 拼接

- **WHEN** `setConfig(['base_uri' => 'https://user-svc.internal'])`，请求 `createRequest('/users/123')`
- **THEN** 实际请求 URL = `https://user-svc.internal/users/123`

#### Scenario: 绝对 URL 优先

- **WHEN** `setConfig(['base_uri' => 'https://user-svc.internal'])`，请求 `createRequest('https://other.com/api')`
- **THEN** 实际请求 URL = `https://other.com/api`（忽略 base_uri）

#### Scenario: base_uri 末尾斜杠处理

- **WHEN** `setConfig(['base_uri' => 'https://user-svc.internal/'])`，请求 `createRequest('/users/123')`
- **THEN** 实际请求 URL = `https://user-svc.internal/users/123`（无双斜杠）

### Requirement: 全局 base_headers 配置

系统 SHALL 支持通过 `setConfig(['base_headers' => ['Authorization' => 'Bearer xxx']])` 设置全局默认 headers。

#### 行为约定

- 所有请求自动携带 base_headers 中的 header
- 请求级同名 header 覆盖全局 base_headers（请求级优先）
- base_headers 值为 null 时跳过该 header
- base_headers 为空数组或 null 时清除
- base_headers 非标量值（嵌套数组/对象）在 setConfig 阶段抛异常（fail-fast）
- base_headers 变更触发 Client 重建（与 proxy/verify_ssl 等配置一致）

#### Scenario: 自动携带全局 header

- **WHEN** `setConfig(['base_headers' => ['Authorization' => 'Bearer xxx']])`，请求 `createRequest($url)->get()`
- **THEN** 请求头包含 `Authorization: Bearer xxx`

#### Scenario: 请求级覆盖全局 header

- **WHEN** `setConfig(['base_headers' => ['Authorization' => 'Bearer global']])`，请求 `->header('Authorization', 'Bearer override')`
- **THEN** 实际发送的 Authorization = `Bearer override`（请求级覆盖全局）
