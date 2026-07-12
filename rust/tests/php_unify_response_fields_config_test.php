<?php
// +----------------------------------------------------------------------+
// | XHCurl 统一响应字段稳定性与 setConfig 校验测试                          |
// |                                                                        |
// | 验证本轮 P1/P2/P3 修复：                                                |
// |   P1-1: XHThreadPool::executeEach 在回调校验后才 take requests          |
// |         （无效回调不导致 requests 丢失）                                 |
// |   P2-1: result_to_php_array 成功路径插入 error_type 空字符串             |
// |   P2-2: fill_response_fields 中 error 无条件插入（None 时为空字符串）    |
// |   P2-3 BREAKING: setConfig 7 处数值配置负值现在抛异常                   |
// |   P3-1: setConfig 两阶段校验（类型错误时不应用任何配置）                |
// |                                                                        |
// | 注意：多数用例需 mock 服务器（/get）。                                   |
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

echo "=== 统一响应字段稳定性与 setConfig 校验测试 ===\n";

// ==================================================================
// P1-1: XHThreadPool executeEach 无效回调时 requests 不丢失
//    executeEach 在回调校验通过后才 take requests，
//    无效回调（非 callable）抛异常时 requests 仍保留在 pool 中。
// ==================================================================
echo "\n=== P1-1: XHThreadPool executeEach 无效回调时 requests 不丢失 ===\n";

function test_threadpool_execute_each_invalid_callback_preserves_requests(): bool {
    global $BASE;
    $pool = new XHThreadPool(2);
    $pool->add(XHCurl::createRequest($BASE . '/get')->get());
    $pool->add(XHCurl::createRequest($BASE . '/get')->get());
    if ($pool->count() !== 2) return false;
    try {
        $pool->executeEach('not_callable');  // 无效回调
        return false;  // 应抛异常
    } catch (\Throwable $e) {
        // 验证 requests 未丢失
        return $pool->count() === 2;
    }
}
check("executeEach 无效回调时 requests 不丢失", test_threadpool_execute_each_invalid_callback_preserves_requests());

// ==================================================================
// P2-1: 成功响应含 error_type 字段
//    result_to_php_array 成功路径插入 error_type 空字符串，
//    保持字段集与失败路径一致。
// ==================================================================
echo "\n=== P2-1: 成功响应含 error_type 字段 ===\n";

function test_response_success_has_error_type(): bool {
    global $BASE;
    $resp = XHCurl::createRequest($BASE . '/get')->get()->execute();
    return array_key_exists('error_type', $resp) && $resp['error_type'] === '';
}
check("成功响应含 error_type 空字符串", test_response_success_has_error_type());

// ==================================================================
// P2-2: 成功响应含 error 字段
//    fill_response_fields 中 error 无条件插入（None 时为空字符串），
//    避免 PHP 端 $resp['error'] 触发 Undefined index。
// ==================================================================
echo "\n=== P2-2: 成功响应含 error 字段 ===\n";

function test_response_success_has_error_field(): bool {
    global $BASE;
    $resp = XHCurl::createRequest($BASE . '/get')->get()->execute();
    return array_key_exists('error', $resp) && $resp['error'] === '';
}
check("成功响应含 error 空字符串", test_response_success_has_error_field());

// ==================================================================
// P2-3: setConfig 负值抛异常（BREAKING）
//    7 处数值配置（connect_timeout/request_timeout/max_response_size/
//    max_redirects/tcp_keepalive_interval/max_connections/fiber_max_concurrency）
//    负值现在抛异常（之前静默跳过）。错误信息含字段名和「负值」关键词。
// ==================================================================
echo "\n=== P2-3: setConfig 负值抛异常 ===\n";

function test_set_config_negative_throws(): bool {
    try {
        XHCurl::setConfig(['connect_timeout' => -5]);
        return false;
    } catch (\Throwable $e) {
        $msg = $e->getMessage();
        return strpos($msg, 'connect_timeout') !== false && strpos($msg, '负值') !== false;
    }
}
check("setConfig 单个负值抛异常", test_set_config_negative_throws());

function test_set_config_multiple_negatives(): bool {
    try {
        XHCurl::setConfig(['connect_timeout' => -1, 'request_timeout' => -2]);
        return false;
    } catch (\Throwable $e) {
        $msg = $e->getMessage();
        return strpos($msg, 'connect_timeout') !== false && strpos($msg, 'request_timeout') !== false;
    }
}
check("setConfig 多个负值同时报错", test_set_config_multiple_negatives());

function test_set_config_zero_ok(): bool {
    try {
        XHCurl::setConfig(['connect_timeout' => 0, 'request_timeout' => 0]);
        return true;
    } catch (\Throwable $e) {
        return false;
    }
}
check("setConfig 0 不抛异常（0 = 无超时/默认）", test_set_config_zero_ok());

// 恢复默认配置，避免影响后续测试
XHCurl::setConfig(['connect_timeout' => 30, 'request_timeout' => 60]);

// 7 处数值配置逐一验证负值抛异常
function test_set_config_all_numeric_negatives_throw(): bool {
    $fields = [
        'connect_timeout',
        'request_timeout',
        'max_response_size',
        'max_redirects',
        'tcp_keepalive_interval',
        'max_connections',
        'fiber_max_concurrency',
    ];
    foreach ($fields as $field) {
        try {
            XHCurl::setConfig([$field => -1]);
            return false;  // 应抛异常
        } catch (\Throwable $e) {
            $msg = $e->getMessage();
            if (strpos($msg, $field) === false || strpos($msg, '负值') === false) {
                return false;
            }
        }
    }
    return true;
}
check("setConfig 7 处数值配置负值均抛异常", test_set_config_all_numeric_negatives_throw());

// ==================================================================
// P3-1: setConfig 类型错误时不部分应用
//    两阶段校验：先校验所有配置项类型，任一错误则不应用任何配置。
//    验证：类型错误时 connect_timeout 保持原值。
// ==================================================================
echo "\n=== P3-1: setConfig 类型错误时不部分应用 ===\n";

function test_set_config_type_error_no_partial_apply(): bool {
    // 先记录当前 connect_timeout
    $before = XHCurl::getConfig();
    $beforeCt = $before['connect_timeout'] ?? null;

    try {
        XHCurl::setConfig(['connect_timeout' => 30, 'verify_ssl' => 'invalid']);
        return false;  // 应抛异常
    } catch (\Throwable $e) {
        // 验证 connect_timeout 未被应用
        $after = XHCurl::getConfig();
        $afterCt = $after['connect_timeout'] ?? null;
        return $beforeCt === $afterCt;  // 保持原值
    }
}
check("setConfig 类型错误时不部分应用", test_set_config_type_error_no_partial_apply());

// 类型错误信息含字段名
function test_set_config_type_error_message_has_field(): bool {
    try {
        XHCurl::setConfig(['verify_ssl' => 'not_a_bool']);
        return false;
    } catch (\Throwable $e) {
        $msg = $e->getMessage();
        return strpos($msg, 'verify_ssl') !== false;
    }
}
check("setConfig 类型错误信息含字段名", test_set_config_type_error_message_has_field());

// ==================================================================
// 附加: 所有 xhrun 失败路径都含 error_type
//    验证 exit_error 和 timeout 两条失败路径均含 error_type 字段。
// ==================================================================
echo "\n=== 附加: xhrun 失败路径含 error_type ===\n";

function test_xhrun_all_failure_paths_have_error_type(): bool {
    // exit_error 路径
    $r1 = xhrun('sh', ['-c', 'exit 1']);
    if (!isset($r1['error_type']) || $r1['error_type'] !== 'exit_error') return false;

    // timeout 路径
    $r2 = xhrun('sleep', ['5'], ['timeout' => 1]);
    if (!isset($r2['error_type']) || $r2['error_type'] !== 'timeout') return false;

    return true;
}
check("xhrun 失败路径均含 error_type", test_xhrun_all_failure_paths_have_error_type());

echo "\n总计: $pass passed, $fail failed\n";
exit($fail > 0 ? 1 : 0);
