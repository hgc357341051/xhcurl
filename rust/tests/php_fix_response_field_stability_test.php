<?php
// +----------------------------------------------------------------------+
// | XHCurl 响应字段稳定性与失败路径完整性测试                                |
// |                                                                        |
// | 验证本轮 P2/P3 修复：                                                   |
// |   P2-1: fill_response_fields 中 remote_addr/version 无条件插入         |
// |         （None 时为空字符串，不再触发 PHP Undefined index）             |
// |   P2-2: xhrun failure_result 含 error_type 字段                         |
// |         （白/黑名单 = "denied"，启动失败 = "spawn_failed"）             |
// |   P2-3: xhrun exit_error 路径含 command 和 error 字段                    |
// |   P2-4: XHRequest::execute() 失败时 elapsed_ms 记录真实耗时（非 0）     |
// |   P2-5: XHMulti 新增 clear() 方法                                       |
// |   P3-1: xhrun 错误措辞从「不能为负数」改为「不能为负值」                |
// |                                                                        |
// | 注意：多数用例需 mock 服务器（/get），P2-4 需真实 DNS 失败（耗时数秒）。   |
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

echo "=== 响应字段稳定性与失败路径完整性测试 ===\n";

// ==================================================================
// P2-1: 响应数组字段稳定性
//    fill_response_fields 中 remote_addr/version 无条件插入，
//    None 时为空字符串，保证字段稳定存在，
//    避免 PHP 端 $resp['remote_addr'] 触发 Undefined index。
// ==================================================================
echo "\n=== P2-1: 响应数组字段稳定性 ===\n";

function test_response_always_has_remote_addr(): bool {
    global $BASE;
    $resp = XHCurl::createRequest($BASE . '/get')->get()->execute();
    return array_key_exists('remote_addr', $resp);
}
check("响应数组始终含 remote_addr 字段", test_response_always_has_remote_addr());

function test_response_always_has_version(): bool {
    global $BASE;
    $resp = XHCurl::createRequest($BASE . '/get')->get()->execute();
    return array_key_exists('version', $resp);
}
check("响应数组始终含 version 字段", test_response_always_has_version());

// 字段值为字符串类型（None 时为空字符串，非 null）
$respFields = XHCurl::createRequest($BASE . '/get')->get()->execute();
check("remote_addr 为字符串类型", is_string($respFields['remote_addr']));
check("version 为字符串类型", is_string($respFields['version']));

// ==================================================================
// P2-2: failure_result error_type 字段
//    xhrun failure_result 辅助函数现在含 error_type 字段：
//    白/黑名单拒绝 → "denied"
//    启动失败 → "spawn_failed"
// ==================================================================
echo "\n=== P2-2: failure_result error_type 字段 ===\n";

function test_xhrun_denied_error_type(): bool {
    // 用白名单模式拒绝非白名单命令
    $r = xhrun('ls', [], ['allow' => ['grep']]);
    return $r['success'] === false
        && isset($r['error_type']) && $r['error_type'] === 'denied';
}
check("xhrun 白名单拒绝 error_type=denied", test_xhrun_denied_error_type());

function test_xhrun_deny_list_error_type(): bool {
    // 黑名单拒绝同样 error_type=denied
    $r = xhrun('rm', ['-rf', '/tmp/nonexistent'], ['deny' => ['rm']]);
    return $r['success'] === false
        && isset($r['error_type']) && $r['error_type'] === 'denied';
}
check("xhrun 黑名单拒绝 error_type=denied", test_xhrun_deny_list_error_type());

function test_xhrun_spawn_failed_error_type(): bool {
    // 启动不存在的命令
    $r = xhrun('/nonexistent/command_xyz', [], []);
    return $r['success'] === false
        && isset($r['error_type']) && $r['error_type'] === 'spawn_failed';
}
check("xhrun 启动失败 error_type=spawn_failed", test_xhrun_spawn_failed_error_type());

// ==================================================================
// P2-3: exit_error 路径字段完整性
//    xhrun exit_error 路径（退出码非 0）现在含 command 和 error 字段。
// ==================================================================
echo "\n=== P2-3: exit_error 路径字段完整性 ===\n";

function test_xhrun_exit_error_has_command(): bool {
    $r = xhrun('sh', ['-c', 'exit 1']);
    return $r['success'] === false
        && isset($r['command']) && $r['command'] === 'sh'
        && isset($r['error']);
}
check("xhrun exit_error 路径含 command 和 error", test_xhrun_exit_error_has_command());

function test_xhrun_exit_error_has_error_type(): bool {
    // exit_error 路径同时含 error_type=exit_error
    $r = xhrun('sh', ['-c', 'exit 2']);
    return $r['success'] === false
        && $r['exit_code'] === 2
        && isset($r['error_type']) && $r['error_type'] === 'exit_error';
}
check("xhrun exit_error 路径含 error_type=exit_error", test_xhrun_exit_error_has_error_type());

// ==================================================================
// P2-4: execute() 失败路径 elapsed_ms 真实耗时
//    XHRequest::execute() 失败时 elapsed_ms 记录真实耗时（非 0）。
//    用 socat /hang 端点（sleep 60s）触发超时失败路径：
//    超时错误走 execute() 的 Err 分支，elapsed_ms 来自 start.elapsed()。
//    （不使用 DNS 失败：沙箱环境 HTTP 代理会拦截 DNS 失败返回 200/502，
//     无法可靠触发 execute() 失败路径。）
//    /hang 由 socat 在 18400 端口提供，timeoutMs(300) 在 300ms 后超时。
// ==================================================================
echo "\n=== P2-4: execute 失败路径 elapsed_ms ===\n";

function test_execute_failure_elapsed_ms_positive(): bool {
    // 请求 /hang 端点（socat sleep 60s），300ms 超时触发失败路径
    $resp = XHCurl::createRequest('http://127.0.0.1:18400/hang')
        ->get()
        ->timeoutMs(300)
        ->execute();
    // 失败时 success=false
    if ($resp['success'] !== false) return false;
    return isset($resp['elapsed_ms']) && $resp['elapsed_ms'] > 0;
}
check("execute 失败路径 elapsed_ms > 0", test_execute_failure_elapsed_ms_positive());

// ==================================================================
// P2-5: XHMulti clear() 方法
//    XHMulti 新增 clear() 方法，清空待执行请求列表，允许复用对象。
// ==================================================================
echo "\n=== P2-5: XHMulti clear() 方法 ===\n";

function test_multi_clear(): bool {
    global $BASE;
    $multi = new XHMulti();
    $multi->add(XHCurl::createRequest($BASE . '/get')->get());
    $multi->add(XHCurl::createRequest($BASE . '/get')->get());
    if ($multi->count() !== 2) return false;
    $multi->clear();
    return $multi->count() === 0;
}
check("XHMulti clear() 清空请求列表", test_multi_clear());

function test_multi_clear_makes_empty(): bool {
    global $BASE;
    $multi = new XHMulti();
    $multi->add(XHCurl::createRequest($BASE . '/get')->get());
    $multi->clear();
    return $multi->isEmpty() === true;
}
check("XHMulti clear() 后 isEmpty()=true", test_multi_clear_makes_empty());

function test_multi_clear_allows_reuse(): bool {
    // clear 后可重新 add 并执行
    global $BASE;
    $multi = new XHMulti();
    $multi->add(XHCurl::createRequest($BASE . '/get')->get());
    $multi->clear();
    $multi->add(XHCurl::createRequest($BASE . '/get')->get());
    if ($multi->count() !== 1) return false;
    $results = $multi->execute();
    return is_array($results) && count($results) === 1 && $results[0]['success'] === true;
}
check("XHMulti clear() 后可复用对象重新执行", test_multi_clear_allows_reuse());

// ==================================================================
// P3-1: 错误措辞统一
//    xhrun 错误措辞从「不能为负数」改为「不能为负值」，
//    与 timeout()/connectTimeout() 等 setter 的措辞一致。
// ==================================================================
echo "\n=== P3-1: 错误措辞统一 ===\n";

function test_xhrun_timeout_negative_error_message(): bool {
    try {
        xhrun('ls', [], ['timeout' => -1]);
        return false;
    } catch (\Throwable $e) {
        return strpos($e->getMessage(), '不能为负值') !== false;
    }
}
check("xhrun 负 timeout 错误含「不能为负值」", test_xhrun_timeout_negative_error_message());

function test_xhrun_timeout_negative_no_old_wording(): bool {
    // 旧措辞「不能为负数」不应再出现
    try {
        xhrun('ls', [], ['timeout' => -1]);
        return true; // 不该到这里
    } catch (\Throwable $e) {
        return strpos($e->getMessage(), '不能为负数') === false;
    }
}
check("xhrun 负 timeout 错误不含旧措辞「不能为负数」", test_xhrun_timeout_negative_no_old_wording());

echo "\n总计: $pass passed, $fail failed\n";
exit($fail > 0 ? 1 : 0);
