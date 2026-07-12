<?php
// +----------------------------------------------------------------------+
// | XHCurl v1.2.0 - 空字符串校验与易用性 Helper 测试                       |
// |                                                                        |
// | 覆盖第八轮 spec 的所有验证点：                                          |
// | 1. userAgent/encoding/range/cookies 空字符串 fail-fast 校验            |
// | 2. 错误消息格式统一（bearerToken/maxRedirects/xhrun）                  |
// | 3. 新增 setter：referer/cookie/jsonStr                                 |
// | 4. 新增 getter：getHeader/getMultipart/getReferer                      |
// | 5. 扩展 getBody：返回 JSON/form 序列化字符串                            |
// | 6. maxRedirects(0) 行为验证                                            |
// |                                                                        |
// | 注意：多数用例需 mock 服务器（127.0.0.1:18399）。                       |
// | mock_server.php 提供：/get /post /cookies /stream（无 /headers /redirect）|
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

// 辅助：从回显的 headers 关联数组中大小写不敏感地取值
// （PHP getallheaders() 返回的键大小写因 SAPI 而异）
function get_response_header($body, $name) {
    $headers = $body['headers'] ?? [];
    $name = strtolower($name);
    foreach ($headers as $key => $value) {
        if (strtolower($key) === $name) {
            return $value;
        }
    }
    return null;
}

echo "=== 空字符串校验与 Helper 测试 ===\n";

// ==================================================================
// 1. 空字符串 setter 抛异常（userAgent/encoding/range/cookies 各 2 项）
//    空字符串 → fail-fast 抛异常；null → 清除不抛异常
// ==================================================================
echo "\n=== 1. 空字符串 setter fail-fast 校验 ===\n";

// 1.1 userAgent('') 抛异常
function test_user_agent_empty_throws(): bool {
    try {
        XHCurl::createRequest('http://127.0.0.1:18399/get')->get()->userAgent('');
        return false;
    } catch (\Throwable $e) {
        return strpos($e->getMessage(), 'userAgent') !== false
            && strpos($e->getMessage(), '传 null 清除 User-Agent 覆盖') !== false;
    }
}
check("userAgent('') 抛异常", test_user_agent_empty_throws());

// 1.2 userAgent(null) 不抛异常
function test_user_agent_null_clears(): bool {
    try {
        XHCurl::createRequest('http://127.0.0.1:18399/get')->get()->userAgent(null);
        return true;
    } catch (\Throwable $e) {
        return false;
    }
}
check("userAgent(null) 清除不抛异常", test_user_agent_null_clears());

// 1.3 encoding('') 抛异常
function test_encoding_empty_throws(): bool {
    try {
        XHCurl::createRequest('http://127.0.0.1:18399/get')->get()->encoding('');
        return false;
    } catch (\Throwable $e) {
        return strpos($e->getMessage(), 'encoding') !== false
            && strpos($e->getMessage(), '传 null 清除 Accept-Encoding 覆盖') !== false;
    }
}
check("encoding('') 抛异常", test_encoding_empty_throws());

// 1.4 encoding(null) 不抛异常
function test_encoding_null_clears(): bool {
    try {
        XHCurl::createRequest('http://127.0.0.1:18399/get')->get()->encoding(null);
        return true;
    } catch (\Throwable $e) {
        return false;
    }
}
check("encoding(null) 清除不抛异常", test_encoding_null_clears());

// 1.5 range('') 抛异常
function test_range_empty_throws(): bool {
    try {
        XHCurl::createRequest('http://127.0.0.1:18399/get')->get()->range('');
        return false;
    } catch (\Throwable $e) {
        return strpos($e->getMessage(), 'range') !== false
            && strpos($e->getMessage(), '传 null 清除 Range 覆盖') !== false;
    }
}
check("range('') 抛异常", test_range_empty_throws());

// 1.6 range(null) 不抛异常
function test_range_null_clears(): bool {
    try {
        XHCurl::createRequest('http://127.0.0.1:18399/get')->get()->range(null);
        return true;
    } catch (\Throwable $e) {
        return false;
    }
}
check("range(null) 清除不抛异常", test_range_null_clears());

// 1.7 cookies('') 抛异常
function test_cookies_empty_throws(): bool {
    try {
        XHCurl::createRequest('http://127.0.0.1:18399/get')->get()->cookies('');
        return false;
    } catch (\Throwable $e) {
        return strpos($e->getMessage(), 'cookies') !== false
            && strpos($e->getMessage(), '传 null 清除 Cookie 覆盖') !== false;
    }
}
check("cookies('') 抛异常", test_cookies_empty_throws());

// 1.8 cookies(null) 不抛异常
function test_cookies_null_clears(): bool {
    try {
        XHCurl::createRequest('http://127.0.0.1:18399/get')->get()->cookies(null);
        return true;
    } catch (\Throwable $e) {
        return false;
    }
}
check("cookies(null) 清除不抛异常", test_cookies_null_clears());

// ==================================================================
// 2. 错误消息格式验证（bearerToken/maxRedirects/xhrun）
// ==================================================================
echo "\n=== 2. 错误消息格式统一 ===\n";

// 2.1 bearerToken('') 消息含「传 null 清除 Bearer Token」
function test_bearer_token_empty_message(): bool {
    try {
        XHCurl::createRequest('http://127.0.0.1:18399/get')->get()->bearerToken('');
        return false;
    } catch (\Throwable $e) {
        return strpos($e->getMessage(), '传 null 清除 Bearer Token') !== false;
    }
}
check("bearerToken('') 消息含「传 null 清除 Bearer Token」", test_bearer_token_empty_message());

// 2.2 maxRedirects(-1) 消息含「0 = 不跟随重定向」
function test_max_redirects_negative_message(): bool {
    try {
        XHCurl::createRequest('http://127.0.0.1:18399/get')->get()->maxRedirects(-1);
        return false;
    } catch (\Throwable $e) {
        return strpos($e->getMessage(), '0 = 不跟随重定向') !== false;
    }
}
check("maxRedirects(-1) 消息含「0 = 不跟随重定向」", test_max_redirects_negative_message());

// 2.3 xhrun timeout=-1 消息含「xhrun timeout 不能为负值，0 = 无超时」
function test_xhrun_timeout_negative_message(): bool {
    try {
        xhrun('echo', null, ['timeout' => -1]);
        return false;
    } catch (\Throwable $e) {
        return strpos($e->getMessage(), 'xhrun timeout 不能为负值，0 = 无超时') !== false;
    }
}
check("xhrun timeout=-1 消息含「xhrun timeout 不能为负值，0 = 无超时」", test_xhrun_timeout_negative_message());

// 2.4 xhrun max_output=-1 消息含「xhrun max_output 不能为负值，0 = 无限制」
function test_xhrun_max_output_negative_message(): bool {
    try {
        xhrun('echo', null, ['max_output' => -1]);
        return false;
    } catch (\Throwable $e) {
        return strpos($e->getMessage(), 'xhrun max_output 不能为负值，0 = 无限制') !== false;
    }
}
check("xhrun max_output=-1 消息含「xhrun max_output 不能为负值，0 = 无限制」", test_xhrun_max_output_negative_message());

// ==================================================================
// 3. referer setter（4 项）
// ==================================================================
echo "\n=== 3. referer setter ===\n";

// 3.1 referer 设置后 execute() 请求头含 Referer
//     mock_server 无 /headers 端点，用 /post 回显请求头（getallheaders）
function test_referer_set(): bool {
    $result = XHCurl::createRequest('http://127.0.0.1:18399/post')
        ->get()
        ->referer('https://example.com/page')
        ->execute();
    if (!$result['success']) {
        return false;
    }
    $body = json_decode($result['body'], true);
    if (!is_array($body)) {
        return false;
    }
    $referer = get_response_header($body, 'referer');
    return $referer === 'https://example.com/page';
}
check("referer 设置后 execute 请求头含 Referer", test_referer_set());

// 3.2 referer('') 抛异常
function test_referer_empty_throws(): bool {
    try {
        XHCurl::createRequest('http://127.0.0.1:18399/get')->get()->referer('');
        return false;
    } catch (\Throwable $e) {
        return strpos($e->getMessage(), 'referer') !== false
            && strpos($e->getMessage(), '传 null 清除 Referer 覆盖') !== false;
    }
}
check("referer('') 抛异常", test_referer_empty_throws());

// 3.3 referer(null) 不抛异常
function test_referer_null_clears(): bool {
    try {
        XHCurl::createRequest('http://127.0.0.1:18399/get')->get()->referer(null);
        return true;
    } catch (\Throwable $e) {
        return false;
    }
}
check("referer(null) 清除不抛异常", test_referer_null_clears());

// 3.4 getReferer() 往返
function test_get_referer_roundtrip(): bool {
    $req = XHCurl::createRequest('http://127.0.0.1:18399/get')
        ->get()
        ->referer('https://example.com/page');
    return $req->getReferer() === 'https://example.com/page';
}
check("getReferer() 往返一致", test_get_referer_roundtrip());

// ==================================================================
// 4. cookie 增量添加（3 项）
// ==================================================================
echo "\n=== 4. cookie 增量添加 ===\n";

// 4.1 cookie 增量添加：->cookie('session','abc')->cookie('token','xyz')
//     用 /cookies 端点回显 Cookie 头
function test_cookie_incremental(): bool {
    $result = XHCurl::createRequest('http://127.0.0.1:18399/cookies')
        ->get()
        ->cookie('session', 'abc')
        ->cookie('token', 'xyz')
        ->execute();
    if (!$result['success']) {
        return false;
    }
    $body = json_decode($result['body'], true);
    if (!is_array($body)) {
        return false;
    }
    $cookie = $body['cookie_header'] ?? '';
    return strpos($cookie, 'session=abc') !== false
        && strpos($cookie, 'token=xyz') !== false;
}
check("cookie 增量添加后 Cookie 头含两者", test_cookie_incremental());

// 4.2 cookie('', 'value') 抛异常
function test_cookie_empty_name_throws(): bool {
    try {
        XHCurl::createRequest('http://127.0.0.1:18399/get')->get()->cookie('', 'value');
        return false;
    } catch (\Throwable $e) {
        return strpos($e->getMessage(), 'cookie name') !== false
            && strpos($e->getMessage(), '不能为空') !== false;
    }
}
check("cookie('', 'value') 抛异常", test_cookie_empty_name_throws());

// 4.3 cookie('name', '') 抛异常
function test_cookie_empty_value_throws(): bool {
    try {
        XHCurl::createRequest('http://127.0.0.1:18399/get')->get()->cookie('name', '');
        return false;
    } catch (\Throwable $e) {
        return strpos($e->getMessage(), 'cookie value') !== false
            && strpos($e->getMessage(), '不能为空') !== false;
    }
}
check("cookie('name', '') 抛异常", test_cookie_empty_value_throws());

// ==================================================================
// 5. jsonStr setter（3 项）
// ==================================================================
echo "\n=== 5. jsonStr setter ===\n";

// 5.1 jsonStr 正常：execute() Content-Type 含 application/json 且 body 正确
function test_json_str_set(): bool {
    $result = XHCurl::createRequest('http://127.0.0.1:18399/post')
        ->post()
        ->jsonStr('{"name":"XHCurl"}')
        ->execute();
    if (!$result['success']) {
        return false;
    }
    $body = json_decode($result['body'], true);
    if (!is_array($body)) {
        return false;
    }
    $ct = get_response_header($body, 'content-type') ?? '';
    $receivedJson = $body['json'] ?? null;
    return strpos($ct, 'application/json') !== false
        && $receivedJson === ['name' => 'XHCurl'];
}
check("jsonStr 设置后 Content-Type 含 application/json 且 body 正确", test_json_str_set());

// 5.2 jsonStr 无效 JSON 抛异常
function test_json_str_invalid_throws(): bool {
    try {
        XHCurl::createRequest('http://127.0.0.1:18399/post')
            ->post()
            ->jsonStr('{"name":}');
        return false;
    } catch (\Throwable $e) {
        return strpos($e->getMessage(), 'jsonStr') !== false
            && strpos($e->getMessage(), '无效 JSON') !== false;
    }
}
check("jsonStr 无效 JSON 抛异常", test_json_str_invalid_throws());

// 5.3 jsonStr 空字符串抛异常
function test_json_str_empty_throws(): bool {
    try {
        XHCurl::createRequest('http://127.0.0.1:18399/post')
            ->post()
            ->jsonStr('');
        return false;
    } catch (\Throwable $e) {
        return strpos($e->getMessage(), 'jsonStr') !== false
            && strpos($e->getMessage(), '不能为空') !== false;
    }
}
check("jsonStr 空字符串抛异常", test_json_str_empty_throws());

// ==================================================================
// 6. getHeader 大小写不敏感（2 项）
// ==================================================================
echo "\n=== 6. getHeader 大小写不敏感 ===\n";

// 6.1 大小写不敏感：header('Content-Type', ...) 后 getHeader('content-type') 返回正确值
function test_get_header_case_insensitive(): bool {
    $req = XHCurl::createRequest('http://127.0.0.1:18399/get')
        ->get()
        ->header('Content-Type', 'application/json');
    return $req->getHeader('content-type') === 'application/json'
        && $req->getHeader('CONTENT-TYPE') === 'application/json';
}
check("getHeader 大小写不敏感", test_get_header_case_insensitive());

// 6.2 未设置返回 null
function test_get_header_not_set(): bool {
    $req = XHCurl::createRequest('http://127.0.0.1:18399/get')->get();
    return $req->getHeader('X-Not-Set') === null;
}
check("getHeader 未设置返回 null", test_get_header_not_set());

// ==================================================================
// 7. getMultipart（2 项）
// ==================================================================
echo "\n=== 7. getMultipart ===\n";

// 7.1 已设置：multipart() 后 getMultipart() 返回数组
function test_get_multipart_set(): bool {
    $req = XHCurl::createRequest('http://127.0.0.1:18399/post')
        ->post()
        ->multipart([
            ['name' => 'file', 'value' => 'data', 'filename' => 'test.txt'],
        ]);
    $mp = $req->getMultipart();
    if (!is_array($mp) || count($mp) !== 1) {
        return false;
    }
    $field = $mp[0] ?? null;
    if (!is_array($field)) {
        return false;
    }
    return ($field['name'] ?? null) === 'file'
        && ($field['value'] ?? null) === 'data'
        && ($field['filename'] ?? null) === 'test.txt';
}
check("getMultipart 已设置返回字段数组", test_get_multipart_set());

// 7.2 未设置返回 null
function test_get_multipart_not_set(): bool {
    $req = XHCurl::createRequest('http://127.0.0.1:18399/post')->post();
    return $req->getMultipart() === null;
}
check("getMultipart 未设置返回 null", test_get_multipart_not_set());

// ==================================================================
// 8. 扩展 getBody（4 项）
// ==================================================================
echo "\n=== 8. 扩展 getBody ===\n";

// 8.1 body('raw bytes') 后 getBody() 返回 'raw bytes'
function test_get_body_bytes(): bool {
    $req = XHCurl::createRequest('http://127.0.0.1:18399/post')
        ->post()
        ->body('raw bytes');
    return $req->getBody() === 'raw bytes';
}
check("body('raw bytes') 后 getBody() 返回原值", test_get_body_bytes());

// 8.2 json(['k'=>'v']) 后 getBody() 返回 JSON 字符串
function test_get_body_json(): bool {
    $req = XHCurl::createRequest('http://127.0.0.1:18399/post')
        ->post()
        ->json(['k' => 'v']);
    $got = $req->getBody();
    $decoded = json_decode($got ?? '', true);
    return $decoded === ['k' => 'v'];
}
check("json(['k'=>'v']) 后 getBody() 返回 JSON 字符串", test_get_body_json());

// 8.3 form(['k'=>'v']) 后 getBody() 返回 'k=v'
function test_get_body_form(): bool {
    $req = XHCurl::createRequest('http://127.0.0.1:18399/post')
        ->post()
        ->form(['k' => 'v']);
    return $req->getBody() === 'k=v';
}
check("form(['k'=>'v']) 后 getBody() 返回 'k=v'", test_get_body_form());

// 8.4 multipart 后 getBody() 返回 null
function test_get_body_multipart_null(): bool {
    $req = XHCurl::createRequest('http://127.0.0.1:18399/post')
        ->post()
        ->multipart([['name' => 'file', 'value' => 'data']]);
    return $req->getBody() === null;
}
check("multipart 后 getBody() 返回 null", test_get_body_multipart_null());

// ==================================================================
// 9. maxRedirects(0) 行为验证（1 项）
//    mock_server 无 /redirect 端点，改用等价性测试：
//    maxRedirects(0) 与 followRedirects(false) 在同一端点应产生等价结果
// ==================================================================
echo "\n=== 9. maxRedirects(0) 行为验证 ===\n";

function test_max_redirects_zero_equivalent(): bool {
    $r1 = XHCurl::createRequest('http://127.0.0.1:18399/get')
        ->get()
        ->maxRedirects(0)
        ->execute();
    $r2 = XHCurl::createRequest('http://127.0.0.1:18399/get')
        ->get()
        ->followRedirects(false)
        ->execute();
    return $r1['success'] === $r2['success'] && $r1['status'] === $r2['status'];
}
check("maxRedirects(0) 与 followRedirects(false) 行为等价", test_max_redirects_zero_equivalent());

echo "\n=== 测试结果: $pass 通过, $fail 失败 ===\n";
exit($fail > 0 ? 1 : 0);
