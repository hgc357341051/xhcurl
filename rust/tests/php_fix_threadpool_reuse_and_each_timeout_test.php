<?php
// +----------------------------------------------------------------------+
// | XHCurl 第五轮测试：pool 复用重建、executeEach 超时、负值抛异常         |
// |                                                                        |
// | 验证 fix-threadpool-reuse-and-each-timeout spec：                       |
// |   Task 1: pool 复用时配置变更重建（maxConcurrency/maxResponseSize）       |
// |   Task 2: executeEach 强制 timeout                                       |
// |   Task 3: 负值抛异常（timeout/maxResponseSize/maxConcurrency）           |
// |                                                                        |
// | 注意：涉及 /hang 的超时测试放最后，避免阻塞 mock 服务器进程。            |
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

echo "=== 第五轮：pool 复用重建、executeEach 超时、负值抛异常 ===\n";

// ==================================================================
// Task 3: 负值抛异常（0 值保持合法）
// ==================================================================
echo "\n=== Task 3: 负值抛异常 ===\n";

// XHMulti 负值抛异常
$caught = false;
try {
    $m = new XHMulti();
    $m->timeout(-1);
} catch (\Throwable $e) {
    $caught = true;
}
check("XHMulti timeout(-1) 抛异常", $caught);

$caught = false;
try {
    $m = new XHMulti();
    $m->maxResponseSize(-1);
} catch (\Throwable $e) {
    $caught = true;
}
check("XHMulti maxResponseSize(-1) 抛异常", $caught);

$caught = false;
try {
    $m = new XHMulti();
    $m->maxConcurrency(-1);
} catch (\Throwable $e) {
    $caught = true;
}
check("XHMulti maxConcurrency(-1) 抛异常", $caught);

// XHThreadPool 负值抛异常
$caught = false;
try {
    $p = new XHThreadPool(2);
    $p->timeout(-5);
} catch (\Throwable $e) {
    $caught = true;
}
check("XHThreadPool timeout(-5) 抛异常", $caught);

$caught = false;
try {
    $p = new XHThreadPool(2);
    $p->maxResponseSize(-100);
} catch (\Throwable $e) {
    $caught = true;
}
check("XHThreadPool maxResponseSize(-100) 抛异常", $caught);

$caught = false;
try {
    $p = new XHThreadPool(2);
    $p->maxConcurrency(-1);
} catch (\Throwable $e) {
    $caught = true;
}
check("XHThreadPool maxConcurrency(-1) 抛异常", $caught);

// 0 值不抛异常（合法）
$ok = true;
try {
    $m = new XHMulti();
    $m->timeout(0)->maxResponseSize(0)->maxConcurrency(0);
    $p = new XHThreadPool(2);
    $p->timeout(0)->maxResponseSize(0)->maxConcurrency(0);
} catch (\Throwable $e) {
    $ok = false;
}
check("0 值不抛异常（合法）", $ok);

// 正值不抛异常
$ok = true;
try {
    $m = new XHMulti();
    $m->timeout(30)->maxResponseSize(1000000)->maxConcurrency(10);
    $p = new XHThreadPool(2);
    $p->timeout(30)->maxResponseSize(1000000)->maxConcurrency(4);
} catch (\Throwable $e) {
    $ok = false;
}
check("正值不抛异常", $ok);

// 错误消息含字段名
$msgHasTimeout = false;
try {
    $p = new XHThreadPool(2);
    $p->timeout(-1);
} catch (\Throwable $e) {
    $msgHasTimeout = strpos($e->getMessage(), 'timeout') !== false;
}
check("timeout 负值错误消息含字段名", $msgHasTimeout);

// ==================================================================
// Task 1: pool 复用时配置变更重建
// ==================================================================
echo "\n=== Task 1: pool 复用配置变更重建 ===\n";

// 两次 execute 间修改 maxConcurrency 不抛异常（重建生效）
$pool = new XHThreadPool(2);
$pool->add(XHCurl::createRequest($BASE . '/get?id=1')->get()->timeout(10));
$pool->add(XHCurl::createRequest($BASE . '/get?id=2')->get()->timeout(10));
$results1 = $pool->execute();
$ok1 = is_array($results1) && count($results1) === 2;
check("第一次 execute 成功", $ok1);

// 修改配置后第二次 execute（应重建 pool 生效）
$pool->maxConcurrency(8);
$pool->add(XHCurl::createRequest($BASE . '/get?id=3')->get()->timeout(10));
$pool->add(XHCurl::createRequest($BASE . '/get?id=4')->get()->timeout(10));
$results2 = $pool->execute();
$ok2 = is_array($results2) && count($results2) === 2;
check("修改配置后第二次 execute 成功", $ok2);

// 未修改配置时复用 pool（无异常、行为一致）
$pool3 = new XHThreadPool(2);
$pool3->add(XHCurl::createRequest($BASE . '/get?id=a')->get()->timeout(10));
$r1 = $pool3->execute();
check("未修改配置第一次 execute", is_array($r1) && count($r1) === 1);

$pool3->add(XHCurl::createRequest($BASE . '/get?id=b')->get()->timeout(10));
$r2 = $pool3->execute();
check("未修改配置第二次 execute 复用 pool", is_array($r2) && count($r2) === 1);

// 修改 maxResponseSize 后重建生效（小限制导致失败）
$pool4 = new XHThreadPool(1);
$pool4->add(XHCurl::createRequest($BASE . '/get')->get()->timeout(10));
$pool4->execute();  // 第一次正常

$pool4->maxResponseSize(1);  // 1 字节限制，极小
$pool4->add(XHCurl::createRequest($BASE . '/get')->get()->timeout(10));
$results = $pool4->execute();
$failed = is_array($results) && count($results) >= 1 && $results[0]['success'] === false;
check("修改 maxResponseSize 后重建生效（超限失败）", $failed);

// ==================================================================
// Task 2: executeEach 强制 timeout（最后执行，用 /hang）
// /hang 由独立 socat 进程在 18400 端口提供
// ==================================================================
echo "\n=== Task 2: executeEach 强制 timeout（最后执行）===\n";

$pool5 = new XHThreadPool(1);
$pool5->add(XHCurl::createRequest('http://127.0.0.1:18400/hang')->get()->timeout(30));
$pool5->timeout(2);  // 2 秒批量超时
$startTime = microtime(true);
$caughtTimeout = false;
try {
    $pool5->executeEach(function($result) {});
} catch (\Throwable $e) {
    $caughtTimeout = true;
}
$elapsed = microtime(true) - $startTime;
check("executeEach timeout 抛异常", $caughtTimeout);
check("executeEach timeout 中止（elapsed < 10s）", $elapsed < 10);

// executeEach 无 timeout 时不抛异常（正常请求）
$pool6 = new XHThreadPool(2);
$pool6->add(XHCurl::createRequest($BASE . '/get')->get()->timeout(10));
$pool6->timeout(0);  // 无超时
$count = $pool6->executeEach(function($result) {});
check("executeEach timeout(0) 无超时正常执行", $count === 1);

echo "\n总计: $pass passed, $fail failed\n";
exit($fail > 0 ? 1 : 0);
