--TEST--
XHRequest 流式回调和请求配置测试
--SKIPIF--
<?php if (!extension_loaded('xhcurl')) echo 'skip xhcurl extension not loaded'; ?>
--FILE--
<?php
// 创建全局管理器
$curl = new XHCurl();
$curl->setTimeout(10);
$curl->setVerifySsl(false);

// 测试请求配置
$request = new XHRequest('https://httpbin.org/post');
$request->setMethod('POST');
$request->setHeader('X-Custom-Header', 'test-value');
$request->setCookie('test-cookie', 'cookie-value');
$request->setJsonBody(['key' => 'value', 'number' => 42]);

// 测试 URL 获取
var_dump($request->getUrl() === 'https://httpbin.org/post');

// 测试流式回调
$chunkReceived = false;
$request->onChunk(function($chunk) use (&$chunkReceived) {
    $chunkReceived = true;
});

// 执行请求
$response = $curl->exec($request);

// 验证请求成功
var_dump($response->getStatusCode() === 200);

// 验证流式回调被触发
var_dump($chunkReceived);

// 验证 JSON 响应
$data = $response->toJsonArray();
var_dump(is_array($data));

// httpbin.org/post 会回显请求信息
// 验证自定义头部被发送
var_dump(isset($data['headers']['X-Custom-Header']));

// 验证 JSON 请求体被发送
var_dump(isset($data['json']['key']));

// 测试全局头部
$curl2 = new XHCurl();
$curl2->setTimeout(10);
$curl2->setVerifySsl(false);
$curl2->setGlobalHeader('X-Global-Header', 'global-value');

$request2 = new XHRequest('https://httpbin.org/get');
$response2 = $curl2->exec($request2);
$data2 = $response2->toJsonArray();
var_dump(isset($data2['headers']['X-Global-Header']));

// 测试全局 Cookie
$curl3 = new XHCurl();
$curl3->setTimeout(10);
$curl3->setVerifySsl(false);
$curl3->setGlobalCookie('global-cookie', 'global-value');

$request3 = new XHRequest('https://httpbin.org/cookies');
$response3 = $curl3->exec($request3);
$data3 = $response3->toJsonArray();
var_dump(isset($data3['cookies']['global-cookie']));

echo "Streaming and config test passed!\n";
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
Streaming and config test passed!
