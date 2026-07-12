<?php
// +----------------------------------------------------------------------+
// | XHCurl 链式 setter 静默失败修复与全局配置生命周期测试                    |
// |                                                                        |
// | 验证 fix-silent-setters-and-config-lifecycle spec 的 Task 1-10 修复：     |
// |   Task 1: setConfig 配置指纹比对，全局 Client 重建（无覆盖请求立即生效）  |
// |   Task 2: cookies/encoding/range/userAgent 非法值抛异常（setter 时）     |
// |   Task 4: form() 含数组/对象值抛异常（setter 时）                        |
// |   Task 5: header() 非法值立即抛异常（fail-fast，非 execute 时）           |
// |   Task 6: connectTimeoutMs(int $ms) 毫秒级连接超时                       |
// |   Task 7: headers(array) 批量设置；单个非法值整体抛异常                    |
// |   Task 8: XHMulti/XHThreadPool 空请求 execute 抛异常                      |
// |   Task 9: HEAD 请求跳过 body 读取（返回空 body + 正常状态码/headers）      |
// |   Task 10: basicAuth() 空值/无冒号抛异常                                  |
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

echo "=== 链式 setter 静默失败修复与全局配置生命周期测试 ===\n";

// ==================================================================
// Task 10: basicAuth() 空值/无冒号校验（setter 时立即抛异常）
//    request.rs basic_auth 返回 XhCurlResult，空串或无 ':' 抛异常。
//    合法格式 'user:pass' 不抛异常。
// ==================================================================
echo "\n=== Task 10: basicAuth() 空值/无冒号校验 ===\n";

// 空字符串
$caughtEmpty = false;
try {
    XHCurl::createRequest($BASE . '/get')->get()->basicAuth('');
} catch (\Throwable $e) {
    $caughtEmpty = true;
}
check("basicAuth('') 抛异常", $caughtEmpty);

// 无冒号
$caughtNoColon = false;
$msgHasFormat = false;
try {
    XHCurl::createRequest($BASE . '/get')->get()->basicAuth('nouserpass');
} catch (\Throwable $e) {
    $caughtNoColon = true;
    $msgHasFormat = strpos($e->getMessage(), 'user:pass') !== false;
}
check("basicAuth('nouserpass') 抛异常", $caughtNoColon);
check("basicAuth 异常 message 含格式提示", $msgHasFormat);

// 合法凭据不抛异常（仅校验 setter，不执行网络请求）
$validOk = true;
try {
    XHCurl::createRequest($BASE . '/get')->get()->basicAuth('user:pass');
} catch (\Throwable $e) {
    $validOk = false;
}
check("basicAuth('user:pass') 合法凭据不抛异常", $validOk);

// ==================================================================
// Task 5: header() fail-fast 校验（setter 时立即抛异常，非 execute 时）
//    php_ext.rs header() 调用 validate_header，HeaderValue::from_str 对
//    控制字符（含 NUL）返回 Err。校验在调用处发生，无需 execute。
// ==================================================================
echo "\n=== Task 5: header() fail-fast 校验 ===\n";

// NUL 字节立即抛异常（不需 execute）
$caughtNul = false;
try {
    XHCurl::createRequest($BASE . '/get')->get()->header('X-Test', "bad\0value");
} catch (\Throwable $e) {
    $caughtNul = true;
}
check("header() 含 NUL 字节立即抛异常", $caughtNul);

// 非法 header 名（含空格）立即抛异常
$caughtBadName = false;
try {
    XHCurl::createRequest($BASE . '/get')->get()->header('Bad Name', 'value');
} catch (\Throwable $e) {
    $caughtBadName = true;
}
check("header() 非法名（含空格）立即抛异常", $caughtBadName);

// 合法 header 不抛异常
$validHeaderOk = true;
try {
    XHCurl::createRequest($BASE . '/get')->get()->header('X-Custom', 'valid-value');
} catch (\Throwable $e) {
    $validHeaderOk = false;
}
check("header() 合法值不抛异常", $validHeaderOk);

// ==================================================================
// Task 7: headers(array) 批量方法
//    先校验全部 header 再存储；单个非法值整体抛异常。
//    合法批量设置成功并生效。
// ==================================================================
echo "\n=== Task 7: headers(array) 批量方法 ===\n";

// 合法批量设置 + 验证发送成功
$result = XHCurl::createRequest($BASE . '/post')
    ->post()
    ->headers(array(
        'X-Batch-1' => 'value1',
        'X-Batch-2' => 'value2',
    ))
    ->timeout(10)
    ->execute();
check("headers() 批量设置不抛异常", $result['success'] === true);
// mock_server /post 回显 headers，验证至少一个批量 header 被发送
$respBody = json_decode($result['body'], true);
$headers = $respBody['headers'] ?? array();
$hasBatch1 = false;
foreach ($headers as $hname => $hval) {
    if (strcasecmp($hname, 'X-Batch-1') === 0) {
        $hasBatch1 = true;
        break;
    }
}
check("headers() 批量 header 实际发送", $hasBatch1);

// 单个非法值整体抛异常（不部分存储）
$caughtBatchInvalid = false;
try {
    XHCurl::createRequest($BASE . '/get')->get()->headers(array(
        'X-Good' => 'ok',
        'X-Bad' => "bad\0value",  // 非法值
    ));
} catch (\Throwable $e) {
    $caughtBatchInvalid = true;
}
check("headers() 单个非法值整体抛异常", $caughtBatchInvalid);

// ==================================================================
// Task 4: form() 含数组/对象值抛异常（setter 时）
//    php_ext.rs php_array_to_form 对非标量值返回 Err。
// ==================================================================
echo "\n=== Task 4: form() 非标量值抛异常 ===\n";

$caughtFormArray = false;
try {
    XHCurl::createRequest($BASE . '/post')
        ->post()
        ->form(array('field' => array('nested' => 'value')));
} catch (\Throwable $e) {
    $caughtFormArray = true;
}
check("form() 含数组值抛异常", $caughtFormArray);

// 合法标量 form 不抛异常
$validFormOk = true;
try {
    XHCurl::createRequest($BASE . '/post')
        ->post()
        ->form(array('k1' => 'v1', 'k2' => 'v2', 'num' => 123));
} catch (\Throwable $e) {
    $validFormOk = false;
}
check("form() 合法标量值不抛异常", $validFormOk);

// ==================================================================
// Task 2: cookies/encoding/range/userAgent 非法值抛异常（setter 时）
//    P2-3 BREAKING: 非 ASCII 值现在在 setter 时立即抛异常（之前延迟到
//    execute 返回 success=false）。php_ext.rs 各 setter 调用
//    XhRequest::validate_ascii_header_value 前置校验，与 to_reqwest
//    内的校验形成双保险。错误信息含字段名和「非 ASCII」关键词。
// ==================================================================
echo "\n=== Task 2: cookies/encoding/range/userAgent 非法值 setter 抛异常 ===\n";

// cookies 含中文（非 ASCII）→ setter 抛异常
$caughtCookiesNonAscii = false;
try {
    XHCurl::createRequest($BASE . '/get')
        ->get()
        ->cookies('session=中文');
} catch (\Throwable $e) {
    $caughtCookiesNonAscii = strpos($e->getMessage(), 'cookies') !== false
                          && strpos($e->getMessage(), '非 ASCII') !== false;
}
check("cookies() 含非 ASCII setter 抛异常", $caughtCookiesNonAscii);

// userAgent 含 emoji（非 ASCII）→ setter 抛异常
$caughtUaNonAscii = false;
try {
    XHCurl::createRequest($BASE . '/get')
        ->get()
        ->userAgent('MyClient 😀');
} catch (\Throwable $e) {
    $caughtUaNonAscii = strpos($e->getMessage(), 'userAgent') !== false
                     && strpos($e->getMessage(), '非 ASCII') !== false;
}
check("userAgent() 含非 ASCII setter 抛异常", $caughtUaNonAscii);

// range 含非 ASCII → setter 抛异常
$caughtRangeNonAscii = false;
try {
    XHCurl::createRequest($BASE . '/get')
        ->get()
        ->range('0-中文');
} catch (\Throwable $e) {
    $caughtRangeNonAscii = strpos($e->getMessage(), 'range') !== false
                        && strpos($e->getMessage(), '非 ASCII') !== false;
}
check("range() 含非 ASCII setter 抛异常", $caughtRangeNonAscii);

// encoding 含非 ASCII → setter 抛异常
$caughtEncodingNonAscii = false;
try {
    XHCurl::createRequest($BASE . '/get')
        ->get()
        ->encoding('gzip, 中文');
} catch (\Throwable $e) {
    $caughtEncodingNonAscii = strpos($e->getMessage(), 'encoding') !== false
                           && strpos($e->getMessage(), '非 ASCII') !== false;
}
check("encoding() 含非 ASCII setter 抛异常", $caughtEncodingNonAscii);

// ==================================================================
// Task 9: HEAD 请求跳过 body 读取
//    executor.rs 对 HEAD 方法跳过 stream.chunk() 循环。
//    返回 success=true、空 body、正常状态码。
// ==================================================================
echo "\n=== Task 9: HEAD 请求跳过 body 读取 ===\n";

$result = XHCurl::createRequest($BASE . '/get')
    ->head()
    ->timeout(10)
    ->execute();
check("HEAD 请求 success=true", $result['success'] === true);
check("HEAD 请求 status=200", isset($result['status']) && $result['status'] === 200);
check("HEAD 请求 body 为空", isset($result['body']) && $result['body'] === '');

// ==================================================================
// Task 8: execute() 空请求列表抛异常
//    XHMulti::execute 与 XHThreadPool::execute 对空请求列表抛异常。
// ==================================================================
echo "\n=== Task 8: execute() 空请求列表抛异常 ===\n";

// XHMulti 空请求
$caughtMultiEmpty = false;
try {
    $multi = new XHMulti();
    $multi->execute();
} catch (\Throwable $e) {
    $caughtMultiEmpty = strpos($e->getMessage(), 'XHMulti') !== false
                     || strpos($e->getMessage(), 'add') !== false;
}
check("XHMulti 空请求 execute 抛异常", $caughtMultiEmpty);

// XHThreadPool 空请求（仅在 CLI 模式下）
$caughtPoolEmpty = false;
$poolMsgMention = false;
try {
    $pool = new XHThreadPool(2);
    $pool->execute();
} catch (\Throwable $e) {
    $caughtPoolEmpty = true;
    $msg = $e->getMessage();
    $poolMsgMention = strpos($msg, 'XHThreadPool') !== false
                   || strpos($msg, 'add') !== false
                   || strpos($msg, 'CLI') !== false;
}
check("XHThreadPool 空请求 execute 抛异常", $caughtPoolEmpty);
check("XHThreadPool 异常 message 含提示", $poolMsgMention);

// ==================================================================
// Task 6: connectTimeoutMs(int $ms) 毫秒级连接超时
//    request.rs connect_timeout_ms 优先级高于 connect_timeout（秒）。
//    验证：方法可链式调用且合法请求仍成功；
//    对不可路由 IP 设置短连接超时，触发连接失败。
// ==================================================================
echo "\n=== Task 6: connectTimeoutMs() 毫秒级连接超时 ===\n";

// 方法可链式调用，合法请求成功
$result = XHCurl::createRequest($BASE . '/get')
    ->get()
    ->connectTimeoutMs(5000)
    ->timeout(10)
    ->execute();
check("connectTimeoutMs() 可链式调用且请求成功", $result['success'] === true);

// 对不可路由 IP 设置短连接超时，触发连接失败
// 192.0.2.1 为 TEST-NET-1（RFC 5737，不可路由），TCP SYN 被丢弃，连接挂起直到超时。
// 若环境有 HTTP 代理拦截，可能转为其他错误；此处仅验证请求最终失败。
$result = XHCurl::createRequest('http://192.0.2.1:80/test')
    ->get()
    ->connectTimeoutMs(300)
    ->timeout(5)
    ->execute();
check("connectTimeoutMs(300) 对不可路由 IP 触发失败", $result['success'] === false);

// ==================================================================
// Task 1: 全局配置指纹比对，全局 Client 重建
//    setConfig 修改影响 Client 构建的配置后，下次无覆盖请求立即用新配置。
//    验证：setConfig → getConfig 往返一致；setConfig 后正常请求仍成功。
//    （proxy/verify_ssl 切换需真实代理/证书环境，此处验证重建机制不破坏请求）
// ==================================================================
echo "\n=== Task 1: 全局配置指纹比对与重建 ===\n";

// 配置往返：getConfig → setConfig($orig) 不报类型错误
$orig = XHCurl::getConfig();
$roundTripOk = true;
try {
    XHCurl::setConfig($orig);
} catch (\Throwable $e) {
    $roundTripOk = false;
}
check("getConfig → setConfig 往返不抛异常", $roundTripOk);

// setConfig 修改后无覆盖请求仍成功（Client 重建机制不破坏正常请求）
$setOk = true;
try {
    XHCurl::setConfig(array(
        'connect_timeout' => 10,
        'request_timeout' => 15,
        'verify_ssl' => true,
        'follow_redirects' => true,
    ));
} catch (\Throwable $e) {
    $setOk = false;
}
check("setConfig 合法配置不抛异常", $setOk);

$result = XHCurl::createRequest($BASE . '/get?id=1')
    ->get()
    ->timeout(10)
    ->execute();
check("setConfig 后无覆盖请求仍成功", $result['success'] === true);

// 恢复默认配置，避免影响后续测试
XHCurl::setConfig(array(
    'connect_timeout' => 30,
    'request_timeout' => 60,
));

echo "\n总计: $pass passed, $fail failed\n";
exit($fail > 0 ? 1 : 0);
