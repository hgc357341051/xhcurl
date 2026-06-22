# XHCurl Rust 版本 - 开发文档与使用说明

> 版本：2.0.0 | Rust 引擎 | PHP 8.0\~8.4

***

## 目录

1. [项目概述](#1-项目概述)
2. [架构设计](#2-架构设计)
3. [编译与安装](#3-编译与安装)
4. [PHP API 完整参考](#4-php-api-完整参考)
   - [XHCurl - 全局管理器](#41-xhcurl---全局管理器)
   - [XHRequest - 请求构建器](#42-xhrequest---请求构建器)
   - [XHResponse - 响应对象](#43-xhresponse---响应对象)
   - [XHMulti - 异步批量执行器](#44-xhmulti---异步批量执行器)
   - [XHThreadPool - 线程池](#45-xhthreadpool---线程池)
5. [Rust 内部 API 参考](#5-rust-内部-api-参考)
6. [链式调用完整示例](#6-链式调用完整示例)
7. [安全机制](#7-安全机制)
8. [性能优化](#8-性能优化)
9. [常见问题](#9-常见问题)

***

## 1. 项目概述

XHCurl Rust 版本是对 C 版本 PHP cURL 扩展的完全重写，核心改进：

| 特性      | C 版本       | Rust 版本                    |
| ------- | ---------- | -------------------------- |
| 并发模型    | 单线程事件循环    | tokio M:N 调度（类 goroutine）  |
| 线程安全    | 运行时检查      | 编译期保证（Send/Sync）           |
| 内存安全    | 手动管理       | 所有权模型（无 double-free/UAF）   |
| 响应体限制   | 无          | 流式读取 + max\_response\_size |
| Channel | 无界（内存溢出风险） | 有界 + 背压控制                  |
| 请求数量    | 无限制        | 上限 10000/批次                |
| FPM/CLI | 需要手动适配     | 自动适配运行时                    |

***

## 2. 架构设计

```
rust/src/
├── lib.rs        # 库入口，模块声明与类型导出
├── error.rs      # 错误类型（XhCurlError 枚举 + thiserror）
├── buffer.rs     # 响应缓冲区（大小限制 + 分段读取）
├── header.rs     # HTTP 头部管理（大小写不敏感）
├── cookie.rs     # Cookie 管理（会话级 + 持久化）
├── curl.rs       # 全局管理器（单例 + RwLock 配置）
├── request.rs    # 请求构建器（Builder 模式 + 链式调用）
├── response.rs   # 响应对象（懒加载 + 流式读取）
├── multi.rs      # 异步批量执行器（tokio M:N 调度）
├── threadpool.rs # 线程池（channel 通信 + 优先级）
└── php_ext.rs    # PHP 扩展入口（ext-php-rs 绑定）
```

### 并发模型

```
XHMulti（FPM + CLI 安全）：
  主线程 (PHP) ──创建 task──> [tokio task 1] ──┐
                              [tokio task 2] ──┤──> [结果 channel] ──> 主线程
                              [tokio task N] ──┘
  使用 new_current_thread() 运行时，单线程调度 N 个异步任务

XHThreadPool（仅 CLI 模式）：
  主线程 (PHP) ──发送任务──> [任务 channel] ──> 工作线程 1
                                              工作线程 2
                                              工作线程 N
  主线程 (PHP) <──接收结果── [结果 channel] <── 工作线程
  使用 new_multi_thread() 运行时，真正的多线程并行
```

***

## 3. 编译与安装

### 3.1 环境要求

- Rust 1.70+（stable 工具链）
- PHP 8.0\~8.4（含开发头文件 `php-config`）
- libclang（ext-php-rs bindgen 依赖）
- OpenSSL 或 rustls（TLS 支持）

### 3.2 编译

```bash
# 进入 Rust 目录
cd rust

# 编译 PHP 扩展（启用 php feature）
cargo build --release --features php

# 编译产物位置
# Linux:   target/release/libxhcurl.so
# macOS:   target/release/libxhcurl.dylib
# Windows: target/release/xhcurl.dll
```

### 3.3 安装

```bash
# 方法 1：手动复制
cp target/release/libxhcurl.so $(php-config --extension-dir)/xhcurl.so

# 方法 2：php.ini 配置
echo "extension=xhcurl" >> $(php-config --ini-path)/php.ini

# 验证
php -m | grep xhcurl
php -r "echo XHCurl::version();"
```

### 3.4 CI/CD 自动编译

项目包含 GitHub Actions 工作流 `.github/workflows/build-rust.yml`，自动编译：

- **Linux**: Ubuntu 22.04 + PHP 8.0\~8.4 → `.so`
- **macOS**: macOS 14 + PHP 8.0\~8.3 → `.dylib`
- **Windows**: Windows 2022 + PHP 8.0\~8.4 NTS/ZTS → `.dll`
- **代码质量**: cargo fmt + clippy + test

***

## 4. PHP API 完整参考

### 4.1 XHCurl - 全局管理器

全局静态类，管理扩展配置和工厂方法。

#### `XHCurl::version(): string`

获取扩展版本号。

```php
$version = XHCurl::version(); // "2.0.0"
```

#### `XHCurl::setConfig(array $config): void`

设置全局配置，影响所有后续请求的默认值。

| 参数键                 | 类型     | 默认值          | 说明                  |
| ------------------- | ------ | ------------ | ------------------- |
| `connect_timeout`   | int    | 30           | 连接超时（秒）             |
| `request_timeout`   | int    | 60           | 请求超时（秒）             |
| `max_response_size` | int    | 10485760     | 最大响应体大小（字节，默认 10MB） |
| `follow_redirects`  | bool   | true         | 是否跟随重定向             |
| `max_redirects`     | int    | 10           | 最大重定向次数             |
| `verify_ssl`        | bool   | true         | 是否验证 SSL 证书         |
| `user_agent`        | string | "XHCurl/2.0" | 默认 User-Agent       |
| `proxy`             | string | null         | 全局代理地址              |

```php
XHCurl::setConfig([
    'connect_timeout'   => 10,
    'request_timeout'   => 30,
    'max_response_size' => 5 * 1024 * 1024, // 5MB
    'follow_redirects'  => true,
    'max_redirects'     => 5,
    'verify_ssl'        => true,
    'user_agent'        => 'MyApp/1.0',
    'proxy'             => 'http://127.0.0.1:7890',
]);
```

#### `XHCurl::getConfig(): array`

获取当前全局配置，返回关联数组。

```php
$config = XHCurl::getConfig();
// [
//     'connect_timeout' => 30,
//     'request_timeout' => 60,
//     'max_response_size' => 10485760,
//     ...
// ]
```

#### `XHCurl::isCli(): bool`

检查是否在 CLI 模式下运行。

```php
if (XHCurl::isCli()) {
    // CLI 模式下可使用 XHThreadPool
}
```

#### `XHCurl::createRequest(string $url): XHRequest`

工厂方法，创建请求构建器。

```php
$request = XHCurl::createRequest("https://api.example.com/users");
```

***

### 4.2 XHRequest - 请求构建器

使用 Builder 模式，支持链式调用。每个方法返回 `$this`，可连续调用。

#### 构造函数

```php
$request = new XHRequest(string $url);
// 或使用工厂方法
$request = XHCurl::createRequest(string $url);
```

#### HTTP 方法设置

| 方法         | 签名                              | 说明                                                    |
| ---------- | ------------------------------- | ----------------------------------------------------- |
| `method()` | `method(string $method): $this` | 设置自定义 HTTP 方法（GET/POST/PUT/DELETE/PATCH/HEAD/OPTIONS） |
| `get()`    | `get(): $this`                  | 设置为 GET 方法                                            |
| `post()`   | `post(): $this`                 | 设置为 POST 方法                                           |
| `put()`    | `put(): $this`                  | 设置为 PUT 方法                                            |
| `delete()` | `delete(): $this`               | 设置为 DELETE 方法                                         |
| `patch()`  | `patch(): $this`                | 设置为 PATCH 方法                                          |
| `head()`   | `head(): $this`                 | 设置为 HEAD 方法（仅获取响应头）                                   |

```php
// 链式调用设置方法
$request = new XHRequest("https://api.example.com/users")
    ->post();
```

#### 请求头设置

| 方法         | 签名                                           | 说明      |
| ---------- | -------------------------------------------- | ------- |
| `header()` | `header(string $name, string $value): $this` | 添加单个请求头 |

```php
$request->header("Authorization", "Bearer token123")
        ->header("Accept", "application/json")
        ->header("X-Custom-Header", "value");
```

#### 请求体设置

| 方法       | 签名                          | 说明                                               |
| -------- | --------------------------- | ------------------------------------------------ |
| `json()` | `json(array $data): $this`  | 设置 JSON 请求体（自动添加 Content-Type: application/json） |
| `form()` | `form(array $data): $this`  | 设置表单请求体（application/x-www-form-urlencoded）       |
| `body()` | `body(string $data): $this` | 设置原始请求体                                          |

```php
// JSON 请求体
$request->json(["name" => "test", "age" => 18]);

// 表单请求体
$request->form(["username" => "admin", "password" => "secret"]);

// 原始请求体
$request->body('{"raw": "data"}');
```

#### 超时设置

| 方法                 | 签名                                    | 说明                |
| ------------------ | ------------------------------------- | ----------------- |
| `timeout()`        | `timeout(int $seconds): $this`        | 设置请求总超时（秒）        |
| `connectTimeout()` | `connectTimeout(int $seconds): $this` | 设置连接超时（秒），仅控制连接阶段 |

```php
$request->timeout(30)          // 请求总超时 30 秒
        ->connectTimeout(5);   // 连接超时 5 秒
```

#### SSL/代理/重定向设置

| 方法                  | 签名                                     | 说明                         |
| ------------------- | -------------------------------------- | -------------------------- |
| `verifySsl()`       | `verifySsl(bool $verify): $this`       | 是否验证 SSL 证书                |
| `userAgent()`       | `userAgent(string $ua): $this`         | 设置 User-Agent              |
| `proxy()`           | `proxy(string $proxy): $this`          | 设置代理（支持 HTTP/HTTPS/SOCKS5） |
| `followRedirects()` | `followRedirects(bool $follow): $this` | 是否跟随重定向                    |
| `maxRedirects()`    | `maxRedirects(int $max): $this`        | 最大重定向次数                    |

```php
$request->verifySsl(false)             // 跳过 SSL 验证（开发环境）
        ->userAgent("MyBot/1.0")       // 自定义 UA
        ->proxy("socks5://127.0.0.1:1080") // SOCKS5 代理
        ->followRedirects(true)        // 跟随重定向
        ->maxRedirects(5);             // 最多 5 次重定向
```

#### 请求标识设置

| 方法              | 签名                                  | 说明                   |
| --------------- | ----------------------------------- | -------------------- |
| `setId()`       | `setId(string $id): $this`          | 设置请求 ID（批量请求时用于关联结果） |
| `setPriority()` | `setPriority(int $priority): $this` | 设置优先级（线程池模式，数值越大越优先） |

```php
$request->setId("api_user_001")
        ->setPriority(10);
```

#### Getter 方法

| 方法            | 签名                    | 说明         |
| ------------- | --------------------- | ---------- |
| `getUrl()`    | `getUrl(): string`    | 获取请求 URL   |
| `getMethod()` | `getMethod(): string` | 获取 HTTP 方法 |
| `getId()`     | `getId(): ?string`    | 获取请求 ID    |

```php
echo $request->getUrl();    // "https://api.example.com/users"
echo $request->getMethod(); // "POST"
echo $request->getId();     // "api_user_001" 或 null
```

***

### 4.3 XHResponse - 响应对象

响应对象由 `XHMulti::execute()` 或 `XHThreadPool::execute()` 返回，不可手动创建。

#### 状态码方法

| 方法            | 签名                  | 说明                             |
| ------------- | ------------------- | ------------------------------ |
| `status()`    | `status(): int`     | HTTP 状态码（200/404/500 等，失败时为 0） |
| `isSuccess()` | `isSuccess(): bool` | 是否 2xx 状态码                     |

```php
$result = $multi->execute();
// 遍历结果
foreach ($result as $item) {
    if ($item['success']) {
        echo "状态码: " . $item['status']; // 200
    }
}
```

#### 响应头方法

| 方法          | 签名                              | 说明            |
| ----------- | ------------------------------- | ------------- |
| `header()`  | `header(string $name): ?string` | 获取指定响应头       |
| `headers()` | `headers(): array`              | 获取所有响应头（关联数组） |

```php
$contentType = $response->header("content-type");
$allHeaders  = $response->headers();
```

#### 响应体方法

| 方法           | 签名                | 说明              |
| ------------ | ----------------- | --------------- |
| `body()`     | `body(): string`  | 获取响应体文本         |
| `json()`     | `json(): array`   | 解析响应体为 PHP 关联数组 |
| `bodySize()` | `bodySize(): int` | 响应体大小（字节）       |

```php
$text = $response->body();              // 原始文本
$data = $response->json();              // 解析为数组
$size = $response->bodySize();          // 字节数
```

#### 元数据方法

| 方法             | 签名                      | 说明                             |
| -------------- | ----------------------- | ------------------------------ |
| `url()`        | `url(): string`         | 最终 URL（可能因重定向变化）               |
| `elapsedMs()`  | `elapsedMs(): int`      | 请求耗时（毫秒）                       |
| `error()`      | `error(): ?string`      | 错误信息（成功时为 null）                |
| `remoteAddr()` | `remoteAddr(): ?string` | 远程服务器地址（IP:Port）               |
| `version()`    | `version(): ?string`    | HTTP 协议版本（"HTTP/1.1"/"HTTP/2"） |

```php
echo $response->url();         // "https://api.example.com/users"
echo $response->elapsedMs();   // 150
echo $response->error();       // null 或 "连接超时"
echo $response->remoteAddr();  // "93.184.216.34:443"
echo $response->version();     // "HTTP/2"
```

***

### 4.4 XHMulti - 异步批量执行器

基于 tokio M:N 调度的异步批量执行器，**FPM 和 CLI 模式均安全使用**。

> 核心原理：每个请求封装为 tokio task（类似 goroutine），tokio 运行时自动在 M 个工作线程上调度 N 个 task。

#### 构造函数

```php
$multi = new XHMulti();
```

#### `add(XHRequest $request): $this`

添加请求到批量执行器，支持链式调用。

- 单次批量上限：**10000 个请求**
- 超过上限抛出异常

```php
$multi = new XHMulti();
$multi->add($request1)
      ->add($request2)
      ->add($request3);
```

#### `maxConcurrency(int $max): $this`

设置最大并发数。

- `0` = 无限制（默认）
- 建议值：50\~500（取决于目标服务器承受能力）

```php
$multi->maxConcurrency(100); // 最多 100 个并发请求
```

#### `execute(): array`

执行所有请求，返回结果数组。

返回结构：

```php
[
    0 => [
        'id'         => string,    // 请求 ID（默认为 URL）
        'success'    => bool,      // 是否成功
        'elapsed_ms' => int,       // 耗时（毫秒）
        'error'      => ?string,   // 错误信息
        'status'     => ?int,      // HTTP 状态码
        'body_size'  => ?int,      // 响应体大小
        'body'       => ?string,   // 响应体文本
        'url'        => ?string,   // 最终 URL
    ],
    1 => [...],
    // ...
]
```

```php
$multi = new XHMulti();
$multi->add(new XHRequest("https://httpbin.org/get"))
      ->add(new XHRequest("https://httpbin.org/status/200"))
      ->maxConcurrency(10);

$results = $multi->execute();

foreach ($results as $result) {
    if ($result['success']) {
        echo "URL: {$result['url']}, 状态码: {$result['status']}, 耗时: {$result['elapsed_ms']}ms\n";
    } else {
        echo "请求失败: {$result['error']}\n";
    }
}
```

***

### 4.5 XHThreadPool - 线程池

基于 tokio 多线程运行时的线程池，**仅 CLI 模式可用**。

> 与 XHMulti 的区别：XHMulti 使用单线程运行时调度异步任务（M:N 协程），XHThreadPool 使用多线程运行时实现真正的并行执行。

#### 构造函数

```php
$pool = new XHThreadPool(int $workers = 0);
```

| 参数         | 类型  | 默认值          | 说明     |
| ---------- | --- | ------------ | ------ |
| `$workers` | int | 0（= CPU 核心数） | 工作线程数量 |

```php
// 使用默认线程数（= CPU 核心数）
$pool = new XHThreadPool();

// 指定 8 个工作线程
$pool = new XHThreadPool(8);
```

#### `add(XHRequest $request): $this`

添加请求到线程池，支持链式调用。

- 单次批量上限：**10000 个请求**
- FPM 模式下调用会抛出异常

```php
$pool = new XHThreadPool(4);
$pool->add($request1)
     ->add($request2)
     ->add($request3);
```

#### `execute(): array`

执行所有请求，返回结果数组（格式同 XHMulti::execute()）。

```php
$pool = new XHThreadPool(4);
$pool->add(new XHRequest("https://httpbin.org/get"))
     ->add(new XHRequest("https://httpbin.org/delay/2"));

$results = $pool->execute();
```

***

## 5. Rust 内部 API 参考

以下为 Rust 层面的 API，供扩展开发使用。

### 5.1 XhRequest - 请求构建器

```rust
use xhcurl::XhRequest;
use xhcurl::HttpMethod;

// 创建请求
let request = XhRequest::new("https://api.example.com/users")
    .post()                                          // POST 方法
    .header("Authorization", "Bearer token")         // 请求头
    .header("Accept", "application/json")            // 请求头
    .body_json_str(r#"{"name": "test"}"#)?           // JSON 请求体
    .timeout(30)                                     // 请求超时 30s
    .connect_timeout(5)                              // 连接超时 5s
    .verify_ssl(true)                                // 验证 SSL
    .user_agent("MyApp/1.0")                         // User-Agent
    .proxy("socks5://127.0.0.1:1080")                // SOCKS5 代理
    .follow_redirects(true)                          // 跟随重定向
    .max_redirects(5)                                // 最多 5 次重定向
    .id("req_001")                                   // 请求 ID
    .priority(10);                                   // 优先级
```

#### 完整方法列表

| 方法                    | 签名                                                             | 说明             |
| --------------------- | -------------------------------------------------------------- | -------------- |
| `new()`               | `new(url: impl Into<String>) -> Self`                          | 创建 GET 请求      |
| `url()`               | `url(url: impl Into<String>) -> Self`                          | 设置 URL         |
| `method()`            | `method(method: HttpMethod) -> Self`                           | 设置 HTTP 方法     |
| `get()`               | `get(self) -> Self`                                            | GET 方法         |
| `post()`              | `post(self) -> Self`                                           | POST 方法        |
| `put()`               | `put(self) -> Self`                                            | PUT 方法         |
| `delete()`            | `delete(self) -> Self`                                         | DELETE 方法      |
| `patch()`             | `patch(self) -> Self`                                          | PATCH 方法       |
| `head()`              | `head(self) -> Self`                                           | HEAD 方法        |
| `options()`           | `options(self) -> Self`                                        | OPTIONS 方法     |
| `header()`            | `header(name: &str, value: &str) -> Self`                      | 添加请求头          |
| `headers()`           | `headers<I, S>(headers: I) -> Self`                            | 批量添加请求头        |
| `body_bytes()`        | `body_bytes(data: Vec<u8>) -> Self`                            | 原始字节请求体        |
| `body_json()`         | `body_json<T: Serialize>(json: &T) -> Result<Self>`            | JSON 请求体       |
| `body_json_str()`     | `body_json_str(json_str: &str) -> Result<Self>`                | JSON 字符串请求体    |
| `body_form()`         | `body_form(form: Vec<(String, String)>) -> Self`               | 表单请求体          |
| `body_multipart()`    | `body_multipart(fields: Vec<MultipartField>) -> Self`          | 多部分表单          |
| `connect_timeout()`   | `connect_timeout(secs: u64) -> Self`                           | 连接超时           |
| `request_timeout()`   | `request_timeout(secs: u64) -> Self`                           | 请求超时           |
| `follow_redirects()`  | `follow_redirects(follow: bool) -> Self`                       | 跟随重定向          |
| `max_redirects()`     | `max_redirects(max: u32) -> Self`                              | 最大重定向次数        |
| `verify_ssl()`        | `verify_ssl(verify: bool) -> Self`                             | SSL 验证         |
| `user_agent()`        | `user_agent(ua: impl Into<String>) -> Self`                    | User-Agent     |
| `proxy()`             | `proxy(proxy: impl Into<String>) -> Self`                      | 代理地址           |
| `id()`                | `id(id: impl Into<String>) -> Self`                            | 请求 ID          |
| `priority()`          | `priority(priority: i32) -> Self`                              | 优先级            |
| `stream_chunk_size()` | `stream_chunk_size(size: usize) -> Self`                       | 流式回调间隔         |
| `to_reqwest()`        | `to_reqwest(&self, client: &Client) -> Result<RequestBuilder>` | 转换为 reqwest 请求 |

### 5.2 XhResponse - 响应对象

```rust
use xhcurl::XhResponse;
use std::time::Duration;

// 从 reqwest 响应创建（异步）
let response = XhResponse::from_reqwest(reqwest_response, elapsed).await?;

// 从已解析的元数据创建（流式读取后）
let response = XhResponse::from_parts(
    status, url, headers, body, elapsed, remote_addr, version
);

// 创建错误响应
let error_response = XhResponse::from_error(
    "连接超时".to_string(),
    "https://example.com".to_string(),
    Duration::from_secs(30),
);
```

#### 完整方法列表

| 方法                  | 签名                                                                                               | 说明             |
| ------------------- | ------------------------------------------------------------------------------------------------ | -------------- |
| `from_reqwest()`    | `async fn from_reqwest(Response, Duration) -> Result<Self>`                                      | 从 reqwest 响应创建 |
| `from_parts()`      | `fn from_parts(u16, String, HashMap, Vec<u8>, Duration, Option<String>, Option<String>) -> Self` | 从元数据创建         |
| `from_error()`      | `fn from_error(String, String, Duration) -> Self`                                                | 创建错误响应         |
| `status()`          | `fn status(&self) -> u16`                                                                        | 状态码            |
| `is_success()`      | `fn is_success(&self) -> bool`                                                                   | 是否 2xx         |
| `is_client_error()` | `fn is_client_error(&self) -> bool`                                                              | 是否 4xx         |
| `is_server_error()` | `fn is_server_error(&self) -> bool`                                                              | 是否 5xx         |
| `is_redirect()`     | `fn is_redirect(&self) -> bool`                                                                  | 是否 3xx         |
| `headers()`         | `fn headers(&self) -> &HeaderManager`                                                            | 响应头管理器         |
| `header()`          | `fn header(&self, name: &str) -> Option<String>`                                                 | 获取指定头          |
| `content_type()`    | `fn content_type(&self) -> Option<String>`                                                       | Content-Type   |
| `content_length()`  | `fn content_length(&self) -> Option<usize>`                                                      | Content-Length |
| `body()`            | `fn body(&self) -> Option<&[u8]>`                                                                | 响应体字节          |
| `body_size()`       | `fn body_size(&self) -> usize`                                                                   | 响应体大小          |
| `body_text()`       | `fn body_text(&self) -> Result<String>`                                                          | 响应体文本          |
| `body_json()`       | `fn body_json(&self) -> Result<Value>`                                                           | JSON 解析        |
| `url()`             | `fn url(&self) -> &str`                                                                          | 最终 URL         |
| `elapsed()`         | `fn elapsed(&self) -> Duration`                                                                  | 请求耗时           |
| `remote_addr()`     | `fn remote_addr(&self) -> Option<&str>`                                                          | 远程地址           |
| `version()`         | `fn version(&self) -> Option<&str>`                                                              | HTTP 版本        |
| `error()`           | `fn error(&self) -> Option<&str>`                                                                | 错误信息           |
| `has_error()`       | `fn has_error(&self) -> bool`                                                                    | 是否有错误          |
| `set_body()`        | `fn set_body(&mut self, data: Vec<u8>)`                                                          | 设置响应体          |
| `read_body()`       | `async fn read_body(&mut self, &mut Response) -> Result<&[u8]>`                                  | 懒加载读取          |

### 5.3 XhMulti - 异步批量执行器

```rust
use xhcurl::XhMulti;

let mut multi = XhMulti::new(client);

// 链式配置
multi.add(request1)?
     .add(request2)?
     .add_many(vec![req3, req4, req5])?;

// 配置
multi = multi.max_concurrency(100)    // 最大并发数
             .timeout(60)             // 全局超时 60s
             .max_response_size(20 * 1024 * 1024); // 最大响应体 20MB

// 启用流式回调
let mut stream_rx = multi.enable_streaming();

// 执行
let results = multi.execute().await?;
```

#### 完整方法列表

| 方法                      | 签名                                                                  | 说明             |
| ----------------------- | ------------------------------------------------------------------- | -------------- |
| `new()`                 | `fn new(client: Client) -> Self`                                    | 创建执行器          |
| `with_default_client()` | `fn with_default_client() -> Result<Self>`                          | 使用默认客户端        |
| `add()`                 | `fn add(&mut self, XhRequest) -> Result<&mut Self>`                 | 添加请求（上限 10000） |
| `add_many()`            | `fn add_many(&mut self, Vec<XhRequest>) -> Result<&mut Self>`       | 批量添加           |
| `max_concurrency()`     | `fn max_concurrency(self, usize) -> Self`                           | 最大并发数          |
| `timeout()`             | `fn timeout(self, u64) -> Self`                                     | 全局超时           |
| `max_response_size()`   | `fn max_response_size(self, usize) -> Self`                         | 最大响应体大小        |
| `enable_streaming()`    | `fn enable_streaming(&mut self) -> Receiver<(String, StreamEvent)>` | 启用流式回调         |
| `execute()`             | `async fn execute(&mut self) -> Result<Vec<RequestResult>>`         | 执行所有请求         |
| `len()`                 | `fn len(&self) -> usize`                                            | 请求数量           |
| `is_empty()`            | `fn is_empty(&self) -> bool`                                        | 是否为空           |
| `clear()`               | `fn clear(&mut self)`                                               | 清空请求           |

### 5.4 XhThreadPool - 线程池

```rust
use xhcurl::{XhThreadPool, ThreadPoolConfig};

// 自定义配置
let config = ThreadPoolConfig {
    worker_count: 8,          // 8 个工作线程
    queue_capacity: 2000,     // 队列容量 2000
    idle_timeout: 120,        // 空闲超时 120s
    enable_priority: true,    // 启用优先级
};

let mut pool = XhThreadPool::new(config, client);
pool.start()?;

// 提交任务
pool.submit(request)?;

// 批量执行
let results = pool.execute_all(requests).await?;

// 关闭
pool.shutdown().await;
```

#### ThreadPoolConfig 字段

| 字段                | 类型    | 默认值     | 说明         |
| ----------------- | ----- | ------- | ---------- |
| `worker_count`    | usize | CPU 核心数 | 工作线程数量     |
| `queue_capacity`  | usize | 1000    | 任务队列容量（有界） |
| `idle_timeout`    | u64   | 60      | 空闲线程超时（秒）  |
| `enable_priority` | bool  | true    | 是否启用优先级    |

#### 完整方法列表

| 方法                      | 签名                                                                              | 说明         |
| ----------------------- | ------------------------------------------------------------------------------- | ---------- |
| `new()`                 | `fn new(config: ThreadPoolConfig, client: Client) -> Self`                      | 创建线程池      |
| `with_default_config()` | `fn with_default_config(client: Client) -> Self`                                | 默认配置       |
| `start()`               | `fn start(&mut self) -> Result<()>`                                             | 启动线程池      |
| `submit()`              | `fn submit(&self, XhRequest) -> Result<()>`                                     | 提交单个请求     |
| `submit_with_stream()`  | `fn submit_with_stream(&self, XhRequest, Sender) -> Result<()>`                 | 提交带流式回调的请求 |
| `execute_all()`         | `async fn execute_all(&mut self, Vec<XhRequest>) -> Result<Vec<RequestResult>>` | 批量执行       |
| `shutdown()`            | `async fn shutdown(&mut self)`                                                  | 关闭线程池      |
| `worker_count()`        | `fn worker_count(&self) -> usize`                                               | 工作线程数      |
| `is_running()`          | `fn is_running(&self) -> bool`                                                  | 是否运行中      |

### 5.5 StreamEvent - 流式回调事件

```rust
pub enum StreamEvent {
    /// 接收到响应头
    Headers { status: u16, headers: HashMap<String, String> },
    /// 接收到响应体数据块
    Chunk { data: Vec<u8> },
    /// 请求完成
    Complete { elapsed: Duration, body_size: usize },
    /// 请求出错
    Error { message: String },
}
```

### 5.6 XhCurlError - 错误类型

```rust
pub enum XhCurlError {
    Request(reqwest::Error),       // 网络请求错误
    UrlParse(url::ParseError),     // URL 格式错误
    Json(serde_json::Error),       // JSON 处理错误
    HttpStatus { status: u16 },    // HTTP 状态码错误
    InvalidArgument(String),       // 参数验证错误
    Memory(String),                // 内存分配失败
    ThreadPool(String),            // 线程池错误
    AsyncTask(JoinError),          // 异步任务错误
    Channel(String),               // 通道通信错误
    Io(std::io::Error),            // I/O 错误
    Generic(String),               // 通用错误
}
```

***

## 6. 链式调用完整示例

### 6.1 基础 GET 请求

```php
<?php
// 创建请求并链式设置参数
$request = (new XHRequest("https://httpbin.org/get"))
    ->get()                                // GET 方法（默认）
    ->header("Accept", "application/json") // 请求头
    ->timeout(10)                          // 超时 10s
    ->verifySsl(true);                     // 验证 SSL

// 使用 XHMulti 执行
$multi = new XHMulti();
$result = $multi->add($request)->execute();

print_r($result);
```

### 6.2 POST JSON 请求

```php
<?php
$request = (new XHRequest("https://api.example.com/users"))
    ->post()                                          // POST 方法
    ->header("Authorization", "Bearer your_token")    // 认证头
    ->json([                                          // JSON 请求体
        'name'  => '张三',
        'email' => 'zhangsan@example.com',
        'age'   => 25,
    ])
    ->timeout(30)                                     // 超时 30s
    ->connectTimeout(5);                              // 连接超时 5s

$multi = new XHMulti();
$result = $multi->add($request)->execute();
```

### 6.3 表单提交

```php
<?php
$request = (new XHRequest("https://example.com/login"))
    ->post()                                          // POST 方法
    ->form([                                          // 表单数据
        'username' => 'admin',
        'password' => 'secret',
        'remember' => '1',
    ])
    ->followRedirects(true)                           // 跟随重定向
    ->maxRedirects(3);                                // 最多 3 次

$multi = new XHMulti();
$result = $multi->add($request)->execute();
```

### 6.4 代理 + 自定义 UA

```php
<?php
$request = (new XHRequest("https://httpbin.org/ip"))
    ->get()
    ->proxy("socks5://127.0.0.1:1080")    // SOCKS5 代理
    ->userAgent("Mozilla/5.0 CustomBot")  // 自定义 UA
    ->verifySsl(false);                   // 跳过 SSL 验证

$multi = new XHMulti();
$result = $multi->add($request)->execute();
```

### 6.5 批量并发请求（XHMulti）

```php
<?php
// 创建多个请求
$urls = [
    'https://httpbin.org/get',
    'https://httpbin.org/status/200',
    'https://httpbin.org/delay/1',
    'https://httpbin.org/delay/2',
    'https://httpbin.org/headers',
];

$multi = new XHMulti();

// 链式添加请求
foreach ($urls as $i => $url) {
    $request = (new XHRequest($url))
        ->get()
        ->setId("req_{$i}")        // 设置请求 ID
        ->timeout(10);

    $multi->add($request);
}

// 设置最大并发数
$multi->maxConcurrency(3);

// 执行
$results = $multi->execute();

// 处理结果
foreach ($results as $result) {
    echo sprintf(
        "[%s] %s - 状态码: %d, 耗时: %dms\n",
        $result['id'],
        $result['success'] ? '成功' : '失败',
        $result['status'] ?? 0,
        $result['elapsed_ms']
    );
}
```

### 6.6 线程池模式（CLI only）

```php
<?php
// 仅 CLI 模式可用
if (!XHCurl::isCli()) {
    die("XHThreadPool 仅在 CLI 模式下可用\n");
}

$pool = new XHThreadPool(8); // 8 个工作线程

// 添加 100 个请求
for ($i = 0; $i < 100; $i++) {
    $request = (new XHRequest("https://httpbin.org/delay/" . ($i % 3)))
        ->get()
        ->setId("pool_req_{$i}")
        ->timeout(15)
        ->setPriority($i < 10 ? 10 : 0); // 前 10 个高优先级

    $pool->add($request);
}

// 执行
$results = $pool->execute();

// 统计
$success = count(array_filter($results, fn($r) => $r['success']));
echo "成功: {$success}/100\n";
```

### 6.7 全局配置 + 链式调用

```php
<?php
// 1. 设置全局默认配置
XHCurl::setConfig([
    'connect_timeout'   => 10,
    'request_timeout'   => 30,
    'max_response_size' => 5 * 1024 * 1024, // 5MB
    'follow_redirects'  => true,
    'max_redirects'     => 5,
    'verify_ssl'        => true,
    'user_agent'        => 'MyApp/2.0',
    'proxy'             => 'http://proxy.example.com:8080',
]);

// 2. 创建请求（继承全局配置，可覆盖）
$request = (new XHRequest("https://api.example.com/data"))
    ->post()
    ->json(['query' => 'test'])
    ->timeout(60)          // 覆盖全局的 30s 超时
    ->proxy("");           // 清除代理（直连）

// 3. 执行
$multi = new XHMulti();
$result = $multi->add($request)->execute();
```

### 6.8 Rust 层面链式调用

```rust
use xhcurl::{XhRequest, XhMulti, XhCurlManager};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 创建全局客户端
    let client = XhCurlManager::global().create_client()?;

    // 创建批量执行器
    let mut multi = XhMulti::new(client);

    // 链式添加请求
    multi
        .add(
            XhRequest::new("https://httpbin.org/get")
                .get()
                .header("X-Request-Id", "001")
                .id("req_001")
        )?
        .add(
            XhRequest::new("https://httpbin.org/post")
                .post()
                .body_json_str(r#"{"key": "value"}"#)?
                .timeout(30)
                .id("req_002")
        )?
        .add(
            XhRequest::new("https://httpbin.org/put")
                .put()
                .body_bytes(b"raw data".to_vec())
                .verify_ssl(false)
                .id("req_003")
        )?;

    // 配置并发数和响应体限制
    multi = multi
        .max_concurrency(50)
        .max_response_size(20 * 1024 * 1024); // 20MB

    // 执行
    let results = multi.execute().await?;

    // 处理结果
    for result in results {
        if result.is_success() {
            let resp = result.response.unwrap();
            println!(
                "[{}] {} - {} ({}ms)",
                result.id,
                resp.status(),
                resp.url(),
                result.elapsed.as_millis()
            );
        } else {
            println!("[{}] 失败: {}", result.id, result.error.unwrap());
        }
    }

    Ok(())
}
```

***

## 7. 安全机制

### 7.1 内存安全

| 机制         | 说明                                       |
| ---------- | ---------------------------------------- |
| 所有权模型      | Rust 编译器保证无 double-free / use-after-free |
| 有界 Channel | 所有 channel 使用有界缓冲区（默认 1024），防止内存溢出       |
| 请求数量上限     | 单次批量最多 10000 个请求                         |
| 响应体大小限制    | 默认 10MB，可配置，流式读取时逐块检查                    |
| 整数溢出检查     | 使用 `checked_add` 防止大小累加溢出                |

### 7.2 线程安全

| 机制                 | 说明                            |
| ------------------ | ----------------------------- |
| Send + Sync bounds | 编译期验证所有跨线程数据是否安全              |
| Channel 通信         | 工作线程与主线程通过 channel 通信，无共享可变状态 |
| Arc 共享             | 共享数据使用 Arc（原子引用计数），线程安全       |
| RwLock 配置          | 全局配置使用 RwLock，支持并发读取          |
| Semaphore 并发控制     | 使用 Semaphore 限制最大并发数          |

### 7.3 运行时安全

| 机制     | 说明                                          |
| ------ | ------------------------------------------- |
| FPM 安全 | XHMulti 使用 `new_current_thread()` 单线程运行时    |
| CLI 模式 | XHThreadPool 使用 `new_multi_thread()` 多线程运行时 |
| 模式检测   | XHThreadPool 在 FPM 模式下自动拒绝执行                |

***

## 8. 性能优化

### 8.1 连接池复用

```php
// 全局配置自动复用连接池
XHCurl::setConfig([
    'max_connections' => 100,  // 最大连接数
    'tcp_keepalive'   => true, // 启用 TCP Keep-Alive
]);
```

### 8.2 并发数调优

```php
// 根据目标服务器承受能力设置
$multi->maxConcurrency(100);  // 通用 API
$multi->maxConcurrency(500);  // 高性能内部服务
$multi->maxConcurrency(10);   // 外部限速 API
```

### 8.3 响应体大小控制

```php
// 限制响应体大小，防止恶意服务器返回超大响应
XHCurl::setConfig([
    'max_response_size' => 1024 * 1024, // 1MB
]);
```

### 8.4 编译优化

```toml
# Cargo.toml release 配置
[profile.release]
lto = true           # 链接时优化
codegen-units = 1    # 最大优化
opt-level = 3        # 最高优化级别
panic = "abort"      # 减小二进制体积
strip = true         # 去除调试符号
```

***

## 9. 常见问题

### Q: XHMulti 和 XHThreadPool 如何选择？

| 场景         | 推荐           | 原因                  |
| ---------- | ------------ | ------------------- |
| FPM Web 服务 | XHMulti      | 单线程运行时，与 PHP FPM 兼容 |
| CLI 脚本     | XHThreadPool | 多线程并行，性能更高          |
| 批量爬虫       | XHThreadPool | 真正并行，充分利用多核         |
| API 聚合     | XHMulti      | 异步并发足够，更安全          |

### Q: 为什么 XHThreadPool 在 FPM 下不可用？

PHP FPM 的内存管理器不是线程安全的。多线程同时操作 PHP 内存会导致未定义行为。XHMulti 使用单线程事件循环 + 异步 I/O，避免了此问题。

### Q: 如何处理大批量请求（超过 10000）？

```php
// 分批执行
$allUrls = [...]; // 50000 个 URL
$batchSize = 5000;

foreach (array_chunk($allUrls, $batchSize) as $batch) {
    $multi = new XHMulti();
    foreach ($batch as $url) {
        $multi->add(new XHRequest($url));
    }
    $results = $multi->execute();
    // 处理结果...
}
```

### Q: 如何调试请求？

```php
// 查看全局配置
$config = XHCurl::getConfig();
print_r($config);

// 查看请求信息
$request = new XHRequest("https://example.com");
echo $request->getUrl();     // URL
echo $request->getMethod();  // HTTP 方法
echo $request->getId();      // 请求 ID
```

### Q: 编译时找不到 php-config？

```bash
# 确保安装了 PHP 开发包
# Ubuntu/Debian
sudo apt install php8.1-dev

# CentOS/RHEL
sudo yum install php8.1-devel

# macOS
brew install php@8.1

# 验证
which php-config
php-config --include-dir
```

