# XHCurl - 高性能 PHP HTTP 客户端扩展（Rust 实现）

[![Build Status](https://github.com/hgc357341051/xhcurl/actions/workflows/build-rust.yml/badge.svg)](https://github.com/hgc357341051/xhcurl/actions/workflows/build-rust.yml)
[![PHP Version](https://img.shields.io/badge/PHP-8.1%20--%208.5-blue.svg)](https://php.net)
[![Rust](https://img.shields.io/badge/Rust-stable%20%7C%20nightly%20(Win)-orange.svg)](https://www.rust-lang.org/)
[![Platform](https://img.shields.io/badge/platform-Linux%20%7C%20macOS%20%7C%20Windows-green.svg)](#平台支持)
[![License](https://img.shields.io/badge/license-MIT-yellow.svg)](LICENSE)

XHCurl 是一个基于 **Rust** 开发的高性能 PHP HTTP 客户端扩展，使用 [ext-php-rs](https://github.com/davidcole1340/ext-php-rs) 直接生成 PHP 扩展，无需 C 桥接层。提供链式调用 API、批量异步请求、线程池并发，以及基于 **PHP Fiber 协程**的真正异步 HTTP 请求能力。

## 目录

- [核心特性](#核心特性)
- [平台支持](#平台支持)
- [安装](#安装)
- [快速开始](#快速开始)
- [协程模式（PHP Fiber）](#协程模式php-fiber)
- [API 文档](#api-文档)
  - [XHCurl - 全局管理器](#xhcurl---全局管理器)
  - [XHRequest - 请求构建器](#xhrequest---请求构建器)
  - [结果数组字段](#结果数组字段)
  - [XHMulti - 批量异步执行器](#xhmulti---批量异步执行器)
  - [XHThreadPool - 线程池](#xhthreadpool---线程池)
  - [xhrun - 安全 Shell 命令执行](#xhrun---安全-shell-命令执行)
- [curl 兼容性对照](#curl-兼容性对照)
- [FPM 与 CLI 模式](#fpm-与-cli-模式)
- [故障排查](#故障排查)
- [开发与贡献](#开发与贡献)

---

## 核心特性

| 特性 | 说明 |
|------|------|
| **Rust 内存安全** | 基于 Rust + reqwest 实现，编译期保证无数据竞争、无空指针解引用 |
| **三种执行模式** | 同步单次（`execute`）、批量异步（`XHMulti`）、协程异步（`await`/`gather`） |
| **PHP Fiber 协程** | `XHCurl::run()` + `await()` 实现协程式异步 HTTP，类似 ReactPHP/AMPHP |
| **并行 gather** | `XHCurl::gather()` 一次性并发 N 个请求，100 请求实测 ~65 倍加速 |
| **链式调用** | 所有 setter 方法返回 `$this`，支持流畅的链式 API |
| **curl 兼容** | 对标 PHP curl 的 `CURLOPT_*`，支持 cookie/auth/TLS/multipart 等 |
| **二进制安全** | `body()`/`multipart()` 字段值、响应体均为二进制安全，可传任意字节 |
| **全局连接复用** | reqwest Client 全局单例，TCP keep-alive + TLS 会话缓存 |
| **请求级配置覆盖** | `verifySsl`/`proxy`/`connectTimeout`/重定向可按请求覆盖全局配置 |
| **自适应运行时** | CLI 模式多线程运行时（M:N 并行），FPM 模式单线程运行时（协作式并发） |
| **用户自定义数据** | `setUserData()`（别名 `userData()`）携带任意结构化数据，随结果原样回传 |
| **响应体大小限制** | 流式读取 + `max_response_size` 防止内存溢出 |
| **流式回调** | 请求级（`each`/`executeEach`）+ 响应体分块级（`onChunk`/`onHeaders`）双流式 |
| **安全 Shell 执行** | `xhrun()` 函数替代 `shell_exec`/`exec`/`system`，默认不经 shell 防注入 |

---

## 平台支持

| 平台 | PHP 版本 | 线程安全 | 状态 |
|------|----------|----------|------|
| Linux (Ubuntu 24.04) | 8.1, 8.2, 8.3, 8.4, 8.5 | NTS | ✅ 支持 |
| macOS 14 | 8.1, 8.2, 8.3, 8.4, 8.5 | NTS | ✅ 支持 |
| Windows (x64) | 8.1, 8.2, 8.3, 8.4, 8.5 | NTS / TS | ✅ 支持 |

> **PHP 版本要求**：**8.1+**。协程模式（`await`/`gather`/`run`）依赖 PHP 8.1 引入的 [Fiber](https://www.php.net/manual/zh/language.fibers.php) 类。

---

## 安装

### 方式一：从源码编译

**前置依赖：**

- Rust 工具链（Linux/macOS 用 stable；Windows 用 nightly —— ext-php-rs 的 `abi_vectorcall` 调用约定是 nightly-only）
- PHP 8.1+ 开发头文件（`php-config` / `php-devel`）
- libclang（ext-php-rs bindgen 需要）
- OpenSSL 开发库（Linux）

**Linux / macOS：**

```bash
# 1. 安装系统依赖
# Ubuntu/Debian
sudo apt-get install -y build-essential pkg-config libssl-dev libclang-dev

# macOS（使用 Homebrew）
brew install llvm pkg-config openssl

# 2. 安装 Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env

# 3. 克隆仓库并编译
git clone https://github.com/hgc357341051/xhcurl.git
cd xhcurl/rust
cargo build --release --features php

# 4. 安装扩展
sudo cp target/release/libxhcurl.so $(php-config --extension-dir)/xhcurl.so
echo "extension=xhcurl.so" | sudo tee /etc/php/8.1/mods-available/xhcurl.ini
sudo phpenmod xhcurl

# 5. 验证
php -m | grep xhcurl
php -r "echo XHCurl::version();"
```

**Windows：**

Windows 编译较复杂，建议直接从 [GitHub Releases](https://github.com/hgc357341051/xhcurl/releases) 下载预编译的 DLL。

### 方式二：从 GitHub Releases 下载预编译包

访问 [Releases 页面](https://github.com/hgc357341051/xhcurl/releases)，根据平台和 PHP 版本选择对应的二进制包：

- `xhcurl-rust-linux-php8.x.so` - Linux
- `xhcurl-rust-macos-php8.x.dylib` - macOS
- `xhcurl-rust-windows-php8.x-nts.dll` - Windows NTS
- `xhcurl-rust-windows-php8.x-ts.dll` - Windows TS（线程安全）

### 验证安装

```bash
php -m | grep xhcurl
# 输出：xhcurl

php -r "echo XHCurl::version();"
# 输出扩展版本号

php -r "var_dump(class_exists('XHCurl'));"
# 输出：bool(true)
```

---

## 快速开始

### 示例 1：单次同步请求

```php
<?php
// 创建请求（链式调用）
$request = XHCurl::createRequest('https://httpbin.org/get')
    ->get()
    ->header('X-Custom-Header', 'hello')
    ->timeout(30);

// 同步执行（execute() 是 XHRequest 的实例方法）
$result = $request->execute();

if ($result['success']) {
    echo "状态码: " . $result['status'] . "\n";
    echo "响应体: " . $result['body'] . "\n";
}
```

### 示例 2：POST JSON 请求

```php
<?php
$result = XHCurl::createRequest('https://httpbin.org/post')
    ->post()
    ->json(['name' => 'XHCurl', 'version' => '1.0'])
    ->header('Authorization', 'Bearer my-token')
    ->timeout(30)
    ->execute();  // 链式末尾直接 execute()

echo $result['body'];
```

### 示例 3：批量异步请求

```php
<?php
$multi = new XHMulti();

$urls = [
    'https://httpbin.org/get?id=1',
    'https://httpbin.org/get?id=2',
    'https://httpbin.org/get?id=3',
];

foreach ($urls as $url) {
    $multi->add(
        XHCurl::createRequest($url)->get()->timeout(10)
    );
}

// 设置最大并发数
$multi->maxConcurrency(10);

// 执行并获取所有结果
$results = $multi->execute();

foreach ($results as $i => $result) {
    echo "请求 {$i}: status=" . $result['status'] . "\n";
}
```

### 示例 4：线程池并发（CLI 模式）

```php
<?php
// 仅 CLI 模式可用
$pool = new XHThreadPool(8); // 8 个工作线程

for ($i = 0; $i < 100; $i++) {
    $pool->add(
        XHCurl::createRequest("https://httpbin.org/get?id={$i}")
            ->get()
            ->timeout(10)
    );
}

$results = $pool->execute();
echo "完成 " . count($results) . " 个请求\n";
```

---

## 协程模式（PHP Fiber）

> **CLI-only 限制**：XHCurl::run() / await() / gather() / each() 仅在 CLI 模式下可用。FPM 模式下调用 run() 会返回错误"XHCurl::run 仅在 CLI 模式下可用（FPM 请用 XHMulti）"。FPM 请使用 XHMulti 或 XHRequest::execute()。

XHCurl 基于 PHP 8.1 的 [Fiber](https://www.php.net/manual/zh/language.fibers.php) 实现了真正的协程式异步 HTTP 请求。HTTP 请求在 Rust 的 tokio 异步运行时上执行，PHP 侧通过 Fiber 挂起/恢复实现非阻塞等待。

### `await()` - 协程式等待单个请求

```php
<?php
// XHCurl::run() 启动协程事件泵
// 回调内使用 XHCurl::await() 挂起当前 Fiber 等待 HTTP 请求完成
$result = XHCurl::run(function() {
    $response = XHCurl::await(
        XHCurl::createRequest('https://httpbin.org/get')
            ->get()
            ->timeout(10)
    );

    // await 返回结果数组
    if ($response['success']) {
        echo "状态码: " . $response['status'] . "\n";
        echo "响应体: " . $response['body'] . "\n";
    }

    return $response;
});
```

### `gather()` - 并发批量请求（真正并行）

`gather()` 一次性将所有请求提交到 tokio 工作线程并行执行，按**完成顺序**返回结果（非提交顺序）。这是性能最高的模式。

```php
<?php
// 构建 100 个请求
$requests = [];
for ($i = 0; $i < 100; $i++) {
    $requests[] = XHCurl::createRequest("https://httpbin.org/get?id={$i}")
        ->get()
        ->timeout(10)
        ->userData(['task_index' => $i, 'tag' => "batch-{$i}"]);
}

// gather 并发执行所有请求
$results = XHCurl::run(function() use ($requests) {
    return XHCurl::gather($requests);
});

// 结果按完成顺序排列（非提交顺序）
foreach ($results as $idx => $result) {
    $userData = json_decode($result['user_data'], true);
    echo "完成 #{$idx}: 提交索引={$userData['task_index']}, status={$result['status']}\n";
}
```

> **性能数据**：100 个请求并发，总耗时 ~150ms（串行需 ~10s，加速约 65 倍）。

### `each()` - 流式回调并发

`each()` 与 `gather()` 行为对比：

| 模式 | 返回值 | 处理方式 |
|------|--------|----------|
| `gather(array $requests)` | `array` —— 全部结果数组 | 累积所有结果后一次性返回 |
| `each(array $requests, callable $callback)` | `int` —— 处理结果总数 | 每个请求完成后立即调用回调（流式处理） |

适用场景：结果数量巨大且无需在内存中保留全部结果时，用 `each()` 边收边处理，避免峰值内存。

> **注意**：协程 `each()` 仅支持请求级流式回调（`$callback`）。
> 如需响应体分块级流式（`onChunk`/`onHeaders`），请使用 `XHMulti::executeEach()` 或 `XHThreadPool::executeEach()`。

```php
<?php
// 构建 100 个请求
$requests = [];
for ($i = 0; $i < 100; $i++) {
    $requests[] = XHCurl::createRequest("https://httpbin.org/get?id={$i}")
        ->get()
        ->timeout(10)
        ->userData(['task_index' => $i]);
}

// gather：累积所有结果后返回（内存中持有 100 个响应体）
$results = XHCurl::run(function() use ($requests) {
    return XHCurl::gather($requests);  // 返回 array
});
// 处理 $results...
foreach ($results as $r) { /* ... */ }

// each：流式回调，每完成一个请求就调用一次（不累积结果数组）
$count = XHCurl::run(function() use ($requests) {
    return XHCurl::each($requests, function(array $result): void {
        // 回调签名：function(array $result): void
        // $result 字段与 gather/await 返回值一致（见「结果数组字段」）
        $userData = json_decode($result['user_data'] ?? '', true);
        if ($result['success']) {
            echo "完成 #{$userData['task_index']}: status={$result['status']}\n";
        } else {
            echo "失败 #{$userData['task_index']}: error={$result['error']}\n";
        }
    });
    // $count = 已回调处理的结果总数（int）
});

echo "已处理 {$count} 个结果\n";
```

> **回调签名**：`function(array $result): void`。回调内不要做长时间阻塞操作（会
> 阻塞 Fiber 调度）。需要写入外部状态时用 `use (&$buffer)` 引用捕获。

### 协程模式串行 vs 并行对比

```php
<?php
// ❌ 串行：一个接一个等待（总耗时 = 所有请求耗时之和）
XHCurl::run(function() {
    $r1 = XHCurl::await(XHCurl::createRequest('https://api.example.com/a')->get());
    $r2 = XHCurl::await(XHCurl::createRequest('https://api.example.com/b')->get());
    $r3 = XHCurl::await(XHCurl::createRequest('https://api.example.com/c')->get());
    // 总耗时 ≈ r1 + r2 + r3
});

// ✅ 并行：同时发起，按完成顺序返回（总耗时 ≈ 最慢的请求）
XHCurl::run(function() {
    $results = XHCurl::gather([
        XHCurl::createRequest('https://api.example.com/a')->get(),
        XHCurl::createRequest('https://api.example.com/b')->get(),
        XHCurl::createRequest('https://api.example.com/c')->get(),
    ]);
    // 总耗时 ≈ max(r1, r2, r3)
    return $results;
});
```

### 用户自定义数据传递

使用 `userData()`（别名 `setUserData()`）携带任意结构化数据（数组/对象），随请求原样回传到结果中：

```php
<?php
$request = XHCurl::createRequest('https://api.example.com/data')
    ->get()
    ->userData([
        'task_id'   => 42,
        'callback'  => 'process_user',
        'context'   => ['user_id' => 1001, 'role' => 'admin'],
    ]);

$result = XHCurl::run(function() use ($request) {
    return XHCurl::await($request);
});

// 从结果中取回自定义数据
$custom = json_decode($result['user_data'], true);
echo "任务ID: " . $custom['task_id'] . "\n";
echo "回调: " . $custom['callback'] . "\n";
```

---

## API 文档

### XHCurl - 全局管理器

所有方法均为静态方法。

| 方法 | 签名 | 说明 |
|------|------|------|
| `version()` | `(): string` | 获取扩展版本号 |
| `setConfig()` | `(array $config): void` | 设置全局配置 |
| `getConfig()` | `(): array` | 获取全局配置 |
| `isCli()` | `(): bool` | 检测是否 CLI 模式 |
| `createRequest()` | `(string $url): XHRequest` | 创建请求构建器 |
| `run()` | `(callable $main): mixed` | 启动协程事件泵，执行主回调 |
| `await()` | `(XHRequest $req): array` | 协程式等待单个请求（须在 `run()` 内） |
| `gather()` | `(array $requests): array` | 并发批量请求，按完成顺序返回（须在 `run()` 内） |
| `each()` | `(array $requests, callable $callback): int` | 流式回调并发执行，回调签名 `function(array $result): void`，返回处理结果总数（须在 `run()` 内） |

> 单请求同步执行请用 `XHRequest::execute()` 实例方法（见下文）。

**全局配置项：**

```php
XHCurl::setConfig([
    'connect_timeout'        => 10,      // 连接超时（秒）
    'request_timeout'        => 30,      // 请求超时（秒）
    'max_response_size'      => 10485760,// 最大响应体（字节，默认 10MB）
    'follow_redirects'       => true,    // 跟随重定向
    'max_redirects'          => 10,      // 最大重定向次数
    'verify_ssl'             => true,    // 验证 SSL 证书
    'http2_enabled'            => true,   // 是否启用 HTTP/2 协商（false 时强制 HTTP/1.1）
    'user_agent'             => 'XHCurl',// User-Agent
    'proxy'                  => null,    // 代理地址
    'tcp_keepalive'          => true,    // TCP keep-alive（连接复用）
    'tcp_keepalive_interval' => 60,      // keep-alive 探测间隔（秒）
    'max_connections'        => 100,     // 连接池上限
    'fiber_max_concurrency'  => 64,      // gather/each 协程并发上限（0=不限）
]);
```

> **类型校验**：`setConfig()` 会对每个配置项做类型检查。数值项传入字符串、
> 布尔项传入数值等不匹配情况会被收集，最终返回包含所有不匹配项名的错误信息，
> 而非静默忽略。负数会被跳过（保留原值），不会 panic。
>
> **配置变更生效**：`setConfig()` 会清空请求级 Client 缓存，确保后续构建的 Client
> 反映最新全局配置（UA/keepalive/连接池/TLS 等）。`fiber_max_concurrency` 仅影响
> 下次 `gather()`/`each()` 调用的 Semaphore 容量。
> **全局变更立即生效**：修改 `proxy`/`verify_ssl`/`user_agent`/`http2_enabled`/
> `tcp_keepalive`/`max_connections` 等影响 Client 构建的配置后，对**无请求级覆盖**
> 的请求立即生效——全局 Client 基于配置指纹比对自动重建，无需重启进程。

### XHRequest - 请求构建器

所有 setter 方法返回 `$this`，支持链式调用。`execute()` 为实例方法，同步执行并返回结果数组。

#### 同步执行

| 方法 | 说明 |
|------|------|
| `execute()` | 同步执行当前请求，返回结果数组（字段见下文「结果数组字段」） |

#### HTTP 方法

| 方法 | 说明 |
|------|------|
| `get()` / `post()` / `put()` / `delete()` / `patch()` / `head()` / `options()` | 设置标准 HTTP 方法 |
| `method(string $method)` | 通过字符串设置方法；无效方法名（如 `'PUTT'` 拼写错误）抛异常。标准方法：GET/POST/PUT/DELETE/PATCH/HEAD/OPTIONS；非标准方法请用 `customMethod()` |
| `customMethod(string $method)` | 自定义方法（CURLOPT_CUSTOMREQUEST，如 PROPFIND/TRACE） |

#### 请求体

| 方法 | 说明 |
|------|------|
| `json(array $data)` | JSON 请求体（自动设置 Content-Type: application/json） |
| `form(array $data)` | 表单请求体（application/x-www-form-urlencoded） |
| `body(string $data)` | 原始请求体（**二进制安全**，可传任意字节） |
| `multipart(array $fields)` | 文件上传（multipart/form-data，**字段值二进制安全**） |

> **二进制安全说明**：`body()` 与 `multipart()` 的字段值通过二进制安全接口读取
> PHP 字符串（PHP 字符串本质是字节序列），不会因含非 UTF-8 字节而丢失或损坏，
> 适合上传图片、压缩数据等任意二进制内容。

**multipart 字段格式：**

```php
->multipart([
    ['name' => 'field1', 'value' => 'text value'],
    ['name' => 'file1', 'value' => 'file content', 'filename' => 'test.txt', 'content_type' => 'text/plain'],
])
```

#### 请求头与认证

| 方法 | 对应 curl | 说明 |
|------|-----------|------|
| `header(string $name, string $value)` | - | 设置请求头 |
| `headers(array $headers): $this` | - | 批量设置请求头，先校验全部再存储；任一 header 名/值非法时整体抛异常（fail-fast） |
| `cookies(string\|array $cookies): $this` | CURLOPT_COOKIE | Cookie，接受字符串或数组；数组形式 `['name' => 'value', ...]` 自动拼接为 `"name=value; name2=value2"`，value 会做 URL 编码（与 PHP `setcookie()` 默认行为一致，防止含 `;`/`=` 的 value 破坏 Cookie 格式或注入伪造 cookie；key 不编码），字符串形式向后兼容直接设置原始 cookie 字符串 |
| `basicAuth(string $credentials)` | CURLOPT_USERPWD | HTTP 基本认证（`user:pass`） |
| `bearerToken(string $token)` | CURLOPT_XOAUTH2_BEARER | Bearer Token 认证 |
| `encoding(string $encoding)` | CURLOPT_ENCODING | Accept-Encoding（如 `gzip, deflate`） |

#### TLS/SSL

| 方法 | 对应 curl | 说明 |
|------|-----------|------|
| `verifySsl(bool $verify)` | CURLOPT_SSL_VERIFYPEER | 验证 SSL 证书 |

> 客户端证书（CURLOPT_SSLCERT/SSLKEY）、CA 证书（CURLOPT_CAINFO）、
> Cookie 文件（CURLOPT_COOKIEFILE/COOKIEJAR）暂未实现。

#### 其他选项

| 方法 | 说明 |
|------|------|
| `timeout(int $seconds)` | 请求超时（秒） |
| `timeoutMs(int $ms): $this` | 请求超时（毫秒精度）。`timeout()` 保持秒级不变（向后兼容）；同时设置时优先级：`timeoutMs` > `timeout`（以毫秒级为准） |
| `connectTimeout(int $seconds)` | 连接超时（秒） |
| `connectTimeoutMs(int $ms): $this` | 连接超时（毫秒精度），与 `connectTimeout(int $seconds)` 对称。0 或负值忽略 |
| `userAgent(string $ua)` | User-Agent |
| `proxy(?string $proxy): $this` | 代理地址（支持 http/https/socks5）；传 `null` 清除请求级代理覆盖（与 `setConfig(['proxy' => null])` 对称） |
| `followRedirects(bool $follow)` | 跟随重定向 |
| `maxRedirects(int $max)` | 最大重定向次数 |
| `range(string $range)` | Range 请求（CURLOPT_RANGE，如 `0-1023`） |
| `setId(string $id)` / `id(string $id)` | 设置请求 ID（用于批量请求时标识结果） |
| `setUserData(array $data)` / `userData(array $data)` | 用户自定义数据（随结果回传，JSON 字符串） |
| `getUrl()` / `getMethod()` | 获取 URL / 方法 |
| `getTimeout()` | `(): ?int` 获取请求级超时（秒），未设置返回 null |
| `getConnectTimeout()` | `(): ?int` 获取连接超时（秒），未设置返回 null |
| `getTimeoutMs()` | `(): ?int` 获取请求级超时（毫秒），未设置返回 null |
| `getConnectTimeoutMs()` | `(): ?int` 获取连接超时（毫秒），未设置返回 null |
| `getHeaders()` | `(): array` 获取已设置的请求头（键名小写） |
| `getCookies()` | `(): ?string` 获取 cookie 字符串，未设置返回 null |
| `getProxy()` | `(): ?string` 获取请求级代理，未设置返回 null |
| `getVerifySsl()` | `(): ?bool` 获取 SSL 验证设置，未设置返回 null |
| `getUserAgent()` | `(): ?string` 获取 User-Agent，未设置返回 null |
| `getId()` | `(): ?string` 获取请求 ID，未设置返回 null |
| `getUserData()` | `(): ?string` 获取用户自定义数据（JSON 字符串），未设置返回 null |

> **timeout 类 0 值语义**：`timeout()`/`timeoutMs()`/`connectTimeout()`/`connectTimeoutMs()`
> 传 `0` 或负值表示跳过设置（使用全局默认值），而非"立即超时"。

> **请求级配置覆盖**：`verifySsl()`/`proxy()`/`connectTimeout()`/`followRedirects()`/
> `maxRedirects()` 都是请求级覆盖，会基于全局配置构建新 Client 应用这些参数。
> 注意这会牺牲连接池复用（新 Client 有独立连接池），仅在显式设置时才触发。
> 无效代理地址会明确报错，而非静默忽略。

### 结果数组字段

`execute()`（XHRequest）/ `XHMulti::execute()` / `XHThreadPool::execute()` /
`XHCurl::await()` / `XHCurl::gather()` **全部以关联数组形式返回结果**，字段完全一致：

> **错误处理分两类**：XHCurl 区分**配置类错误**与**请求级失败**，处理方式不同。
>
> **配置类错误（抛 PHP 异常）**——属于"调用方用法错误"，应通过 `try`/`catch` 捕获：
> - 无效代理配置（`global_client()` 初始化失败）
> - 运行时初始化失败
> - `json()` 序列化失败（含无法序列化的内容如资源）
> - `setUserData()`/`userData()` 序列化失败
> - `method()` 无效 HTTP 方法名
> - `cookies()` 参数类型错误（非字符串非数组）；数组形式下整型/浮点/布尔值自动转字符串（`true→"1"`、`123→"123"`），数组/对象/资源抛异常
> - `header()`/`headers()`：非法 header 名/值（含控制字符、NUL）调用时立即抛异常（fail-fast）；`headers()` 传入列表数组（整数键）抛异常，提示用关联数组
> - `body()`：非字符串输入（`null`/`int`/`array` 等）抛异常
> - `multipart()`：字段缺少 `name` 或 `name` 为空抛异常；非数组元素抛异常
> - `form()`：含数组/对象/资源值时抛异常（提示用 `multipart()` 或 `json()`）
> - `basicAuth()`：空字符串或无冒号分隔符时抛异常（提示格式 `user:pass`）
>
> **请求级失败（返回 `success=false` 数组）**——属于"网络/服务端层面失败"，不抛异常：
> - HTTP 请求失败（超时、DNS、SSL、连接拒绝）
> - 返回非 2xx 状态码（如果配置了）
> - 响应体超限
> - `cookies()`/`encoding()`/`range()`/`userAgent()`：含非 ASCII 字节时在 `execute()` 阶段返回 `success=false` 结果（`error` 字段含字段名和原始值）
>
> 所有 API（`execute()`/`XHMulti::execute()`/`XHThreadPool::execute()`/
> `await()`/`gather()`/`each()`）在请求级失败时**统一返回 `success=false` 的结果数组**，
> 不抛 PHP 异常。请用 `try`/`catch` 捕获配置类异常，用 `if ($result['success'])` 判断请求级失败，从 `$result['error']` 读取原因。

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | string | 请求 ID（未设置 `setId()`/`id()` 时默认为请求 URL，所有执行路径统一） |
| `success` | bool | 是否成功 |
| `status` | int | HTTP 状态码 |
| `body` | string | 响应体（**二进制安全**，保留原始字节） |
| `body_size` | int | 响应体大小（字节） |
| `headers` | array | 所有响应头（键名小写；含非 ASCII 字节时用替换字符保留） |
| `url` | string | 最终 URL（重定向后） |
| `elapsed_ms` | int | 请求耗时（毫秒） |
| `remote_addr` | string | 远程服务器地址（可选） |
| `version` | string | HTTP 协议版本（可选，如 `HTTP/1.1`） |
| `error` | string | 错误信息（失败时，可选） |
| `error_type` | string | 错误类型枚举（失败时，可选；详见下文「失败路径字段说明」） |
| `user_data` | string | 用户自定义数据（JSON 字符串，设置了 `setUserData()`/`userData()` 时） |

> 所有 API 均直接返回上述关联数组，不返回对象。批量上限 `MAX_REQUESTS_PER_BATCH = 10000`，
> 超出会在执行前拒绝（避免先克隆再拒绝导致 OOM）。

#### 失败路径字段说明（`success === false`）

请求失败时（连接超时、DNS 失败、TLS 错误、被服务端拒绝等），`success` 为 `false`，其余字段语义与成功路径略有差异：

| 字段 | 失败路径取值 | 说明 |
|------|-------------|------|
| `id` | 始终存在 | 与成功路径一致（未设 `setId()`/`id()` 时为请求 URL） |
| `success` | `false` | 固定 |
| `status` | `0` | **哨兵值**，不是真实 HTTP 状态码（无响应到达时无状态码可言） |
| `body` | `""`（空字符串） | 无响应体 |
| `body_size` | `0` | 无响应体 |
| `headers` | `[]`（空数组） | 无响应头 |
| `url` | 可能为空或缺失 | 无最终 URL 时为空字符串或不出现 |
| `remote_addr` | 可能为空或缺失 | 未建立连接时无远程地址 |
| `version` | 可能为空或缺失 | 无 HTTP 协议版本 |
| `error` | 错误信息字符串 | **失败路径的核心字段**，包含错误原因 |
| `error_type` | 错误类型枚举字符串 | 可能值：`dns`/`timeout`/`ssl`/`connection`/`unknown`；用于程序化区分错误类型，而非解析 `error` 字符串。成功路径不含此字段（或为空字符串） |
| `elapsed_ms` | 始终存在 | 已耗时（毫秒），即使失败也会返回 |
| `user_data` | 设置了 `setUserData()`/`userData()` 时存在 | 与成功路径一致 |

> **判断成败只看 `success`**：不要用 `status === 0` 判断失败——某些边缘场景下成功路径的
> 状态码也可能为 0（如 HTTP 0xx），且失败路径 `status` 恒为 0 是约定哨兵值，并非 HTTP 规范。
> 失败时优先读取 `error` 字段获取原因；若需程序化区分错误类型（如超时重试、SSL 失败告警），
> 读取 `error_type` 枚举值，而非解析 `error` 字符串。

#### 错误处理示例

```php
<?php
// 完整的错误处理模式：try/catch 捕获配置类异常 + if success 判断请求级失败

try {
    $result = XHCurl::createRequest('https://api.example.com/users')
        ->get()
        ->timeout(10)
        ->execute();
    
    if (!$result['success']) {
        // 请求级失败：网络/服务端层面错误，不抛异常
        $errorType = $result['error_type'] ?? 'unknown';
        switch ($errorType) {
            case 'timeout':
                echo "请求超时，建议重试\n";
                break;
            case 'dns':
                echo "DNS 解析失败，检查网络或 DNS 配置\n";
                break;
            case 'ssl':
                echo "SSL 证书验证失败\n";
                break;
            case 'connection':
                echo "连接被拒绝或重置\n";
                break;
            default:
                echo "未知错误: " . ($result['error'] ?? '无错误信息') . "\n";
        }
        return;
    }
    
    // 请求成功
    echo "状态码: {$result['status']}\n";
    echo "响应: {$result['body']}\n";
    
} catch (\Throwable $e) {
    // 配置类异常：调用方用法错误（无效代理、序列化失败、非法参数等）
    echo "配置错误: " . $e->getMessage() . "\n";
}
```

### XHMulti - 批量异步执行器

基于 tokio 的 M:N 异步并发（CLI 多线程并行，FPM 协作式并发）。

| 方法 | 说明 |
|------|------|
| `__construct()` | 创建批量执行器 |
| `add(XHRequest $req): $this` | 添加请求（带数量上限检查） |
| `maxConcurrency(int $max): $this` | 最大并发数（0 = 无限制） |
| `maxResponseSize(int $size): $this` | 单响应最大字节数（0 = 用全局默认 10MB） |
| `timeout(int $seconds): $this` | 设置整体执行超时（秒，0 = 无超时） |
| `execute(): array` | 执行所有请求，返回结果数组（按完成顺序） |
| `executeEach(callable $onResult, ?callable $onChunk = null, ?callable $onHeaders = null): int` | 流式回调执行，详见下文 |
| `count(): int` | 返回待执行请求数 |
| `isEmpty(): bool` | 是否有待执行请求 |
| `getMaxConcurrency(): int` | 获取配置的最大并发数（未配置返回 0） |
| `getMaxResponseSize(): int` | 获取配置的最大响应体大小（未配置返回 0） |
| `getTimeout(): int` | 获取配置的批量级超时秒数（未配置返回 0） |

```php
$multi = new XHMulti();
$multi->add($request1);
$multi->add($request2);
$multi->maxConcurrency(10);  // 最大并发数
$results = $multi->execute(); // 返回结果数组
```

> 结果按**完成顺序**排列（非提交顺序）。用结果的 `id` 字段关联业务上下文，
> 而非依赖数组索引。
>
> **空请求抛异常**：请求列表为空时 `execute()` 抛异常（而非返回空数组），请先调用 `add()`。
>
> **execute() 消费请求列表**：执行后已添加的请求会被清空，重用同一对象需重新 `add()`。

#### `executeEach` 响应体分块级流式回调

`executeEach` 支持两个可选回调参数，用于在请求过程中实时处理响应体数据：

```php
<?php
$multi = new XHMulti();
$multi->add(XHCurl::createRequest('http://example.com/large-file')->get());

// 仅请求级流式（向后兼容，行为不变）
$multi->executeEach(function(array $result): void {
    echo "请求完成: status={$result['status']}\n";
});

// 请求级 + 响应体分块级流式
$multi->add(XHCurl::createRequest('http://example.com/stream')->get());
$multi->executeEach(
    // $onResult: 每个请求完成时调用（必传）
    function(array $result): void {
        echo "完成: {$result['id']}\n";
    },
    // $onChunk: 每收到一块响应体时调用（可选，二进制安全）
    function(string $requestId, string $chunk): void {
        echo "收到 {$requestId} 的 " . strlen($chunk) . " 字节\n";
    },
    // $onHeaders: 收到响应头时调用（可选）
    function(string $requestId, int $status, array $headers): void {
        echo "{$requestId} 响应头: HTTP {$status}\n";
    }
);
```

**回调签名：**

| 回调 | 签名 | 触发时机 |
|------|------|----------|
| `$onResult` | `function(array $result): void` | 每个请求完成时（必传） |
| `$onChunk` | `function(string $requestId, string $chunk): void` | 每收到一块响应体时（可选，`$chunk` 二进制安全） |
| `$onHeaders` | `function(string $requestId, int $status, array $headers): void` | 收到响应头时（可选，每个请求触发一次） |

> `$onChunk` 的所有 chunk 拼接后等于完整响应体（与 `$result['body']` 一致）。
> 两个参数均为可选，不传时行为与之前完全一致（向后兼容）。
>
> **`onHeaders` 回调 headers 键名小写**：`$onHeaders` 回调收到的 `$headers` 数组键名为
> 小写（与响应头一致），如需访问 `Content-Type` 请用小写键 `content-type`。

### XHThreadPool - 线程池

仅 CLI 模式可用（FPM 多线程会与 PHP 内存管理器冲突）。创建独立工作线程池处理请求，
同对象多次 `execute()` 复用工作线程。

| 方法 | 说明 |
|------|------|
| `__construct(int $workers = 0)` | 创建线程池（0 = 默认工作线程数） |
| `add(XHRequest $req): $this` | 添加请求（带数量上限检查） |
| `maxConcurrency(int $max): $this` | 最大并发数（0 = 无限制） |
| `maxResponseSize(int $size): $this` | 单响应最大字节数（0 = 用全局默认 10MB） |
| `timeout(int $seconds): $this` | 设置整体执行超时（秒，0 = 无超时） |
| `execute(): array` | 执行所有请求，返回结果数组（按完成顺序） |
| `executeEach(callable $onResult, ?callable $onChunk = null, ?callable $onHeaders = null): int` | 流式回调执行（签名与 `XHMulti::executeEach` 一致，仅 CLI 可用） |
| `count(): int` | 返回待执行请求数 |
| `isEmpty(): bool` | 是否有待执行请求 |
| `getMaxConcurrency(): int` | 获取配置的最大并发数（未配置返回 0） |
| `getMaxResponseSize(): int` | 获取配置的最大响应体大小（未配置返回 0） |
| `getTimeout(): int` | 获取配置的批量级超时秒数（未配置返回 0） |

```php
$pool = new XHThreadPool(8);  // 8 个工作线程
$pool->add($request1);
$pool->add($request2);
$pool->maxConcurrency(4);     // 限制并发为 4
$pool->timeout(30);            // 整体超时 30 秒
$results = $pool->execute();
```

> **空请求抛异常**：请求列表为空时 `execute()` 抛异常（而非返回空数组），请先调用 `add()`。
>
> **execute() 消费请求列表**：执行后已添加的请求会被清空，重用同一对象需重新 `add()`。
>
> **队列容量超限抛异常**：当批量请求数超过线程池队列容量（默认 1000）时，`execute()`
> 抛异常（含失败数量），而非静默返回部分结果。建议分批执行或调整队列容量。

### 流式回调类型

XHCurl 提供两种不同层级的"流式回调"，用户常混淆，此处明确区分：

| 类型 | 触发时机 | 回调参数 | 适用方法 |
|------|----------|----------|----------|
| **请求级流式** | 每个请求**完成后**触发 | `function(array $result): void` | `each()`、`XHMulti::executeEach()`、`XHThreadPool::executeEach()` 的 `$onResult` 参数 |
| **响应体分块级流式** | 请求过程中每收到一块数据触发 | `onChunk`/`onHeaders` | `XHMulti::executeEach()`、`XHThreadPool::executeEach()` 的可选参数 |

**请求级流式（`$onResult`）**：批量并发执行时，每完成一个请求立即调用回调处理，不累积全部结果（内存恒定）。所有类（协程 `each`、`XHMulti`、`XHThreadPool`）均支持。

**响应体分块级流式（`$onChunk`/`$onHeaders`）**：在单个请求过程中，每收到一块响应体数据就触发 `onChunk` 回调，收到响应头时触发 `onHeaders` 回调。适用于大文件流式下载、SSE（Server-Sent Events）、NDJSON 流式解析等场景。仅 `XHMulti::executeEach()` 和 `XHThreadPool::executeEach()` 支持此层级。

> **协程 `each()`** 仅支持请求级流式（`$onResult`），不支持 `$onChunk`/`$onHeaders`。如需响应体分块级流式，请用 `XHMulti::executeEach()` 或 `XHThreadPool::executeEach()`。

### 请求级流式回调行为契约

XHCurl 的三种执行模式均支持**请求级流式回调**——每完成一个请求就立即回调处理成功或失败的结果，不累积结果数组，内存恒定：

1. **协程 `XHCurl::each(array $requests, callable $callback): int`** —— 静态方法，必须在 `XHCurl::run()` 内调用，仅 CLI
2. **`XHMulti::executeEach(callable $onResult, ?callable $onChunk = null, ?callable $onHeaders = null): int`** —— 实例方法，先 `add()` 再执行，CLI + FPM
3. **`XHThreadPool::executeEach(callable $onResult, ?callable $onChunk = null, ?callable $onHeaders = null): int`** —— 实例方法，先 `add()` 再执行，仅 CLI

三者主回调签名一致：`function(array $result): void`，`$result` 字段由共享的 `result_to_php_array` 生成（`id`/`success`/`status`/`body`/`headers`/`elapsed_ms`/`body_size`/`url`/`error?`/`error_type?`/`user_data?`，详见「结果数组字段」）。

#### 统一行为契约

三种模式的请求级流式回调共享以下行为约定：

- **即时触发**：每完成一个请求（成功或失败）立即调用回调，不等其他请求
- **结果字段一致**：回调收到的 `$result` 数组字段与 `execute()`/`gather()` 等返回值完全一致（由 `result_to_php_array` 统一生成）
- **失败也回调**：失败请求同样触发回调（`success=false`、`status=0`、`body=""`、`error=...`），不抛 PHP 异常
- **不累积、内存恒定**：结果处理权交给用户回调，框架不持有全部结果数组，适合海量请求场景
- **回调异常中止**：回调内抛出异常时会中止剩余任务并向上传播异常（具体中止机制见下表）
- **返回 false 中止**：`$onResult` 返回 `false`（严格 `=== false`）时中止剩余任务并返回已处理数（不视为错误，详见下文「回调返回值控制中止」）
- **返回处理总数**：方法返回 `int`，表示已回调处理的结果总数

#### 回调返回值控制中止

除了用异常控制流程外，三种流式回调还支持通过 `$onResult` 的**返回值**显式控制是否中止剩余任务：

- **返回 `false`（严格 `=== false`）** → 立即中止剩余任务，方法返回已处理的结果数（`int`，**不视为错误**）
- **返回 `true` / `null` / `void` / 任意非 false 值** → 继续处理剩余任务（向后兼容）
- **抛异常** → 中止剩余任务并返回错误（向后兼容）

这让 PHP 使用者能根据业务情况显式决定"遇到异常时继续还是中断"，而非依赖异常控制流程。

业务场景：遇到请求失败时中止剩余任务：

```php
$multi->executeEach(function($result) {
    if ($result['success'] === false) {
        // 请求失败，中止剩余任务
        return false;
    }
    // 正常处理
    writeToDb($result);
});
```

业务场景：记录失败但继续处理剩余任务：

```php
$multi->executeEach(function($result) {
    if ($result['success'] === false) {
        logError($result['error']);
        return; // 不返回 false，继续处理剩余任务
    }
    writeToDb($result);
});
```

> **注意事项**：
> - 仅严格 `=== false` 才中止（`0`、`''`、`[]`、`null` 都视为继续，避免 PHP 弱类型陷阱）
> - 返回 `false` 中止时，后台剩余任务被 abort（与抛异常一致的清理机制）
> - 方法返回已处理数（`int`），不抛异常、不返回错误

#### 三者对比

| 维度 | 协程 `each()` | `XHMulti::executeEach()` | `XHThreadPool::executeEach()` |
|------|----------------|--------------------------|-------------------------------|
| 调用方式 | 静态方法，需包裹在 `run()` 内 | 实例方法，先 `add()` | 实例方法，先 `add()` |
| 并发模型 | Semaphore（限制 in-flight 数） | Semaphore（限制 in-flight 数） | 固定 worker 池 |
| 默认并发上限 | 64（`fiber_max_concurrency`） | 0 = 不限 | CPU 核心数 |
| SAPI 限制 | 仅 CLI | CLI + FPM | 仅 CLI |
| 批量级超时 | 无 | 有（`timeout()`） | 无 |
| `onChunk`/`onHeaders` | 不支持 | 支持 | 支持 |
| 回调异常处理 | abort 全部 task | `abort_tasks()` | drop pool → abort workers |
| 返回 `false` 中止 | 支持（abort 全部 task） | 支持（`abort_tasks()`） | 支持（drop pool → abort workers） |

> 协程 `each()` 仅支持请求级流式回调（`$callback`），不支持响应体分块级流式（`$onChunk`/`$onHeaders`）。如需后者，请用 `XHMulti::executeEach()` 或 `XHThreadPool::executeEach()`。

---

## xhrun - 安全 Shell 命令执行

`xhrun()` 是一个全局函数，用于替代 PHP 内置的 `shell_exec` / `exec` / `system` /
`passthru` / `proc_open`，提供更安全的跨平台 shell 命令执行能力。

### 安全模型

| 特性 | 说明 |
|------|------|
| **默认不经 shell** | 命令和参数通过 `Command::arg()` 逐个传递，天然避免 shell 注入 |
| **超时控制** | 命令超时后自动终止子进程，避免卡死 |
| **输出大小限制** | `max_output` 防止失控命令耗尽内存 |
| **白名单/黑名单** | `allow`/`deny` 选项限制可执行命令 |
| **二进制安全** | stdout/stderr/stdin 均保留原始字节 |
| **跨平台** | `shell => true` 时自动选择 `cmd /C`（Windows）或 `sh -c`（Unix） |

### PHP 签名

```php
xhrun(string $command, array $args = [], array $options = []): array
```

### 参数

- `command`: 要执行的命令（如 `"ls"`、`"ping"`、`"cmd"`）。
- `args`: 命令参数数组（如 `["-la", "/tmp"]`）。每个元素作为一个独立参数，
  **不经过 shell 解析**。`shell => true` 时，`args` 会做 shell 转义后再拼接进命令行，
  防止参数中的元字符（`;`/`$()`/反引号）注入命令。
- `options`: 选项数组，支持以下键：

| 键 | 类型 | 默认 | 说明 |
|----|------|------|------|
| `timeout` | int | 60 | 超时秒数，0 = 无超时 |
| `max_output` | int | 64MB | 每个流（stdout/stderr）的最大输出字节数，0 = 无限制 |
| `cwd` | string | 继承 | 工作目录 |
| `env` | array | 继承 | 环境变量键值对 |
| `shell` | bool | false | 是否通过系统 shell 执行（启用管道/通配符/重定向；`args` 会做 shell 转义防注入，但 `command` 仍按字面传 shell，处理不可信输入时建议用默认非 shell 路径） |
| `allow` | array | [] | 命令白名单（设置后仅允许这些命令） |
| `deny` | array | [] | 命令黑名单 |
| `input` | string | 无 | 传给命令 stdin 的数据（二进制安全） |

### 返回

关联数组，字段：

| 字段 | 类型 | 说明 |
|------|------|------|
| `success` | bool | 是否成功执行（exit_code == 0 且未超时/超限） |
| `exit_code` | int | 进程退出码。超时/启动失败时为 -1 |
| `stdout` | string | 标准输出（二进制安全） |
| `stderr` | string | 标准错误输出（二进制安全） |
| `elapsed_ms` | int | 执行耗时（毫秒） |
| `pid` | int | 子进程 PID |
| `timed_out` | bool | 是否因超时被终止 |
| `truncated` | bool | 输出是否因超过 max_output 被截断 |
| `error` | string | 错误信息（启动失败、超限等，可选） |
| `command` | string | 失败时的命令名（白名单/黑名单拒绝、超时、截断等错误路径返回，可选） |

### 示例

```php
<?php
// 1. 基本用法（不经过 shell，安全）
$r = xhrun('ls', ['-la', '/tmp']);
if ($r['success']) {
    echo $r['stdout'];
}

// 2. 带超时和环境变量
$r = xhrun('ping', ['-c', '4', 'example.com'], [
    'timeout' => 10,
    'env' => ['PATH' => '/usr/bin'],
]);

// 3. 安全验证：参数不经 shell 解析，以下内容会被完整 echo，不会执行 rm
$r = xhrun('echo', ['foo; rm -rf /']);
echo $r['stdout'];  // 输出: foo; rm -rf /

// 4. 需要管道时显式启用 shell（注意：需自行确保输入安全）
$r = xhrun('ls -la /tmp | grep foo', [], ['shell' => true]);

// 5. 白名单限制（仅允许 ls 和 cat）
$r = xhrun('ls', ['-la'], ['allow' => ['ls', 'cat']]);

// 6. 黑名单（禁止 rm/shutdown）
$r = xhrun('rm', ['-rf', '/'], ['deny' => ['rm', 'shutdown']]);
// $r['success'] === false, $r['error'] 含拒绝原因

// 7. stdin 输入（二进制安全）
$r = xhrun('cat', [], ['input' => "hello\nbinary: \x00\x01\x02"]);

// 8. 工作目录
$r = xhrun('pwd', [], ['cwd' => '/tmp']);
```

### 与 PHP 内置函数对比

| 对比项 | `shell_exec` | `exec` | `xhrun` |
|--------|-------------|--------|---------|
| 默认经 shell | 是（易注入） | 是 | **否**（防注入） |
| 参数分离 | 否 | 否 | **是** |
| 超时控制 | 无 | 无 | **有** |
| 输出大小限制 | 无 | 无 | **有** |
| 白名单/黑名单 | 无 | 无 | **有** |
| 二进制安全输出 | 是 | 是 | **是** |
| stderr 独立捕获 | 否 | 否 | **是** |
| 跨平台 shell | - | - | **自动**（cmd/sh） |

---

## curl 兼容性对照

| PHP curl 选项 | XHCurl 方法 | 状态 |
|---------------|-------------|------|
| CURLOPT_URL | `createRequest($url)` | ✅ |
| CURLOPT_HTTPGET / POST / PUT / DELETE | `get()` / `post()` / `put()` / `delete()` | ✅ |
| CURLOPT_CUSTOMREQUEST | `customMethod()` | ✅ |
| CURLOPT_HTTPHEADER | `header()` | ✅ |
| CURLOPT_POSTFIELDS (JSON) | `json()` | ✅ |
| CURLOPT_POSTFIELDS (form) | `form()` | ✅ |
| CURLOPT_POSTFIELDS (raw, 二进制安全) | `body()` | ✅ |
| CURLOPT_HTTPPOST (multipart) | `multipart()`（字段值二进制安全） | ✅ |
| CURLOPT_COOKIE | `cookies()` | ✅ |
| CURLOPT_USERPWD | `basicAuth()` | ✅ |
| CURLOPT_XOAUTH2_BEARER | `bearerToken()` | ✅ |
| CURLOPT_SSL_VERIFYPEER | `verifySsl()` | ✅ |
| CURLOPT_ENCODING | `encoding()` | ✅ |
| CURLOPT_RANGE | `range()` | ✅ |
| CURLOPT_TIMEOUT | `timeout()` / `timeoutMs()` | ✅ |
| CURLOPT_CONNECTTIMEOUT | `connectTimeout()` | ✅ |
| CURLOPT_USERAGENT | `userAgent()` | ✅ |
| CURLOPT_PROXY | `proxy()` | ✅ |
| CURLOPT_FOLLOWLOCATION | `followRedirects()` | ✅ |
| CURLOPT_MAXREDIRS | `maxRedirects()` | ✅ |
| CURLOPT_CAINFO | - | ❌ 暂未实现 |
| CURLOPT_SSLCERT | - | ❌ 暂未实现 |
| CURLOPT_SSLKEY | - | ❌ 暂未实现 |
| CURLOPT_COOKIEFILE | - | ❌ 暂未实现 |
| CURLOPT_COOKIEJAR | - | ❌ 暂未实现 |

---

## 迁移注意事项

从 PHP curl / Guzzle 迁移到 XHCurl 时，注意以下行为差异：

### timeout 0 值语义
- **curl**：`CURLOPT_TIMEOUT, 0` 在某些版本表示"无超时"，另一些版本表示"立即超时"（行为不一致）
- **XHCurl**：`timeout(0)`/`timeoutMs(0)`/`connectTimeout(0)`/`connectTimeoutMs(0)` 统一表示"跳过设置（使用全局默认值）"，而非立即超时

### 响应头键名小写
- **curl**：`curl_getinfo()` 或 `$curlinfo` 保留原始大小写
- **XHCurl**：`result['headers']` 和 `getHeaders()` 返回的响应头键名**统一为小写**（遵循 HTTP/2 规范，HTTP/1.x 亦如此）。如需访问 `Content-Type`，请用小写键 `content-type`

### cookies 数组形式自动 URL 编码
- **curl**：`CURLOPT_COOKIE` 接受原始字符串，不做任何编码
- **XHCurl**：`cookies(['name' => 'value'])` 数组形式会对 value 做 URL 编码（与 PHP `setcookie()` 行为一致），防止含 `;`/`=` 的 value 破坏 Cookie 格式或注入伪造 cookie。字符串形式 `cookies('name=value')` 不做编码（向后兼容，直接设置原始字符串）
- **迁移建议**：从 curl 的 `CURLOPT_COOKIE` 字符串形式迁移时，若 cookie value 含特殊字符，建议改用数组形式以获得自动编码保护

### 批量配置与单请求配置分离
- **Guzzle**：`TransferOption::TIMEOUT` 同时控制单请求和批量
- **XHCurl**：单请求超时用 `XHRequest::timeout()`，批量级总超时用 `XHMulti::timeout()`/`XHThreadPool::timeout()`（两者独立）

---

## FPM 与 CLI 模式

XHCurl 根据 PHP SAPI 自动选择运行时：

| 模式 | 运行时类型 | 并发模型 | 可用功能 |
|------|-----------|----------|---------|
| **CLI** | 多线程 tokio 运行时 | M:N 并行（工作线程 = CPU 核心数） | 全部功能：协程 `run/await/gather/each`、`XHMulti`、`XHRequest::execute()`、`XHThreadPool` |
| **FPM** | 单线程 tokio 运行时 | 协作式并发（类似 Node.js） | `XHMulti`、`XHRequest::execute()`；协程 `run/await/gather/each` 与 `XHThreadPool` **不可用** |

> **协程仅 CLI 可用**：`XHCurl::run()` 在 FPM 模式下会被显式拒绝。FPM 模式请用
> `XHMulti` 或 `XHRequest::execute()` 实现并发与同步请求。

```php
if (XHCurl::isCli()) {
    // CLI 模式：可用线程池与协程
    $pool = new XHThreadPool(8);
} else {
    // FPM 模式：仅可用 XHMulti 或 XHRequest::execute()（协程/线程池不可用）
    $multi = new XHMulti();
}
```

> **安全说明**：FPM 多进程模式下，每个 worker 进程持有独立的 tokio 运行时（进程级单例），请求间复用。tokio 工作线程绝不触碰 PHP API/zval，结果通过线程安全 channel 回传。

---

## 故障排查

### 扩展加载失败

```bash
# 检查 PHP 错误日志
php -d display_errors=1 -d extension=xhcurl -r "echo XHCurl::version();"

# 常见原因：
# 1. PHP 版本低于 8.1（需要 8.1+，Fiber 协程依赖）
# 2. 扩展文件与 PHP 版本/线程安全模式不匹配
# 3. 缺少系统依赖（Linux: libssl, macOS: 无额外依赖）
```

### 协程模式报错 "await 必须在 run() 内调用"

`await()` 和 `gather()` 必须在 `XHCurl::run()` 的回调内调用：

```php
// ❌ 错误：在 run() 外调用
XHCurl::await($request);

// ✅ 正确：在 run() 回调内调用
XHCurl::run(function() {
    XHCurl::await($request);
});
```

### FPM 下调用 XHCurl::run() 报错

报错信息：`XHCurl::run 仅在 CLI 模式下可用（FPM 请用 XHMulti）`。

协程模式（`run()`/`await()`/`gather()`/`each()`）依赖多线程 tokio 运行时，仅在 CLI 模式下可用。FPM 模式下 `XHCurl::run()` 会被显式拒绝。

```php
// ❌ 错误：在 FPM 下调用 run()
XHCurl::run(function() {
    XHCurl::await($request);  // run() 已返回错误，此处不会执行
});

// ✅ 正确：FPM 下用 XHMulti（批量并发）或 XHRequest::execute()（单次同步）
$multi = new XHMulti();
$multi->add($request);
$results = $multi->execute();  // 协作式并发，可在 FPM 下运行

// 或单次同步
$result = $request->execute();
```

> 用 `XHCurl::isCli()` 在运行时检测 SAPI，并据此选择可用 API。

### Windows DLL 加载失败

- 确认 PHP 版本和线程安全模式（NTS/ZTS）与 DLL 匹配
- 使用 `php -v` 查看线程安全信息
- NTS = Non Thread Safe，ZTS = Zend Thread Safe

### 请求超时 / 连接失败

**现象**：`$result['success'] === false`，`$result['error']` 含 "timeout" 或 "connection" 字样。

**排查**：
- 检查目标 URL 是否可达（`curl -v <url>`）
- 确认 `setConfig(['connect_timeout' => N])` 和 `->timeout(N)` 设置是否合理
- 网络隔离环境（如容器内）确认 DNS 解析正常

### 代理配置无效

**现象**：请求报错 "error sending request for url" 或 "proxy" 相关错误。

**排查**：
- `XHCurl::setConfig(['proxy' => 'http://proxy:8080'])` 格式需含 scheme（http/socks5）
- 无效代理地址会在首次请求时报错（setConfig 不预校验，fail-fast 到请求时）
- 可用 `XHCurl::getConfig()['proxy']` 确认配置已生效

### 响应体超过大小限制

**现象**：响应被截断，`$result['body_size']` 接近 `max_response_size` 配置值。

**排查**：
- 默认限制 10MB（`setConfig(['max_response_size' => 10_000_000])`）
- 大文件下载需调高限制：`setConfig(['max_response_size' => 100_000_000])`（100MB）
- 截断时 `success` 仍为 true，但 `body` 不完整；检查 `body_size` 与预期是否匹配

### 流式回调不触发

**现象**：传入 `$onChunk`/`$onHeaders` 后回调未执行。

**排查**：
- 确认回调参数位置正确：`executeEach($onResult, $onChunk, $onHeaders)`，`$onResult` 是必传的第一个参数
- 确认 `$onChunk` 和 `$onHeaders` 的签名匹配（`function(string $requestId, ...)`）
- 响应体较小时可能只产生一个 chunk（reqwest 内部缓冲），属正常行为
- `$onHeaders` 每个请求仅触发一次（收到响应头时）
- `$onChunk`/`$onHeaders` 仅 `XHMulti::executeEach()` 和 `XHThreadPool::executeEach()` 支持，协程 `each()` 不支持

---

## 开发与贡献

### 项目结构

```
xhcurl/
├── rust/                    # Rust 源码
│   ├── src/
│   │   ├── lib.rs          # 库入口
│   │   ├── php_ext.rs      # PHP 扩展绑定（ext-php-rs）
│   │   ├── fiber.rs        # PHP Fiber 协程桥接
│   │   ├── request.rs      # 请求构建器
│   │   ├── response.rs     # 响应对象
│   │   ├── executor.rs     # 公共请求执行器（multi/threadpool/fiber 共用）
│   │   ├── multi.rs        # 批量异步执行器（tokio spawn + Semaphore）
│   │   ├── threadpool.rs   # 线程池（worker + channel）
│   │   ├── curl.rs         # 全局配置与客户端管理器
│   │   ├── header.rs       # 请求头管理
│   │   └── error.rs        # 错误类型与常量
│   ├── Cargo.toml
│   └── tests/              # 集成测试
├── .github/workflows/
│   └── build-rust.yml      # CI/CD 流水线
└── README.md
```

### 本地开发

```bash
cd rust

# 编译（debug 模式，含 PHP 扩展）
cargo build --features php

# 运行单元测试
cargo test --lib

# 代码格式检查
cargo fmt -- --check

# 静态分析（必须加 --features php，否则 #[php_impl] 块内的代码不会被检查）
cargo clippy --all-targets --features php -- -D warnings

# 编译 release 扩展
cargo build --release --features php

# 加载扩展测试
php -d extension=target/release/libxhcurl.so -r "echo XHCurl::version();"
```

> **PHP 扩展编译环境要求**：需要 libclang（ext-php-rs bindgen）和 PHP 8.1+ 开发头文件。
> Linux 还需 OpenSSL 开发库。详见「安装」一节。

### CI/CD 流水线

GitHub Actions（`build-rust.yml`）在 push 到 main 或创建 tag 时自动触发，编译以下矩阵：

- **Linux**：PHP 8.1~8.5（Ubuntu 24.04）
- **macOS**：PHP 8.1~8.5（macOS 14）
- **Windows**：PHP 8.1~8.5 NTS/ZTS（Windows 2022）
- **Lint & Test**：cargo fmt + clippy + 单元测试

---

## License

MIT
