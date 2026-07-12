<?php
// +----------------------------------------------------------------------+
// | XHCurl v1.6.0 - retry() 与 clone 测试                                |
// |                                                                      |
// | 覆盖：                                                                |
// | 1. retry() 重试机制（默认不重试/网络错误重试/HTTP 错误不重试/负值异常）|
// | 2. clone 深拷贝（独立修改/保留配置/链式调用）                         |
// | 3. withOptions 集成 retry key                                         |
// +----------------------------------------------------------------------+

echo "=== retry() 与 clone 测试 ===\n\n";

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

// 清理 flaky 计数器
foreach (glob('/tmp/xhcurl_flaky_*.count') as $f) @unlink($f);

// ==================================================================
// 1. retry() 测试
// ==================================================================
echo "=== 1. retry() 重试机制 ===\n";

// 1.1 retry(0) 默认不重试，attempts=1
function test_retry_zero_no_retry(): bool {
    $r = XHCurl::createRequest('http://127.0.0.1:18399/get')
        ->get()
        ->retry(0)
        ->execute();
    return $r['success'] === true && $r['attempts'] === 1;
}
check("1.1 retry(0) 默认不重试，attempts=1", test_retry_zero_no_retry());

// 1.2 retry(2) + 网络错误（超时）→ 重试后仍失败，attempts=3
function test_retry_network_error_retries(): bool {
    $r = XHCurl::createRequest('http://127.0.0.1:18400/hang')  // socat /hang
        ->get()
        ->timeout(1)       // 1 秒超时触发网络错误
        ->retry(2, 10)     // 重试 2 次，间隔 10ms
        ->execute();
    // 超时是网络错误（status=0），会重试。3 次尝试都超时，最终失败。
    return $r['success'] === false && $r['attempts'] === 3;
}
check("1.2 retry(2) + 网络错误重试后仍失败，attempts=3", test_retry_network_error_retries());

// 1.3 retry(2) + 正常请求 → 成功，attempts=1（无失败无需重试）
function test_retry_success_no_retry(): bool {
    $r = XHCurl::createRequest('http://127.0.0.1:18399/get')
        ->get()
        ->retry(2, 10)
        ->execute();
    return $r['success'] === true && $r['attempts'] === 1;
}
check("1.3 retry(2) + 正常请求 attempts=1", test_retry_success_no_retry());

// 1.4 retry(3) + /flaky?fail=0（直接 200）→ attempts=1
function test_retry_flaky_fail0(): bool {
    @unlink('/tmp/xhcurl_flaky_0.count');
    $r = XHCurl::createRequest('http://127.0.0.1:18399/flaky?fail=0')
        ->get()
        ->retry(3, 10)
        ->execute();
    return $r['success'] === true && $r['attempts'] === 1;
}
check("1.4 retry(3) + /flaky?fail=0 直接成功 attempts=1", test_retry_flaky_fail0());

// 1.5 retry(3) + /flaky?fail=2（返回 503）→ 不重试（HTTP 错误），attempts=1
function test_retry_http_error_no_retry(): bool {
    @unlink('/tmp/xhcurl_flaky_2.count');
    $r = XHCurl::createRequest('http://127.0.0.1:18399/flaky?fail=2')
        ->get()
        ->retry(3, 10)
        ->execute();
    // 503 是 HTTP 错误（服务器已响应），不重试。attempts=1，status=503。
    return $r['success'] === false && $r['attempts'] === 1 && $r['status'] === 503;
}
check("1.5 retry(3) + /flaky?fail=2 (503) 不重试 attempts=1", test_retry_http_error_no_retry());

// 1.6 retry(-1) 抛异常
function test_retry_negative_times_throws(): bool {
    try {
        XHCurl::createRequest('http://127.0.0.1:18399/get')
            ->get()
            ->retry(-1)
            ->execute();
        return false;
    } catch (Throwable $e) {
        return true;
    }
}
check("1.6 retry(-1) 抛异常", test_retry_negative_times_throws());

// 1.7 retry(1, -100) 抛异常
function test_retry_negative_delay_throws(): bool {
    try {
        XHCurl::createRequest('http://127.0.0.1:18399/get')
            ->get()
            ->retry(1, -100)
            ->execute();
        return false;
    } catch (Throwable $e) {
        return true;
    }
}
check("1.7 retry(1, -100) 抛异常", test_retry_negative_delay_throws());

// 1.8 retry(1, 50) + 正常请求 → delay 不影响成功路径耗时
function test_retry_delay_no_affect_success(): bool {
    $start = microtime(true);
    $r = XHCurl::createRequest('http://127.0.0.1:18399/get')
        ->get()
        ->retry(1, 50)
        ->execute();
    $elapsed = microtime(true) - $start;
    // 成功路径不触发延迟（delay 仅在重试时生效），耗时远小于 50ms
    return $r['success'] === true && $r['attempts'] === 1 && $elapsed < 0.05;
}
check("1.8 retry(1, 50) + 正常请求 delay 不影响成功路径", test_retry_delay_no_affect_success());

// 1.9 executeJson() + retry(2) → 成功路径 attempts=1
function test_execute_json_with_retry(): bool {
    $r = XHCurl::createRequest('http://127.0.0.1:18399/echo-json')
        ->get()
        ->retry(2, 10)
        ->executeJson();
    // /echo-json 返回 {"received": true, "method": "GET"}
    return is_array($r) && isset($r['received']);
}
check("1.9 executeJson() + retry(2) 成功", test_execute_json_with_retry());

// ==================================================================
// 2. clone 测试
// ==================================================================
echo "\n=== 2. clone 深拷贝 ===\n";

// 2.1 clone $req → 修改克隆对象不影响原对象
function test_clone_independent_modification(): bool {
    $req1 = XHCurl::createRequest('http://127.0.0.1:18399/get')
        ->get()
        ->header('X-Test', 'original');
    $req2 = clone $req1;
    $req2->header('X-Test', 'modified');
    // 执行 req1，验证 header 未被 clone 修改影响
    $r1 = $req1->execute();
    if (!$r1['success']) return false;
    // 通过 /echo-attempts 验证 headers
    $req1b = XHCurl::createRequest('http://127.0.0.1:18399/echo-attempts')
        ->get()
        ->header('X-Test', 'original');
    $req2b = clone $req1b;
    $req2b->header('X-Test', 'modified');
    $r1b = $req1b->execute();
    $r2b = $req2b->execute();
    if (!$r1b['success'] || !$r2b['success']) return false;
    $b1 = json_decode($r1b['body'], true);
    $b2 = json_decode($r2b['body'], true);
    $h1 = [];
    $h2 = [];
    foreach ($b1['headers'] as $name => $value) {
        if (strcasecmp($name, 'X-Test') === 0) $h1[] = $value;
    }
    foreach ($b2['headers'] as $name => $value) {
        if (strcasecmp($name, 'X-Test') === 0) $h2[] = $value;
    }
    return $h1 === ['original'] && $h2 === ['modified'];
}
check("2.1 clone 修改克隆不影响原对象", test_clone_independent_modification());

// 2.2 clone $req → 保留所有配置（headers/body/timeout/retry）
function test_clone_preserves_config(): bool {
    $template = XHCurl::createRequest('http://127.0.0.1:18399/echo-attempts')
        ->get()
        ->header('Authorization', 'Bearer token123')
        ->timeout(30)
        ->retry(2, 100);
    $clone = clone $template;
    // 验证 clone 保留了 retry 配置
    $retry = $clone->getRetry();
    if (!is_array($retry) || $retry['times'] !== 2 || $retry['delay_ms'] !== 100) return false;
    // 验证 clone 保留了 header
    $r = $clone->execute();
    if (!$r['success']) return false;
    $body = json_decode($r['body'], true);
    $hasAuth = false;
    foreach ($body['headers'] as $name => $value) {
        if (strcasecmp($name, 'Authorization') === 0 && $value === 'Bearer token123') $hasAuth = true;
    }
    return $hasAuth;
}
check("2.2 clone 保留所有配置", test_clone_preserves_config());

// 2.3 clone 后链式调用 → 正常工作
function test_clone_then_chain(): bool {
    $template = XHCurl::createRequest('http://127.0.0.1:18399/echo-attempts')
        ->get()
        ->header('X-Template', 'yes');
    $r = (clone $template)
        ->header('X-Chain', 'appended')
        ->execute();
    if (!$r['success']) return false;
    $body = json_decode($r['body'], true);
    $hasTemplate = $hasChain = false;
    foreach ($body['headers'] as $name => $value) {
        if (strcasecmp($name, 'X-Template') === 0) $hasTemplate = true;
        if (strcasecmp($name, 'X-Chain') === 0) $hasChain = true;
    }
    return $hasTemplate && $hasChain;
}
check("2.3 clone 后链式调用正常", test_clone_then_chain());

// ==================================================================
// 3. withOptions 集成 retry
// ==================================================================
echo "\n=== 3. withOptions 集成 retry ===\n";

// 3.1 withOptions(['retry' => ['times' => 2, 'delay_ms' => 100]]) 等价 retry(2, 100)
function test_with_options_retry(): bool {
    $req = XHCurl::createRequest('http://127.0.0.1:18399/get')
        ->get()
        ->withOptions(['retry' => ['times' => 2, 'delay_ms' => 100]]);
    $retry = $req->getRetry();
    return is_array($retry) && $retry['times'] === 2 && $retry['delay_ms'] === 100;
}
check("3.1 withOptions retry 等价 retry()", test_with_options_retry());

// 3.2 withOptions(['retry' => ['times' => 2]]) delay_ms 默认 0
function test_with_options_retry_default_delay(): bool {
    $req = XHCurl::createRequest('http://127.0.0.1:18399/get')
        ->get()
        ->withOptions(['retry' => ['times' => 2]]);
    $retry = $req->getRetry();
    return is_array($retry) && $retry['times'] === 2 && $retry['delay_ms'] === 0;
}
check("3.2 withOptions retry delay_ms 默认 0", test_with_options_retry_default_delay());

// 3.3 withOptions(['retry' => ['times' => -1]]) 抛异常
function test_with_options_retry_negative_throws(): bool {
    try {
        XHCurl::createRequest('http://127.0.0.1:18399/get')
            ->get()
            ->withOptions(['retry' => ['times' => -1]])
            ->execute();
        return false;
    } catch (Throwable $e) {
        return true;
    }
}
check("3.3 withOptions retry times 负值抛异常", test_with_options_retry_negative_throws());

// 3.4 withOptions(['retry' => 'invalid']) 抛异常（非数组）
function test_with_options_retry_not_array_throws(): bool {
    try {
        XHCurl::createRequest('http://127.0.0.1:18399/get')
            ->get()
            ->withOptions(['retry' => 'invalid'])
            ->execute();
        return false;
    } catch (Throwable $e) {
        return true;
    }
}
check("3.4 withOptions retry 非数组抛异常", test_with_options_retry_not_array_throws());

// 3.5 withOptions(['retry' => null]) 跳过（保持原值）
function test_with_options_retry_null_skipped(): bool {
    $req = XHCurl::createRequest('http://127.0.0.1:18399/get')
        ->get()
        ->retry(3, 200)
        ->withOptions(['retry' => null]);
    $retry = $req->getRetry();
    return is_array($retry) && $retry['times'] === 3 && $retry['delay_ms'] === 200;
}
check("3.5 withOptions retry null 跳过保持原值", test_with_options_retry_null_skipped());

// ==================================================================
// 最终结果
// ==================================================================
echo "\n=== 测试结果: $pass 通过, $fail 失败 ===\n";
exit($fail > 0 ? 1 : 0);
