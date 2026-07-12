<?php
// +----------------------------------------------------------------------+
// | XHCurl v1.5.0 - withOptions() 与全局 base 配置测试                       |
// |                                                                        |
// | 覆盖本轮新增的 3 个功能：                                              |
// | 1. withOptions() 批量设置选项（多 key / 未知 key 抛异常 / null 跳过 /   |
// |    与链式 setter 混用 / headers null 跳过 / 多次调用累加）             |
// | 2. base_uri 全局基础 URL（相对 URL 拼接 / 绝对 URL 优先 / 末尾斜杠 /    |
// |    null 清除）                                                          |
// | 3. base_headers 全局基础头（自动携带 / 请求级覆盖 / null 清除）          |
// | 4. base_uri + base_headers + withOptions 组合使用                       |
// |                                                                        |
// | 注意：需 mock 服务器（127.0.0.1:18399）。每个 base_uri/base_headers       |
// |       测试在结束后立即 setConfig(null) 清除，避免污染后续测试。          |
// +----------------------------------------------------------------------+

echo "=== withOptions() 与全局 base 配置测试 ===\n\n";

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

// ==================================================================
// 1. withOptions() 测试
// ==================================================================
echo "=== 1. withOptions() 批量设置选项 ===\n";

// 1.1 withOptions 批量设置多个选项
function test_with_options_batch(): bool {
    $r = XHCurl::createRequest('http://127.0.0.1:18399/base-test')
        ->get()
        ->withOptions([
            'headers' => ['X-Custom' => 'test-value'],
            'query' => ['page' => '1'],
            'timeout' => 30,
        ])
        ->execute();
    if (!$r['success']) return false;
    $body = json_decode($r['body'], true);
    // 验证 headers 设置成功
    $hasCustom = false;
    foreach ($body['headers'] as $name => $value) {
        if (strcasecmp($name, 'X-Custom') === 0 && $value === 'test-value') $hasCustom = true;
    }
    // 验证 query 设置成功（URL 含 page=1）
    $hasQuery = strpos($body['url'], 'page=1') !== false;
    return $hasCustom && $hasQuery;
}
check("1.1 withOptions 批量设置多个选项", test_with_options_batch());

// 1.2 withOptions 未知 key 抛异常
function test_with_options_unknown_key_throws(): bool {
    $exception = false;
    try {
        XHCurl::createRequest('http://127.0.0.1:18399/get')
            ->get()
            ->withOptions(['timedout' => 30])  // 拼写错误
            ->execute();
    } catch (Throwable $e) {
        $exception = true;
    }
    return $exception;
}
check("1.2 withOptions 未知 key 抛异常", test_with_options_unknown_key_throws());

// 1.3 withOptions null 值跳过
function test_with_options_null_skipped(): bool {
    $exception = false;
    try {
        $r = XHCurl::createRequest('http://127.0.0.1:18399/get')
            ->get()
            ->withOptions(['timeout' => 30, 'proxy' => null])
            ->execute();
        // proxy=null 应跳过，不抛异常，请求成功
        return $r['success'] === true;
    } catch (Throwable $e) {
        return false;
    }
}
check("1.3 withOptions null 值跳过", test_with_options_null_skipped());

// 1.4 withOptions 与链式 setter 混用
function test_with_options_mixed_with_setters(): bool {
    $r = XHCurl::createRequest('http://127.0.0.1:18399/base-test')
        ->get()
        ->withOptions(['headers' => ['X-First' => 'a']])
        ->header('X-Second', 'b')  // 链式 setter 在 withOptions 之后
        ->execute();
    if (!$r['success']) return false;
    $body = json_decode($r['body'], true);
    $hasFirst = $hasSecond = false;
    foreach ($body['headers'] as $name => $value) {
        if (strcasecmp($name, 'X-First') === 0) $hasFirst = true;
        if (strcasecmp($name, 'X-Second') === 0) $hasSecond = true;
    }
    return $hasFirst && $hasSecond;
}
check("1.4 withOptions 与链式 setter 混用", test_with_options_mixed_with_setters());

// 1.5 withOptions headers 中 null 值跳过
function test_with_options_headers_null_skipped(): bool {
    $r = XHCurl::createRequest('http://127.0.0.1:18399/base-test')
        ->get()
        ->withOptions(['headers' => ['X-Keep' => 'yes', 'X-Skip' => null]])
        ->execute();
    if (!$r['success']) return false;
    $body = json_decode($r['body'], true);
    $hasKeep = false;
    $noSkip = true;
    foreach ($body['headers'] as $name => $value) {
        if (strcasecmp($name, 'X-Keep') === 0) $hasKeep = true;
        if (strcasecmp($name, 'X-Skip') === 0) $noSkip = false;
    }
    return $hasKeep && $noSkip;
}
check("1.5 withOptions headers 中 null 值跳过", test_with_options_headers_null_skipped());

// 1.6 withOptions 多次调用累加
function test_with_options_multiple_calls(): bool {
    $r = XHCurl::createRequest('http://127.0.0.1:18399/base-test')
        ->get()
        ->withOptions(['headers' => ['X-First' => 'a']])
        ->withOptions(['headers' => ['X-Second' => 'b']])
        ->execute();
    if (!$r['success']) return false;
    $body = json_decode($r['body'], true);
    $hasFirst = $hasSecond = false;
    foreach ($body['headers'] as $name => $value) {
        if (strcasecmp($name, 'X-First') === 0) $hasFirst = true;
        if (strcasecmp($name, 'X-Second') === 0) $hasSecond = true;
    }
    return $hasFirst && $hasSecond;
}
check("1.6 withOptions 多次调用累加", test_with_options_multiple_calls());

// ==================================================================
// 2. base_uri 测试
// ==================================================================
echo "\n=== 2. base_uri 全局基础 URL ===\n";

// 2.1 base_uri 相对 URL 拼接
function test_base_uri_relative_url(): bool {
    XHCurl::setConfig(['base_uri' => 'http://127.0.0.1:18399']);
    $r = XHCurl::createRequest('/base-test')->get()->execute();
    XHCurl::setConfig(['base_uri' => null]);  // 清除
    if (!$r['success']) return false;
    $body = json_decode($r['body'], true);
    // URL 应为 http://127.0.0.1:18399/base-test
    return strpos($body['url'], '/base-test') !== false;
}
check("2.1 base_uri 相对 URL 拼接", test_base_uri_relative_url());

// 2.2 base_uri 绝对 URL 优先
function test_base_uri_absolute_url_priority(): bool {
    XHCurl::setConfig(['base_uri' => 'http://127.0.0.1:18399']);
    $r = XHCurl::createRequest('http://127.0.0.1:18399/base-test')->get()->execute();
    XHCurl::setConfig(['base_uri' => null]);
    if (!$r['success']) return false;
    $body = json_decode($r['body'], true);
    return strpos($body['url'], '/base-test') !== false;
}
check("2.2 base_uri 绝对 URL 优先", test_base_uri_absolute_url_priority());

// 2.3 base_uri 末尾斜杠处理
function test_base_uri_trailing_slash(): bool {
    XHCurl::setConfig(['base_uri' => 'http://127.0.0.1:18399/']);  // 末尾有斜杠
    $r = XHCurl::createRequest('/base-test')->get()->execute();
    XHCurl::setConfig(['base_uri' => null]);
    if (!$r['success']) return false;
    $body = json_decode($r['body'], true);
    // URL 不应包含双斜杠（127.0.0.1:18399//base-test 错误）
    return strpos($body['url'], '//base-test') === false && strpos($body['url'], '/base-test') !== false;
}
check("2.3 base_uri 末尾斜杠处理", test_base_uri_trailing_slash());

// 2.4 base_uri 为 null 时清除
function test_base_uri_null_clears(): bool {
    XHCurl::setConfig(['base_uri' => 'http://127.0.0.1:18399']);
    XHCurl::setConfig(['base_uri' => null]);  // 清除
    // 清除后相对 URL 无 host，execute 返回 success=false（与 execute() 的"错误作为值"
    // 设计一致；createRequest 不抛异常因为相对 URL 在有 base_uri 时合法）
    $r = XHCurl::createRequest('/base-test')->get()->execute();
    return $r['success'] === false;
}
check("2.4 base_uri 为 null 时清除", test_base_uri_null_clears());

// ==================================================================
// 3. base_headers 测试
// ==================================================================
echo "\n=== 3. base_headers 全局基础头 ===\n";

// 3.1 base_headers 自动携带全局 header
function test_base_headers_auto_attach(): bool {
    XHCurl::setConfig(['base_headers' => ['X-Global' => 'global-value']]);
    $r = XHCurl::createRequest('http://127.0.0.1:18399/base-test')->get()->execute();
    XHCurl::setConfig(['base_headers' => null]);  // 清除
    if (!$r['success']) return false;
    $body = json_decode($r['body'], true);
    $hasGlobal = false;
    foreach ($body['headers'] as $name => $value) {
        if (strcasecmp($name, 'X-Global') === 0 && $value === 'global-value') $hasGlobal = true;
    }
    return $hasGlobal;
}
check("3.1 base_headers 自动携带全局 header", test_base_headers_auto_attach());

// 3.2 base_headers 请求级覆盖全局
function test_base_headers_request_overrides_global(): bool {
    XHCurl::setConfig(['base_headers' => ['X-Override' => 'global']]);
    $r = XHCurl::createRequest('http://127.0.0.1:18399/base-test')
        ->get()
        ->header('X-Override', 'request')
        ->execute();
    XHCurl::setConfig(['base_headers' => null]);
    if (!$r['success']) return false;
    $body = json_decode($r['body'], true);
    $overrideValue = null;
    foreach ($body['headers'] as $name => $value) {
        if (strcasecmp($name, 'X-Override') === 0) $overrideValue = $value;
    }
    return $overrideValue === 'request';
}
check("3.2 base_headers 请求级覆盖全局", test_base_headers_request_overrides_global());

// 3.3 base_headers 为 null 时清除
function test_base_headers_null_clears(): bool {
    XHCurl::setConfig(['base_headers' => ['X-Temp' => 'temp']]);
    XHCurl::setConfig(['base_headers' => null]);  // 清除
    $r = XHCurl::createRequest('http://127.0.0.1:18399/base-test')->get()->execute();
    if (!$r['success']) return false;
    $body = json_decode($r['body'], true);
    $hasTemp = false;
    foreach ($body['headers'] as $name => $value) {
        if (strcasecmp($name, 'X-Temp') === 0) $hasTemp = true;
    }
    return !$hasTemp;  // 清除后不应有 X-Temp
}
check("3.3 base_headers 为 null 时清除", test_base_headers_null_clears());

// ==================================================================
// 4. 组合测试
// ==================================================================
echo "\n=== 4. 组合测试 ===\n";

// 4.1 base_uri + base_headers 组合使用
function test_base_uri_and_headers_combined(): bool {
    XHCurl::setConfig([
        'base_uri' => 'http://127.0.0.1:18399',
        'base_headers' => ['X-Auth' => 'Bearer token123'],
    ]);
    $r = XHCurl::createRequest('/base-test')
        ->get()
        ->withOptions([
            'headers' => ['X-Trace' => 'trace-id'],
            'query' => ['user' => '42'],
        ])
        ->execute();
    XHCurl::setConfig(['base_uri' => null, 'base_headers' => null]);
    if (!$r['success']) return false;
    $body = json_decode($r['body'], true);
    // 验证 URL 拼接
    $urlOk = strpos($body['url'], '/base-test') !== false && strpos($body['url'], 'user=42') !== false;
    // 验证 headers（全局 + 请求级都在）
    $hasAuth = $hasTrace = false;
    foreach ($body['headers'] as $name => $value) {
        if (strcasecmp($name, 'X-Auth') === 0 && $value === 'Bearer token123') $hasAuth = true;
        if (strcasecmp($name, 'X-Trace') === 0 && $value === 'trace-id') $hasTrace = true;
    }
    return $urlOk && $hasAuth && $hasTrace;
}
check("4.1 base_uri + base_headers 组合使用", test_base_uri_and_headers_combined());

// ==================================================================
// 最终结果
// ==================================================================
echo "\n=== 测试结果: $pass 通过, $fail 失败 ===\n";
exit($fail > 0 ? 1 : 0);
