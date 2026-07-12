<?php
// +----------------------------------------------------------------------+
// | XHCurl 字段集统一与安全性测试（v1.1.0）                                  |
// |                                                                        |
// | 验证本轮 P2/P3 修复：                                                   |
// |   P2-4.1: 失败路径补 remote_addr/version 字段                          |
// |   P2-1.1: proxy('') 空字符串抛异常                                     |
// |   P2-1.2: setConfig(['proxy' => '']) 空字符串校验                       |
// |   P2-2.1: xhrun('') 空 command 抛异常                                   |
// |   P2-2.2: xhrun cwd 空字符串抛异常                                      |
// |   P3-4.2 BREAKING: fiber_gather([]) 空请求抛异常                       |
// |   P3-2.3: allow/deny 支持标量转换与空字符串跳过                         |
// |   P3-2.4: args 错误信息含索引                                           |
// |                                                                        |
// | 注意：多数用例需 mock 服务器（127.0.0.1:18399）与 socat（18400）。      |
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

echo "=== 字段集统一与安全性测试 ===\n";

// ==================================================================
// P2-4.1: 失败路径补 remote_addr/version 字段
//    所有失败结果（无响应）应含 remote_addr="" 和 version="" 字段。
// ==================================================================
echo "\n=== P2-4.1: 失败路径字段集统一 ===\n";

function test_execute_failure_has_remote_addr(): bool {
    // 用 socat hanging 端点 + timeoutMs 触发超时失败
    $req = XHCurl::createRequest('http://127.0.0.1:18400/hang')
        ->get()
        ->timeoutMs(300)
        ->setId('test-remote');
    $r = $req->execute();
    return $r['success'] === false && array_key_exists('remote_addr', $r) && $r['remote_addr'] === '';
}
check("execute 失败含 remote_addr 空字符串", test_execute_failure_has_remote_addr());

function test_execute_failure_has_version(): bool {
    $req = XHCurl::createRequest('http://127.0.0.1:18400/hang')
        ->get()
        ->timeoutMs(300);
    $r = $req->execute();
    return $r['success'] === false && array_key_exists('version', $r) && $r['version'] === '';
}
check("execute 失败含 version 空字符串", test_execute_failure_has_version());

function test_execute_failure_field_set_consistent(): bool {
    // 成功/失败路径字段集应完全一致
    $successReq = XHCurl::createRequest('http://127.0.0.1:18399/get')->get()->timeout(15);
    $successResult = $successReq->execute();

    $failureReq = XHCurl::createRequest('http://127.0.0.1:18400/hang')->get()->timeoutMs(300);
    $failureResult = $failureReq->execute();

    $successFields = array_keys($successResult);
    $failureFields = array_keys($failureResult);
    sort($successFields);
    sort($failureFields);
    return $successFields === $failureFields;
}
check("execute 成功/失败路径字段集完全一致", test_execute_failure_field_set_consistent());

function test_multi_execute_failure_has_fields(): bool {
    // XHMulti 批量失败结果也应有这两个字段
    $multi = new XHMulti();
    $multi->add(XHCurl::createRequest('http://127.0.0.1:18400/hang')->get()->timeoutMs(300)->setId('multi-fail'));
    $results = $multi->execute();
    $r = $results[0];
    return $r['success'] === false
        && array_key_exists('remote_addr', $r)
        && array_key_exists('version', $r);
}
check("XHMulti 失败结果含 remote_addr/version", test_multi_execute_failure_has_fields());

// ==================================================================
// P2-1.1: proxy('') 空字符串抛异常
// ==================================================================
echo "\n=== P2-1.1: proxy('') 空字符串校验 ===\n";

function test_proxy_empty_throws(): bool {
    $req = XHCurl::createRequest('http://example.com');
    try {
        $req->proxy('');
        return false;
    } catch (\Throwable $e) {
        return strpos($e->getMessage(), 'proxy') !== false && strpos($e->getMessage(), '空') !== false;
    }
}
check("proxy('') 抛异常", test_proxy_empty_throws());

function test_proxy_null_clears(): bool {
    // null 仍正常清除（不抛异常）
    $req = XHCurl::createRequest('http://example.com');
    try {
        $req->proxy('http://proxy:8080');
        $req->proxy(null);
        return $req->getProxy() === null;
    } catch (\Throwable $e) {
        return false;
    }
}
check("proxy(null) 清除代理仍正常", test_proxy_null_clears());

// ==================================================================
// P2-1.2: setConfig(['proxy' => '']) 空字符串校验
// ==================================================================
echo "\n=== P2-1.2: setConfig proxy 空字符串校验 ===\n";

function test_setconfig_proxy_empty_throws(): bool {
    try {
        XHCurl::setConfig(['proxy' => '']);
        return false;
    } catch (\Throwable $e) {
        return strpos($e->getMessage(), 'proxy') !== false;
    }
}
check("setConfig(['proxy' => '']) 校验失败", test_setconfig_proxy_empty_throws());

function test_setconfig_proxy_null_ok(): bool {
    // null 清除代理仍正常
    try {
        XHCurl::setConfig(['proxy' => null]);
        return true;
    } catch (\Throwable $e) {
        return false;
    }
}
check("setConfig(['proxy' => null]) 正常", test_setconfig_proxy_null_ok());

// ==================================================================
// P2-2.1: xhrun('') 空 command 抛异常
// ==================================================================
echo "\n=== P2-2.1: xhrun 空 command 校验 ===\n";

function test_xhrun_empty_command_throws(): bool {
    try {
        xhrun('', []);
        return false;
    } catch (\Throwable $e) {
        return strpos($e->getMessage(), 'command') !== false && strpos($e->getMessage(), '空') !== false;
    }
}
check("xhrun('') 抛异常", test_xhrun_empty_command_throws());

// ==================================================================
// P2-2.2: xhrun cwd 空字符串抛异常
// ==================================================================
echo "\n=== P2-2.2: xhrun cwd 空字符串校验 ===\n";

function test_xhrun_cwd_empty_throws(): bool {
    try {
        xhrun('echo', ['hi'], ['cwd' => '']);
        return false;
    } catch (\Throwable $e) {
        return strpos($e->getMessage(), 'cwd') !== false && strpos($e->getMessage(), '空') !== false;
    }
}
check("xhrun cwd='' 抛异常", test_xhrun_cwd_empty_throws());

function test_xhrun_cwd_non_empty_ok(): bool {
    // 非空 cwd 仍正常工作
    try {
        $r = xhrun('pwd', [], ['cwd' => '/tmp']);
        return $r['success'] === true;
    } catch (\Throwable $e) {
        return false;
    }
}
check("xhrun cwd='/tmp' 正常工作", test_xhrun_cwd_non_empty_ok());

// ==================================================================
// P3-4.2: fiber_gather([]) 空请求抛异常（BREAKING）
// ==================================================================
echo "\n=== P3-4.2: fiber_gather([]) 空请求抛异常 ===\n";

function test_fiber_gather_empty_throws(): bool {
    $threw = false;
    $message = '';
    try {
        XHCurl::run(function() {
            return XHCurl::gather(array());
        });
    } catch (\Throwable $e) {
        $threw = true;
        $message = $e->getMessage();
    }
    return $threw && strpos($message, '没有待执行请求') !== false;
}
check("XHCurl::gather([]) 空请求抛异常", test_fiber_gather_empty_throws());

// ==================================================================
// P3-2.3: allow/deny 支持标量转换与空字符串跳过
// ==================================================================
echo "\n=== P3-2.3: allow/deny 标量转换 ===\n";

function test_allow_int_elements(): bool {
    // allow 数组含 int 元素不应报错（标量转换）
    $r = xhrun('ls', ['-la'], ['allow' => ['ls', 123]]);
    return $r['success'] === true;
}
check("xhrun allow 含 int 元素正常工作", test_allow_int_elements());

function test_allow_empty_string_skipped(): bool {
    // allow 数组含空字符串元素应被跳过（不报错，不匹配）
    $r = xhrun('ls', ['-la'], ['allow' => ['ls', '']]);
    return $r['success'] === true;
}
check("xhrun allow 含空字符串跳过", test_allow_empty_string_skipped());

// ==================================================================
// P3-2.4: args 错误信息含索引
// ==================================================================
echo "\n=== P3-2.4: args 错误信息含索引 ===\n";

function test_args_error_contains_index(): bool {
    // args 含非标量元素应报错且信息含索引
    $threw = false;
    $message = '';
    try {
        xhrun('echo', array(array('nested')));
    } catch (\Throwable $e) {
        $threw = true;
        $message = $e->getMessage();
    }
    return $threw && strpos($message, '第 0 个') !== false;
}
check("xhrun args 错误信息含索引", test_args_error_contains_index());

// ==================================================================
// setConfig 负值错误信息去冗余
// ==================================================================
echo "\n=== setConfig 负值错误信息 ===\n";

function test_setconfig_negative_error_no_redundancy(): bool {
    // 负值错误信息不应重复"为负值"
    try {
        XHCurl::setConfig(['connect_timeout' => -1]);
        return false;
    } catch (\Throwable $e) {
        $msg = $e->getMessage();
        // 应含"为负值"一次（前缀），不应含"不能为负值"（冗余）
        return strpos($msg, '为负值') !== false && strpos($msg, '不能为负值') === false;
    }
}
check("setConfig 负值错误信息无冗余", test_setconfig_negative_error_no_redundancy());

echo "\n=== 测试结果: $pass 通过, $fail 失败 ===\n";
exit($fail > 0 ? 1 : 0);
