<?php
// XHCurl 响应体分块流式回调测试（onChunk / onHeaders）
// 验证：executeEach 的可选 $onChunk/$onHeaders 参数
//   - onChunk 回调触发且 chunk 拼接后等于完整 body
//   - onHeaders 回调触发且 status/headers 正确
//   - XHMulti 和 XHThreadPool 两条路径都验证
//   - 不传可选参数时行为不变（回归）
//   - 回调异常时中止剩余任务
// 使用本地 HTTP 服务器（127.0.0.1:18399，提供 /get 和 /stream 端点）

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

echo "=== 响应体分块流式回调测试（onChunk / onHeaders）===\n";

// ============================================================
// XHMulti 路径
// ============================================================
echo "\n--- XHMulti::executeEach ---\n";

// 1. onChunk 回调触发，chunk 拼接后等于完整 body
$multi = new XHMulti();
$multi->add(XHCurl::createRequest($BASE . '/stream?n=20&size=1024')
    ->get()
    ->timeout(15)
    ->setId('multi-stream-1'));

$multiChunks = array();
$multiResultBody = null;
$multiResultId = null;
$multiResultCount = $multi->executeEach(
    // $onResult
    function($result) use (&$multiResultBody, &$multiResultId) {
        $multiResultBody = $result['body'];
        $multiResultId = $result['id'];
    },
    // $onChunk
    function($requestId, $chunk) use (&$multiChunks) {
        $multiChunks[] = $chunk;
    }
);

check("multi onResult 回调触发（返回 1）", $multiResultCount === 1);
check("multi onChunk 至少触发 1 次", count($multiChunks) >= 1);
$concatenated = implode('', $multiChunks);
check("multi onChunk 拼接后等于完整 body", $concatenated === $multiResultBody);
check("multi onChunk body 非空", strlen($multiResultBody) > 0);

// 2. onHeaders 回调触发，status 和 headers 正确
$multi2 = new XHMulti();
$multi2->add(XHCurl::createRequest($BASE . '/get')
    ->get()
    ->timeout(15)
    ->setId('multi-headers-1'));

$multiHeadersCalled = false;
$multiHeadersStatus = null;
$multiHeadersArray = null;
$multi2ResultBody = null;
$multi2->executeEach(
    function($result) use (&$multi2ResultBody) {
        $multi2ResultBody = $result['body'];
    },
    null, // 不传 onChunk
    function($requestId, $status, $headers) use (&$multiHeadersCalled, &$multiHeadersStatus, &$multiHeadersArray) {
        $multiHeadersCalled = true;
        $multiHeadersStatus = $status;
        $multiHeadersArray = $headers;
    }
);

check("multi onHeaders 回调触发", $multiHeadersCalled);
check("multi onHeaders status=200", $multiHeadersStatus === 200);
check("multi onHeaders 是数组", is_array($multiHeadersArray));
// /get 端点设置 Content-Type: application/json
// reqwest 将响应头名称规范化为小写
check("multi onHeaders 含 Content-Type",
    isset($multiHeadersArray['Content-Type']) ||
    isset($multiHeadersArray['content-type']));

// 3. 同时传 onChunk 和 onHeaders
$multi3 = new XHMulti();
$multi3->add(XHCurl::createRequest($BASE . '/stream?n=10&size=512')
    ->get()
    ->timeout(15)
    ->setId('multi-both'));
$multi3Chunks = array();
$multi3HeadersStatus = null;
$multi3ResultBody = null;
$multi3->executeEach(
    function($result) use (&$multi3ResultBody) {
        $multi3ResultBody = $result['body'];
    },
    function($requestId, $chunk) use (&$multi3Chunks) {
        $multi3Chunks[] = $chunk;
    },
    function($requestId, $status, $headers) use (&$multi3HeadersStatus) {
        $multi3HeadersStatus = $status;
    }
);
check("multi 同时 onChunk+onHeaders: status=200", $multi3HeadersStatus === 200);
check("multi 同时 onChunk+onHeaders: chunk 拼接等于 body", implode('', $multi3Chunks) === $multi3ResultBody);

// 4. 多请求并发时 onChunk 按 requestId 关联
$multi4 = new XHMulti();
for ($i = 0; $i < 3; $i++) {
    $multi4->add(XHCurl::createRequest($BASE . '/stream?n=5&size=256')
        ->get()
        ->timeout(15)
        ->setId('multi-multi-' . $i));
}
$multi4ChunkIds = array();
$multi4Bodies = array();
$multi4->executeEach(
    function($result) use (&$multi4Bodies) {
        $multi4Bodies[$result['id']] = $result['body'];
    },
    function($requestId, $chunk) use (&$multi4ChunkIds) {
        if (!isset($multi4ChunkIds[$requestId])) {
            $multi4ChunkIds[$requestId] = 0;
        }
        $multi4ChunkIds[$requestId]++;
    }
);
check("multi 多请求并发: 回调覆盖 3 个 requestId", count($multi4ChunkIds) === 3);

// ============================================================
// XHThreadPool 路径
// ============================================================
echo "\n--- XHThreadPool::executeEach ---\n";

// 5. onChunk 回调触发（XHThreadPool）
$pool = new XHThreadPool(4);
$pool->add(XHCurl::createRequest($BASE . '/stream?n=20&size=1024')
    ->get()
    ->timeout(15)
    ->setId('pool-stream-1'));

$poolChunks = array();
$poolResultBody = null;
$poolResultCount = $pool->executeEach(
    function($result) use (&$poolResultBody) {
        $poolResultBody = $result['body'];
    },
    function($requestId, $chunk) use (&$poolChunks) {
        $poolChunks[] = $chunk;
    }
);

check("pool onResult 回调触发（返回 1）", $poolResultCount === 1);
check("pool onChunk 至少触发 1 次", count($poolChunks) >= 1);
check("pool onChunk 拼接后等于完整 body", implode('', $poolChunks) === $poolResultBody);
check("pool onChunk body 非空", strlen($poolResultBody) > 0);

// 6. onHeaders 回调触发（XHThreadPool）
$pool2 = new XHThreadPool(4);
$pool2->add(XHCurl::createRequest($BASE . '/get')
    ->get()
    ->timeout(15)
    ->setId('pool-headers-1'));

$poolHeadersCalled = false;
$poolHeadersStatus = null;
$pool2->executeEach(
    function($result) {},
    null,
    function($requestId, $status, $headers) use (&$poolHeadersCalled, &$poolHeadersStatus) {
        $poolHeadersCalled = true;
        $poolHeadersStatus = $status;
    }
);
check("pool onHeaders 回调触发", $poolHeadersCalled);
check("pool onHeaders status=200", $poolHeadersStatus === 200);

// 7. 同时传 onChunk 和 onHeaders（XHThreadPool）
$pool3 = new XHThreadPool(4);
$pool3->add(XHCurl::createRequest($BASE . '/stream?n=10&size=512')
    ->get()
    ->timeout(15)
    ->setId('pool-both'));
$pool3Chunks = array();
$pool3HeadersStatus = null;
$pool3ResultBody = null;
$pool3->executeEach(
    function($result) use (&$pool3ResultBody) {
        $pool3ResultBody = $result['body'];
    },
    function($requestId, $chunk) use (&$pool3Chunks) {
        $pool3Chunks[] = $chunk;
    },
    function($requestId, $status, $headers) use (&$pool3HeadersStatus) {
        $pool3HeadersStatus = $status;
    }
);
check("pool 同时 onChunk+onHeaders: status=200", $pool3HeadersStatus === 200);
check("pool 同时 onChunk+onHeaders: chunk 拼接等于 body", implode('', $pool3Chunks) === $pool3ResultBody);

// ============================================================
// 回归测试：不传可选参数时行为不变
// ============================================================
echo "\n--- 向后兼容（不传 onChunk/onHeaders）---\n";

// 8. XHMulti 不传可选参数，行为与之前一致
$regMulti = new XHMulti();
$regMulti->add(XHCurl::createRequest($BASE . '/get')->get()->timeout(15)->setId('reg-1'));
$regCallbackResult = null;
$regCount = $regMulti->executeEach(function($result) use (&$regCallbackResult) {
    $regCallbackResult = $result;
});
check("multi 回归: 不传可选参数返回 1", $regCount === 1);
check("multi 回归: onResult 回调触发", $regCallbackResult !== null);
check("multi 回归: success=true", $regCallbackResult['success'] === true);

// 9. XHThreadPool 不传可选参数，行为与之前一致
$regPool = new XHThreadPool(2);
$regPool->add(XHCurl::createRequest($BASE . '/get')->get()->timeout(15)->setId('reg-pool-1'));
$regPoolResult = null;
$regPoolCount = $regPool->executeEach(function($result) use (&$regPoolResult) {
    $regPoolResult = $result;
});
check("pool 回归: 不传可选参数返回 1", $regPoolCount === 1);
check("pool 回归: onResult 回调触发", $regPoolResult !== null);

// ============================================================
// 回调异常中止
// ============================================================
echo "\n--- 回调异常中止 ---\n";

// 10. onChunk 回调抛异常，中止剩余任务
$excMulti = new XHMulti();
$excMulti->add(XHCurl::createRequest($BASE . '/stream?n=50&size=2048')
    ->get()
    ->timeout(15)
    ->setId('exc-stream'));
$excChunkCount = 0;
$excErrorReturned = false;
$excErrorMessage = '';
try {
    $excMulti->executeEach(
        function($result) {},
        function($requestId, $chunk) use (&$excChunkCount) {
            $excChunkCount++;
            if ($excChunkCount >= 2) {
                throw new Exception("onChunk 异常中止测试");
            }
        }
    );
} catch (Throwable $e) {
    $excErrorReturned = true;
    $excErrorMessage = $e->getMessage();
}
check("multi onChunk 异常后返回错误", $excErrorReturned);
check("multi onChunk 异常 message 正确传播", strpos($excErrorMessage, 'onChunk 异常中止测试') !== false);

// 11. onHeaders 回调抛异常，中止剩余任务
$excPool = new XHThreadPool(4);
$excPool->add(XHCurl::createRequest($BASE . '/get')->get()->timeout(15)->setId('exc-pool-headers'));
$excPoolError = false;
$excPoolMsg = '';
try {
    $excPool->executeEach(
        function($result) {},
        null,
        function($requestId, $status, $headers) {
            throw new Exception("onHeaders 异常中止测试");
        }
    );
} catch (Throwable $e) {
    $excPoolError = true;
    $excPoolMsg = $e->getMessage();
}
check("pool onHeaders 异常后返回错误", $excPoolError);
check("pool onHeaders 异常 message 正确传播", strpos($excPoolMsg, 'onHeaders 异常中止测试') !== false);

echo "\n=== 测试结果: $pass 通过, $fail 失败 ===\n";
exit($fail > 0 ? 1 : 0);
