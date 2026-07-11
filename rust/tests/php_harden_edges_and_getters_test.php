<?php
// +----------------------------------------------------------------------+
// | XHCurl 边界硬化与 getter 补充测试                                       |
// |                                                                        |
// | 验证 harden-edges-and-add-getters spec 的 Task 1-10 修复：              |
// |   Task 1: timeout(0) 跳过设置（不立即超时），与全局 config timeout=0 一致 |
// |   Task 2: connectTimeout(0) 不触发 Client 重建（请求仍成功）             |
// |   Task 3: XHThreadPool submit 失败返回错误（队列满时抛异常）              |
// |   Task 4: cookies() 数组值标量类型转换（int/float/bool），非标量抛异常   |
// |   Task 5: multipart() 字段校验（空 name / 非数组元素抛异常）              |
// |   Task 6: body() 非字符串/二进制抛异常                                   |
// |   Task 7: headers() 列表数组（整数键）抛异常                              |
// |   Task 8: xhrun env 标量类型转换（int/float/bool），非标量抛异常          |
// |   Task 9: XHRequest getter 补充（getTimeout/getHeaders/getCookies 等）   |
// |   Task 10: XHMulti/XHThreadPool count()/isEmpty()                        |
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

echo "=== 边界硬化与 getter 补充测试 ===\n";

// ==================================================================
// Task 1: timeout(0) 跳过设置（不立即超时）
//    request.rs to_reqwest/build_request_client 和 curl.rs create_client_builder
//    中 timeout=0 / connect_timeout=0 跳过设置（视为「使用默认/无超时」），
//    避免 Duration::from_secs(0) 立即超时。
// ==================================================================
echo "\n=== Task 1: timeout(0) 跳过设置 ===\n";

// timeout(0) 请求应成功（不立即超时）
$result = XHCurl::createRequest($BASE . '/get')
    ->get()
    ->timeout(0)
    ->execute();
check("timeout(0) 请求成功（不立即超时）", $result['success'] === true);

// timeoutMs(0) 请求应成功
$result = XHCurl::createRequest($BASE . '/get')
    ->get()
    ->timeoutMs(0)
    ->execute();
check("timeoutMs(0) 请求成功（不立即超时）", $result['success'] === true);

// connectTimeout(0) 请求应成功
$result = XHCurl::createRequest($BASE . '/get')
    ->get()
    ->connectTimeout(0)
    ->execute();
check("connectTimeout(0) 请求成功（不立即超时）", $result['success'] === true);

// connectTimeoutMs(0) 请求应成功
$result = XHCurl::createRequest($BASE . '/get')
    ->get()
    ->connectTimeoutMs(0)
    ->execute();
check("connectTimeoutMs(0) 请求成功（不立即超时）", $result['success'] === true);

// 全局 config timeout=0 不立即超时
$origConfig = XHCurl::getConfig();
XHCurl::setConfig(array(
    'connect_timeout' => 0,
    'request_timeout' => 0,
));
$result = XHCurl::createRequest($BASE . '/get')
    ->get()
    ->execute();
check("全局 config timeout=0 请求成功（不立即超时）", $result['success'] === true);

// 恢复默认配置
XHCurl::setConfig($origConfig);

// ==================================================================
// Task 2: connectTimeout(0) 不触发 Client 重建
//    request.rs needs_request_client 中 connect_timeout=0 不触发重建，
//    与 OverrideKey::from_request 的 filter 逻辑一致。
//    验证：connectTimeout(0) 请求成功（Client 复用全局默认，不重建）。
// ==================================================================
echo "\n=== Task 2: connectTimeout(0) 不触发 Client 重建 ===\n";

$result = XHCurl::createRequest($BASE . '/get')
    ->get()
    ->connectTimeout(0)
    ->execute();
check("connectTimeout(0) 请求成功（不重建 Client）", $result['success'] === true);

// connectTimeout(0) + connectTimeoutMs(0) 同时设置也不重建
$result = XHCurl::createRequest($BASE . '/get')
    ->get()
    ->connectTimeout(0)
    ->connectTimeoutMs(0)
    ->execute();
check("connectTimeout(0)+connectTimeoutMs(0) 请求成功", $result['success'] === true);

// ==================================================================
// Task 3: XHThreadPool submit 失败返回错误
//    threadpool.rs execute_all 和 php_ext.rs execute_each 中，
//    submit 失败（队列满）时返回错误而非静默跳过。
//    验证：添加超过队列容量（DEFAULT_QUEUE_CAPACITY=1000）的请求，
//    executeEach 应抛异常含「提交失败」和「队列容量 1000」。
// ==================================================================
echo "\n=== Task 3: XHThreadPool submit 失败返回错误 ===\n";

// 创建超过队列容量（1000）的请求，触发 submit 失败
// 使用 1 worker + 1100 请求：submit loop 同步执行（无 await），
// worker 在 submit loop 期间无法消费，channel 填满后剩余 submit 失败。
$pool = new XHThreadPool(1);
for ($i = 0; $i < 1100; $i++) {
    $pool->add(XHCurl::createRequest($BASE . '/get')->get());
}
$caughtSubmitFail = false;
$msgHasQueueCap = false;
try {
    $pool->executeEach(function($r) {});
} catch (\Throwable $e) {
    $caughtSubmitFail = true;
    $msg = $e->getMessage();
    $msgHasQueueCap = strpos($msg, '提交失败') !== false
                   && strpos($msg, '队列容量 1000') !== false;
}
check("XHThreadPool 队列满 submit 失败抛异常", $caughtSubmitFail);
check("异常 message 含「提交失败」和「队列容量 1000」", $msgHasQueueCap);

// ==================================================================
// Task 4: cookies() 数组值标量类型转换
//    php_ext.rs cookies() 数组分支对齐 form() 类型转换：
//    string/int/float/bool 转字符串，数组/对象/资源抛异常。
// ==================================================================
echo "\n=== Task 4: cookies() 数组值标量类型转换 ===\n";

// int 值 → 转字符串
$result = XHCurl::createRequest($BASE . '/post')
    ->post()
    ->cookies(array('count' => 42, 'session' => 'abc'))
    ->timeout(10)
    ->execute();
check("cookies() int 值不抛异常", $result['success'] === true);

// float 值 → 转字符串
$result = XHCurl::createRequest($BASE . '/post')
    ->post()
    ->cookies(array('rate' => 3.14))
    ->timeout(10)
    ->execute();
check("cookies() float 值不抛异常", $result['success'] === true);

// bool 值 → 转字符串
$result = XHCurl::createRequest($BASE . '/post')
    ->post()
    ->cookies(array('flag' => true, 'flag2' => false))
    ->timeout(10)
    ->execute();
check("cookies() bool 值不抛异常", $result['success'] === true);

// 数组值 → 抛异常
$caughtCookieArray = false;
try {
    XHCurl::createRequest($BASE . '/post')
        ->post()
        ->cookies(array('nested' => array(1, 2, 3)));
} catch (\Throwable $e) {
    $caughtCookieArray = strpos($e->getMessage(), 'cookies') !== false
                      || strpos($e->getMessage(), '标量') !== false;
}
check("cookies() 数组值抛异常", $caughtCookieArray);

// ==================================================================
// Task 5: multipart() 字段校验
//    php_ext.rs multipart() 任一字段非法立即抛异常：
//    非数组元素 / name 缺失或为空 / value 缺失。
// ==================================================================
echo "\n=== Task 5: multipart() 字段校验 ===\n";

// 合法 multipart 不抛异常
$mpOk = true;
try {
    XHCurl::createRequest($BASE . '/post')
        ->post()
        ->multipart(array(
            array('name' => 'field1', 'value' => 'text value'),
            array('name' => 'file1', 'value' => 'file content', 'filename' => 'test.txt', 'content_type' => 'text/plain'),
        ));
} catch (\Throwable $e) {
    $mpOk = false;
}
check("multipart() 合法字段不抛异常", $mpOk);

// 非数组元素 → 抛异常
$caughtMpNotArray = false;
try {
    XHCurl::createRequest($BASE . '/post')
        ->post()
        ->multipart(array(
            array('name' => 'field1', 'value' => 'val1'),
            'invalid_string_field',
        ));
} catch (\Throwable $e) {
    $caughtMpNotArray = strpos($e->getMessage(), 'multipart') !== false
                     || strpos($e->getMessage(), '不是数组') !== false;
}
check("multipart() 非数组元素抛异常", $caughtMpNotArray);

// name 缺失 → 抛异常
$caughtMpNoName = false;
try {
    XHCurl::createRequest($BASE . '/post')
        ->post()
        ->multipart(array(
            array('value' => 'val_without_name'),
        ));
} catch (\Throwable $e) {
    $caughtMpNoName = strpos($e->getMessage(), 'name') !== false;
}
check("multipart() name 缺失抛异常", $caughtMpNoName);

// name 为空字符串 → 抛异常
$caughtMpEmptyName = false;
try {
    XHCurl::createRequest($BASE . '/post')
        ->post()
        ->multipart(array(
            array('name' => '', 'value' => 'val'),
        ));
} catch (\Throwable $e) {
    $caughtMpEmptyName = strpos($e->getMessage(), 'name') !== false
                       && strpos($e->getMessage(), '空') !== false;
}
check("multipart() name 为空抛异常", $caughtMpEmptyName);

// value 缺失 → 抛异常
$caughtMpNoValue = false;
try {
    XHCurl::createRequest($BASE . '/post')
        ->post()
        ->multipart(array(
            array('name' => 'field_without_value'),
        ));
} catch (\Throwable $e) {
    $caughtMpNoValue = strpos($e->getMessage(), 'value') !== false;
}
check("multipart() value 缺失抛异常", $caughtMpNoValue);

// ==================================================================
// Task 6: body() 非字符串/二进制抛异常
//    php_ext.rs body() else 分支返回 Err 而非 Vec::new()。
// ==================================================================
echo "\n=== Task 6: body() 非字符串抛异常 ===\n";

// 字符串 body 不抛异常
$bodyOk = true;
try {
    XHCurl::createRequest($BASE . '/post')
        ->post()
        ->body('hello world')
        ->timeout(10)
        ->execute();
} catch (\Throwable $e) {
    $bodyOk = false;
}
check("body() 字符串不抛异常", $bodyOk);

// 数组 body → 抛异常
$caughtBodyArray = false;
try {
    XHCurl::createRequest($BASE . '/post')
        ->post()
        ->body(array(1, 2, 3));
} catch (\Throwable $e) {
    $caughtBodyArray = strpos($e->getMessage(), 'body') !== false
                    && (strpos($e->getMessage(), '字符串') !== false
                        || strpos($e->getMessage(), '二进制') !== false);
}
check("body() 数组抛异常", $caughtBodyArray);

// null body → 抛异常
$caughtBodyNull = false;
try {
    XHCurl::createRequest($BASE . '/post')
        ->post()
        ->body(null);
} catch (\Throwable $e) {
    $caughtBodyNull = strpos($e->getMessage(), 'body') !== false;
}
check("body() null 抛异常", $caughtBodyNull);

// ==================================================================
// Task 7: headers() 列表数组（整数键）抛异常
//    php_ext.rs headers() 检测整数键时抛异常，
//    提示使用关联数组 ['name' => 'value'] 形式。
// ==================================================================
echo "\n=== Task 7: headers() 列表数组校验 ===\n";

// 关联数组不抛异常
$headersOk = true;
try {
    XHCurl::createRequest($BASE . '/get')
        ->get()
        ->headers(array('X-Test' => 'value1', 'X-Test2' => 'value2'));
} catch (\Throwable $e) {
    $headersOk = false;
}
check("headers() 关联数组不抛异常", $headersOk);

// 列表数组（整数键）→ 抛异常
$caughtHeadersList = false;
try {
    XHCurl::createRequest($BASE . '/get')
        ->get()
        ->headers(array('X-Bad-Header', 'X-Another-Bad'));
} catch (\Throwable $e) {
    $caughtHeadersList = strpos($e->getMessage(), '列表数组') !== false
                      || strpos($e->getMessage(), '整数键') !== false
                      || strpos($e->getMessage(), '关联数组') !== false;
}
check("headers() 列表数组抛异常", $caughtHeadersList);

// 混合数组（部分整数键）→ 抛异常
$caughtHeadersMixed = false;
try {
    XHCurl::createRequest($BASE . '/get')
        ->get()
        ->headers(array('X-Good' => 'ok', 'bad_list_item'));
} catch (\Throwable $e) {
    $caughtHeadersMixed = strpos($e->getMessage(), '列表数组') !== false
                       || strpos($e->getMessage(), '整数键') !== false;
}
check("headers() 混合数组（含整数键）抛异常", $caughtHeadersMixed);

// ==================================================================
// Task 8: xhrun env 标量类型转换
//    php_ext.rs xhrun env 对齐 form() 类型转换：
//    string/int/float/bool 转字符串，数组/对象/资源抛异常。
// ==================================================================
echo "\n=== Task 8: xhrun env 标量类型转换 ===\n";

// int 值 → 转字符串，命令可见
$r = xhrun('echo $TESTVAR', [], ['shell' => true, 'env' => ['TESTVAR' => 42]]);
check("xhrun env int 值转字符串", trim($r['stdout']) === '42');

// float 值 → 转字符串
$r = xhrun('echo $TESTVAR', [], ['shell' => true, 'env' => ['TESTVAR' => 3.14]]);
check("xhrun env float 值转字符串", trim($r['stdout']) === '3.14');

// bool true → 转字符串 "1"
$r = xhrun('echo $TESTVAR', [], ['shell' => true, 'env' => ['TESTVAR' => true]]);
check("xhrun env bool true 转字符串 1", trim($r['stdout']) === '1');

// bool false → 转字符串 "0"
$r = xhrun('echo $TESTVAR', [], ['shell' => true, 'env' => ['TESTVAR' => false]]);
check("xhrun env bool false 转字符串 0", trim($r['stdout']) === '0');

// 字符串值 → 正常传递
$r = xhrun('echo $TESTVAR', [], ['shell' => true, 'env' => ['TESTVAR' => 'hello']]);
check("xhrun env 字符串值正常传递", trim($r['stdout']) === 'hello');

// 数组值 → 抛异常
$caughtEnvArray = false;
try {
    xhrun('echo $TESTVAR', [], ['shell' => true, 'env' => ['TESTVAR' => array(1, 2, 3)]]);
} catch (\Throwable $e) {
    $caughtEnvArray = strpos($e->getMessage(), 'env') !== false
                   && strpos($e->getMessage(), '标量') !== false;
}
check("xhrun env 数组值抛异常", $caughtEnvArray);

// ==================================================================
// Task 9: XHRequest getter 补充
//    php_ext.rs 新增 getTimeout/getConnectTimeout/getHeaders/getCookies/
//    getProxy/getVerifySsl/getUserAgent/getId/getUserData。
// ==================================================================
echo "\n=== Task 9: XHRequest getter 补充 ===\n";

// getTimeout / getConnectTimeout
$req = XHCurl::createRequest($BASE . '/get')->get()->timeout(10)->connectTimeout(5);
check("getTimeout() 返回设置的值", $req->getTimeout() === 10);
check("getConnectTimeout() 返回设置的值", $req->getConnectTimeout() === 5);

// 未设置时返回 null
$req2 = XHCurl::createRequest($BASE . '/get')->get();
check("getTimeout() 未设置返回 null", $req2->getTimeout() === null);
check("getConnectTimeout() 未设置返回 null", $req2->getConnectTimeout() === null);

// getHeaders
$req3 = XHCurl::createRequest($BASE . '/get')
    ->get()
    ->header('X-Test', 'value1')
    ->header('X-Another', 'value2');
$headers = $req3->getHeaders();
check("getHeaders() 返回数组", is_array($headers));
check("getHeaders() 含设置的 header（小写键）", isset($headers['x-test']) && $headers['x-test'] === 'value1');
check("getHeaders() 含另一个 header", isset($headers['x-another']) && $headers['x-another'] === 'value2');
check("getHeaders() 空 header 返回空数组", count(XHCurl::createRequest($BASE . '/get')->get()->getHeaders()) === 0);

// getCookies
$req4 = XHCurl::createRequest($BASE . '/get')->get()->cookies('session=abc; lang=zh');
check("getCookies() 返回字符串 cookie", $req4->getCookies() === 'session=abc; lang=zh');
check("getCookies() 未设置返回 null", XHCurl::createRequest($BASE . '/get')->get()->getCookies() === null);

// getCookies 数组形式
$req4b = XHCurl::createRequest($BASE . '/get')->get()->cookies(array('session' => 'abc', 'lang' => 'zh'));
$cookieStr = $req4b->getCookies();
check("getCookies() 数组形式返回拼接字符串", $cookieStr !== null && strpos($cookieStr, 'session=abc') !== false && strpos($cookieStr, 'lang=zh') !== false);

// getProxy
$req5 = XHCurl::createRequest($BASE . '/get')->get()->proxy('http://127.0.0.1:7890');
check("getProxy() 返回设置的代理", $req5->getProxy() === 'http://127.0.0.1:7890');
check("getProxy() 未设置返回 null", XHCurl::createRequest($BASE . '/get')->get()->getProxy() === null);

// getVerifySsl
$req6 = XHCurl::createRequest($BASE . '/get')->get()->verifySsl(false);
check("getVerifySsl() 返回 false", $req6->getVerifySsl() === false);
$req6b = XHCurl::createRequest($BASE . '/get')->get()->verifySsl(true);
check("getVerifySsl() 返回 true", $req6b->getVerifySsl() === true);
check("getVerifySsl() 未设置返回 null", XHCurl::createRequest($BASE . '/get')->get()->getVerifySsl() === null);

// getUserAgent
$req7 = XHCurl::createRequest($BASE . '/get')->get()->userAgent('MyCustomUA/1.0');
check("getUserAgent() 返回设置的 UA", $req7->getUserAgent() === 'MyCustomUA/1.0');
check("getUserAgent() 未设置返回 null", XHCurl::createRequest($BASE . '/get')->get()->getUserAgent() === null);

// getId
$req8 = XHCurl::createRequest($BASE . '/get')->get()->id('req-abc-123');
check("getId() 返回设置的 ID", $req8->getId() === 'req-abc-123');
check("getId() 未设置返回 null", XHCurl::createRequest($BASE . '/get')->get()->getId() === null);

// getUserData
$req9 = XHCurl::createRequest($BASE . '/get')->get()->userData(array('key' => 'val', 'num' => 42));
$ud = $req9->getUserData();
check("getUserData() 返回非 null", $ud !== null);
check("getUserData() 含 JSON 序列化数据", strpos($ud, 'key') !== false && strpos($ud, 'val') !== false);
check("getUserData() 未设置返回 null", XHCurl::createRequest($BASE . '/get')->get()->getUserData() === null);

// getUrl / getMethod（已有 getter 回归验证）
check("getUrl() 返回 URL", XHCurl::createRequest($BASE . '/get')->get()->getUrl() === $BASE . '/get');
check("getMethod() 返回方法", XHCurl::createRequest($BASE . '/get')->get()->getMethod() === 'GET');

// ==================================================================
// Task 10: XHMulti/XHThreadPool count()/isEmpty()
//    php_ext.rs 新增 count() 返回待执行请求数，isEmpty() 返回是否为空。
// ==================================================================
echo "\n=== Task 10: XHMulti/XHThreadPool count()/isEmpty() ===\n";

// XHMulti count/isEmpty
$multi = new XHMulti();
check("XHMulti isEmpty() 初始为空", $multi->isEmpty() === true);
check("XHMulti count() 初始为 0", $multi->count() === 0);

$multi->add(XHCurl::createRequest($BASE . '/get')->get());
$multi->add(XHCurl::createRequest($BASE . '/get')->get());
check("XHMulti count() 添加后为 2", $multi->count() === 2);
check("XHMulti isEmpty() 添加后非空", $multi->isEmpty() === false);

$multi3 = new XHMulti();
$multi3->add(XHCurl::createRequest($BASE . '/get')->get());
$multi3->add(XHCurl::createRequest($BASE . '/get')->get());
$multi3->add(XHCurl::createRequest($BASE . '/get')->get());
check("XHMulti count() 三个请求为 3", $multi3->count() === 3);

// XHThreadPool count/isEmpty（仅 CLI 模式）
$pool = new XHThreadPool(2);
check("XHThreadPool isEmpty() 初始为空", $pool->isEmpty() === true);
check("XHThreadPool count() 初始为 0", $pool->count() === 0);

$pool->add(XHCurl::createRequest($BASE . '/get')->get());
$pool->add(XHCurl::createRequest($BASE . '/get')->get());
check("XHThreadPool count() 添加后为 2", $pool->count() === 2);
check("XHThreadPool isEmpty() 添加后非空", $pool->isEmpty() === false);

echo "\n总计: $pass passed, $fail failed\n";
exit($fail > 0 ? 1 : 0);
