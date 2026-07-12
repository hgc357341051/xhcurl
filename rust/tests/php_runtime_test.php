<?php
// XHCurl 功能测试脚本
// 测试本轮所有代码改动的正确性

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

// 1. 扩展加载 + 版本
check("扩展加载", extension_loaded('xhcurl'));
check("version() 返回字符串", is_string(XHCurl::version()) && strlen(XHCurl::version()) > 0);
check("isCli() 返回布尔值", is_bool(XHCurl::isCli()));

// 2. 配置读写
XHCurl::setConfig(['request_timeout' => 45, 'connect_timeout' => 5]);
$cfg = XHCurl::getConfig();
check("setConfig/getConfig 读写", $cfg['request_timeout'] === 45 && $cfg['connect_timeout'] === 5);

// 恢复默认
XHCurl::setConfig(['request_timeout' => 30, 'connect_timeout' => 10]);

// 3. createRequest 返回正确类型
$req = XHCurl::createRequest('https://httpbin.org/get');
check("createRequest 返回 XHRequest", $req instanceof XHRequest);

// 4. 链式调用
$req2 = XHCurl::createRequest('https://httpbin.org/get')
    ->get()
    ->header('X-Test', 'value')
    ->timeout(15);
check("链式调用 get/header/timeout", $req2 instanceof XHRequest);
check("getUrl()", $req2->getUrl() === 'https://httpbin.org/get');
check("getMethod()", $req2->getMethod() === 'GET');

// 5. setUserData + getId
$req3 = XHCurl::createRequest('https://httpbin.org/get')
    ->setId('test-id-123')
    ->setUserData(['task' => 'upload', 'seq' => 42]);
check("setId + setUserData 链式", $req3 instanceof XHRequest);

// ===== xhrun 测试（本地命令，不需网络）=====
echo "\n--- xhrun 测试 ---\n";

// 6. xhrun 基本执行
$r = xhrun('echo', ['hello world']);
check("xhrun echo success", $r['success'] === true);
check("xhrun echo stdout", trim($r['stdout']) === 'hello world');
check("xhrun echo exit_code", $r['exit_code'] === 0);
check("xhrun 返回 pid", isset($r['pid']) && $r['pid'] > 0);
check("xhrun 返回 elapsed_ms", isset($r['elapsed_ms']) && $r['elapsed_ms'] >= 0);
check("xhrun timed_out=false", $r['timed_out'] === false);
check("xhrun truncated=false", $r['truncated'] === false);

// 7. xhrun 失败命令（非零退出码）
$r = xhrun('sh', ['-c', 'exit 3']);
check("xhrun 非零退出码 success=false", $r['success'] === false);
check("xhrun 非零退出码 exit_code=3", $r['exit_code'] === 3);

// 8. xhrun 超时测试（sleep 超过 timeout）
$start = microtime(true);
$r = xhrun('sleep', ['5'], ['timeout' => 1]);
$elapsed = microtime(true) - $start;
check("xhrun 超时 success=false", $r['success'] === false);
check("xhrun 超时 timed_out=true", $r['timed_out'] === true);
check("xhrun 超时 exit_code=-1", $r['exit_code'] === -1);
check("xhrun 超时含 command 字段", isset($r['command']) && $r['command'] === 'sleep');
check("xhrun 超时含 error 字段", isset($r['error']) && strpos($r['error'], '超时') !== false);
check("xhrun 超时实际耗时合理", $elapsed >= 0.9 && $elapsed < 3.0);

// 9. xhrun 截断测试（输出超过 max_output）
// 用 yes+head 产生 1000 字节（sh 兼容，不依赖 bash 花括号展开）
$r = xhrun('sh', ['-c', 'yes A | head -c 1000'], ['max_output' => 100]);
check("xhrun 截断 success=false", $r['success'] === false);
check("xhrun 截断 truncated=true", $r['truncated'] === true);
check("xhrun 截断 stdout 长度受限", strlen($r['stdout']) <= 100);
check("xhrun 截断含 command 字段", isset($r['command']));

// 10. xhrun 二进制安全 stdin
$r = xhrun('cat', [], ['input' => "binary\x00\x01\x02data"]);
check("xhrun stdin 二进制安全", $r['success'] === true && $r['stdout'] === "binary\x00\x01\x02data");

// 11. xhrun 白名单拒绝
$r = xhrun('ls', ['-la'], ['allow' => ['cat', 'echo']]);
check("xhrun 白名单拒绝 success=false", $r['success'] === false);
check("xhrun 白名单拒绝含 command 字段", isset($r['command']) && $r['command'] === 'ls');
check("xhrun 白名单拒绝 error 含白名单", strpos($r['error'], '白名单') !== false);

// 12. xhrun 黑名单拒绝
$r = xhrun('rm', ['-rf', '/tmp/nonexistent'], ['deny' => ['rm']]);
check("xhrun 黑名单拒绝 success=false", $r['success'] === false);
check("xhrun 黑名单拒绝 error 含黑名单", strpos($r['error'], '黑名单') !== false);

// 13. xhrun 负数 timeout 应抛出异常（配置校验错误，不是返回失败结果）
//     P3-1: 错误措辞从「不能为负数」改为「不能为负值」
$negTimeoutError = null;
try {
    xhrun('echo', ['test'], ['timeout' => -1]);
} catch (\Throwable $e) {
    $negTimeoutError = $e->getMessage();
}
check("xhrun 负数 timeout 抛异常", $negTimeoutError !== null && strpos($negTimeoutError, '负值') !== false);

// 14. xhrun 不经 shell 的注入防护
$r = xhrun('echo', ['foo; rm -rf /']);
check("xhrun 防注入（参数不经 shell）", $r['success'] === true && trim($r['stdout']) === 'foo; rm -rf /');

echo "\n=== 测试结果: $pass 通过, $fail 失败 ===\n";
exit($fail > 0 ? 1 : 0);
