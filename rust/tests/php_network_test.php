<?php
// XHCurl 网络功能测试 - 验证 json 转换和 execute() 返回字段
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

echo "=== execute() 返回字段测试 ===\n";

// 1. execute() 返回 id 和 user_data 字段
$req = XHCurl::createRequest($BASE . '/get')
    ->get()
    ->setId('my-request-id-001')
    ->setUserData(array('task' => 'test', 'seq' => 42))
    ->timeout(15);

$result = $req->execute();

check("execute() success", $result['success'] === true);
check("execute() 含 id 字段", isset($result['id']) && $result['id'] === 'my-request-id-001');
check("execute() 含 user_data 字段", isset($result['user_data']));
$ud = json_decode($result['user_data'], true);
check("execute() user_data 内容正确", $ud['task'] === 'test' && $ud['seq'] === 42);
check("execute() 含 status", isset($result['status']) && $result['status'] === 200);
check("execute() 含 body", isset($result['body']) && strlen($result['body']) > 0);
check("execute() 含 body_size", isset($result['body_size']) && $result['body_size'] > 0);
check("execute() 含 headers", isset($result['headers']) && is_array($result['headers']));
check("execute() 含 url", isset($result['url']));
check("execute() 含 elapsed_ms", isset($result['elapsed_ms']) && $result['elapsed_ms'] >= 0);
check("execute() 含 success 字段", array_key_exists('success', $result));

// 2. 未设置 setId 时 id 默认为 URL
$req2 = XHCurl::createRequest($BASE . '/get')->get()->timeout(15);
$result2 = $req2->execute();
check("未设 setId 时 id=URL", $result2['id'] === $BASE . '/get');
check("未设 setUserData 时无 user_data 字段", !array_key_exists('user_data', $result2));

// 3. Set-Cookie 重复头合并（RFC 7230 §3.2.2）
check("Set-Cookie 重复头合并", isset($result2['headers']['set-cookie']) && strpos($result2['headers']['set-cookie'], ', ') !== false);

echo "\n=== JSON 请求体测试（列表数组应转为 JSON 数组）===\n";

// 4. json() 列表数组 → JSON 数组（非对象）
$jsonData3 = array(
    'tags' => array('a', 'b', 'c'),
    'nums' => array(1, 2, 3),
    'nested' => array(array(1, 2), array(3, 4)),
);
$req3 = XHCurl::createRequest($BASE . '/post')
    ->post()
    ->json($jsonData3)
    ->timeout(15);

$result3 = $req3->execute();
check("json 列表请求 success", $result3['success'] === true);

$body = json_decode($result3['body'], true);
$sent = $body['json'] ?? null;

check("json 请求体含 tags", isset($sent['tags']));
check("json tags 是数组（非对象）", is_array($sent['tags']) && array_is_list($sent['tags']));
check("json tags 内容正确", $sent['tags'] === array('a', 'b', 'c'));
check("json nums 是数组", is_array($sent['nums']) && array_is_list($sent['nums']));
check("json nums 内容正确", $sent['nums'] === array(1, 2, 3));
check("json nested 是数组数组", is_array($sent['nested']) && array_is_list($sent['nested']) && array_is_list($sent['nested'][0]));
check("json nested 内容正确", $sent['nested'] === array(array(1, 2), array(3, 4)));

// 5. 关联数组 → JSON 对象
$jsonData4 = array('name' => 'XHCurl', 'version' => '2.0', 'active' => true);
$req4 = XHCurl::createRequest($BASE . '/post')
    ->post()
    ->json($jsonData4)
    ->timeout(15);

$result4 = $req4->execute();
$body4 = json_decode($result4['body'], true);
$sent4 = $body4['json'] ?? null;

check("json 关联数组 success", $result4['success'] === true);
check("json 关联数组 name", $sent4['name'] === 'XHCurl');
check("json 关联数组 version", $sent4['version'] === '2.0');
check("json 关联数组 active (bool)", $sent4['active'] === true);

// 6. 空数组 → JSON 空数组
$jsonData5 = array('empty_list' => array());
$req5 = XHCurl::createRequest($BASE . '/post')
    ->post()
    ->json($jsonData5)
    ->timeout(15);

$result5 = $req5->execute();
$body5 = json_decode($result5['body'], true);
$sent5 = $body5['json'] ?? null;

check("json 空数组 success", $result5['success'] === true);
check("json 空数组是数组（非对象）", is_array($sent5['empty_list']) && array_is_list($sent5['empty_list']) && count($sent5['empty_list']) === 0);

echo "\n=== JSON 响应解析测试（json_to_php_array 嵌套类型）===\n";

// 7. 响应中的嵌套 null/object/array
$jsonData6 = array(
    'nested' => array(
        'null_val' => null,
        'obj' => array('a' => 1),
        'arr' => array(1, null, array('x' => 2)),
    ),
);
$req6 = XHCurl::createRequest($BASE . '/post')
    ->post()
    ->json($jsonData6)
    ->timeout(15);

$result6 = $req6->execute();
check("json 响应解析 success", $result6['success'] === true);

// 服务器回显请求体在 json 字段
$respBody = json_decode($result6['body'], true);
$nested = $respBody['json']['nested'] ?? null;

check("json 响应含 nested", $nested !== null);
check("json 嵌套 null_val 存在", array_key_exists('null_val', $nested));
check("json 嵌套 null_val 为 null", $nested['null_val'] === null);
check("json 嵌套 obj 是数组", is_array($nested['obj']));
check("json 嵌套 obj.a=1", $nested['obj']['a'] === 1);
check("json 嵌套 arr 是列表", is_array($nested['arr']) && array_is_list($nested['arr']));
check("json 嵌套 arr[0]=1", $nested['arr'][0] === 1);
check("json 嵌套 arr[1]=null", $nested['arr'][1] === null);
check("json 嵌套 arr[2] 是数组", is_array($nested['arr'][2]));
check("json 嵌套 arr[2].x=2", $nested['arr'][2]['x'] === 2);

echo "\n=== gather 协程测试 ===\n";

// 8. gather 并发请求
$requests = array();
for ($i = 0; $i < 5; $i++) {
    $requests[] = XHCurl::createRequest($BASE . '/get?id=' . $i)
        ->get()
        ->timeout(15)
        ->setUserData(array('index' => $i));
}

$results = XHCurl::run(function() use ($requests) {
    return XHCurl::gather($requests);
});

check("gather 返回数组", is_array($results) && count($results) === 5);
$allSuccess = true;
foreach ($results as $r) {
    if (!$r['success']) {
        $allSuccess = false;
    }
}
check("gather 全部成功", $allSuccess);

// 检查每个结果都含 id 和 user_data
$hasFields = true;
foreach ($results as $r) {
    if (!isset($r['id']) || !isset($r['user_data'])) {
        $hasFields = false;
        break;
    }
}
check("gather 结果含 id 和 user_data", $hasFields);

echo "\n=== 测试结果: $pass 通过, $fail 失败 ===\n";
exit($fail > 0 ? 1 : 0);
