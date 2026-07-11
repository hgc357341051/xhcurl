<?php
// +----------------------------------------------------------------------+
// | XHCurl PHP 使用者视角 API 一致性测试                                  |
// |                                                                        |
// | 验证 align-php-user-api-consistency spec 的 P1/P2 修复：                |
// |   P1-1: cookies() 数组整数键抛异常（与 headers() 一致）                  |
// |   P1-2: maxConcurrency(0) 文档对齐实现（使用 CPU 核心数，非"无限制"）    |
// |   P1-3: XHThreadPool 构造函数负值抛异常（与 maxConcurrency setter 一致）  |
// |   P1-4: 补全 6 个 getter（getBasicAuth/getBearerToken/getFollowRedirects |
// |         /getMaxRedirects/getEncoding/getRange）                          |
// |   P1-5: xhrun 失败路径补 error_type（timeout/output_too_large/exit_error）|
// |   P2-1: bearerToken('') 空值校验（与 basicAuth 一致）                    |
// |   P2-2: xhrun 超时 kill 进程组（Unix，防 shell 模式孙进程泄漏）          |
// |   P2-3: executeEach 空请求列表抛异常（与 execute() 一致）                |
// |                                                                        |
// | 注意：本文件多数用例为本地行为校验（getter/构造/校验/xhrun），不需 mock  |
// | 服务器。$BASE 仅用于构造请求 URL（不实际发起请求）。                      |
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

echo "=== PHP 使用者视角 API 一致性测试 ===\n";

// ==================================================================
// P1-1: cookies() 数组整数键抛异常
//    php_ext.rs cookies() 数组分支遇到整数键（列表数组）抛异常，
//    错误信息含 "cookies" 和 "整数键"/"关联数组" 关键词，与 headers() 一致。
// ==================================================================
echo "\n=== P1-1: cookies() 数组整数键抛异常 ===\n";

function test_cookies_integer_keys_throw(): bool
{
    global $BASE;
    try {
        XHCurl::createRequest($BASE . '/get')
            ->get()
            ->cookies(array('foo', 'bar')); // 列表数组（整数键 0/1）
        return false; // 应抛异常
    } catch (\Throwable $e) {
        $msg = $e->getMessage();
        return strpos($msg, 'cookies') !== false
            && (strpos($msg, '整数键') !== false || strpos($msg, '关联数组') !== false);
    }
}
check("cookies() 列表数组抛异常", test_cookies_integer_keys_throw());

// 关联数组不抛异常（正常设置）
$cookieOk = true;
try {
    XHCurl::createRequest($BASE . '/get')
        ->get()
        ->cookies(array('name' => 'value', 'name2' => 'value2'));
} catch (\Throwable $e) {
    $cookieOk = false;
}
check("cookies() 关联数组不抛异常", $cookieOk);

// ==================================================================
// P1-2 + P1-3: XHThreadPool 构造函数
//    __construct(0) 或不传参 → 使用默认（CPU 核心数），不抛异常
//    __construct(负值) → 抛异常（与 maxConcurrency setter 一致）
// ==================================================================
echo "\n=== P1-3: XHThreadPool 构造函数负值校验 ===\n";

function test_threadpool_construct_zero_ok(): bool
{
    // 0 = 使用默认（CPU 核心数），不抛异常
    try {
        $p = new XHThreadPool(0);
        return $p !== null;
    } catch (\Throwable $e) {
        return false;
    }
}
check("XHThreadPool(0) 构造成功", test_threadpool_construct_zero_ok());

// 不传参也 OK
$noArgOk = true;
try {
    $p = new XHThreadPool();
} catch (\Throwable $e) {
    $noArgOk = false;
}
check("XHThreadPool() 不传参构造成功", $noArgOk);

function test_threadpool_construct_negative_throws(): bool
{
    // 负值抛异常，错误信息含 "workers" 和 "负值"
    try {
        new XHThreadPool(-1);
        return false; // 应抛异常
    } catch (\Throwable $e) {
        $msg = $e->getMessage();
        return strpos($msg, 'workers') !== false && strpos($msg, '负值') !== false;
    }
}
check("XHThreadPool(-1) 抛异常", test_threadpool_construct_negative_throws());

// maxConcurrency(0) 不抛异常（使用默认 CPU 核心数）
$mcZeroOk = true;
try {
    $p = new XHThreadPool(2);
    $p->maxConcurrency(0);
} catch (\Throwable $e) {
    $mcZeroOk = false;
}
check("maxConcurrency(0) 不抛异常（使用默认）", $mcZeroOk);

// maxConcurrency(-1) 抛异常
function test_max_concurrency_negative_throws(): bool
{
    try {
        $p = new XHThreadPool(2);
        $p->maxConcurrency(-1);
        return false;
    } catch (\Throwable $e) {
        $msg = $e->getMessage();
        return strpos($msg, 'maxConcurrency') !== false || strpos($msg, '负值') !== false;
    }
}
check("maxConcurrency(-1) 抛异常", test_max_concurrency_negative_throws());

// ==================================================================
// P1-4: 补全 6 个 getter
//    每个 setter 对应一个 getter，设置后能取回，未设置返回 null。
// ==================================================================
echo "\n=== P1-4: 补全 6 个 getter ===\n";

// getBasicAuth
function test_get_basic_auth(): bool
{
    global $BASE;
    $req = XHCurl::createRequest($BASE . '/get')->get()->basicAuth('user:pass');
    if ($req->getBasicAuth() !== 'user:pass') {
        return false;
    }
    // 未设置返回 null
    $req2 = XHCurl::createRequest($BASE . '/get')->get();
    return $req2->getBasicAuth() === null;
}
check("getBasicAuth() 设置后取回 + 未设置返回 null", test_get_basic_auth());

// getBearerToken
function test_get_bearer_token(): bool
{
    global $BASE;
    $req = XHCurl::createRequest($BASE . '/get')->get()->bearerToken('abc123');
    if ($req->getBearerToken() !== 'abc123') {
        return false;
    }
    $req2 = XHCurl::createRequest($BASE . '/get')->get();
    return $req2->getBearerToken() === null;
}
check("getBearerToken() 设置后取回 + 未设置返回 null", test_get_bearer_token());

// getFollowRedirects
function test_get_follow_redirects(): bool
{
    global $BASE;
    $req = XHCurl::createRequest($BASE . '/get')->get()->followRedirects(true);
    if ($req->getFollowRedirects() !== true) {
        return false;
    }
    $req2 = XHCurl::createRequest($BASE . '/get')->get()->followRedirects(false);
    if ($req2->getFollowRedirects() !== false) {
        return false;
    }
    $req3 = XHCurl::createRequest($BASE . '/get')->get();
    return $req3->getFollowRedirects() === null;
}
check("getFollowRedirects() 设置后取回 + 未设置返回 null", test_get_follow_redirects());

// getMaxRedirects
function test_get_max_redirects(): bool
{
    global $BASE;
    $req = XHCurl::createRequest($BASE . '/get')->get()->maxRedirects(5);
    if ($req->getMaxRedirects() !== 5) {
        return false;
    }
    $req2 = XHCurl::createRequest($BASE . '/get')->get();
    return $req2->getMaxRedirects() === null;
}
check("getMaxRedirects() 设置后取回 + 未设置返回 null", test_get_max_redirects());

// getEncoding
function test_get_encoding(): bool
{
    global $BASE;
    $req = XHCurl::createRequest($BASE . '/get')->get()->encoding('gzip, deflate');
    if ($req->getEncoding() !== 'gzip, deflate') {
        return false;
    }
    $req2 = XHCurl::createRequest($BASE . '/get')->get();
    return $req2->getEncoding() === null;
}
check("getEncoding() 设置后取回 + 未设置返回 null", test_get_encoding());

// getRange
function test_get_range(): bool
{
    global $BASE;
    $req = XHCurl::createRequest($BASE . '/get')->get()->range('0-1023');
    if ($req->getRange() !== '0-1023') {
        return false;
    }
    $req2 = XHCurl::createRequest($BASE . '/get')->get();
    return $req2->getRange() === null;
}
check("getRange() 设置后取回 + 未设置返回 null", test_get_range());

// ==================================================================
// P2-1: bearerToken('') 空值校验
//    空字符串抛异常（与 basicAuth 空值校验一致），错误信息含
//    "bearerToken" 和 "空" 关键词。
// ==================================================================
echo "\n=== P2-1: bearerToken 空值校验 ===\n";

function test_bearer_token_empty_throws(): bool
{
    global $BASE;
    try {
        XHCurl::createRequest($BASE . '/get')->get()->bearerToken('');
        return false; // 应抛异常
    } catch (\Throwable $e) {
        $msg = $e->getMessage();
        return strpos($msg, 'bearerToken') !== false && strpos($msg, '空') !== false;
    }
}
check("bearerToken('') 抛异常", test_bearer_token_empty_throws());

// 非空 token 不抛异常
$nonEmptyOk = true;
try {
    XHCurl::createRequest($BASE . '/get')->get()->bearerToken('valid-token');
} catch (\Throwable $e) {
    $nonEmptyOk = false;
}
check("bearerToken('valid-token') 不抛异常", $nonEmptyOk);

// ==================================================================
// P1-5: xhrun 失败路径补 error_type 枚举
//    timeout → error_type='timeout'
//    exit_code != 0（非超时/截断）→ error_type='exit_error'
//    成功路径不含 error_type 字段
// ==================================================================
echo "\n=== P1-5: xhrun 失败路径 error_type ===\n";

function test_xhrun_timeout_error_type(): bool
{
    // sleep 5 秒，1 秒超时
    $r = xhrun('sleep', ['5'], ['timeout' => 1]);
    if ($r['success'] !== false || $r['timed_out'] !== true) {
        return false;
    }
    return isset($r['error_type']) && $r['error_type'] === 'timeout';
}
check("xhrun 超时 error_type=timeout", test_xhrun_timeout_error_type());

function test_xhrun_exit_error_error_type(): bool
{
    // exit 1 → 退出码非 0
    $r = xhrun('sh', ['-c', 'exit 1']);
    if ($r['success'] !== false || $r['exit_code'] !== 1) {
        return false;
    }
    return isset($r['error_type']) && $r['error_type'] === 'exit_error';
}
check("xhrun 退出码非0 error_type=exit_error", test_xhrun_exit_error_error_type());

function test_xhrun_success_no_error_type(): bool
{
    // 成功路径不含 error_type 字段
    $r = xhrun('echo', ['hello']);
    if ($r['success'] !== true) {
        return false;
    }
    return !array_key_exists('error_type', $r);
}
check("xhrun 成功不含 error_type", test_xhrun_success_no_error_type());

// 截断 → error_type=output_too_large
$r = xhrun('sh', ['-c', 'yes A | head -c 1000'], ['max_output' => 100]);
check("xhrun 截断 error_type=output_too_large", $r['success'] === false && isset($r['error_type']) && $r['error_type'] === 'output_too_large');

// ==================================================================
// P2-2: xhrun 超时 kill 进程组（Unix，防 shell 模式孙进程泄漏）
//    shell 模式 `sleep 60 & sleep 120`：sh 后台启动 sleep 60，前台 sleep 120
//    timeout=1 后 killpg 杀整个进程组（含后台 sleep 60），无残留孙进程。
// ==================================================================
echo "\n=== P2-2: xhrun 超时 kill 进程组 ===\n";

function test_xhrun_shell_timeout_kills_grandchildren(): bool
{
    // 用 pgrep -x sleep 精确匹配进程名 "sleep"（不能用 -f "sleep 60"，
    // 因为 pgrep 自身命令行含 "sleep 60" 会误匹配自身，导致假阳性）
    // 先快照现有 sleep 进程 PID，xhrun 后再快照，新增 PID 应为 0
    $before = @shell_exec('pgrep -x sleep 2>/dev/null');
    $beforePids = $before === null ? array() : array_filter(explode("\n", trim($before)));

    // shell 模式：sleep 60 后台运行，sleep 120 前台运行
    $r = xhrun('sleep 60 & sleep 120', [], ['shell' => true, 'timeout' => 1]);
    if ($r['success'] !== false || $r['timed_out'] !== true) {
        return false;
    }
    // 等待 killpg 生效与进程回收
    usleep(500000);
    $after = @shell_exec('pgrep -x sleep 2>/dev/null');
    $afterPids = $after === null ? array() : array_filter(explode("\n", trim($after)));
    // 新增 PID（killpg 应已杀掉 sleep 60/120，无残留）
    $newPids = array_diff($afterPids, $beforePids);
    return count($newPids) === 0;
}
check("xhrun shell 超时杀进程组（无残留孙进程）", test_xhrun_shell_timeout_kills_grandchildren());

// ==================================================================
// P2-3: executeEach 空请求列表抛异常
//    XHMulti::executeEach 和 XHThreadPool::executeEach 空请求
//    抛异常（与 execute() 一致），不再返回 Ok(0)。
// ==================================================================
echo "\n=== P2-3: executeEach 空请求抛异常 ===\n";

function test_multi_execute_each_empty_throws(): bool
{
    $multi = new XHMulti();
    try {
        $multi->executeEach(function ($result) {});
        return false; // 应抛异常
    } catch (\Throwable $e) {
        return strpos($e->getMessage(), '没有待执行请求') !== false;
    }
}
check("XHMulti executeEach 空请求抛异常", test_multi_execute_each_empty_throws());

function test_threadpool_execute_each_empty_throws(): bool
{
    $pool = new XHThreadPool(2);
    try {
        $pool->executeEach(function ($result) {});
        return false; // 应抛异常
    } catch (\Throwable $e) {
        return strpos($e->getMessage(), '没有待执行请求') !== false;
    }
}
check("XHThreadPool executeEach 空请求抛异常", test_threadpool_execute_each_empty_throws());

echo "\n总计: $pass passed, $fail failed\n";
exit($fail > 0 ? 1 : 0);
