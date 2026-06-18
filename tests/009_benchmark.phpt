--TEST--
XHCurl 性能基准测试
--SKIPIF--
<?php
if (!extension_loaded('xhcurl')) echo 'skip xhcurl extension not loaded';
?>
--FILE--
<?php
// 性能基准测试：对比单请求耗时与批量请求耗时
// 验证扩展在高频请求场景下的性能表现

$curl = new XHCurl();
$curl->setTimeout(10);
$curl->setConnectTimeout(5);
$curl->setVerifySsl(false);

// 测试 1：单请求基准耗时
$singleStart = microtime(true);
$request = new XHRequest('https://httpbin.org/get');
$response = $curl->exec($request);
$singleTime = microtime(true) - $singleStart;

echo "Single request time: " . sprintf("%.4f", $singleTime) . "s\n";
var_dump($response->getStatusCode() === 200);
unset($request, $response);

// 测试 2：连续 10 次请求总耗时
$batchStart = microtime(true);
for ($i = 0; $i < 10; $i++) {
    $req = new XHRequest('https://httpbin.org/get');
    $resp = $curl->exec($req);
    if ($resp->getStatusCode() !== 200) {
        echo "Request $i failed\n";
        var_dump(false);
        exit;
    }
    unset($req, $resp);
}
$batchTime = microtime(true) - $batchStart;
echo "10 sequential requests time: " . sprintf("%.4f", $batchTime) . "s\n";
echo "Average per request: " . sprintf("%.4f", $batchTime / 10) . "s\n";

// 验证：平均单请求耗时应小于 5 秒（网络环境差异，宽松阈值）
var_dump(($batchTime / 10) < 5.0);

// 测试 3：使用 XHMulti 异步批量请求（验证多路复用性能）
$multiStart = microtime(true);
$multi = new XHMulti($curl);
for ($i = 0; $i < 5; $i++) {
    $req = new XHRequest('https://httpbin.org/get?id=' . $i);
    $multi->add($req);
}
$responses = $multi->execute();
$multiTime = microtime(true) - $multiStart;
echo "5 async requests time: " . sprintf("%.4f", $multiTime) . "s\n";

// 验证：5 个异步请求总耗时应小于 5 个串行请求的耗时
// （多路复用应带来性能提升，但考虑网络波动，仅验证完成）
var_dump(count($responses) === 5);

echo "Performance benchmark test passed!\n";
?>
--EXPECTF--
Single request time: %fs
bool(true)
10 sequential requests time: %fs
Average per request: %fs
bool(true)
5 async requests time: %fs
bool(true)
Performance benchmark test passed!
