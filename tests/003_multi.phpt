--TEST--
XHMulti 批量异步请求测试
--SKIPIF--
<?php if (!extension_loaded('xhcurl')) echo 'skip xhcurl extension not loaded'; ?>
--FILE--
<?php
// 创建全局管理器
$curl = new XHCurl();
$curl->setTimeout(10);
$curl->setVerifySsl(false);

// 创建批量执行器
$multi = new XHMulti($curl);

// 添加多个请求
$request1 = new XHRequest('https://httpbin.org/get');
$request2 = new XHRequest('https://httpbin.org/get');
$request3 = new XHRequest('https://httpbin.org/get');

$multi->add($request1);
$multi->add($request2);
$multi->add($request3);

// 测试请求数量
var_dump($multi->count() === 3);

// 执行批量请求
$responses = $multi->execute();

// 测试返回数组
var_dump(is_array($responses));
var_dump(count($responses) === 3);

// 测试每个响应
foreach ($responses as $i => $response) {
    var_dump($response instanceof XHResponse);
    var_dump($response->getStatusCode() === 200);
}

// 执行后请求数量应重置
var_dump($multi->count() === 0);

echo "Multi test passed!\n";
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
Multi test passed!
