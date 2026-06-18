--TEST--
XHCurl 同步请求测试
--SKIPIF--
<?php if (!extension_loaded('xhcurl')) echo 'skip xhcurl extension not loaded'; ?>
--FILE--
<?php
// 创建全局管理器
$curl = new XHCurl();

// 设置全局配置
$curl->setTimeout(10);
$curl->setConnectTimeout(5);
$curl->setVerifySsl(false);
$curl->setUserAgent('XHCurl-Test/1.0');

// 创建 GET 请求
$request = new XHRequest('https://httpbin.org/get');
$response = $curl->exec($request);

// 测试响应对象
var_dump($response instanceof XHResponse);

// 测试状态码
var_dump($response->getStatusCode() === 200);

// 测试 Content-Type
var_dump($response->getContentType() !== null);

// 测试响应体长度
var_dump($response->getBodyLength() > 0);

// 测试分段读取响应体
$chunk = $response->getBodyChunk(0, 100);
var_dump(strlen($chunk) > 0);

// 测试 isJson
var_dump($response->isJson());

// 测试 toJsonArray
$data = $response->toJsonArray();
var_dump(is_array($data));

// 测试请求耗时
var_dump($response->getTotalTime() > 0);

// 测试错误信息（成功请求应为 null）
var_dump($response->getError() === null);

// 测试头部操作
var_dump($response->hasHeader('content-type'));
var_dump($response->getHeader('content-type') !== null);

echo "Sync test passed!\n";
?>
--EXPECTF--
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
Sync test passed!
