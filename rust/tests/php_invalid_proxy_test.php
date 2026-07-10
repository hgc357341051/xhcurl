<?php
// 测试无效代理配置 - 应 panic（首次使用 global_client 时）
echo "测试无效代理配置...\n";

// 设置空字符串代理（reqwest::Proxy::all("") 会返回 Err）
XHCurl::setConfig(array('proxy' => ''));

// 第一次调用 execute() 会触发 global_client() 初始化
// create_client_builder 对空代理返回 Err，global_client 应 panic
$req = XHCurl::createRequest('http://127.0.0.1:18399/get')->get()->timeout(5);
$result = $req->execute();

echo "错误：应该 panic 但没有\n";
exit(1);

