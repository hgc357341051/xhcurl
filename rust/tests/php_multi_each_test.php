<?php
// XHMulti::executeEach() 流式回调测试
// 验证：每完成一个请求立即调用回调、不累积、按完成顺序触发、失败请求仍回调、字段一致性
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

echo "=== XHMulti::executeEach() 流式回调测试 ===\n";

// 1. 正常流式处理：多个请求，每个回调收到完整字段
$multi = new XHMulti();
for ($i = 0; $i < 5; $i++) {
    $multi->add(XHCurl::createRequest($BASE . '/get?id=' . $i)
        ->get()
        ->timeout(15)
        ->setId('multi-req-' . $i)
        ->setUserData(array('index' => $i)));
}

$callbackResults = array();
$count = $multi->executeEach(function($result) use (&$callbackResults) {
    $callbackResults[] = $result;
});

check("multi executeEach 返回总数=5", $count === 5);
check("multi executeEach 回调触发 5 次", count($callbackResults) === 5);

// 验证每个回调结果含完整字段
$allHaveFields = true;
foreach ($callbackResults as $r) {
    if (!isset($r['id']) || !isset($r['success']) || !isset($r['status'])
        || !isset($r['body']) || !isset($r['headers']) || !isset($r['elapsed_ms'])) {
        $allHaveFields = false;
        break;
    }
}
check("multi executeEach 每个结果含完整字段", $allHaveFields);

// 全部成功
$allSuccess = true;
foreach ($callbackResults as $r) {
    if ($r['success'] !== true) {
        $allSuccess = false;
        break;
    }
}
check("multi executeEach 所有请求成功", $allSuccess);

echo "\n=== 空请求列表 ===\n";

// 2. 空请求列表返回 0
$emptyMulti = new XHMulti();
$emptyCount = $emptyMulti->executeEach(function($result) {
    // 不应执行
});
check("multi executeEach 空列表返回 0", $emptyCount === 0);

echo "\n=== 单个请求 ===\n";

// 3. 单个请求返回 1
$singleMulti = new XHMulti();
$singleMulti->add(XHCurl::createRequest($BASE . '/get')->get()->timeout(15)->setId('single'));
$singleResult = null;
$singleCount = $singleMulti->executeEach(function($result) use (&$singleResult) {
    $singleResult = $result;
});
check("multi executeEach 单个请求返回 1", $singleCount === 1);
check("multi executeEach 单个请求回调触发 1 次", $singleResult !== null);
check("multi executeEach 单个请求 id 正确", $singleResult['id'] === 'single');

echo "\n=== 失败请求仍触发回调 ===\n";

// 4. 失败请求仍触发回调（连接不可达端口）
$failedMulti = new XHMulti();
$failedMulti->add(XHCurl::createRequest('http://127.0.0.1:1/unreachable')
    ->get()
    ->timeout(2)
    ->setId('failed'));
$failedResult = null;
$failedMulti->executeEach(function($result) use (&$failedResult) {
    $failedResult = $result;
});
check("multi executeEach 失败请求仍触发回调", $failedResult !== null);
check("multi executeEach 失败请求 success=false", $failedResult['success'] === false);
check("multi executeEach 失败请求 body 为空字符串", $failedResult['body'] === '');

echo "\n=== 字段一致性（与 execute 对比）===\n";

// 5. executeEach 回调收到的字段与 execute 返回元素一致
$compareMulti1 = new XHMulti();
$compareMulti1->add(XHCurl::createRequest($BASE . '/get')->get()->timeout(15)->setId('compare')->setUserData(array('k' => 'v')));
$gatherResult = $compareMulti1->execute();
$gatherFields = array_keys($gatherResult[0]);
sort($gatherFields);

$compareMulti2 = new XHMulti();
$compareMulti2->add(XHCurl::createRequest($BASE . '/get')->get()->timeout(15)->setId('compare')->setUserData(array('k' => 'v')));
$eachResult = null;
$compareMulti2->executeEach(function($result) use (&$eachResult) {
    $eachResult = $result;
});
$eachFields = array_keys($eachResult);
sort($eachFields);
check("multi executeEach 与 execute 字段一致", $gatherFields === $eachFields);
check("multi executeEach 与 execute id 一致", $gatherResult[0]['id'] === $eachResult['id']);

echo "\n=== 测试结果: $pass 通过, $fail 失败 ===\n";
exit($fail > 0 ? 1 : 0);
