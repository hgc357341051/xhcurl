# 静态 HTTP 方法便捷调用 Spec

## Why
当前 PHP 使用者每次发起 HTTP 请求都需写 3 行样板代码：`createRequest($url)->get()->execute()`。
Guzzle/axios/PHP `requests` 库均提供 `Client::get($url, $opts)` 一行调用便捷方法，
XHCurl 缺失该高频用法，与 PHP 生态主流 HTTP 客户端 API 习惯不一致。

## What Changes
- 新增 `XHCurl::get($url, array $options = []): array` 静态便捷方法
- 新增 `XHCurl::post($url, $body = null, array $options = []): array` 静态便捷方法
- 新增 `XHCurl::put($url, $body = null, array $options = []): array` 静态便捷方法
- 新增 `XHCurl::delete($url, array $options = []): array` 静态便捷方法
- 新增 `XHCurl::patch($url, $body = null, array $options = []): array` 静态便捷方法
- 新增 `XHCurl::head($url, array $options = []): array` 静态便捷方法
- 所有方法返回与 `execute()` 完全一致的结果数组（含 12 字段 + attempts/truncated）
- `$options` 复用 `withOptions()` 的 18 个 key（含 retry），语义完全一致
- `$body` 参数（post/put/patch）接受 string/array/null：
  - string → `body()`（原始字节体）
  - array → `json()`（自动序列化为 JSON，设置 Content-Type: application/json）
  - null → 无请求体
- 版本 1.6.0 → 1.7.0，非 BREAKING
- mock_server 新增 `/echo-method` 端点（回显请求方法与请求体），用于测试
- README 新增「静态便捷方法」小节
- CHANGELOG 新增 [1.7.0] 条目
- 新增 `php_add_static_http_methods_test.php` 测试文件

## Impact
- Affected specs: 无（新增独立功能）
- Affected code:
  - `rust/src/php_ext.rs`：XHCurl 类新增 6 个静态方法（PHP 绑定）
  - `rust/tests/mock_server.php`：新增 `/echo-method` 端点
  - `rust/Cargo.toml`：版本号 1.6.0 → 1.7.0
  - `README.md`：新增静态便捷方法文档
  - `CHANGELOG.md`：新增 [1.7.0] 条目
  - `rust/tests/php_add_static_http_methods_test.php`：新增测试文件

## ADDED Requirements
### Requirement: 静态 HTTP 方法便捷调用
系统 SHALL 提供 `XHCurl::get/post/put/delete/patch/head` 6 个静态方法，
一行完成「创建请求 + 设置方法 + 可选选项 + 执行」，返回与 `execute()` 一致的结果数组。

#### Scenario: GET 请求带选项
- **WHEN** 调用 `XHCurl::get('http://localhost:18399/get', ['timeout' => 10, 'query' => ['k' => 'v']])`
- **THEN** 返回结果数组，`success=true`，`status=200`，`body` 含响应内容

#### Scenario: POST 请求带 JSON body
- **WHEN** 调用 `XHCurl::post('http://localhost:18399/echo-method', ['name' => 'test'])`
- **THEN** 返回结果数组，`success=true`，`status=200`，body 含回显的 POST 方法与 JSON body

#### Scenario: 无选项的简单调用
- **WHEN** 调用 `XHCurl::get('http://localhost:18399/get')`
- **THEN** 等价于 `XHCurl::createRequest($url)->get()->execute()`

#### Scenario: body 参数类型
- **WHEN** post/put/patch 的 $body 为 string → 用 body()（原始字节体）
- **WHEN** post/put/patch 的 $body 为 array → 用 json()（自动序列化）
- **WHEN** post/put/patch 的 $body 为 null → 无请求体

#### Scenario: options 复用 withOptions 语义
- **WHEN** $options 传入 `['timeout' => 5, 'headers' => ['X-Trace' => '1'], 'retry' => ['times' => 2]]`
- **THEN** 等价于 `->withOptions($options)`，支持全部 18 个 key

#### Scenario: 未知 options key 抛异常
- **WHEN** $options 含未知 key（如 `['timedout' => 5]`）
- **THEN** 抛异常 "不支持的选项 key: timedout"（fail-fast，与 withOptions 一致）

#### Scenario: 链式调用不可用
- **WHEN** 静态方法返回 array（非 $this）
- **THEN** 不可链式调用（与 execute() 返回值一致，是一次性终端调用）

## MODIFIED Requirements
### Requirement: XHCurl 静态 API
现有 `XHCurl::createRequest()`/`setConfig()`/`getConfig()`/`version()`/`await()`/`gather()`/`each()`/`run()` 静态方法保持不变。
新增 6 个静态便捷方法 `get/post/put/delete/patch/head`，作为 `createRequest()->method()->execute()` 的语法糖。
