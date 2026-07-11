<?php
// +----------------------------------------------------------------------+
// | XHCurl 第四轮测试：API 对称、Cookie 安全与 Getter 补全                   |
// |                                                                        |
// | 验证 align-threadpool-api-and-cookie-safety spec：                       |
// |   Task 1-3: XHThreadPool 新增 timeout/maxResponseSize/maxConcurrency    |
// |   Task 4:   cookies(array) 对 value URL 编码（防注入）                   |
// |   Task 5:   getTimeoutMs/getConnectTimeoutMs                            |
// |   Task 6:   XHMulti/XHThreadPool 批次配置 getter                          |
// |                                                                        |
// | 注意：涉及 /hang 的超时测试放最后，避免阻塞 mock 服务器进程影响其他测试。  |
// +----------------------------------------------------------------------+

$BASE = 'http://127.0.0.1:18399';

$pass = 0;
$fail = 0;
function check($name, $cond) {
    global $pass, $fail;
    if ($cond) {
        echo "[PASS] $name\n";
        $pass++;
    } else {
        echo "[FAIL] $name\n";
        $fail++;
    }
}

echo "=== 第四轮：API 对称、Cookie 安全与 Getter 补全 ===\n";

// ==================================================================
// Task 4: cookies(array) 对 value URL 编码（防注入）
//    php_ext.rs cookies() 数组分支用 form_urlencoded::byte_serialize
//    对 value 编码后再拼接，防止含 ;/= 的 value 破坏 Cookie 格式。
// ==================================================================
echo "\n=== Task 4: cookies(array) URL 编码（防注入）===\n";

// cookie value 含分号和等号（注入攻击场景）
// 期望：value 被编码，不会注入第二个 cookie
$result = XHCurl::createRequest($BASE . '/cookies')
    ->get()
    ->cookies(array('user' => 'a; admin=1'))
    ->timeout(10)
    ->execute();
check("cookies 注入测试 execute success", $result['success'] === true);
$body = json_decode($result['body'], true);
$cookieHeader = $body['cookie_header'] ?? '';
// 编码后应为 user=a%3B+admin%3D1，而非 user=a; admin=1（后者会注入第二个 cookie）
check("cookies value 含 ; 被编码", strpos($cookieHeader, 'a%3B') !== false || strpos($cookieHeader, 'a%3b') !== false);
check("cookies 不注入伪造 cookie", strpos($cookieHeader, 'admin=1') === false || strpos($cookieHeader, 'admin=1') === false && strpos($cookieHeader, '%3D1') !== false);

// 正常字母数字 value 不受影响
$result = XHCurl::createRequest($BASE . '/cookies')
    ->get()
    ->cookies(array('session' => 'abc123', 'token' => 'xyz789'))
    ->timeout(10)
    ->execute();
$body = json_decode($result['body'], true);
$cookieHeader = $body['cookie_header'] ?? '';
check("cookies 正常字母数字不受影响", strpos($cookieHeader, 'session=abc123') !== false && strpos($cookieHeader, 'token=xyz789') !== false);

// 整型 value（标量转换后编码，数字不变）
$result = XHCurl::createRequest($BASE . '/cookies')
    ->get()
    ->cookies(array('count' => 42))
    ->timeout(10)
    ->execute();
$body = json_decode($result['body'], true);
$cookieHeader = $body['cookie_header'] ?? '';
check("cookies 整型 value 正常", strpos($cookieHeader, 'count=42') !== false);

// 布尔 value（true → "1"）
$result = XHCurl::createRequest($BASE . '/cookies')
    ->get()
    ->cookies(array('flag' => true))
    ->timeout(10)
    ->execute();
$body = json_decode($result['body'], true);
$cookieHeader = $body['cookie_header'] ?? '';
check("cookies 布尔 true → 1", strpos($cookieHeader, 'flag=1') !== false);

// 字符串形式 cookies（向后兼容，不编码）
$result = XHCurl::createRequest($BASE . '/cookies')
    ->get()
    ->cookies('raw=value; direct=true')
    ->timeout(10)
    ->execute();
check("cookies 字符串形式向后兼容", $result['success'] === true);

// 数组/对象值仍抛异常（不被编码"救活"）
$caughtArrayValue = false;
try {
    XHCurl::createRequest($BASE . '/cookies')
        ->get()
        ->cookies(array('bad' => array('nested' => 'value')));
} catch (\Throwable $e) {
    $caughtArrayValue = true;
}
check("cookies 数组值仍抛异常", $caughtArrayValue);

// ==================================================================
// Task 5: getTimeoutMs/getConnectTimeoutMs
//    PhpXhRequest 新增毫秒级 getter，与 setter 对称。
// ==================================================================
echo "\n=== Task 5: getTimeoutMs/getConnectTimeoutMs ===\n";

// timeoutMs 设置后 getTimeoutMs 返回设置值
$req = XHCurl::createRequest($BASE . '/get')->get()->timeoutMs(500);
$ms = $req->getTimeoutMs();
check("getTimeoutMs 返回设置值 500", $ms === 500);

// 未设置返回 null
$req2 = XHCurl::createRequest($BASE . '/get')->get();
$ms2 = $req2->getTimeoutMs();
check("getTimeoutMs 未设置返回 null", $ms2 === null);

// connectTimeoutMs 设置后 getConnectTimeoutMs 返回设置值
$req3 = XHCurl::createRequest($BASE . '/get')->get()->connectTimeoutMs(300);
$ms3 = $req3->getConnectTimeoutMs();
check("getConnectTimeoutMs 返回设置值 300", $ms3 === 300);

// 未设置返回 null
$req4 = XHCurl::createRequest($BASE . '/get')->get();
$ms4 = $req4->getConnectTimeoutMs();
check("getConnectTimeoutMs 未设置返回 null", $ms4 === null);

// 秒级 getter 不返回毫秒值（单位独立）
$req5 = XHCurl::createRequest($BASE . '/get')->get()->timeoutMs(500);
$secs = $req5->getTimeout();
check("getTimeout 不返回毫秒值（单位独立）", $secs === null);

// ==================================================================
// Task 6: XHMulti/XHThreadPool 批次配置 getter
//    XHMulti 和 XHThreadPool 各新增 getMaxConcurrency/getMaxResponseSize/getTimeout
// ==================================================================
echo "\n=== Task 6: XHMulti/XHThreadPool 批次配置 getter ===\n";

// XHMulti getter
$multi = new XHMulti();
check("XHMulti getMaxConcurrency 默认 0", $multi->getMaxConcurrency() === 0);
check("XHMulti getMaxResponseSize 默认 0", $multi->getMaxResponseSize() === 0);
check("XHMulti getTimeout 默认 0", $multi->getTimeout() === 0);

$multi->maxConcurrency(10)->maxResponseSize(5000000)->timeout(30);
check("XHMulti getMaxConcurrency 返回 10", $multi->getMaxConcurrency() === 10);
check("XHMulti getMaxResponseSize 返回 5000000", $multi->getMaxResponseSize() === 5000000);
check("XHMulti getTimeout 返回 30", $multi->getTimeout() === 30);

// XHThreadPool getter
$pool = new XHThreadPool(4);
check("XHThreadPool getMaxConcurrency 默认 0（workers 在构造函数设置）", $pool->getMaxConcurrency() === 0 || $pool->getMaxConcurrency() === 4);
check("XHThreadPool getMaxResponseSize 默认 0", $pool->getMaxResponseSize() === 0);
check("XHThreadPool getTimeout 默认 0", $pool->getTimeout() === 0);

$pool->maxConcurrency(8)->maxResponseSize(2000000)->timeout(60);
check("XHThreadPool getMaxConcurrency 返回 8", $pool->getMaxConcurrency() === 8);
check("XHThreadPool getMaxResponseSize 返回 2000000", $pool->getMaxResponseSize() === 2000000);
check("XHThreadPool getTimeout 返回 60", $pool->getTimeout() === 60);

// ==================================================================
// Task 1-3: XHThreadPool 新增 setter 链式调用 + 行为生效
// ==================================================================
echo "\n=== Task 1-3: XHThreadPool timeout/maxResponseSize/maxConcurrency ===\n";

// 链式调用不抛异常
$chainOk = true;
try {
    $pool2 = new XHThreadPool(2);
    $pool2->timeout(30)->maxResponseSize(1000000)->maxConcurrency(4);
} catch (\Throwable $e) {
    $chainOk = false;
}
check("XHThreadPool 链式调用不抛异常", $chainOk);

// 负值跳过（不修改原值）
$pool3 = new XHThreadPool(2);
$pool3->maxConcurrency(5);
$pool3->maxConcurrency(-1);  // 负值应跳过
check("XHThreadPool maxConcurrency 负值跳过", $pool3->getMaxConcurrency() === 5);

$pool3->timeout(30);
$pool3->timeout(-1);
check("XHThreadPool timeout 负值跳过", $pool3->getTimeout() === 30);

// 0 值表示"无超时/使用默认"
$pool4 = new XHThreadPool(2);
$pool4->timeout(0);
check("XHThreadPool timeout(0) 表示无超时", $pool4->getTimeout() === 0);

// maxResponseSize 0 表示使用全局默认
$pool5 = new XHThreadPool(2);
$pool5->maxResponseSize(0);
check("XHThreadPool maxResponseSize(0) 表示使用全局默认", $pool5->getMaxResponseSize() === 0);

// 实际执行：正常请求成功（setter 不破坏执行）
$pool6 = new XHThreadPool(2);
$pool6->add(XHCurl::createRequest($BASE . '/get?id=1')->get()->timeout(10));
$pool6->add(XHCurl::createRequest($BASE . '/get?id=2')->get()->timeout(10));
$pool6->maxConcurrency(2)->timeout(30);
$results = $pool6->execute();
check("XHThreadPool 设置 setter 后 execute 成功", is_array($results) && count($results) === 2);
if (is_array($results) && count($results) >= 2) {
    $allSuccess = $results[0]['success'] === true && $results[1]['success'] === true;
    check("XHThreadPool execute 结果全部 success", $allSuccess);
} else {
    check("XHThreadPool execute 结果全部 success", false);
}

// ==================================================================
// Task 2: maxResponseSize 生效（响应超限返回失败）
//    设置很小的 maxResponseSize，请求 /stream 大响应，应失败。
// ==================================================================
echo "\n=== Task 2: maxResponseSize 生效 ===\n";

$pool7 = new XHThreadPool(1);
$pool7->add(XHCurl::createRequest($BASE . '/stream?n=10&size=1024')->get()->timeout(10));
$pool7->maxResponseSize(100);  // 100 字节，远小于响应
$results = $pool7->execute();
if (is_array($results) && count($results) >= 1) {
    check("maxResponseSize 超限返回 success=false", $results[0]['success'] === false);
} else {
    check("maxResponseSize 超限返回 success=false", false);
}

// maxResponseSize 足够大时正常
$pool8 = new XHThreadPool(1);
$pool8->add(XHCurl::createRequest($BASE . '/get')->get()->timeout(10));
$pool8->maxResponseSize(1000000);  // 1MB，足够
$results = $pool8->execute();
if (is_array($results) && count($results) >= 1) {
    check("maxResponseSize 足够大时正常 success=true", $results[0]['success'] === true);
} else {
    check("maxResponseSize 足够大时正常 success=true", false);
}

// ==================================================================
// Task 1: timeout 生效（批量超时后中止）
//    设置很短的 timeout + 添加 /hang 请求，execute 应在超时后返回错误。
//    注意：/hang 端点会阻塞 mock 服务器进程 60 秒，此测试放最后。
// ==================================================================
echo "\n=== Task 1: timeout 生效（最后执行，避免阻塞）===\n";

$pool9 = new XHThreadPool(1);
$pool9->add(XHCurl::createRequest($BASE . '/hang')->get()->timeout(30));
$pool9->timeout(2);  // 2 秒批量超时
$startTime = microtime(true);
$timeoutExceptionCaught = false;
try {
    $pool9->execute();
} catch (\Throwable $e) {
    $timeoutExceptionCaught = true;
}
$elapsed = microtime(true) - $startTime;
// 批量超时应抛异常，且 elapsed 不应等满 30 秒（约 2 秒中止）
check("XHThreadPool timeout 抛异常", $timeoutExceptionCaught);
check("XHThreadPool timeout 中止执行（elapsed < 10s）", $elapsed < 10);

echo "\n总计: $pass passed, $fail failed\n";
exit($fail > 0 ? 1 : 0);
