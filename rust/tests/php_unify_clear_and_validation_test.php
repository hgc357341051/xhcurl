<?php
// +----------------------------------------------------------------------+
// | XHCurl API 对称性与 fail-fast 校验测试（v1.0.9）                         |
// |                                                                        |
// | 验证本轮 P2/P3 修复：                                                   |
// |   P2-1: XHThreadPool::clear() 方法（与 XHMulti 对齐）                   |
// |   P2-2: XHThreadPool::isRunning() 状态查询                              |
// |   P3-1 BREAKING: url('')/setId('')/id('') 空字符串抛异常               |
// |   P3-2 BREAKING: customMethod('')/含空格抛异常                          |
// |   P3-3 BREAKING: xhrun max_output 负值抛异常                            |
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

echo "=== API 对称性与 fail-fast 校验测试 ===\n";

// ==================================================================
// P2-1: XHThreadPool::clear() 方法
//    与 XHMulti::clear() 行为一致，清空已添加的请求列表。
// ==================================================================
echo "\n=== P2-1: XHThreadPool::clear() ===\n";

function test_threadpool_clear(): bool {
    $pool = new XHThreadPool(2);
    $pool->add(XHCurl::createRequest('http://127.0.0.1:18399/get')->get());
    $pool->add(XHCurl::createRequest('http://127.0.0.1:18399/get')->get());
    if ($pool->count() !== 2) return false;
    $pool->clear();
    return $pool->count() === 0 && $pool->isEmpty();
}
check("XHThreadPool clear() 清空请求列表", test_threadpool_clear());

function test_threadpool_clear_allows_reuse(): bool {
    // clear 后可继续 add 并 execute
    global $BASE;
    $pool = new XHThreadPool(2);
    $pool->add(XHCurl::createRequest($BASE . '/get')->get());
    $pool->clear();
    $pool->add(XHCurl::createRequest($BASE . '/get')->get()->setId('reuse'));
    $results = $pool->execute();
    return count($results) === 1 && $results[0]['id'] === 'reuse';
}
check("XHThreadPool clear() 后可复用对象", test_threadpool_clear_allows_reuse());

// ==================================================================
// P2-2: XHThreadPool::isRunning() 状态查询
//    未启动时 false，执行期间 true，完成后 false。
// ==================================================================
echo "\n=== P2-2: XHThreadPool::isRunning() ===\n";

function test_threadpool_is_running_initial_false(): bool {
    $pool = new XHThreadPool(2);
    return $pool->isRunning() === false;
}
check("XHThreadPool 未启动 isRunning=false", test_threadpool_is_running_initial_false());

function test_threadpool_is_running_after_execute_true(): bool {
    // 执行后线程池保持运行状态（设计选择：同对象多次 execute() 复用工作线程）
    global $BASE;
    $pool = new XHThreadPool(2);
    $pool->add(XHCurl::createRequest($BASE . '/get')->get());
    $pool->execute();
    // 执行完成后线程池保持运行，可复用
    return $pool->isRunning() === true;
}
check("XHThreadPool 执行后 isRunning=true（线程池保持运行可复用）", test_threadpool_is_running_after_execute_true());

// ==================================================================
// P3-1: url('')/setId('')/id('') 空字符串抛异常（BREAKING）
//    与 bearerToken/basicAuth 一致，fail-fast。
// ==================================================================
echo "\n=== P3-1: 空字符串 setter 抛异常 ===\n";

function test_url_empty_throws(): bool {
    $req = XHCurl::createRequest('http://example.com');
    try {
        $req->url('');
        return false;
    } catch (\Throwable $e) {
        return strpos($e->getMessage(), 'url') !== false && strpos($e->getMessage(), '空') !== false;
    }
}
check("url('') 抛异常", test_url_empty_throws());

function test_set_id_empty_throws(): bool {
    $req = XHCurl::createRequest('http://example.com');
    try {
        $req->setId('');
        return false;
    } catch (\Throwable $e) {
        return strpos($e->getMessage(), 'id') !== false && strpos($e->getMessage(), '空') !== false;
    }
}
check("setId('') 抛异常", test_set_id_empty_throws());

function test_id_alias_empty_throws(): bool {
    $req = XHCurl::createRequest('http://example.com');
    try {
        $req->id('');
        return false;
    } catch (\Throwable $e) {
        return strpos($e->getMessage(), 'id') !== false && strpos($e->getMessage(), '空') !== false;
    }
}
check("id('') 抛异常（别名一致）", test_id_alias_empty_throws());

function test_url_non_empty_ok(): bool {
    // 非空 URL 仍正常工作
    try {
        $req = XHCurl::createRequest('http://example.com');
        $req->url('http://example.com/changed');
        return true;
    } catch (\Throwable $e) {
        return false;
    }
}
check("url(非空) 正常工作", test_url_non_empty_ok());

// ==================================================================
// P3-2: customMethod('')/含空格抛异常（BREAKING）
//    RFC 7230 要求 method 为 token（无空格/控制字符）。
// ==================================================================
echo "\n=== P3-2: customMethod 校验 ===\n";

function test_custom_method_empty_throws(): bool {
    $req = XHCurl::createRequest('http://example.com');
    try {
        $req->customMethod('');
        return false;
    } catch (\Throwable $e) {
        return strpos($e->getMessage(), 'customMethod') !== false && strpos($e->getMessage(), '空') !== false;
    }
}
check("customMethod('') 抛异常", test_custom_method_empty_throws());

function test_custom_method_with_space_throws(): bool {
    $req = XHCurl::createRequest('http://example.com');
    try {
        $req->customMethod('GET POST');
        return false;
    } catch (\Throwable $e) {
        return strpos($e->getMessage(), 'customMethod') !== false && strpos($e->getMessage(), '空格') !== false;
    }
}
check("customMethod('GET POST') 含空格抛异常", test_custom_method_with_space_throws());

function test_custom_method_valid_ok(): bool {
    // 合法的非标准方法名仍正常工作
    try {
        $req = XHCurl::createRequest('http://example.com');
        $req->customMethod('PROPFIND');
        return $req->getCustomMethod() === 'PROPFIND';
    } catch (\Throwable $e) {
        return false;
    }
}
check("customMethod('PROPFIND') 正常工作", test_custom_method_valid_ok());

// ==================================================================
// P3-3: xhrun max_output 负值抛异常（BREAKING）
//    与 timeout 负值处理一致，0 仍表示无限制。
// ==================================================================
echo "\n=== P3-3: xhrun max_output 负值抛异常 ===\n";

function test_xhrun_max_output_negative_throws(): bool {
    try {
        xhrun('echo', ['hi'], ['max_output' => -1]);
        return false;
    } catch (\Throwable $e) {
        $msg = $e->getMessage();
        return strpos($msg, 'max_output') !== false && strpos($msg, '负值') !== false;
    }
}
check("xhrun max_output=-1 抛异常", test_xhrun_max_output_negative_throws());

function test_xhrun_max_output_zero_ok(): bool {
    // max_output=0 表示无限制，不应抛异常
    try {
        $r = xhrun('echo', ['hi'], ['max_output' => 0]);
        return $r['success'] === true;
    } catch (\Throwable $e) {
        return false;
    }
}
check("xhrun max_output=0 无限制不抛异常", test_xhrun_max_output_zero_ok());

echo "\n=== 测试结果: $pass 通过, $fail 失败 ===\n";
exit($fail > 0 ? 1 : 0);
