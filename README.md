# XHCurl - 高性能 PHP HTTP 客户端扩展

[![Build Status](https://github.com/hgc357341051/xhcurl/actions/workflows/build.yml/badge.svg)](https://github.com/hgc357341051/xhcurl/actions/workflows/build.yml)
[![PHP Version](https://img.shields.io/badge/PHP-8.0%20--%208.4-blue.svg)](https://php.net)
[![Platform](https://img.shields.io/badge/platform-Linux%20%7C%20macOS%20%7C%20Windows-green.svg)](#平台支持)
[![License](https://img.shields.io/badge/license-MIT-yellow.svg)](LICENSE)

XHCurl 是一个基于 libcurl 的高性能 PHP C 扩展，提供类似 curl 的 HTTP 客户端能力，支持单次请求、批量异步请求、多线程并发请求，并针对大数据量场景做了内存优化。

## 目录

- [核心特性](#核心特性)
- [平台支持](#平台支持)
- [安装](#安装)
- [快速开始](#快速开始)
- [API 文档](#api-文档)
  - [XHCurl - 全局管理器](#xhcurl---全局管理器)
  - [XHRequest - 请求构建器](#xhrequest---请求构建器)
  - [XHResponse - 懒加载响应](#xhresponse---懒加载响应)
  - [XHMulti - 批量异步执行器](#xhmulti---批量异步执行器)
  - [XHThreadPool - CLI 线程池](#xhthreadpool---cli-线程池)
- [使用场景](#使用场景)
- [内存优化设计](#内存优化设计)
- [FPM 与 CLI 模式注意事项](#fpm-与-cli-模式注意事项)
- [故障排查](#故障排查)
- [开发与贡献](#开发与贡献)

---

## 核心特性

| 特性 | 说明 |
|------|------|
| **三种执行模式** | 同步单次（`exec`）、批量异步（`XHMulti`）、多线程并发（`XHThreadPool`） |
| **HTTP/2 支持** | 自动协议协商升级，启用多路复用（multiplexing）提升并发性能 |
| **失败重试** | 网络错误或 HTTP 5xx 自动重试，可配置重试次数和间隔 |
| **流式回调** | `onChunk()` / `onHeader()` 实时处理响应数据，避免一次性加载到内存 |
| **懒加载响应** | `getBodyChunk(offset, length)` 按需分段读取响应体，防止内存溢出 |
| **两级配置** | 全局配置（`XHCurl`）+ 请求级配置（`XHRequest`），请求级覆盖全局 |
| **FPM/CLI 通用** | `XHMulti` 基于 `curl_multi` 单进程异步 I/O，FPM 和 CLI 均安全可用 |
| **CLI 多线程** | `XHThreadPool` 真多线程并发，仅 CLI 模式可用，避免 FPM 线程安全问题 |
| **内存安全** | 响应体存储在 C 侧 `malloc` 缓冲区，不计入 PHP `memory_limit`，通过 `max_response_size` 限制 |
| **共享会话** | 基于 `curl_share` 在多个请求间共享 DNS 缓存、SSL 会话、Cookie |
| **JSON 函数缓存** | MINIT 阶段缓存 `json_encode`/`json_decode` 函数指针，避免每次哈希查找 |

---

## 平台支持

| 平台 | PHP 版本 | 线程安全 | 状态 |
|------|----------|----------|------|
| Linux (Ubuntu 22.04) | 8.0, 8.1, 8.2, 8.3, 8.4 | NTS | ✅ 支持 |
| macOS 13/14 | 8.0, 8.1, 8.2, 8.3 | NTS | ✅ 支持 |
| Windows Server 2022 | 8.0, 8.1, 8.2, 8.3 | NTS | ✅ 支持 |

> **注意**：`XHThreadPool` 在所有平台上仅 CLI 模式可用。

---

## 安装

### 方式一：从源码编译

**Linux / macOS：**

```bash
# 1. 安装系统依赖
# Ubuntu/Debian
sudo apt-get install -y libcurl4-openssl-dev autoconf automake libtool pkg-config

# CentOS/RHEL
sudo yum install -y libcurl-devel autoconf automake libtool pkg-config

# macOS（使用 Homebrew）
brew install autoconf automake libtool pkg-config curl

# 2. 克隆仓库
git clone https://github.com/hgc357341051/xhcurl.git
cd xhcurl

# 3. 编译安装
phpize
./configure --enable-xhcurl
make -j$(nproc)
sudo make install

# 4. 启用扩展
echo "extension=xhcurl.so" | sudo tee /etc/php/8.x/mods-available/xhcurl.ini
sudo phpenmod xhcurl

# 5. 验证安装
php -m | grep xhcurl
php -r "echo xhcurl_version();"
```

**Windows：**

Windows 编译较复杂，建议直接从 [GitHub Releases](https://github.com/hgc357341051/xhcurl/releases) 下载预编译的 DLL。

```powershell
# 1. 下载对应 PHP 版本的 php_xhcurl.dll
# 2. 复制到 PHP 扩展目录（如 C:\php\ext\）
# 3. 在 php.ini 中添加
# extension=xhcurl
```

### 方式二：从 GitHub Releases 下载预编译包

访问 [Releases 页面](https://github.com/hgc357341051/xhcurl/releases)，根据平台和 PHP 版本选择对应的二进制包：

- `xhcurl-linux-php8.x.so` - Linux
- `xhcurl-macos-php8.x.so` - macOS
- `xhcurl-windows-php8.x-nts.dll` - Windows

### 验证安装

```bash
php -m | grep xhcurl
# 输出：xhcurl

php -r "var_dump(xhcurl_version());"
# 输出：string(5) "1.0.0"

php -r "var_dump(class_exists('XHCurl'));"
# 输出：bool(true)
```

---

## 快速开始

### 示例 1：单次同步请求

```php
<?php
// 创建全局管理器
$curl = new XHCurl();
$curl->setTimeout(30);
$curl->setVerifySsl(false);
$curl->setUserAgent('MyApp/1.0');

// 创建请求
$request = new XHRequest('https://httpbin.org/get');
$request->setHeader('X-Custom-Header', 'hello');

// 同步执行
$response = $curl->exec($request);

// 处理响应
echo "状态码: " . $response->getStatusCode() . "\n";
echo "Content-Type: " . $response->getContentType() . "\n";
echo "响应体长度: " . $response->getBodyLength() . "\n";

// 分段读取响应体（避免内存溢出）
$offset = 0;
while ($offset < $response->getBodyLength()) {
    $chunk = $response->getBodyChunk($offset, 1024);
    echo $chunk;
    $offset += strlen($chunk);
}

// 如果是 JSON，按需解析为数组
if ($response->isJson()) {
    $data = $response->toJsonArray();
    print_r($data);
}
```

### 示例 2：批量异步请求（FPM 推荐）

```php
<?php
$curl = new XHCurl();
$curl->setTimeout(10);

// 创建批量执行器
$multi = new XHMulti($curl);

// 添加多个请求
$multi->add(new XHRequest('https://httpbin.org/get?id=1'));
$multi->add(new XHRequest('https://httpbin.org/get?id=2'));
$multi->add(new XHRequest('https://httpbin.org/get?id=3'));

// 并发执行所有请求（基于 curl_multi，单进程异步 I/O）
$responses = $multi->execute();

foreach ($responses as $i => $response) {
    echo "请求 $i: 状态码=" . $response->getStatusCode() 
         . ", 耗时=" . $response->getTotalTime() . "s\n";
}
```

### 示例 3：多线程并发（仅 CLI）

```php
<?php
// 仅在 CLI 模式下运行
if (php_sapi_name() !== 'cli') {
    exit("XHThreadPool 仅 CLI 模式可用\n");
}

$curl = new XHCurl();

// 创建线程池（4 个工作线程）
$pool = new XHThreadPool($curl, 4);

// 添加 100 个请求
for ($i = 0; $i < 100; $i++) {
    $req = new XHRequest("https://httpbin.org/get?id=$i");
    $pool->add($req);
}

// 多线程并发执行
$responses = $pool->execute();

echo "完成请求数: " . count($responses) . "\n";
```

### 示例 4：流式回调处理大响应

```php
<?php
$curl = new XHCurl();
$curl->setMaxResponseSize(100 * 1024 * 1024); // 允许 100MB 响应

$request = new XHRequest('https://example.com/large-file.zip');

// 注册流式回调：每收到一段数据就触发
$totalReceived = 0;
$request->onChunk(function($chunk) use (&$totalReceived) {
    $totalReceived += strlen($chunk);
    // 实时写入文件，避免内存堆积
    file_put_contents('download.zip', $chunk, FILE_APPEND);
    
    // 每 1MB 输出进度
    if ($totalReceived % (1024 * 1024) < strlen($chunk)) {
        echo "已下载: " . round($totalReceived / 1024 / 1024, 2) . " MB\n";
    }
});

$response = $curl->exec($request);
echo "下载完成，总大小: " . $response->getBodyLength() . " 字节\n";
```

### 示例 5：POST JSON 请求

```php
<?php
$curl = new XHCurl();
$curl->setGlobalHeader('Authorization', 'Bearer your-token-here');

$request = new XHRequest('https://api.example.com/users');
$request->setMethod('POST');
$request->setJsonBody([
    'name' => '张三',
    'email' => 'zhangsan@example.com',
    'age' => 30,
]);

$response = $curl->exec($request);

if ($response->getStatusCode() === 201) {
    $user = $response->toJsonArray();
    echo "创建用户成功，ID: " . $user['id'] . "\n";
} else {
    echo "请求失败: " . $response->getError() . "\n";
}
```

### 示例 6：全局 Cookie 与请求级 Cookie

```php
<?php
$curl = new XHCurl();

// 设置全局 Cookie（所有请求都会携带）
$curl->setGlobalCookie('session_id', 'abc123', 'example.com', '/');
$curl->setGlobalCookie('tracking', 'xyz789');

// 请求 1：使用全局 Cookie
$req1 = new XHRequest('https://example.com/api/profile');

// 请求 2：覆盖全局 Cookie + 添加请求级 Cookie
$req2 = new XHRequest('https://example.com/api/admin');
$req2->setCookie('session_id', 'admin-session'); // 覆盖全局
$req2->setCookie('admin_flag', '1');              // 请求级独有

$multi = new XHMulti($curl);
$multi->add($req1);
$multi->add($req2);
$responses = $multi->execute();
```

### 示例 7：HTTP/2 多路复用

```php
<?php
$curl = new XHCurl();

// 启用 HTTP/2（默认已启用，对支持的服务器自动升级协议）
// HTTP/2 多路复用允许在单个 TCP 连接上并发多个请求，大幅提升性能
$curl->setHttp2(true);
$curl->setTimeout(10);

// 批量请求同一域名，HTTP/2 多路复用将复用同一连接
$multi = new XHMulti($curl);
for ($i = 0; $i < 20; $i++) {
    $multi->add(new XHRequest("https://httpbin.org/get?id=$i"));
}

$start = microtime(true);
$responses = $multi->execute();
$elapsed = microtime(true) - $start;

echo "20 个请求耗时: " . round($elapsed, 3) . " 秒\n";
echo "相比 HTTP/1.1 的 20 个串行连接，HTTP/2 多路复用显著降低延迟\n";
```

### 示例 8：失败自动重试

```php
<?php
$curl = new XHCurl();

// 配置重试：网络错误（连接超时、DNS 失败等）或 HTTP 5xx 自动重试
// 参数1：重试次数（0 = 不重试）
// 参数2：重试间隔（毫秒，默认 100ms）
$curl->setRetry(3, 200);
$curl->setTimeout(5);
$curl->setConnectTimeout(3);

// 对不稳定的服务器发起请求，自动重试最多 3 次
$request = new XHRequest('https://httpbin.org/status/500');
$response = $curl->exec($request);

// 即使重试 3 次后仍为 500，也不会崩溃
echo "最终状态码: " . $response->getStatusCode() . "\n";
echo "错误信息: " . ($response->getError() ?? '无') . "\n";

// 重试场景示例：
// 1. 服务器临时 502/503/504 → 重试后可能恢复
// 2. 网络抖动导致连接超时 → 重试后成功
// 3. DNS 解析失败 → 重试后可能解析成功
```

---

## API 文档

### XHCurl - 全局管理器

全局配置管理器，所有请求共享此实例的配置（超时、代理、全局头部/Cookie 等）。

#### 方法

| 方法 | 说明 |
|------|------|
| `__construct()` | 构造函数，初始化 curl_share 共享会话 |
| `setGlobalHeader(string $name, string $value): void` | 设置全局请求头 |
| `setGlobalCookie(string $name, string $value, string $domain = '', string $path = '/'): void` | 设置全局 Cookie |
| `setTimeout(int $seconds): void` | 设置默认请求超时（默认 30 秒） |
| `setConnectTimeout(int $seconds): void` | 设置默认连接超时（默认 10 秒） |
| `setVerifySsl(bool $verify): void` | 是否验证 SSL 证书（默认 true） |
| `setUserAgent(string $ua): void` | 设置默认 User-Agent |
| `setProxy(string $proxy): void` | 设置代理（如 `http://host:port` 或 `socks5://host:port`） |
| `setMaxResponseSize(int $bytes): void` | 设置最大响应体大小（默认 10MB，超过则截断） |
| `setHttp2(bool $enabled): void` | 启用/禁用 HTTP/2（默认启用，支持多路复用） |
| `setRetry(int $count, int $delayMs = 100): void` | 设置失败重试（网络错误或 5xx 自动重试） |
| `exec(XHRequest $request): XHResponse` | 同步执行单个请求 |

#### 示例

```php
$curl = new XHCurl();
$curl->setGlobalHeader('Authorization', 'Bearer token');
$curl->setGlobalHeader('Accept', 'application/json');
$curl->setTimeout(60);
$curl->setConnectTimeout(10);
$curl->setVerifySsl(true);
$curl->setUserAgent('MyApp/2.0');
$curl->setProxy('http://proxy.example.com:8080');
$curl->setMaxResponseSize(50 * 1024 * 1024); // 50MB

// HTTP/2 配置（默认已启用，对支持 HTTP/2 的服务器自动升级协议）
$curl->setHttp2(true);  // 启用 HTTP/2 多路复用（默认）
// $curl->setHttp2(false); // 禁用 HTTP/2，强制使用 HTTP/1.1

// 失败重试配置（网络错误或 HTTP 5xx 自动重试）
$curl->setRetry(3, 200);  // 最多重试 3 次，间隔 200ms
// $curl->setRetry(0);     // 禁用重试（默认）
```

---

### XHRequest - 请求构建器

构建单个 HTTP 请求的配置，支持链式设置。

#### 方法

| 方法 | 说明 |
|------|------|
| `__construct(string $url)` | 构造函数，指定请求 URL |
| `setMethod(string $method): void` | 设置 HTTP 方法（GET/POST/PUT/DELETE/PATCH/HEAD/OPTIONS） |
| `setHeader(string $name, string $value): void` | 设置请求级头部（覆盖全局同名头部） |
| `setCookie(string $name, string $value): void` | 设置请求级 Cookie（覆盖全局同名 Cookie） |
| `setBody(string $body): void` | 设置原始请求体 |
| `setJsonBody(array $data): void` | 设置 JSON 请求体（自动设置 Content-Type） |
| `setTimeout(int $seconds): void` | 设置请求级超时（0 表示使用全局默认值） |
| `setConnectTimeout(int $seconds): void` | 设置请求级连接超时 |
| `setFollowRedirects(bool $follow, int $max = 5): void` | 是否跟随重定向 |
| `onChunk(callable $callback): void` | 注册流式数据回调（仅 `exec` 和 `XHMulti` 可用） |
| `onHeader(callable $callback): void` | 注册响应头回调 |
| `getUrl(): string` | 获取请求 URL |

#### 流式回调签名

```php
// onChunk 回调：每收到一段响应体数据时触发
$request->onChunk(function(string $chunk): void {
    // $chunk 是本次接收的数据片段
    // 可以写入文件、数据库，或进行流式处理
});

// onHeader 回调：每收到一行响应头时触发
$request->onHeader(function(string $headerLine): void {
    // $headerLine 格式如 "Content-Type: application/json\r\n"
});
```

> **注意**：`onChunk` / `onHeader` 在 `XHThreadPool` 模式下不可用（线程安全限制）。

---

### XHResponse - 懒加载响应

响应对象，**不一次性返回所有数据**，支持按需分段读取，避免内存溢出。

#### 方法

| 方法 | 说明 |
|------|------|
| `getStatusCode(): int` | 获取 HTTP 状态码 |
| `getHeader(string $name): ?string` | 获取指定响应头（不区分大小写） |
| `getHeaders(): array` | 获取所有响应头（数据量大时慎用） |
| `hasHeader(string $name): bool` | 检查响应头是否存在 |
| `getBodyChunk(int $offset, int $length): string` | **分段读取响应体**（核心方法） |
| `getBodyLength(): int` | 获取响应体总长度 |
| `getContentType(): ?string` | 获取 Content-Type 头部值 |
| `isJson(): bool` | 判断是否为 JSON 响应 |
| `toJsonArray(): ?array` | 将响应体解析为 PHP 数组（按需解析） |
| `getError(): ?string` | 获取错误信息（成功时为 null） |
| `getTotalTime(): float` | 获取请求总耗时（秒） |

#### 分段读取示例

```php
$response = $curl->exec($request);

$totalLen = $response->getBodyLength();
echo "响应体总大小: $totalLen 字节\n";

// 每次读取 4KB
$offset = 0;
$chunkSize = 4096;
while ($offset < $totalLen) {
    $chunk = $response->getBodyChunk($offset, $chunkSize);
    if ($chunk === '') break;
    
    // 处理 $chunk
    processChunk($chunk);
    
    $offset += strlen($chunk);
}
```

---

### XHMulti - 批量异步执行器

基于 `curl_multi` 接口的批量并发执行器，**FPM 和 CLI 模式通用**。

#### 方法

| 方法 | 说明 |
|------|------|
| `__construct(XHCurl $curl)` | 构造函数，关联全局管理器 |
| `add(XHRequest $request): void` | 添加请求到批量队列 |
| `execute(): array` | 并发执行所有请求，返回 `XHResponse[]` |
| `count(): int` | 获取已添加的请求数量 |

#### 特点

- 单进程异步 I/O 多路复用，无线程安全问题
- 支持 `onChunk` / `onHeader` 流式回调
- 请求按添加顺序返回响应
- 执行后自动清空队列，可重复使用

#### 示例

```php
$curl = new XHCurl();
$multi = new XHMulti($curl);

// 添加请求
for ($i = 0; $i < 10; $i++) {
    $multi->add(new XHRequest("https://httpbin.org/get?id=$i"));
}

echo "待执行请求数: " . $multi->count() . "\n";

// 并发执行
$responses = $multi->execute();

foreach ($responses as $i => $response) {
    echo "[$i] 状态码: " . $response->getStatusCode() . "\n";
}

// 队列已清空，可继续添加新请求
echo "执行后请求数: " . $multi->count() . "\n"; // 0
```

---

### XHThreadPool - CLI 线程池

基于多线程的并发执行器，**仅 CLI 模式可用**，提供真正的并行处理能力。

#### 方法

| 方法 | 说明 |
|------|------|
| `__construct(XHCurl $curl, int $workers = 4)` | 构造函数，指定工作线程数（1-64） |
| `add(XHRequest $request): void` | 添加请求到线程池队列 |
| `execute(): array` | 多线程并发执行，返回 `XHResponse[]` |
| `count(): int` | 获取已添加的请求数量 |

#### 限制

- **仅 CLI 模式可用**，FPM 下会抛出 `XHCurlException`
- **不支持流式回调**（`onChunk` / `onHeader` 在线程池模式下无效）
- 工作线程中不调用任何 PHP 函数，结果回主线程后创建 `XHResponse` 对象

#### 示例

```php
<?php
// 检查 CLI 模式
if (php_sapi_name() !== 'cli') {
    exit("XHThreadPool 仅 CLI 模式可用\n");
}

$curl = new XHCurl();

// 创建 8 线程的线程池
$pool = new XHThreadPool($curl, 8);

// 添加 1000 个请求
for ($i = 0; $i < 1000; $i++) {
    $pool->add(new XHRequest("https://httpbin.org/get?id=$i"));
}

echo "开始执行 " . $pool->count() . " 个请求...\n";
$start = microtime(true);

$responses = $pool->execute();

$elapsed = microtime(true) - $start;
echo "完成，耗时: " . round($elapsed, 2) . " 秒\n";

// 统计成功/失败
$success = 0;
$failed = 0;
foreach ($responses as $response) {
    if ($response->getStatusCode() === 200) {
        $success++;
    } else {
        $failed++;
    }
}
echo "成功: $success, 失败: $failed\n";
```

---

## 使用场景

### 场景 1：FPM Web 服务（推荐 XHMulti）

```php
<?php
// 在 FPM Web 服务中处理多个 API 调用
$curl = new XHCurl();
$curl->setGlobalHeader('Authorization', 'Bearer ' . $apiToken);
$curl->setTimeout(5); // FPM 下建议短超时

$multi = new XHMulti($curl);

// 并发获取用户信息、订单、消息
$multi->add(new XHRequest("https://api.example.com/user/$userId"));
$multi->add(new XHRequest("https://api.example.com/orders/$userId"));
$multi->add(new XHRequest("https://api.example.com/messages/$userId"));

$responses = $multi->execute();

$user = $responses[0]->toJsonArray();
$orders = $responses[1]->toJsonArray();
$messages = $responses[2]->toJsonArray();
```

### 场景 2：CLI 批量任务（推荐 XHThreadPool）

```php
<?php
// CLI 脚本批量爬取数据
$curl = new XHCurl();
$curl->setProxy('http://proxy:8080');
$curl->setMaxResponseSize(5 * 1024 * 1024); // 限制 5MB

$pool = new XHThreadPool($curl, 16); // 16 线程

$urls = file('urls.txt', FILE_IGNORE_NEW_LINES | FILE_SKIP_EMPTY_LINES);
foreach ($urls as $url) {
    $pool->add(new XHRequest(trim($url)));
}

$responses = $pool->execute();

foreach ($responses as $i => $response) {
    if ($response->getStatusCode() === 200) {
        // 分段读取，避免内存溢出
        $content = '';
        $offset = 0;
        while ($offset < $response->getBodyLength()) {
            $content .= $response->getBodyChunk($offset, 8192);
            $offset += 8192;
        }
        file_put_contents("output/$i.html", $content);
    }
}
```

### 场景 3：大文件下载（流式回调）

```php
<?php
$curl = new XHCurl();
$curl->setMaxResponseSize(1024 * 1024 * 1024); // 允许 1GB

$request = new XHRequest('https://example.com/huge-file.bin');

$fp = fopen('huge-file.bin', 'wb');
$request->onChunk(function($chunk) use ($fp) {
    fwrite($fp, $chunk); // 直接写入文件，内存占用恒定
});

$response = $curl->exec($request);
fclose($fp);

echo "下载完成: " . $response->getBodyLength() . " 字节\n";
```

---

## 内存优化设计

XHCurl 针对大数据量场景做了以下内存优化：

### 1. 响应体存储在 C 侧缓冲区

响应体数据存储在 C 语言的 `malloc` 缓冲区中，**不计入 PHP `memory_limit`**，避免因 PHP 内存限制导致请求失败。

### 2. 分段读取（懒加载）

`XHResponse::getBodyChunk($offset, $length)` 允许按需分段读取响应体，而不是一次性返回整个字符串。这对于大响应体（如文件下载）至关重要：

```php
// ❌ 错误：一次性读取（可能内存溢出）
$body = '';
for ($i = 0; $i < $response->getBodyLength(); $i += 1024) {
    $body .= $response->getBodyChunk($i, 1024); // 字符串拼接低效
}

// ✅ 正确：流式处理
$offset = 0;
while ($offset < $response->getBodyLength()) {
    $chunk = $response->getBodyChunk($offset, 8192);
    processChunk($chunk); // 立即处理，不累积
    $offset += strlen($chunk);
}
```

### 3. 最大响应体限制

`XHCurl::setMaxResponseSize($bytes)` 设置硬限制，超过此大小的响应会被截断并返回错误，防止恶意服务器导致内存耗尽。

### 4. 流式回调

`onChunk` 回调在数据到达时实时触发，可以立即写入文件或数据库，内存占用恒定，与响应体大小无关。

### 5. 缓冲区扩容策略

C 侧缓冲区采用指数增长策略（每次翻倍），减少频繁 `realloc` 调用。

### 6. HTTP/2 多路复用

启用 HTTP/2 后，对同一主机的多个请求复用单个 TCP 连接，减少连接建立开销和内存占用。`XHMulti` 批量请求场景下性能提升尤为明显。

### 7. JSON 函数指针缓存

在 `MINIT` 阶段一次性查找 `json_encode`/`json_decode` 函数指针并缓存，后续调用直接使用 `zend_call_known_function`，避免每次调用都做函数表哈希查找。

---

## FPM 与 CLI 模式注意事项

### FPM 模式

| 项目 | 说明 |
|------|------|
| **可用功能** | `XHCurl`、`XHRequest`、`XHResponse`、`XHMulti` |
| **不可用** | `XHThreadPool`（会抛出 `XHCurlException`） |
| **超时建议** | 设置较短超时（5-10 秒），避免占用 worker 进程 |
| **内存** | 响应体不计入 `memory_limit`，但仍需通过 `setMaxResponseSize` 限制 |
| **对象生命周期** | 每个请求独立，请求结束后 PHP GC 自动回收，无跨请求泄漏 |
| **并发** | 使用 `XHMulti` 基于 `curl_multi` 单进程异步 I/O |

### CLI 模式

| 项目 | 说明 |
|------|------|
| **可用功能** | 所有功能，包括 `XHThreadPool` |
| **多线程** | `XHThreadPool` 真多线程，工作线程不调用 PHP 函数（线程安全） |
| **流式回调** | `onChunk`/`onHeader` 仅在 `exec` 和 `XHMulti` 中可用，`XHThreadPool` 中无效 |
| **长时间运行** | 适合批量任务、爬虫、数据同步等场景 |
| **资源释放** | 脚本结束时自动释放所有资源 |

### 内存泄漏防护

- 所有 C 侧资源（`malloc`、`curl_easy`、`curl_multi`、`curl_share`）都有对应的释放函数
- PHP 对象析构时（`free_obj`）自动释放关联的 C 资源
- `RSHUTDOWN` 钩子提供兜底清理
- 缓冲区所有权转移设计避免重复释放

---

## 故障排查

### 常见问题

**Q: 编译报错 `curl/curl.h: No such file or directory`**

A: 未安装 libcurl 开发库。安装方式：
```bash
# Ubuntu/Debian
sudo apt-get install libcurl4-openssl-dev

# CentOS/RHEL
sudo yum install libcurl-devel

# macOS
brew install curl
```

**Q: FPM 下使用 `XHThreadPool` 报错**

A: `XHThreadPool` 仅 CLI 模式可用。在 FPM 下请使用 `XHMulti`。

**Q: `getBodyChunk` 返回空字符串**

A: 检查：
1. `offset` 是否超出 `getBodyLength()` 范围
2. `length` 是否为 0 或负数
3. 响应是否成功（`getStatusCode()` 是否为 200）

**Q: 大响应体导致内存溢出**

A:
1. 使用 `setMaxResponseSize()` 限制最大响应体大小
2. 使用 `onChunk` 流式回调，实时处理数据而非累积
3. 使用 `getBodyChunk` 分段读取，而非一次性获取

**Q: `toJsonArray()` 返回 null**

A:
1. 检查 `isJson()` 是否为 true
2. 检查响应体是否为有效的 JSON
3. JSON 解析失败时返回 null（不抛出异常）

**Q: 多线程模式下流式回调不触发**

A: `XHThreadPool` 模式下不支持流式回调（线程安全限制）。如需流式处理，请使用 `XHMulti` 或 `exec`。

---

## 开发与贡献

### 项目结构

```
xhcurl/
├── config.m4              # Unix 构建配置
├── config.w32             # Windows 构建配置
├── php_xhcurl.h           # 公共头文件
├── xhcurl_priv.h          # 私有头文件（数据结构+内部声明）
├── xhcurl.c               # 模块入口 + XHCurl 类
├── xhcurl_buffer.c        # 缓冲区管理 + 头部/Cookie/回调/上下文实现
├── xhcurl_response.c      # XHResponse 懒加载响应类
├── xhcurl_request.c       # XHRequest 请求构建器类
├── xhcurl_multi.c         # XHMulti 批量异步执行器类
├── xhcurl_threadpool.c    # XHThreadPool CLI线程池类
├── tests/                 # 测试套件
│   ├── 001_basic.phpt
│   ├── 002_sync.phpt
│   ├── 003_multi.phpt
│   ├── 004_streaming.phpt
│   └── 005_lazy_loading.phpt
└── .github/workflows/
    └── build.yml          # CI/CD 流水线
```

### 本地开发

```bash
# 克隆仓库
git clone https://github.com/hgc357341051/xhcurl.git
cd xhcurl

# 编译（开发模式，带调试符号）
phpize
./configure --enable-xhcurl CFLAGS="-g -O0 -Wall -Wextra"
make -j$(nproc)

# 运行测试
make test NO_INTERACTION=1

# 手动测试
php -d extension=modules/xhcurl.so tests/manual_test.php
```

### CI/CD

项目使用 GitHub Actions 自动编译多平台二进制：

- **触发条件**：push 到 `main` 分支、创建 `v*` 标签、Pull Request
- **支持平台**：Linux (Ubuntu 22.04)、macOS (13/14)、Windows (Server 2022)
- **支持 PHP 版本**：8.0, 8.1, 8.2, 8.3, 8.4
- **发布**：创建 `v1.0.0` 等标签时自动发布 GitHub Release

### 提交代码

```bash
# 创建功能分支
git checkout -b feature/your-feature

# 提交（遵循 Conventional Commits）
git commit -m "feat: add new feature"

# 推送并发起 PR
git push origin feature/your-feature
```

---

## License

MIT License. See [LICENSE](LICENSE).
