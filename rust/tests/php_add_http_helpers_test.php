<?php
// +----------------------------------------------------------------------+
// | XHCurl v1.4.0 - HTTP 便捷方法测试                                      |
// |                                                                        |
// | 覆盖本轮新增的 4 个方法：                                              |
// | 1. query() URL 查询参数构建（追加/合并/累加/标量转换/空数组/嵌套异常）  |
// | 2. accept() Accept header 设置                                         |
// | 3. contentType() Content-Type header 设置                              |
// | 4. executeJson() JSON 响应自动解析（成功/非JSON异常/请求失败异常）       |
// |                                                                        |
// | 注意：需 mock 服务器（127.0.0.1:18399）与 socat（18400）。              |
// +----------------------------------------------------------------------+

echo "=== HTTP 便捷方法测试 ===\n\n";

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

// 辅助：从 /post 回显的 headers 中按大小写不敏感方式取头值
function get_header_ci($headers, $name) {
    if (!is_array($headers)) {
        return null;
    }
    $name = strtolower($name);
    foreach ($headers as $k => $v) {
        if (strtolower($k) === $name) {
            return $v;
        }
    }
    return null;
}

// ==================================================================
// 1. query() 测试
// ==================================================================
echo "=== 1. query() 查询参数构建 ===\n";

// 1.1 query() 追加参数到无参数 URL
function test_query_append_to_clean_url(): bool {
    $r = XHCurl::createRequest('http://127.0.0.1:18399/echo-query')
        ->get()
        ->query(['page' => 1, 'limit' => '10'])
        ->execute();
    $body = json_decode($r['body'], true);
    return $r['success'] === true
        && isset($body['query']['page'])
        && $body['query']['page'] === '1'
        && $body['query']['limit'] === '10';
}
check("1.1 query() 追加参数到无参数 URL", test_query_append_to_clean_url());

// 1.2 query() 合并已有 URL 查询参数
function test_query_merge_with_existing(): bool {
    $r = XHCurl::createRequest('http://127.0.0.1:18399/echo-query?existing=1')
        ->get()
        ->query(['page' => '2'])
        ->execute();
    $body = json_decode($r['body'], true);
    return $r['success'] === true
        && $body['query']['existing'] === '1'
        && $body['query']['page'] === '2';
}
check("1.2 query() 合并已有 URL 查询参数", test_query_merge_with_existing());

// 1.3 query() 多次调用累加
function test_query_accumulate_multiple_calls(): bool {
    $r = XHCurl::createRequest('http://127.0.0.1:18399/echo-query')
        ->get()
        ->query(['a' => '1'])
        ->query(['b' => '2'])
        ->execute();
    $body = json_decode($r['body'], true);
    return $r['success'] === true
        && $body['query']['a'] === '1'
        && $body['query']['b'] === '2';
}
check("1.3 query() 多次调用累加", test_query_accumulate_multiple_calls());

// 1.4 query() 标量值转换
// bool true → "1"（或 "true"，Rust 实现可能不同）
// int 5 → "5"
// float 1.5 → "1.5"
// null → ""（或被跳过）
function test_query_scalar_conversion(): bool {
    $r = XHCurl::createRequest('http://127.0.0.1:18399/echo-query')
        ->get()
        ->query(['active' => true, 'count' => 5, 'rate' => 1.5, 'name' => null])
        ->execute();
    $body = json_decode($r['body'], true);
    if (!$r['success'] || !isset($body['query'])) {
        return false;
    }
    $q = $body['query'];
    $activeOk = isset($q['active']) && in_array($q['active'], ['1', 'true'], true);
    $countOk  = isset($q['count'])  && $q['count']  === '5';
    $rateOk   = isset($q['rate'])   && $q['rate']   === '1.5';
    // null 可能被转为空字符串，也可能被直接跳过
    $nameVal  = $q['name'] ?? null;
    $nameOk   = ($nameVal === '' || $nameVal === null || !array_key_exists('name', $q));
    return $activeOk && $countOk && $rateOk && $nameOk;
}
check("1.4 query() 标量值转换", test_query_scalar_conversion());

// 1.5 query([]) 空数组不抛异常
function test_query_empty_array_no_exception(): bool {
    $exception = false;
    try {
        XHCurl::createRequest('http://127.0.0.1:18399/echo-query')
            ->get()
            ->query([])
            ->execute();
    } catch (Throwable $e) {
        $exception = true;
    }
    return !$exception;
}
check("1.5 query([]) 空数组不抛异常", test_query_empty_array_no_exception());

// 1.6 query() 嵌套数组抛异常
function test_query_nested_array_throws(): bool {
    $exception = false;
    try {
        XHCurl::createRequest('http://127.0.0.1:18399/echo-query')
            ->get()
            ->query(['nested' => ['inner' => 'value']])
            ->execute();
    } catch (Throwable $e) {
        $exception = true;
    }
    return $exception;
}
check("1.6 query() 嵌套数组抛异常", test_query_nested_array_throws());

// ==================================================================
// 2. accept() 测试
// ==================================================================
echo "\n=== 2. accept() Accept header 设置 ===\n";

// 2.1 accept() 设置 Accept header（通过 /post 回显头验证）
function test_accept_sets_header(): bool {
    $r = XHCurl::createRequest('http://127.0.0.1:18399/post')
        ->get()
        ->accept('application/json')
        ->execute();
    if ($r['success'] !== true) {
        return false;
    }
    $body = json_decode($r['body'], true);
    if (!isset($body['headers']) || !is_array($body['headers'])) {
        return false;
    }
    $accept = get_header_ci($body['headers'], 'Accept');
    return $accept === 'application/json';
}
check("2.1 accept() 设置 Accept header", test_accept_sets_header());

// 2.2 accept('') 空字符串抛异常
function test_accept_empty_string_throws(): bool {
    $exception = false;
    try {
        XHCurl::createRequest('http://127.0.0.1:18399/get')
            ->get()
            ->accept('')
            ->execute();
    } catch (Throwable $e) {
        $exception = true;
    }
    return $exception;
}
check("2.2 accept('') 空字符串抛异常", test_accept_empty_string_throws());

// ==================================================================
// 3. contentType() 测试
// ==================================================================
echo "\n=== 3. contentType() Content-Type header 设置 ===\n";

// 3.1 contentType() 设置 Content-Type header（通过 /post 回显头验证）
function test_contenttype_sets_header(): bool {
    $r = XHCurl::createRequest('http://127.0.0.1:18399/post')
        ->post()
        ->contentType('application/xml')
        ->body('<xml/>')
        ->execute();
    if ($r['success'] !== true) {
        return false;
    }
    $body = json_decode($r['body'], true);
    if (!isset($body['headers']) || !is_array($body['headers'])) {
        return false;
    }
    $ct = get_header_ci($body['headers'], 'Content-Type');
    // 可能含 charset 后缀，用 starts with 判断
    return $ct !== null && strpos($ct, 'application/xml') === 0;
}
check("3.1 contentType() 设置 Content-Type header", test_contenttype_sets_header());

// 3.2 contentType('') 空字符串抛异常
function test_contenttype_empty_string_throws(): bool {
    $exception = false;
    try {
        XHCurl::createRequest('http://127.0.0.1:18399/get')
            ->post()
            ->contentType('')
            ->body('test')
            ->execute();
    } catch (Throwable $e) {
        $exception = true;
    }
    return $exception;
}
check("3.2 contentType('') 空字符串抛异常", test_contenttype_empty_string_throws());

// ==================================================================
// 4. accept()/contentType() 多次调用覆盖
// ==================================================================
echo "\n=== 4. 多次调用覆盖 ===\n";

// 4.1 accept() 多次调用覆盖（最终为 application/json）
function test_accept_multiple_calls_override(): bool {
    $r = XHCurl::createRequest('http://127.0.0.1:18399/post')
        ->get()
        ->accept('text/html')
        ->accept('application/json')
        ->execute();
    if ($r['success'] !== true) {
        return false;
    }
    $body = json_decode($r['body'], true);
    if (!isset($body['headers']) || !is_array($body['headers'])) {
        return false;
    }
    $accept = get_header_ci($body['headers'], 'Accept');
    return $accept === 'application/json';
}
check("4.1 accept() 多次调用覆盖（最终为 application/json）", test_accept_multiple_calls_override());

// ==================================================================
// 5. executeJson() 测试
// ==================================================================
echo "\n=== 5. executeJson() JSON 响应解析 ===\n";

// 5.1 executeJson() 成功解析 JSON 响应
function test_execute_json_success(): bool {
    $data = XHCurl::createRequest('http://127.0.0.1:18399/echo-json')
        ->get()
        ->executeJson();
    return is_array($data)
        && isset($data['received'])
        && $data['received'] === true
        && isset($data['method'])
        && $data['method'] === 'GET';
}
check("5.1 executeJson() 成功解析 JSON 响应", test_execute_json_success());

// 5.2 executeJson() 非 JSON Content-Type 抛异常
function test_execute_json_non_json_throws(): bool {
    $exception = false;
    $msg = '';
    try {
        XHCurl::createRequest('http://127.0.0.1:18399/text')
            ->get()
            ->executeJson();
    } catch (Throwable $e) {
        $exception = true;
        $msg = $e->getMessage();
    }
    // 异常消息应包含 "application/json" 字样
    return $exception && strpos($msg, 'application/json') !== false;
}
check("5.2 executeJson() 非 JSON Content-Type 抛异常", test_execute_json_non_json_throws());

// 5.3 executeJson() 请求失败抛异常（通过 socat 18400 超时触发）
function test_execute_json_request_failure_throws(): bool {
    $exception = false;
    try {
        XHCurl::createRequest('http://127.0.0.1:18400/hang')
            ->get()
            ->timeoutMs(200)
            ->executeJson();
    } catch (Throwable $e) {
        $exception = true;
    }
    return $exception;
}
check("5.3 executeJson() 请求失败抛异常", test_execute_json_request_failure_throws());

// 5.4 execute() 返回数组结构不受 executeJson 影响
function test_execute_structure_unchanged(): bool {
    $r = XHCurl::createRequest('http://127.0.0.1:18399/echo-json')
        ->get()
        ->execute();
    return $r['success'] === true
        && isset($r['body'])
        && isset($r['headers'])
        && isset($r['status']);
}
check("5.4 execute() 返回数组结构不受 executeJson 影响", test_execute_structure_unchanged());

// ==================================================================
// 6. 最终结果
// ==================================================================
echo "\n=== 测试结果: $pass 通过, $fail 失败 ===\n";
exit($fail > 0 ? 1 : 0);
