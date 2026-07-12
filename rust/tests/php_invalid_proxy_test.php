<?php
// +----------------------------------------------------------------------+
// | 测试无效代理配置 - fail-fast 与不 panic 保证                            |
// |                                                                        |
// | v1.0.7 改进：global_client() 初始化失败（如无效代理）不再 panic 杀死   |
// | PHP 进程，而是返回错误以 PHP 异常形式抛出，用户可 try/catch。           |
// |                                                                        |
// | v1.1.0 改进（BREAKING）：空字符串代理在 setConfig() 阶段 fail-fast       |
// | 抛异常，不再延迟到 execute()。非空但格式非法的代理仍由 global_client()  |
// | 在 execute() 阶段报错（保留 v1.0.7 的"不 panic"保证）。                 |
// |                                                                        |
// | 本测试验证：                                                            |
// | 1. 空字符串代理 → setConfig() 立即抛异常（fail-fast）                   |
// | 2. 非空但非法的代理（"://"）→ execute() 抛异常，不 panic                 |
// | 3. 异常消息含代理错误描述                                                |
// | 4. 全局单例 OnceLock 缓存错误后，后续调用仍返回相同错误（不 panic）       |
// +----------------------------------------------------------------------+

echo "测试无效代理配置...\n";

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

// --------------------------------------------------------------------
// 1. 空字符串代理：v1.1.0 fail-fast，setConfig() 立即抛异常
// --------------------------------------------------------------------
$emptyProxyException = false;
$emptyProxyMessage = '';
try {
    XHCurl::setConfig(array('proxy' => ''));
} catch (\Throwable $e) {
    $emptyProxyException = true;
    $emptyProxyMessage = $e->getMessage();
}
check("空字符串代理 setConfig() 立即抛异常（fail-fast）", $emptyProxyException);
check("空字符串代理异常消息含 proxy", strpos($emptyProxyMessage, 'proxy') !== false);

// 恢复默认配置
XHCurl::setConfig(array('proxy' => null));

// --------------------------------------------------------------------
// 2. 非空但非法的代理（"://" 缺 scheme）→ execute() 抛异常，不 panic
//    验证 v1.0.7 的"global_client 失败不 panic"保证仍然成立
// --------------------------------------------------------------------
XHCurl::setConfig(array('proxy' => '://'));

$req = XHCurl::createRequest('http://127.0.0.1:18399/get')->get()->timeout(5);

$exceptionCaught = false;
$exceptionMessage = '';
try {
    $req->execute();
} catch (\Throwable $e) {
    $exceptionCaught = true;
    $exceptionMessage = $e->getMessage();
}

check("非法代理 execute() 抛出异常（不 panic）", $exceptionCaught);
check("异常消息含代理错误描述", strpos($exceptionMessage, '代理') !== false || strpos($exceptionMessage, 'proxy') !== false || strpos($exceptionMessage, 'builder') !== false);

// --------------------------------------------------------------------
// 3. 全局 Client 未缓存成功（失败不写入 OnceLock）：第二次 execute() 仍抛相同错误
//    实现细节：create_client 失败时不更新 RwLock，下次调用检测到配置仍不匹配，
//    会再次尝试创建并返回相同错误。用户修正配置后可重试（不永久卡死）。
// --------------------------------------------------------------------
$secondException = false;
$secondMessage = '';
try {
    $req2 = XHCurl::createRequest('http://127.0.0.1:18399/get')->get()->timeout(5);
    $req2->execute();
} catch (\Throwable $e) {
    $secondException = true;
    $secondMessage = $e->getMessage();
}
check("失败不缓存成功：第二次仍抛异常（可重试）", $secondException);
check("两次错误消息一致", $secondMessage === $exceptionMessage);

// 恢复默认配置（避免影响后续测试）
XHCurl::setConfig(array('proxy' => null));

echo "\n=== 测试结果: $pass 通过, $fail 失败 ===\n";
exit($fail > 0 ? 1 : 0);
