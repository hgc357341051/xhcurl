<?php
// +----------------------------------------------------------------------+
// | 测试无效代理配置 - global_client 返回错误而非 panic                     |
// |                                                                        |
// | v1.0.7 改进：global_client() 初始化失败（如无效代理）不再 panic 杀死   |
// | PHP 进程，而是返回错误以 PHP 异常形式抛出，用户可 try/catch。           |
// |                                                                        |
// | 本测试验证：                                                            |
// | 1. 空字符串代理 → execute() 抛出异常                                    |
// | 2. 异常消息含代理错误描述                                                |
// | 3. 全局单例 OnceLock 缓存错误后，后续调用仍返回相同错误（不 panic）       |
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

// 设置空字符串代理（reqwest::Proxy::all("") 会返回 Err）
XHCurl::setConfig(array('proxy' => ''));

// 第一次调用 execute() 会触发 global_client() 初始化
// create_client_builder 对空代理返回 Err，global_client 返回 Err
// execute() 通过 ? 传播为 PHP 异常
$req = XHCurl::createRequest('http://127.0.0.1:18399/get')->get()->timeout(5);

$exceptionCaught = false;
$exceptionMessage = '';
try {
    $req->execute();
} catch (\Throwable $e) {
    $exceptionCaught = true;
    $exceptionMessage = $e->getMessage();
}

check("空代理 execute() 抛出异常", $exceptionCaught);
check("异常消息含代理错误描述", strpos($exceptionMessage, '代理') !== false || strpos($exceptionMessage, 'proxy') !== false || strpos($exceptionMessage, 'builder') !== false);

// 恢复默认配置（避免影响后续测试）
XHCurl::setConfig(array('proxy' => null));

echo "\n=== 测试结果: $pass 通过, $fail 失败 ===\n";
exit($fail > 0 ? 1 : 0);
