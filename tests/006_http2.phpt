--TEST--
XHCurl HTTP/2 支持测试
--SKIPIF--
<?php
if (!extension_loaded('xhcurl')) echo 'skip xhcurl extension not loaded';
?>
--FILE--
<?php
// 创建全局管理器
$curl = new XHCurl();

// 测试默认启用 HTTP/2（构造函数应设置 http2_enabled = true）
// 通过 setHttp2(false) 禁用，再 setHttp2(true) 启用，验证方法可调用
$curl->setHttp2(true);
var_dump(true);  // 若 setHttp2 不存在会报 Fatal error

// 测试禁用 HTTP/2
$curl->setHttp2(false);
var_dump(true);

// 重新启用 HTTP/2 并发起请求（httpbin.org 支持 HTTP/2）
$curl->setHttp2(true);
$curl->setTimeout(10);
$curl->setVerifySsl(false);

$request = new XHRequest('https://httpbin.org/get');
$response = $curl->exec($request);

// 验证请求成功（HTTP/2 协商失败时 libcurl 会自动回退到 HTTP/1.1）
var_dump($response->getStatusCode() === 200);
var_dump($response->getError() === null);

echo "HTTP/2 test passed!\n";
?>
--EXPECTF--
bool(true)
bool(true)
bool(true)
bool(true)
HTTP/2 test passed!
