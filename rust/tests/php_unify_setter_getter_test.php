<?php
// +----------------------------------------------------------------------+
// | XHCurl PHP setter/getter 契约统一测试                                  |
// |                                                                        |
// | 验证 unify-setter-getter-contract spec 的 P1/P2/P3 修复：               |
// |   P1-1: getMethod() 在 customMethod 设置后返回自定义方法名；             |
// |         新增 getCustomMethod(): ?string                                  |
// |   P2-1 BREAKING: timeout/timeoutMs/connectTimeout/connectTimeoutMs/    |
// |         maxRedirects 负值现在抛异常（之前静默跳过）                       |
// |   P2-3 BREAKING: userAgent/encoding/range/cookies 非 ASCII 现在在       |
// |         setter 时抛异常（之前延迟到 execute 返回失败）                    |
// |   P3-1: basicAuth/bearerToken/userAgent/encoding/range/cookies 现在     |
// |         接受 null 参数清除已设值                                          |
// |   P3-2: 新增 getBody(): ?string getter                                   |
// |   P3-3: 新增 url(string $url): $self_ 链式 setter                       |
// |                                                                        |
// | 注意：本文件全部为本地行为校验（getter/setter/校验），不需 mock 服务器。  |
// | $BASE 仅用于构造请求 URL（不实际发起请求）。                                |
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

echo "=== setter/getter 契约统一测试 ===\n";

// ==================================================================
// P1-1: getMethod() + getCustomMethod()
//    getMethod() 在 customMethod 设置后返回自定义方法名；
//    getCustomMethod() 返回自定义方法字符串或 null（未设置时）。
// ==================================================================
echo "\n=== P1-1: getMethod + getCustomMethod ===\n";

function test_get_method_after_custom_method(): bool {
    global $BASE;
    $req = XHCurl::createRequest($BASE . '/get')->get()->customMethod('PROPFIND');
    return $req->getMethod() === 'PROPFIND';
}
check("getMethod() customMethod 设置后返回自定义方法", test_get_method_after_custom_method());

function test_get_method_without_custom_method(): bool {
    global $BASE;
    $req = XHCurl::createRequest($BASE . '/get')->get();
    return $req->getMethod() === 'GET';
}
check("getMethod() 未设 customMethod 返回标准方法", test_get_method_without_custom_method());

function test_get_custom_method(): bool {
    global $BASE;
    $req = XHCurl::createRequest($BASE . '/get')->get()->customMethod('PROPFIND');
    if ($req->getCustomMethod() !== 'PROPFIND') return false;
    $req2 = XHCurl::createRequest($BASE . '/get')->get();
    return $req2->getCustomMethod() === null;
}
check("getCustomMethod() 设置后取回 + 未设置返回 null", test_get_custom_method());

// ==================================================================
// P2-1: 数值 setter 负值抛异常
//    timeout/timeoutMs/connectTimeout/connectTimeoutMs/maxRedirects
//    负值现在抛异常（之前静默跳过），错误信息含字段名和「负值」关键词。
//    0 值仍然合法（表示使用全局默认/无超时）。
// ==================================================================
echo "\n=== P2-1: 数值 setter 负值抛异常 ===\n";

function test_timeout_negative_throws(): bool {
    global $BASE;
    try {
        XHCurl::createRequest($BASE . '/get')->get()->timeout(-1);
        return false;
    } catch (\Throwable $e) {
        return strpos($e->getMessage(), 'timeout') !== false
            && strpos($e->getMessage(), '负值') !== false;
    }
}
check("timeout(-1) 抛异常", test_timeout_negative_throws());

function test_timeout_ms_negative_throws(): bool {
    global $BASE;
    try {
        XHCurl::createRequest($BASE . '/get')->get()->timeoutMs(-1);
        return false;
    } catch (\Throwable $e) {
        return strpos($e->getMessage(), 'timeoutMs') !== false
            && strpos($e->getMessage(), '负值') !== false;
    }
}
check("timeoutMs(-1) 抛异常", test_timeout_ms_negative_throws());

function test_connect_timeout_negative_throws(): bool {
    global $BASE;
    try {
        XHCurl::createRequest($BASE . '/get')->get()->connectTimeout(-5);
        return false;
    } catch (\Throwable $e) {
        return strpos($e->getMessage(), 'connectTimeout') !== false
            && strpos($e->getMessage(), '负值') !== false;
    }
}
check("connectTimeout(-5) 抛异常", test_connect_timeout_negative_throws());

function test_connect_timeout_ms_negative_throws(): bool {
    global $BASE;
    try {
        XHCurl::createRequest($BASE . '/get')->get()->connectTimeoutMs(-100);
        return false;
    } catch (\Throwable $e) {
        return strpos($e->getMessage(), 'connectTimeoutMs') !== false
            && strpos($e->getMessage(), '负值') !== false;
    }
}
check("connectTimeoutMs(-100) 抛异常", test_connect_timeout_ms_negative_throws());

function test_max_redirects_negative_throws(): bool {
    global $BASE;
    try {
        XHCurl::createRequest($BASE . '/get')->get()->maxRedirects(-3);
        return false;
    } catch (\Throwable $e) {
        return strpos($e->getMessage(), 'maxRedirects') !== false
            && strpos($e->getMessage(), '负值') !== false;
    }
}
check("maxRedirects(-3) 抛异常", test_max_redirects_negative_throws());

function test_timeout_zero_ok(): bool {
    global $BASE;
    try {
        XHCurl::createRequest($BASE . '/get')->get()->timeout(0);
        return true;
    } catch (\Throwable $e) {
        return false;
    }
}
check("timeout(0) 不抛异常（合法）", test_timeout_zero_ok());

// ==================================================================
// P2-3: ASCII 校验前置
//    userAgent/encoding/range/cookies 非 ASCII 现在在 setter 时抛异常
//    （之前延迟到 execute 返回 success=false）。
//    错误信息含字段名和「非 ASCII」关键词。
// ==================================================================
echo "\n=== P2-3: ASCII 校验前置（setter 时抛异常）===\n";

function test_user_agent_non_ascii_throws(): bool {
    global $BASE;
    try {
        XHCurl::createRequest($BASE . '/get')->get()->userAgent('Client 😀');
        return false;
    } catch (\Throwable $e) {
        return strpos($e->getMessage(), 'userAgent') !== false
            && strpos($e->getMessage(), '非 ASCII') !== false;
    }
}
check("userAgent 非 ASCII 抛异常", test_user_agent_non_ascii_throws());

function test_encoding_non_ascii_throws(): bool {
    global $BASE;
    try {
        XHCurl::createRequest($BASE . '/get')->get()->encoding('gzip, 中文');
        return false;
    } catch (\Throwable $e) {
        return strpos($e->getMessage(), 'encoding') !== false
            && strpos($e->getMessage(), '非 ASCII') !== false;
    }
}
check("encoding 非 ASCII 抛异常", test_encoding_non_ascii_throws());

function test_range_non_ascii_throws(): bool {
    global $BASE;
    try {
        XHCurl::createRequest($BASE . '/get')->get()->range('0-中文');
        return false;
    } catch (\Throwable $e) {
        return strpos($e->getMessage(), 'range') !== false
            && strpos($e->getMessage(), '非 ASCII') !== false;
    }
}
check("range 非 ASCII 抛异常", test_range_non_ascii_throws());

function test_cookies_non_ascii_throws(): bool {
    global $BASE;
    try {
        XHCurl::createRequest($BASE . '/get')->get()->cookies('session=中文');
        return false;
    } catch (\Throwable $e) {
        return strpos($e->getMessage(), 'cookies') !== false
            && strpos($e->getMessage(), '非 ASCII') !== false;
    }
}
check("cookies 非 ASCII 抛异常", test_cookies_non_ascii_throws());

function test_user_agent_ascii_ok(): bool {
    global $BASE;
    try {
        XHCurl::createRequest($BASE . '/get')->get()->userAgent('MyClient/1.0');
        return true;
    } catch (\Throwable $e) {
        return false;
    }
}
check("userAgent ASCII 值不抛异常", test_user_agent_ascii_ok());

// ==================================================================
// P3-1: null-clear 语义
//    basicAuth/bearerToken/userAgent/encoding/range/cookies 接受
//    null 参数清除已设值（getter 随后返回 null）。
//    bearerToken('') 空字符串仍然抛异常（与 null 清除共存）。
// ==================================================================
echo "\n=== P3-1: null-clear 语义 ===\n";

function test_basic_auth_null_clears(): bool {
    global $BASE;
    $req = XHCurl::createRequest($BASE . '/get')->get()->basicAuth('user:pass');
    if ($req->getBasicAuth() !== 'user:pass') return false;
    $req->basicAuth(null);
    return $req->getBasicAuth() === null;
}
check("basicAuth(null) 清除已设值", test_basic_auth_null_clears());

function test_bearer_token_null_clears(): bool {
    global $BASE;
    $req = XHCurl::createRequest($BASE . '/get')->get()->bearerToken('abc123');
    if ($req->getBearerToken() !== 'abc123') return false;
    $req->bearerToken(null);
    return $req->getBearerToken() === null;
}
check("bearerToken(null) 清除已设值", test_bearer_token_null_clears());

function test_bearer_token_empty_still_throws(): bool {
    global $BASE;
    // bearerToken('') 空字符串仍然抛异常（与 null 清除共存）
    try {
        XHCurl::createRequest($BASE . '/get')->get()->bearerToken('');
        return false;
    } catch (\Throwable $e) {
        return strpos($e->getMessage(), 'bearerToken') !== false
            && strpos($e->getMessage(), '空') !== false;
    }
}
check("bearerToken('') 空字符串仍抛异常（不被 null-clear 破坏）", test_bearer_token_empty_still_throws());

function test_user_agent_null_clears(): bool {
    global $BASE;
    $req = XHCurl::createRequest($BASE . '/get')->get()->userAgent('MyClient/1.0');
    if ($req->getUserAgent() !== 'MyClient/1.0') return false;
    $req->userAgent(null);
    return $req->getUserAgent() === null;
}
check("userAgent(null) 清除已设值", test_user_agent_null_clears());

function test_encoding_null_clears(): bool {
    global $BASE;
    $req = XHCurl::createRequest($BASE . '/get')->get()->encoding('gzip, deflate');
    if ($req->getEncoding() !== 'gzip, deflate') return false;
    $req->encoding(null);
    return $req->getEncoding() === null;
}
check("encoding(null) 清除已设值", test_encoding_null_clears());

function test_range_null_clears(): bool {
    global $BASE;
    $req = XHCurl::createRequest($BASE . '/get')->get()->range('0-1023');
    if ($req->getRange() !== '0-1023') return false;
    $req->range(null);
    return $req->getRange() === null;
}
check("range(null) 清除已设值", test_range_null_clears());

function test_cookies_null_clears(): bool {
    global $BASE;
    $req = XHCurl::createRequest($BASE . '/get')->get()->cookies('session=abc');
    if ($req->getCookies() !== 'session=abc') return false;
    $req->cookies(null);
    return $req->getCookies() === null;
}
check("cookies(null) 清除已设值", test_cookies_null_clears());

// ==================================================================
// P3-2: getBody() getter
//    返回 body() 设置的原始字符串或 null（未设置时）。
//    仅返回通过 body() 设置的原始字节体；JSON/表单/multipart 不在此返回。
// ==================================================================
echo "\n=== P3-2: getBody() getter ===\n";

function test_get_body(): bool {
    global $BASE;
    $req = XHCurl::createRequest($BASE . '/post')->post()->body('raw text');
    if ($req->getBody() !== 'raw text') return false;
    $req2 = XHCurl::createRequest($BASE . '/post')->post();
    return $req2->getBody() === null;
}
check("getBody() 设置后取回 + 未设置返回 null", test_get_body());

// ==================================================================
// P3-3: url() 链式 setter
//    允许在构造后变更 URL，便于复用已配置的请求模板。
// ==================================================================
echo "\n=== P3-3: url() 链式 setter ===\n";

function test_url_setter(): bool {
    global $BASE;
    $req = XHCurl::createRequest($BASE . '/get')->get()->url($BASE . '/post');
    return $req->getUrl() === $BASE . '/post';
}
check("url() setter 变更 URL 后 getUrl() 返回新值", test_url_setter());

echo "\n总计: $pass passed, $fail failed\n";
exit($fail > 0 ? 1 : 0);
