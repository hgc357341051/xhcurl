--TEST--
XHCurl 内存泄漏检测测试
--SKIPIF--
<?php
if (!extension_loaded('xhcurl')) echo 'skip xhcurl extension not loaded';
?>
--FILE--
<?php
// 内存泄漏检测：通过多次循环请求，验证内存使用量稳定不增长
// 注意：PHP 内部有内存池，可能存在一次性分配的缓存，因此允许小幅波动

// 创建全局管理器（循环外创建，复用连接池）
$curl = new XHCurl();
$curl->setTimeout(10);
$curl->setConnectTimeout(5);
$curl->setVerifySsl(false);
$curl->setRetry(1, 50);  // 启用 1 次重试，同时验证重试路径无泄漏

// 初始内存使用量（字节）
$initialMemory = memory_get_usage(true);

// 执行 20 次请求
$iterations = 20;
for ($i = 0; $i < $iterations; $i++) {
    $request = new XHRequest('https://httpbin.org/get');
    $response = $curl->exec($request);

    // 显式释放响应对象引用，触发 PHP GC 回收
    unset($request);
    unset($response);
}

// 执行完毕后的内存使用量
$finalMemory = memory_get_usage(true);

// 计算内存增长量
$memoryGrowth = $finalMemory - $initialMemory;

// 输出内存使用情况（用于调试）
echo "Initial memory: " . $initialMemory . " bytes\n";
echo "Final memory:   " . $finalMemory . " bytes\n";
echo "Memory growth:  " . $memoryGrowth . " bytes\n";

// 验证：内存增长应小于 2MB（允许 PHP 内部缓存的波动）
// 如果存在严重泄漏，20 次请求后内存增长会远超此值
var_dump($memoryGrowth < 2 * 1024 * 1024);

// 第二轮：测试 JSON 解析路径的内存泄漏
$initialMemory2 = memory_get_usage(true);
for ($i = 0; $i < $iterations; $i++) {
    $request = new XHRequest('https://httpbin.org/json');
    $response = $curl->exec($request);
    // 触发 JSON 解析（验证 json_decode 缓存路径无泄漏）
    $data = $response->toJsonArray();
    unset($request, $response, $data);
}
$finalMemory2 = memory_get_usage(true);
$memoryGrowth2 = $finalMemory2 - $initialMemory2;
echo "JSON path memory growth: " . $memoryGrowth2 . " bytes\n";
var_dump($memoryGrowth2 < 2 * 1024 * 1024);

// 第三轮：测试流式回调路径的内存泄漏
$initialMemory3 = memory_get_usage(true);
for ($i = 0; $i < $iterations; $i++) {
    $received = '';
    $request = new XHRequest('https://httpbin.org/stream/3');
    $request->onChunk(function($data) use (&$received) {
        $received .= $data;
    });
    $response = $curl->exec($request);
    unset($request, $response, $received);
}
$finalMemory3 = memory_get_usage(true);
$memoryGrowth3 = $finalMemory3 - $initialMemory3;
echo "Streaming path memory growth: " . $memoryGrowth3 . " bytes\n";
var_dump($memoryGrowth3 < 2 * 1024 * 1024);

echo "Memory leak test passed!\n";
?>
--EXPECTF--
Initial memory: %d bytes
Final memory:   %d bytes
Memory growth:  %d bytes
bool(true)
JSON path memory growth: %d bytes
bool(true)
Streaming path memory growth: %d bytes
bool(true)
Memory leak test passed!
