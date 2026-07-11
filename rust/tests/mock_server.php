<?php
// +----------------------------------------------------------------------+
// | XHCurl 测试用 Mock HTTP 服务器                                         |
// |                                                                        |
// | 通过 PHP 内置服务器运行（php -S 127.0.0.1:18399 mock_server.php）       |
// | 提供 /get、/post、/hang 端点，供 PHP 测试套件使用。                      |
// |                                                                        |
// | 端点说明：                                                              |
// |   /get   返回 200 + 两个 Set-Cookie 头（测试重复头合并）+ JSON body      |
// |   /post  回显请求：{"json": <解析后的JSON请求体>, ...}                   |
// |   /hang  挂起连接 60 秒（测试超时中止）                                  |
// +----------------------------------------------------------------------+

$uri = $_SERVER['REQUEST_URI'] ?? '';
$path = parse_url($uri, PHP_URL_PATH) ?: '';

if ($path === '/get') {
    // 返回 200 + 两个 Set-Cookie 头（测试 RFC 7230 §3.2.2 重复头合并）
    // 第二参数 false = 追加而非替换，确保两个 Set-Cookie 都发送
    header('Set-Cookie: session=abc123; Path=/', false);
    header('Set-Cookie: tracking=xyz789; Path=/', false);
    header('Content-Type: application/json');
    echo json_encode(['url' => $uri, 'args' => $_GET]);
    return;
}

if ($path === '/post') {
    // 回显请求体（模拟 httpbin.org/post 行为）
    // 请求体可能是 JSON、form-data、raw 等
    $rawBody = file_get_contents('php://input');
    $jsonBody = null;
    if ($rawBody !== '' && $rawBody !== false) {
        $decoded = json_decode($rawBody, true);
        $jsonBody = $decoded;
    }
    header('Content-Type: application/json');
    echo json_encode([
        'json' => $jsonBody,
        'data' => $rawBody,
        'headers' => getallheaders(),
        'url' => $uri,
    ]);
    return;
}

if ($path === '/hang') {
    // 挂起连接 60 秒，模拟不响应的服务器（测试超时中止）
    sleep(60);
    echo 'ok';
    return;
}

// 默认：404
http_response_code(404);
echo json_encode(['error' => 'not found', 'path' => $path]);
