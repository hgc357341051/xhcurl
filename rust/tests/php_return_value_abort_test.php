<?php
// XHCurl 三种流式回调"返回值控制中止"功能测试
//
// 验证：$onResult 回调返回 false（严格 === false）时中止剩余任务并返回已处理数（非错误）。
//   返回其他值（true/null/void/0/''/[]）继续处理；抛异常仍中止（向后兼容）。
//   1. XHMulti 回调返回 false 中止
//   2. XHThreadPool 回调返回 false 中止
//   3. 协程 each 回调返回 false 中止
//   4. XHMulti 回调返回 true 继续（向后兼容）
//   5. 回调返回 null/void 继续（向后兼容）
//   6. 弱类型陷阱避免：回调返回 0/''/[] 继续
//   7. 回调抛异常仍中止（回归）
// 使用本地 HTTP 服务器（127.0.0.1:18399，提供 /get 端点）

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

echo "=== 返回值控制中止 - 三种流式回调功能测试 ===\n";

// ============================================================
// 1. XHMulti 回调返回 false 中止
//    提交 5 个请求，第 2 次回调返回 false
//    断言：executeEach 返回 2（已处理数，int 非 error）；回调仅触发 2 次
// ============================================================
echo "\n--- XHMulti 回调返回 false 中止 ---\n";

$multi = new XHMulti();
for ($i = 0; $i < 5; $i++) {
    $multi->add(XHCurl::createRequest($BASE . '/get?id=' . $i)
        ->get()
        ->timeout(15)
        ->setId('multi-rv-' . $i));
}

$callCount = 0;
$ret = $multi->executeEach(function($result) use (&$callCount) {
    $callCount++;
    if ($callCount === 2) {
        return false; // 第 2 次中止
    }
    // 其他次不返回值（void）
});

check("multi 返回 false 中止：返回已处理数 2（int 非 error）", $ret === 2);
check("multi 返回 false 中止：回调仅触发 2 次", $callCount === 2);

// ============================================================
// 2. XHThreadPool 回调返回 false 中止
//    提交 5 个请求，第 2 次回调返回 false
//    断言：executeEach 返回 2；回调仅触发 2 次
// ============================================================
echo "\n--- XHThreadPool 回调返回 false 中止 ---\n";

$pool = new XHThreadPool();
for ($i = 0; $i < 5; $i++) {
    $pool->add(XHCurl::createRequest($BASE . '/get?id=' . $i)
        ->get()
        ->timeout(15)
        ->setId('pool-rv-' . $i));
}

$callCount = 0;
$ret = $pool->executeEach(function($result) use (&$callCount) {
    $callCount++;
    if ($callCount === 2) {
        return false; // 第 2 次中止
    }
});

check("pool 返回 false 中止：返回已处理数 2（int 非 error）", $ret === 2);
check("pool 返回 false 中止：回调仅触发 2 次", $callCount === 2);

// ============================================================
// 3. 协程 each 回调返回 false 中止
//    用 XHCurl::run(function() { ... }) 包裹
//    提交 5 个请求，第 2 次回调返回 false
//    断言：each() 通过 run 返回 2；回调仅触发 2 次
// ============================================================
echo "\n--- 协程 each 回调返回 false 中止 ---\n";

$fiberRequests = array();
for ($i = 0; $i < 5; $i++) {
    $fiberRequests[] = XHCurl::createRequest($BASE . '/get?id=' . $i)
        ->get()
        ->timeout(15)
        ->setId('fiber-rv-' . $i);
}

$callCount = 0;
$ret = XHCurl::run(function() use ($fiberRequests, &$callCount) {
    return XHCurl::each($fiberRequests, function($result) use (&$callCount) {
        $callCount++;
        if ($callCount === 2) {
            return false; // 第 2 次中止
        }
    });
});

check("fiber 返回 false 中止：返回已处理数 2（int 非 error）", $ret === 2);
check("fiber 返回 false 中止：回调仅触发 2 次", $callCount === 2);

// ============================================================
// 4. XHMulti 回调返回 true 继续（向后兼容）
//    提交 3 个请求，回调每次返回 true
//    断言：executeEach 返回 3；回调触发 3 次
// ============================================================
echo "\n--- XHMulti 回调返回 true 继续（向后兼容）---\n";

$multiTrue = new XHMulti();
for ($i = 0; $i < 3; $i++) {
    $multiTrue->add(XHCurl::createRequest($BASE . '/get?id=' . $i)
        ->get()
        ->timeout(15)
        ->setId('multi-true-' . $i));
}

$callCount = 0;
$ret = $multiTrue->executeEach(function($result) use (&$callCount) {
    $callCount++;
    return true; // 继续处理
});

check("multi 返回 true 继续：返回 3", $ret === 3);
check("multi 返回 true 继续：回调触发 3 次", $callCount === 3);

// ============================================================
// 5. 回调返回 null/void 继续（向后兼容）
//    提交 3 个请求到 XHMulti，回调不返回值（隐式 null）
//    断言：executeEach 返回 3；回调触发 3 次
// ============================================================
echo "\n--- 回调返回 null/void 继续（向后兼容）---\n";

$multiVoid = new XHMulti();
for ($i = 0; $i < 3; $i++) {
    $multiVoid->add(XHCurl::createRequest($BASE . '/get?id=' . $i)
        ->get()
        ->timeout(15)
        ->setId('multi-void-' . $i));
}

$callCount = 0;
$ret = $multiVoid->executeEach(function($result) use (&$callCount) {
    $callCount++;
    // 不返回值（void/隐式 null）
});

check("multi 返回 void/null 继续：返回 3", $ret === 3);
check("multi 返回 void/null 继续：回调触发 3 次", $callCount === 3);

// ============================================================
// 6. 弱类型陷阱避免：回调返回 0/''/[] 继续
//    提交 3 个请求到 XHMulti，回调返回 0（整数零）
//    断言：executeEach 返回 3（不是 1，因为 0 !== false）；回调触发 3 次
// ============================================================
echo "\n--- 弱类型陷阱避免：回调返回 0 继续（0 !== false）---\n";

$multiZero = new XHMulti();
for ($i = 0; $i < 3; $i++) {
    $multiZero->add(XHCurl::createRequest($BASE . '/get?id=' . $i)
        ->get()
        ->timeout(15)
        ->setId('multi-zero-' . $i));
}

$callCount = 0;
$ret = $multiZero->executeEach(function($result) use (&$callCount) {
    $callCount++;
    return 0; // 整数零，严格 !== false，应继续
});

check("multi 返回 0 继续（0 !== false）：返回 3（不是 1）", $ret === 3);
check("multi 返回 0 继续（0 !== false）：回调触发 3 次", $callCount === 3);

// ============================================================
// 7. 回调抛异常仍中止（回归）
//    提交 5 个请求到 XHMulti，回调第 2 次抛 RuntimeException
//    用 try/catch 捕获
//    断言：捕获到异常（message 含 "rv abort by exception"）；$callCount <= 2
// ============================================================
echo "\n--- 回调抛异常仍中止（回归）---\n";

$multiThrow = new XHMulti();
for ($i = 0; $i < 5; $i++) {
    $multiThrow->add(XHCurl::createRequest($BASE . '/get?id=' . $i)
        ->get()
        ->timeout(15)
        ->setId('multi-throw-' . $i));
}

$callCount = 0;
$caught = false;
$errorMessage = '';
try {
    $multiThrow->executeEach(function($result) use (&$callCount) {
        $callCount++;
        if ($callCount === 2) {
            throw new RuntimeException("rv abort by exception");
        }
    });
} catch (Throwable $e) {
    $caught = true;
    $errorMessage = $e->getMessage();
}

check("multi 抛异常中止：捕获到异常", $caught);
check("multi 抛异常中止：message 含 'rv abort by exception'", strpos($errorMessage, 'rv abort by exception') !== false);
check("multi 抛异常中止：回调触发次数 <= 2", $callCount <= 2);

echo "\n总计: $pass passed, $fail failed\n";
exit($fail > 0 ? 1 : 0);
