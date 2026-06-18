--TEST--
XHResponse 懒加载和内存安全测试
--SKIPIF--
<?php if (!extension_loaded('xhcurl')) echo 'skip xhcurl extension not loaded'; ?>
--FILE--
<?php
// 创建全局管理器
$curl = new XHCurl();
$curl->setTimeout(10);
$curl->setVerifySsl(false);

// 测试响应体分段读取
$request = new XHRequest('https://httpbin.org/get');
$response = $curl->exec($request);

// 获取总长度
$totalLen = $response->getBodyLength();
var_dump($totalLen > 0);

// 分段读取（每次 100 字节）
$offset = 0;
$chunkSize = 100;
$readTotal = 0;
while ($offset < $totalLen) {
    $chunk = $response->getBodyChunk($offset, $chunkSize);
    $readLen = strlen($chunk);
    if ($readLen === 0) break;
    $readTotal += $readLen;
    $offset += $chunkSize;
}

// 验证分段读取的总字节数与实际长度一致
var_dump($readTotal === $totalLen);

// 测试越界读取
$overChunk = $response->getBodyChunk($totalLen + 100, 100);
var_dump($overChunk === '');

// 测试负偏移量
$negChunk = $response->getBodyChunk(-1, 100);
var_dump($negChunk === '');

// 测试头部按需获取（不一次性加载所有头部）
$ct = $response->getHeader('content-type');
var_dump($ct !== null);

// 测试不存在的头部
$noHeader = $response->getHeader('x-nonexistent-header');
var_dump($noHeader === null);

// 测试 hasHeader
var_dump($response->hasHeader('content-type'));
var_dump(!$response->hasHeader('x-nonexistent-header'));

// 测试最大响应体大小限制
$curlLimited = new XHCurl();
$curlLimited->setTimeout(10);
$curlLimited->setVerifySsl(false);
$curlLimited->setMaxResponseSize(100); // 设置极小限制

$requestLimited = new XHRequest('https://httpbin.org/get');
$responseLimited = $curlLimited->exec($requestLimited);

// 超过限制的请求应该返回错误
var_dump($responseLimited->getError() !== null);

echo "Lazy loading and memory safety test passed!\n";
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
Lazy loading and memory safety test passed!
