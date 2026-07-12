<?php
// +----------------------------------------------------------------------+
// | XHCurl 统一 xhrun 字段集与入口 fail-fast 校验测试                       |
// |                                                                        |
// | 验证本轮 P2/P3 修复（v1.0.8）：                                          |
// |   P2-1: xhrun 成功路径补 error_type=""/error=""/command 字段            |
// |         （成功/失败字段集一致，与 HTTP API 风格对齐）                   |
// |   P2-2: README 同步（此处通过实际字段验证，不直接断言 README）            |
// |   P3-1 BREAKING: XHCurl::each([], $cb) 空请求抛异常                    |
// |         （与 XHMulti/XHThreadPool executeEach 一致）                    |
// |   P3-2: to_info_map 死代码已删除（此处通过 cargo test 验证，不测 PHP）   |
// |   P3-3: createRequest('')/new XHRequest('') 空字符串 URL 抛异常        |
// |         （fail-fast，不延迟到 execute）                                 |
// |                                                                        |
// | 注意：xhrun 测试需要 echo/ls/sh 等命令（Linux 沙箱可用）。              |
// +----------------------------------------------------------------------+

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

echo "=== 统一 xhrun 字段集与入口 fail-fast 校验测试 ===\n";

// ==================================================================
// P2-1: xhrun 成功路径字段集统一
//    成功路径插入 error_type=""/error=""/command，
//    使成功/失败字段集完全一致（11 个字段）。
// ==================================================================
echo "\n=== P2-1: xhrun 成功路径字段集统一 ===\n";

function test_xhrun_success_has_error_type(): bool {
    $r = xhrun('echo', ['hello']);
    if ($r['success'] !== true) {
        return false;
    }
    return array_key_exists('error_type', $r) && $r['error_type'] === '';
}
check("xhrun 成功路径含 error_type 空字符串", test_xhrun_success_has_error_type());

function test_xhrun_success_has_error(): bool {
    $r = xhrun('echo', ['hello']);
    if ($r['success'] !== true) {
        return false;
    }
    return array_key_exists('error', $r) && $r['error'] === '';
}
check("xhrun 成功路径含 error 空字符串", test_xhrun_success_has_error());

function test_xhrun_success_has_command(): bool {
    $r = xhrun('echo', ['hello']);
    if ($r['success'] !== true) {
        return false;
    }
    return array_key_exists('command', $r) && $r['command'] === 'echo';
}
check("xhrun 成功路径含 command 字段", test_xhrun_success_has_command());

function test_xhrun_success_failure_field_set_consistent(): bool {
    // 成功路径与失败路径字段集应完全一致
    $success = xhrun('echo', ['ok']);
    $failure = xhrun('sh', ['-c', 'exit 1']);
    $successFields = array_keys($success);
    $failureFields = array_keys($failure);
    sort($successFields);
    sort($failureFields);
    return $successFields === $failureFields;
}
check("xhrun 成功/失败路径字段集一致", test_xhrun_success_failure_field_set_consistent());

// ==================================================================
// P3-1: XHCurl::each 空请求抛异常（BREAKING）
//    原返回 Ok(0)，现抛异常，与 XHMulti/XHThreadPool executeEach 一致。
// ==================================================================
echo "\n=== P3-1: XHCurl::each 空请求抛异常 ===\n";

function test_fiber_each_empty_throws(): bool {
    $threw = false;
    $message = '';
    try {
        XHCurl::run(function() {
            return XHCurl::each(array(), function($result) {
                // 不应执行
            });
        });
    } catch (\Throwable $e) {
        $threw = true;
        $message = $e->getMessage();
    }
    return $threw && strpos($message, '没有待执行请求') !== false;
}
check("XHCurl::each 空请求抛异常", test_fiber_each_empty_throws());

// ==================================================================
// P3-3: createRequest 空字符串 URL 抛异常（fail-fast）
//    原 createRequest('') 不报错，延迟到 execute() 才报错。
//    现入口即校验，错误信息含 "url" 和 "空"。
// ==================================================================
echo "\n=== P3-3: createRequest 空字符串 URL 抛异常 ===\n";

function test_create_request_empty_url_throws(): bool {
    $threw = false;
    $message = '';
    try {
        XHCurl::createRequest('');
    } catch (\Throwable $e) {
        $threw = true;
        $message = $e->getMessage();
    }
    return $threw
        && strpos($message, 'url') !== false
        && strpos($message, '空') !== false;
}
check("createRequest('') 抛异常", test_create_request_empty_url_throws());

function test_xhrequest_construct_empty_url_throws(): bool {
    $threw = false;
    $message = '';
    try {
        new XHRequest('');
    } catch (\Throwable $e) {
        $threw = true;
        $message = $e->getMessage();
    }
    return $threw
        && strpos($message, 'url') !== false
        && strpos($message, '空') !== false;
}
check("new XHRequest('') 抛异常", test_xhrequest_construct_empty_url_throws());

function test_create_request_non_empty_url_ok(): bool {
    // 非空 URL 仍正常工作（不抛异常）
    try {
        $req = XHCurl::createRequest('http://example.com');
        return $req !== null;
    } catch (\Throwable $e) {
        return false;
    }
}
check("createRequest(非空 URL) 正常工作", test_create_request_non_empty_url_ok());

// ==================================================================
// 综合：xhrun 失败路径字段集不变（error_type/error/command 仍存在）
// ==================================================================
echo "\n=== xhrun 失败路径字段集不变 ===\n";

function test_xhrun_failure_still_has_error_type(): bool {
    $r = xhrun('sh', ['-c', 'exit 1']);
    if ($r['success'] !== false) {
        return false;
    }
    return array_key_exists('error_type', $r) && $r['error_type'] === 'exit_error';
}
check("xhrun 失败路径 error_type=exit_error 不变", test_xhrun_failure_still_has_error_type());

function test_xhrun_timeout_failure_still_has_error_type(): bool {
    $r = xhrun('sleep', ['5'], ['timeout' => 1]);
    if ($r['success'] !== false || $r['timed_out'] !== true) {
        return false;
    }
    return array_key_exists('error_type', $r) && $r['error_type'] === 'timeout';
}
check("xhrun 超时失败 error_type=timeout 不变", test_xhrun_timeout_failure_still_has_error_type());

echo "\n=== 测试结果: $pass 通过, $fail 失败 ===\n";
exit($fail > 0 ? 1 : 0);
