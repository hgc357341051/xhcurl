--TEST--
XHCurl 重试机制测试
--SKIPIF--
<?php
if (!extension_loaded('xhcurl')) echo 'skip xhcurl extension not loaded';
?>
--FILE--
<?php
// 创建全局管理器
$curl = new XHCurl();

// 测试 setRetry 方法可调用性
// 设置重试 3 次，间隔 50ms
$curl->setRetry(3, 50);
var_dump(true);  // 若方法不存在会报 Fatal error

// 测试仅传必填参数（delayMs 使用默认值）
$curl->setRetry(2);
var_dump(true);

// 测试禁用重试
$curl->setRetry(0);
var_dump(true);

// 测试对不存在的主机发起请求，验证重试不会导致崩溃
// 使用一个保证无法解析的域名
$curl->setRetry(2, 10);  // 重试 2 次，间隔 10ms 加快测试速度
$curl->setConnectTimeout(2);  // 连接超时 2 秒
$curl->setTimeout(3);

$request = new XHRequest('https://nonexistent-domain-xyz-invalid-12345.com/');
$response = $curl->exec($request);

// 验证：请求应失败（网络错误），但不崩溃
// 错误信息应不为 null
var_dump($response->getError() !== null);
// 状态码应为 0（未收到 HTTP 响应）
var_dump($response->getStatusCode() === 0);

// 测试对返回 5xx 的接口重试（httpbin 提供 /status/500 模拟 500 错误）
$curl->setRetry(1, 10);
$curl->setConnectTimeout(5);
$curl->setTimeout(10);

$request2 = new XHRequest('https://httpbin.org/status/500');
$response2 = $curl->exec($request2);

// 验证：最终状态码应为 500（重试后仍为 500）
var_dump($response2->getStatusCode() === 500);

echo "Retry test passed!\n";
?>
--EXPECTF--
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
Retry test passed!
