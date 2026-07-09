# XHCurl - 高性能 PHP HTTP 客户端扩展（Rust 实现）

[![Build Status](https://github.com/hgc357341051/xhcurl/actions/workflows/build-rust.yml/badge.svg)](https://github.com/hgc357341051/xhcurl/actions/workflows/build-rust.yml)
[![PHP Version](https://img.shields.io/badge/PHP-8.1%20--%208.4-blue.svg)](https://php.net)
[![Rust](https://img.shields.io/badge/Rust-stable-orange.svg)](https://www.rust-lang.org/)
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
  - [XHResponse - 响应对象](#xhresponse---响应对象)
  - [XHMulti - 批量异步执行器](#xhmulti---批量异步执行器)
  - [XHThreadPool - 线程池](#xhthreadpool---线程池)
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
| **curl 兼容** | 对标 PHP curl 的 `CURLOPT_*`，支持 cookie/auth/TLS 证书/multipart 等 |
| **全局连接复用** | reqwest Client 全局单例，TCP keep-alive + TLS 会话缓存 |
| **自适应运行时** | CLI 模式多线程运行时（M:N 并行），FPM 模式单线程运行时（协作式并发） |
| **用户自定义数据** | `setUserData()` 携带任意结构化数据，随结果原样回传 |
| **响应体大小限制** | 流式读取 + `max_response_size` 防止内存溢出 |

---

## 平台支持

| 平台 | PHP 版本 | 线程安全 | 状态 |
|------|----------|----------|------|
| Linux (Ubuntu 22.04) | 8.1, 8.2, 8.3, 8.4 | NTS | ✅ 支持 |
| macOS 14 | 8.1, 8.2, 8.3 | NTS | ✅ 支持 |
| Windows (x64) | 8.1, 8.2, 8.3, 8.4 | NTS / ZTS | ✅ 支持 |

> **PHP 版本要求**：**8.1+**。协程模式（`await`/`gather`/`run`）依赖 PHP 8.1 引入的 [Fiber](https://www.php.net/manual/zh/language.fibers.php) 类。

---

## 安装

### 方式一：从源码编译

**前置依赖：**

- Rust 工具链（stable）
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
- `xhcurl-rust-windows-php8.x-zts.dll` - Windows ZTS

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

// 同步执行
$result = XHCurl::execute($request);

if ($result['success']) {
    echo "状态码: " . $result['status'] . "\n";
    echo "响应体: " . $result['body'] . "\n";
}
```

### 示例 2：POST JSON 请求

```php
<?php
$request = XHCurl::createRequest('https://httpbin.org/post')
    ->post()
    ->json(['name' => 'XHCurl', 'version' => '1.0'])
    ->header('Authorization', 'Bearer my-token')
    ->timeout(30);

$result = XHCurl::execute($request);
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
        ->setUserData(['task_index' => $i, 'tag' => "batch-{$i}"]);
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

使用 `setUserData()` 携带任意结构化数据（数组/对象），随请求原样回传到结果中：

```php
<?php
$request = XHCurl::createRequest('https://api.example.com/data')
    ->get()
    ->setUserData([
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
| `execute()` | `(XHRequest $req): array` | 同步执行单个请求 |
| `run()` | `(callable $main): mixed` | 启动协程事件泵，执行主回调 |
| `await()` | `(XHRequest $req): array` | 协程式等待单个请求（须在 `run()` 内） |
| `gather()` | `(array $requests): array` | 并发批量请求，按完成顺序返回（须在 `run()` 内） |

**全局配置项：**

```php
XHCurl::setConfig([
    'connect_timeout'    => 10,      // 连接超时（秒）
    'request_timeout'    => 30,      // 请求超时（秒）
    'max_response_size'  => 10485760,// 最大响应体（字节，默认 10MB）
    'follow_redirects'   => true,    // 跟随重定向
    'max_redirects'      => 10,      // 最大重定向次数
    'verify_ssl'         => true,    // 验证 SSL 证书
    'user_agent'         => 'XHCurl',// User-Agent
    'proxy'              => null,    // 代理地址
]);
```

### XHRequest - 请求构建器

所有 setter 方法返回 `$this`，支持链式调用。

#### HTTP 方法

| 方法 | 说明 |
|------|------|
| `get()` / `post()` / `put()` / `delete()` / `patch()` / `head()` | 设置标准 HTTP 方法 |
| `method(string $method)` | 通过字符串设置方法 |
| `customMethod(string $method)` | 自定义方法（CURLOPT_CUSTOMREQUEST，如 PROPFIND/TRACE） |

#### 请求体

| 方法 | 说明 |
|------|------|
| `json(array $data)` | JSON 请求体（自动设置 Content-Type） |
| `form(array $data)` | 表单请求体（application/x-www-form-urlencoded） |
| `body(string $data)` | 原始请求体 |
| `multipart(array $fields)` | 文件上传（multipart/form-data） |

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
| `cookies(string $cookies)` | CURLOPT_COOKIE | Cookie 字符串 |
| `cookieFile(string $path)` | CURLOPT_COOKIEFILE | Cookie 读取文件 |
| `cookieJar(string $path)` | CURLOPT_COOKIEJAR | Cookie 存储文件 |
| `basicAuth(string $credentials)` | CURLOPT_USERPWD | HTTP 基本认证（`user:pass`） |
| `bearerToken(string $token)` | CURLOPT_XOAUTH2_BEARER | Bearer Token 认证 |
| `encoding(string $encoding)` | CURLOPT_ENCODING | Accept-Encoding（如 `gzip, deflate`） |

#### TLS/SSL

| 方法 | 对应 curl | 说明 |
|------|-----------|------|
| `verifySsl(bool $verify)` | CURLOPT_SSL_VERIFYPEER | 验证 SSL 证书 |
| `caInfo(string $path)` | CURLOPT_CAINFO | 自定义 CA 证书路径 |
| `sslCert(string $path)` | CURLOPT_SSLCERT | 客户端证书路径 |
| `sslKey(string $path)` | CURLOPT_SSLKEY | 客户端密钥路径 |
| `sslKeyPassword(string $password)` | CURLOPT_SSLKEYPASSWD | 密钥密码 |

#### 其他选项

| 方法 | 说明 |
|------|------|
| `timeout(int $seconds)` | 请求超时（秒） |
| `connectTimeout(int $seconds)` | 连接超时（秒） |
| `userAgent(string $ua)` | User-Agent |
| `proxy(string $proxy)` | 代理地址 |
| `followRedirects(bool $follow)` | 跟随重定向 |
| `maxRedirects(int $max)` | 最大重定向次数 |
| `range(string $range)` | Range 请求（CURLOPT_RANGE，如 `0-1023`） |
| `setUserData(array $data)` | 用户自定义数据（随结果回传） |
| `setPriority(int $priority)` | 请求优先级（线程池模式） |
| `getUrl()` / `getMethod()` | 获取 URL / 方法 |

### XHResponse - 响应对象

> 注意：在 `execute()` / `await()` / `gather()` 模式下，结果以**数组**形式返回。`XHResponse` 对象用于 `XHMulti` / `XHThreadPool` 的回调场景。

**结果数组字段：**

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | string | 请求 ID |
| `success` | bool | 是否成功 |
| `status` | int | HTTP 状态码 |
| `body` | string | 响应体 |
| `body_size` | int | 响应体大小（字节） |
| `url` | string | 最终 URL（重定向后） |
| `elapsed_ms` | int | 请求耗时（毫秒） |
| `error` | string | 错误信息（失败时） |
| `user_data` | string | 用户自定义数据（JSON 字符串） |

### XHMulti - 批量异步执行器

```php
$multi = new XHMulti();
$multi->add($request1);
$multi->add($request2);
$multi->maxConcurrency(10);  // 最大并发数
$results = $multi->execute(); // 返回结果数组
```

### XHThreadPool - 线程池

仅 CLI 模式可用。创建独立工作线程池处理请求。

```php
$pool = new XHThreadPool(8);  // 8 个工作线程
$pool->add($request1);
$pool->add($request2);
$results = $pool->execute();
```

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
| CURLOPT_HTTPPOST (multipart) | `multipart()` | ✅ |
| CURLOPT_COOKIE | `cookies()` | ✅ |
| CURLOPT_COOKIEFILE | `cookieFile()` | ✅ |
| CURLOPT_COOKIEJAR | `cookieJar()` | ✅ |
| CURLOPT_USERPWD | `basicAuth()` | ✅ |
| CURLOPT_XOAUTH2_BEARER | `bearerToken()` | ✅ |
| CURLOPT_CAINFO | `caInfo()` | ✅ |
| CURLOPT_SSLCERT | `sslCert()` | ✅ |
| CURLOPT_SSLKEY | `sslKey()` | ✅ |
| CURLOPT_SSLKEYPASSWD | `sslKeyPassword()` | ✅ |
| CURLOPT_SSL_VERIFYPEER | `verifySsl()` | ✅ |
| CURLOPT_ENCODING | `encoding()` | ✅ |
| CURLOPT_RANGE | `range()` | ✅ |
| CURLOPT_TIMEOUT | `timeout()` | ✅ |
| CURLOPT_CONNECTTIMEOUT | `connectTimeout()` | ✅ |
| CURLOPT_USERAGENT | `userAgent()` | ✅ |
| CURLOPT_PROXY | `proxy()` | ✅ |
| CURLOPT_FOLLOWLOCATION | `followRedirects()` | ✅ |
| CURLOPT_MAXREDIRS | `maxRedirects()` | ✅ |

---

## FPM 与 CLI 模式

XHCurl 根据 PHP SAPI 自动选择运行时：

| 模式 | 运行时类型 | 并发模型 | 可用功能 |
|------|-----------|----------|---------|
| **CLI** | 多线程 tokio 运行时 | M:N 并行（工作线程 = CPU 核心数） | 全部功能，含 `XHThreadPool` |
| **FPM** | 单线程 tokio 运行时 | 协作式并发（类似 Node.js） | 除 `XHThreadPool` 外全部功能 |

```php
if (XHCurl::isCli()) {
    // CLI 模式：可用线程池
    $pool = new XHThreadPool(8);
} else {
    // FPM 模式：用 XHMulti 或协程
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

### Windows DLL 加载失败

- 确认 PHP 版本和线程安全模式（NTS/ZTS）与 DLL 匹配
- 使用 `php -v` 查看线程安全信息
- NTS = Non Thread Safe，ZTS = Zend Thread Safe

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
│   │   ├── multi.rs        # 批量异步执行器
│   │   ├── threadpool.rs   # 线程池
│   │   ├── curl.rs         # 客户端管理器
│   │   ├── header.rs       # 请求头管理
│   │   ├── cookie.rs       # Cookie 管理
│   │   ├── buffer.rs       # 缓冲区
│   │   └── error.rs        # 错误类型
│   ├── Cargo.toml
│   └── tests/              # PHP 测试脚本
├── .github/workflows/
│   └── build-rust.yml      # CI/CD 流水线
└── README.md
```

### 本地开发

```bash
cd rust

# 编译（debug 模式）
cargo build --features php

# 运行单元测试
cargo test --lib

# 代码格式检查
cargo fmt -- --check

# 静态分析
cargo clippy -- -D warnings

# 加载扩展测试
php -d extension=target/debug/libxhcurl.so -r "echo XHCurl::version();"
```

### CI/CD 流水线

GitHub Actions（`build-rust.yml`）在 push 到 main 或创建 tag 时自动触发，编译以下矩阵：

- **Linux**：PHP 8.1~8.4（Ubuntu 22.04）
- **macOS**：PHP 8.1~8.3（macOS 14）
- **Windows**：PHP 8.1~8.4 NTS/ZTS（Windows 2022）
- **Lint & Test**：cargo fmt + clippy + 单元测试

---

## License

MIT
