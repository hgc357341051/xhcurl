<?php
// XHCurl 三种执行模式"回调异常中止剩余任务"行为一致性测试
//
// 验证：XHCurl 三种执行模式（XHMulti / XHThreadPool / 协程 each）在请求级
// 流式回调异常时都应中止剩余任务，三者行为一致。
//   1. XHMulti::executeEach 回调异常中止剩余任务
//   2. XHThreadPool::executeEach 回调异常中止剩余任务（本次修复重点）
//   3. XHThreadPool 回调异常后 pool 重建可复用
//   4. 协程 each() 回调异常中止剩余任务
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

echo "=== 回调异常中止剩余任务 - 三种执行模式一致性测试 ===\n";

// ============================================================
// 1. XHMulti::executeEach 回调异常中止剩余任务
// ============================================================
echo "\n--- XHMulti::executeEach 回调异常中止 ---\n";

$multi = new XHMulti();
for ($i = 0; $i < 5; $i++) {
    $multi->add(XHCurl::createRequest($BASE . '/get?id=' . $i)
        ->get()
        ->timeout(15)
        ->setId('multi-abort-' . $i));
}

$multiCount = 0;
$multiErrorReturned = false;
$multiErrorMessage = '';
try {
    $multi->executeEach(function($result) use (&$multiCount) {
        $multiCount++;
        if ($multiCount >= 2) {
            throw new RuntimeException("multi abort test");
        }
    });
} catch (Throwable $e) {
    $multiErrorReturned = true;
    $multiErrorMessage = $e->getMessage();
}

check("multi 回调异常后捕获到异常", $multiErrorReturned);
check("multi 异常 message 含 'multi abort test'", strpos($multiErrorMessage, 'multi abort test') !== false);
check("multi 回调触发次数 <= 2（剩余请求不触发回调）", $multiCount <= 2);

// ============================================================
// 2. XHThreadPool::executeEach 回调异常中止剩余任务（修复重点）
//    修复前：回调异常后 pool 未 drop，剩余任务仍会触发回调
//    修复后：回调异常时不存回 pool → drop pool → abort workers
// ============================================================
echo "\n--- XHThreadPool::executeEach 回调异常中止（修复重点）---\n";

$pool = new XHThreadPool();
for ($i = 0; $i < 5; $i++) {
    $pool->add(XHCurl::createRequest($BASE . '/get?id=' . $i)
        ->get()
        ->timeout(15)
        ->setId('pool-abort-' . $i));
}

$poolCount = 0;
$poolErrorReturned = false;
$poolErrorMessage = '';
try {
    $pool->executeEach(function($result) use (&$poolCount) {
        $poolCount++;
        if ($poolCount >= 2) {
            throw new RuntimeException("pool abort test");
        }
    });
} catch (Throwable $e) {
    $poolErrorReturned = true;
    $poolErrorMessage = $e->getMessage();
}

check("pool 回调异常后捕获到异常", $poolErrorReturned);
check("pool 异常 message 含 'pool abort test'", strpos($poolErrorMessage, 'pool abort test') !== false);
check("pool 回调触发次数 <= 2（剩余请求不触发回调，本次修复重点）", $poolCount <= 2);

// ============================================================
// 3. XHThreadPool 回调异常后 pool 重建可复用
//    用同一个 $pool 对象再次提交 3 个请求，回调正常处理
//    验证修复后 pool 被正确 drop 并可重建（pool.is_none() → 重建）
// ============================================================
echo "\n--- XHThreadPool 回调异常后 pool 重建可复用 ---\n";

for ($i = 10; $i < 13; $i++) {
    $pool->add(XHCurl::createRequest($BASE . '/get?id=' . $i)
        ->get()
        ->timeout(15)
        ->setId('pool-reuse-' . $i));
}

$reuseCount = 0;
$reuseReturned = null;
try {
    $reuseReturned = $pool->executeEach(function($result) use (&$reuseCount) {
        $reuseCount++;
    });
} catch (Throwable $e) {
    // pool 重建后不应抛异常
}

check("pool 重建后再次 executeEach 返回 3", $reuseReturned === 3);
check("pool 重建后回调触发 3 次", $reuseCount === 3);

// ============================================================
// 4. 协程 each() 回调异常中止剩余任务
//    协程异常通过 run() 的返回值/异常传出（与 php_each_test.php 模式一致）
// ============================================================
echo "\n--- 协程 each() 回调异常中止 ---\n";

$fiberRequests = array();
for ($i = 0; $i < 5; $i++) {
    $fiberRequests[] = XHCurl::createRequest($BASE . '/get?id=' . $i)
        ->get()
        ->timeout(15)
        ->setId('fiber-abort-' . $i);
}

$fiberCount = 0;
$fiberErrorReturned = false;
$fiberErrorMessage = '';
try {
    XHCurl::run(function() use ($fiberRequests, &$fiberCount) {
        return XHCurl::each($fiberRequests, function($result) use (&$fiberCount) {
            $fiberCount++;
            if ($fiberCount >= 2) {
                throw new RuntimeException("fiber abort test");
            }
        });
    });
} catch (Throwable $e) {
    $fiberErrorReturned = true;
    $fiberErrorMessage = $e->getMessage();
}

check("fiber 回调异常后捕获到异常", $fiberErrorReturned);
check("fiber 异常 message 含 'fiber abort test'", strpos($fiberErrorMessage, 'fiber abort test') !== false);
check("fiber 回调触发次数 <= 2", $fiberCount <= 2);

echo "\n总计: $pass passed, $fail failed\n";
exit($fail > 0 ? 1 : 0);
