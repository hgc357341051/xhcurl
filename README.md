# XHCurl - 高性能 PHP HTTP 扩展

类似 curl 的 PHP C 扩展，支持单次/批量/多线程同步异步请求。

## 特性

- **单次请求**: `$curl->exec($request)` 同步阻塞
- **批量异步**: `new XHMulti($curl)` 基于 curl_multi，FPM/CLI 通用
- **多线程并发**: `new XHThreadPool($curl)` 真多线程，仅 CLI
- **流式回调**: `onChunk()` / `onHeader()` 实时处理大数据
- **懒加载响应**: `getBodyChunk(offset, len)` 按需读取，防内存溢出
- **全局+请求级配置**: headers/cookies 均支持两级设置

## 快速使用

```php
$curl = new XHCurl();
$curl->setGlobalHeader('Authorization', 'Bearer xxx');
$curl->setTimeout(30);

// 单次同步
$req = new XHRequest('https://api.example.com/data');
$req->setMethod('POST');
$res = $curl->exec($req);
echo $res->getStatusCode();           // int
echo $res->getBodyChunk(0, 1024);    // string (分段读取)
$data = $res->toJsonArray();         // array (按需解析)

// 批量异步
$multi = new XHMulti($curl);
$multi->add(new XHRequest('https://a.com'));
$multi->add(new XHRequest('https://b.com'));
$responses = $multi->execute();
```

## 编译安装

```bash
phpize && ./configure --enable-xhcurl && make && make install
```

然后在 `php.ini` 中添加：
```ini
extension=xhcurl.so
```

## CI/CD

[![Build](https://github.com/hgc357341051/xhcurl/actions/workflows/build.yml/badge.svg)](https://github.com/hgc357341051/xhcurl/actions/workflows/build.yml)

自动编译 Linux / macOS / Windows 多平台 + PHP 8.0~8.4 多版本。
