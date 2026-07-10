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

echo "\n=== 测试结果: $pass 通过, $fail 失败 ===\n";
exit($fail > 0 ? 1 : 0);
