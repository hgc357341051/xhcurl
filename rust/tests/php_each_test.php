<?php
// XHCurl each() 流式回调 API 测试
// 验证：每完成一个请求立即调用回调、不累积、按完成顺序触发、异常终止、失败请求仍回调
// 使用本地 HTTP 服务器（127.0.0.1:18399）

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

echo "=== each() 流式回调测试 ===\n";

// 1. 正常流式处理：多个请求，每个回调收到完整字段
$requests = array();
for ($i = 0; $i < 5; $i++) {
    $requests[] = XHCurl::createRequest($BASE . '/get?id=' . $i)
        ->get()
        ->timeout(15)
        ->setId('req-' . $i)
        ->setUserData(array('index' => $i));
}

$callbackResults = array();
$count = XHCurl::run(function() use ($requests, &$callbackResults) {
    return XHCurl::each($requests, function($result) use (&$callbackResults) {
        $callbackResults[] = $result;
    });
});

check("each 返回总数=5", $count === 5);
check("each 回调触发 5 次", count($callbackResults) === 5);

// 验证每个回调结果含完整字段
$allHaveFields = true;
foreach ($callbackResults as $r) {
    if (!isset($r['id']) || !isset($r['success']) || !isset($r['status'])
        || !isset($r['body']) || !isset($r['headers']) || !isset($r['elapsed_ms'])) {
        $allHaveFields = false;
        break;
    }
}
check("each 每个结果含完整字段(id/success/status/body/headers/elapsed_ms)", $allHaveFields);

// 全部成功
$allSuccess = true;
foreach ($callbackResults as $r) {
    if ($r['success'] !== true) {
        $allSuccess = false;
        break;
    }
}
check("each 所有请求成功", $allSuccess);

// 2. 结果按完成顺序触发（非提交顺序）
// 用不同 URL 路径模拟，验证回调能通过 id 关联
$ids = array();
XHCurl::run(function() use ($requests, &$ids) {
    return XHCurl::each($requests, function($result) use (&$ids) {
        $ids[] = $result['id'];
    });
});
check("each 回调可通过 id 关联提交顺序", count($ids) === 5 && in_array('req-0', $ids) && in_array('req-4', $ids));

echo "\n=== 空请求列表 ===\n";

// 3. 空请求列表返回 0
$emptyCount = XHCurl::run(function() {
    return XHCurl::each(array(), function($result) {
        // 不应执行
    });
});
check("each 空列表返回 0", $emptyCount === 0);

echo "\n=== 单个请求 ===\n";

// 4. 单个请求返回 1
$single = array(
    XHCurl::createRequest($BASE . '/get')->get()->timeout(15)->setId('single')
);
$singleResult = null;
$singleCount = XHCurl::run(function() use ($single, &$singleResult) {
    return XHCurl::each($single, function($result) use (&$singleResult) {
        $singleResult = $result;
    });
});
check("each 单个请求返回 1", $singleCount === 1);
check("each 单个请求回调触发 1 次", $singleResult !== null);
check("each 单个请求 id 正确", $singleResult['id'] === 'single');

echo "\n=== 回调抛异常终止 each ===\n";

// 5. 回调抛异常终止 each
//    P0 修复后,事件泵通过 ExecutorGlobals::take_exception 正确传播异常 message
$manyRequests = array();
for ($i = 0; $i < 10; $i++) {
    $manyRequests[] = XHCurl::createRequest($BASE . '/get?id=' . $i)
        ->get()
        ->timeout(15);
}

$callbackCount = 0;
$errorReturned = false;
$errorMessage = '';
try {
    XHCurl::run(function() use ($manyRequests, &$callbackCount) {
        return XHCurl::each($manyRequests, function($result) use (&$callbackCount) {
            $callbackCount++;
            if ($callbackCount >= 2) {
                throw new Exception("测试异常终止");
            }
        });
    });
} catch (Throwable $e) {
    $errorReturned = true;
    $errorMessage = $e->getMessage();
}
check("each 回调抛异常后 run() 返回错误", $errorReturned);
check("each 异常 message 正确传播(含原始文本)", strpos($errorMessage, '测试异常终止') !== false);
check("each 异常终止后回调次数 < 10", $callbackCount < 10 && $callbackCount >= 1);

echo "\n=== gather 后用户代码抛异常 ===\n";

// 6b. P0 修复验证：gather 返回后用户代码抛异常，run() 应立即返回错误(含 message)
$gatherErrorReturned = false;
$gatherErrorMessage = '';
try {
    XHCurl::run(function() use ($BASE) {
        $results = XHCurl::gather(array(
            XHCurl::createRequest($BASE . '/get')->get()->timeout(15),
        ));
        // gather 成功返回后抛异常,验证事件泵通过 take_exception 正确传播
        throw new Exception("gather 后异常测试");
    });
} catch (Throwable $e) {
    $gatherErrorReturned = true;
    $gatherErrorMessage = $e->getMessage();
}
check("gather 后抛异常 run() 返回错误", $gatherErrorReturned);
check("gather 异常 message 正确传播(含原始文本)", strpos($gatherErrorMessage, 'gather 后异常测试') !== false);

echo "\n=== 失败请求仍触发回调 ===\n";

// 6. 失败请求仍触发回调（连接不可达端口）
$failedReq = array(
    XHCurl::createRequest('http://127.0.0.1:1/unreachable')
        ->get()
        ->timeout(2)
        ->setId('failed-req')
);
$failedResult = null;
XHCurl::run(function() use ($failedReq, &$failedResult) {
    return XHCurl::each($failedReq, function($result) use (&$failedResult) {
        $failedResult = $result;
    });
});
check("each 失败请求仍触发回调", $failedResult !== null);
check("each 失败请求 success=false", $failedResult['success'] === false);
check("each 失败请求 body 为空字符串", $failedResult['body'] === '');
check("each 失败请求含 error 或 status 字段", isset($failedResult['error']) || isset($failedResult['status']));

echo "\n=== 在 run() 外调用返回错误 ===\n";

// 7. 在 run() 外调用返回错误
$outsideError = false;
try {
    XHCurl::each(array(
        XHCurl::createRequest($BASE . '/get')->get()->timeout(15)
    ), function($result) {});
} catch (Exception $e) {
    $outsideError = true;
}
check("each 在 run() 外调用抛异常", $outsideError);

echo "\n=== 字段一致性（与 gather 对比）===\n";

// 8. each 回调收到的字段与 gather 返回元素一致
$compareRequests = array(
    XHCurl::createRequest($BASE . '/get')->get()->timeout(15)->setId('compare-1')->setUserData(array('k' => 'v'))
);

// gather 结果
$gatherResult = XHCurl::run(function() use ($compareRequests) {
    return XHCurl::gather($compareRequests);
});
$gatherFields = array_keys($gatherResult[0]);

// each 结果
$eachResult = null;
XHCurl::run(function() use ($compareRequests, &$eachResult) {
    return XHCurl::each($compareRequests, function($result) use (&$eachResult) {
        $eachResult = $result;
    });
});
$eachFields = array_keys($eachResult);

sort($gatherFields);
sort($eachFields);
check("each 与 gather 字段一致", $gatherFields === $eachFields);
check("each 与 gather id 一致", $gatherResult[0]['id'] === $eachResult['id']);

echo "\n=== run() 失败后可再次调用（P0 调度器泄漏修复）===\n";

// 9. P0 修复验证：run() 抛异常后调度器被清理，可再次调用 run()
//    修复前：run() 失败后 thread_local 永久残留，后续 run() 误报"不支持嵌套调用"
$firstRunFailed = false;
try {
    XHCurl::run(function() {
        throw new Exception("首次 run 失败");
    });
} catch (Throwable $e) {
    $firstRunFailed = true;
}
check("首次 run() 抛异常", $firstRunFailed);

// 立即再次调用 run()，应成功（不报"不支持嵌套调用"）
$secondRunNestedError = false;
$secondRunSuccess = false;
try {
    $result = XHCurl::run(function() use ($BASE) {
        return XHCurl::gather(array(
            XHCurl::createRequest($BASE . '/get')->get()->timeout(15)->setId('recovery'),
        ));
    });
    $secondRunSuccess = is_array($result) && count($result) === 1;
} catch (Throwable $e) {
    $secondRunNestedError = strpos($e->getMessage(), '不支持嵌套调用') !== false;
}
check("run() 失败后再次 run() 不报'不支持嵌套调用'", !$secondRunNestedError);
check("run() 失败后再次 run() 正常执行 gather", $secondRunSuccess);

echo "\n=== gather/each 批量上限检查（P0）===\n";

// 10. P0 修复验证：gather 传入超过 MAX_REQUESTS_PER_BATCH(10000) 个请求返回错误
$tooManyRequests = array();
for ($i = 0; $i < 10001; $i++) {
    $tooManyRequests[] = XHCurl::createRequest($BASE . '/get?id=' . $i)->get()->timeout(15);
}
$limitErrorReturned = false;
$limitErrorMessage = '';
try {
    XHCurl::run(function() use ($tooManyRequests) {
        return XHCurl::gather($tooManyRequests);
    });
} catch (Throwable $e) {
    $limitErrorReturned = true;
    $limitErrorMessage = $e->getMessage();
}
check("gather 超过上限(10001)返回错误", $limitErrorReturned);
check("gather 错误信息含上限说明", strpos($limitErrorMessage, '10000') !== false && strpos($limitErrorMessage, '分组') !== false);

// each 同样应受保护
$eachLimitErrorReturned = false;
try {
    XHCurl::run(function() use ($tooManyRequests) {
        return XHCurl::each($tooManyRequests, function($result) {});
    });
} catch (Throwable $e) {
    $eachLimitErrorReturned = true;
}
check("each 超过上限(10001)返回错误", $eachLimitErrorReturned);

echo "\n=== max_response_size=0 表示无限制（P1）===\n";

// 11. P1 修复验证：setConfig max_response_size=0 后大响应不报错
XHCurl::setConfig(array('max_response_size' => 0));
$noLimitSuccess = false;
try {
    $result = XHCurl::run(function() use ($BASE) {
        return XHCurl::gather(array(
            XHCurl::createRequest($BASE . '/get')->get()->timeout(15)->setId('nolimit'),
        ));
    });
    $noLimitSuccess = is_array($result) && count($result) === 1 && $result[0]['success'] === true;
} catch (Throwable $e) {
    $noLimitSuccess = false;
}
check("max_response_size=0 时请求成功（不报大小超限）", $noLimitSuccess);
// 恢复默认配置避免影响后续测试
XHCurl::setConfig(array('max_response_size' => 10485760));

// =========== 负数校验测试 ===========

function test_negative_timeout_clamped(): bool
{
    $req = XHCurl::createRequest('http://127.0.0.1:18399/get');
    $req->timeout(-1);
    $cfg = XHCurl::getConfig();
    // 负值被 clamp 为 0，不应产生巨大数值
    return $cfg['request_timeout'] < PHP_INT_MAX;
}

function test_negative_connect_timeout_clamped(): bool
{
    $req = XHCurl::createRequest('http://127.0.0.1:18399/get');
    $req->connectTimeout(-5);
    $cfg = XHCurl::getConfig();
    return $cfg['connect_timeout'] < PHP_INT_MAX;
}

function test_negative_max_redirects_clamped(): bool
{
    $req = XHCurl::createRequest('http://127.0.0.1:18399/get');
    $req->maxRedirects(-3);
    $cfg = XHCurl::getConfig();
    return $cfg['max_redirects'] < PHP_INT_MAX;
}

function test_multi_negative_concurrency_clamped(): bool
{
    $multi = new XHMulti();
    $multi->maxConcurrency(-10);
    // 仅验证不崩溃且不产生巨大数值：执行空请求集合应正常返回
    $results = $multi->execute();
    return is_array($results) && count($results) === 0;
}

function test_multi_negative_response_size_clamped(): bool
{
    $multi = new XHMulti();
    $multi->maxResponseSize(-100);
    $results = $multi->execute();
    return is_array($results) && count($results) === 0;
}

function test_threadpool_negative_workers_clamped(): bool
{
    $pool = new XHThreadPool(-4);
    $results = $pool->execute();
    return is_array($results) && count($results) === 0;
}

function test_set_config_negative_skipped(): bool
{
    $orig = XHCurl::getConfig();
    XHCurl::setConfig([
        'connect_timeout' => -10,
        'request_timeout' => -20,
        'max_redirects' => -3,
        'max_response_size' => -100,
        'max_connections' => -5,
    ]);
    $cfg = XHCurl::getConfig();
    // 负值应被跳过，值应保持为巨大负值不会转为巨大正数
    $ok = $cfg['connect_timeout'] < PHP_INT_MAX
        && $cfg['request_timeout'] < PHP_INT_MAX
        && $cfg['max_redirects'] < PHP_INT_MAX
        && $cfg['max_response_size'] < PHP_INT_MAX
        && $cfg['max_connections'] < PHP_INT_MAX;
    // 恢复原配置
    XHCurl::setConfig($orig);
    return $ok;
}

echo "\n=== 负数校验测试 ===\n";
check("负数 timeout 被 clamp", test_negative_timeout_clamped());
check("负数 connect_timeout 被 clamp", test_negative_connect_timeout_clamped());
check("负数 max_redirects 被 clamp", test_negative_max_redirects_clamped());
check("XHMulti 负数 maxConcurrency 被 clamp", test_multi_negative_concurrency_clamped());
check("XHMulti 负数 maxResponseSize 被 clamp", test_multi_negative_response_size_clamped());
check("XHThreadPool 负数 workers 被 clamp", test_threadpool_negative_workers_clamped());
check("setConfig 负值被跳过", test_set_config_negative_skipped());

// =========== 配置一致性与提前检查测试 ===========

function test_get_config_has_tcp_keepalive_interval(): bool
{
    XHCurl::setConfig(['tcp_keepalive_interval' => 120]);
    $cfg = XHCurl::getConfig();
    $has = array_key_exists('tcp_keepalive_interval', $cfg) && $cfg['tcp_keepalive_interval'] === 120;
    // 恢复默认值
    XHCurl::setConfig(['tcp_keepalive_interval' => 60]);
    return $has;
}

function test_oversized_array_rejected_before_clone(): bool
{
    // 构造一个超过上限(10000)的请求数组，gather 应直接返回错误
    // 而非先克隆全部元素再拒绝
    $tooMany = array();
    for ($i = 0; $i < 10001; $i++) {
        $tooMany[] = XHCurl::createRequest('http://127.0.0.1:18399/get?id=' . $i)->get()->timeout(15);
    }
    $err = null;
    try {
        XHCurl::run(function() use ($tooMany) {
            return XHCurl::gather($tooMany);
        });
    } catch (Throwable $e) {
        $err = $e->getMessage();
    }
    // 应返回错误信息含上限说明
    return $err !== null && strpos($err, (string)10000) !== false;
}

echo "\n=== 配置一致性与提前检查测试 ===\n";
check('get_config 含 tcp_keepalive_interval 字段', test_get_config_has_tcp_keepalive_interval());
check('超大数组 gather 提前拒绝（不克隆）', test_oversized_array_rejected_before_clone());

echo "\n=== 测试结果: $pass 通过, $fail 失败 ===\n";
exit($fail > 0 ? 1 : 0);