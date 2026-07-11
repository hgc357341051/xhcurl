<?php
// +----------------------------------------------------------------------+
// | XHCurl 测试用 Mock HTTP 服务器                                         |
// |                                                                        |
// | 通过 PHP 内置服务器运行（php -S 127.0.0.1:18399 mock_server.php）       |
// | 提供 /get、/post、/cookies、/stream 端点，供 PHP 测试套件使用。           |
// |                                                                        |
// | 端点说明：                                                              |
// |   /get     返回 200 + 两个 Set-Cookie 头（测试重复头合并）+ JSON body      |
// |   /post    回显请求：{"json": <解析后的JSON请求体>, ...}                   |
// |   /cookies 回显请求的 Cookie 头（测试 cookies() 方法实际发送的内容）        |
// |   /stream  流式输出大响应体，分多次 flush，触发 onChunk 多次回调            |
// |                                                                        |
// | 注意：/hang 端点已移除，改由独立 socat 进程在 18400 端口提供              |
// | （socat TCP-LISTEN:18400,fork,reuseaddr SYSTEM:'sleep 60'），               |
// | fork 模式不阻塞，避免 PHP 内置单进程服务器被 sleep 阻塞。                   |
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

if ($path === '/cookies') {
    // 回显请求的 Cookie 头（用于测试 cookies() 方法实际发送的内容）
    header('Content-Type: application/json');
    echo json_encode([
        'cookie_header' => $_SERVER['HTTP_COOKIE'] ?? '',
        'headers' => getallheaders(),
    ]);
    return;
}

if ($path === '/stream') {
    // 流式测试端点：分多次输出大响应体，触发 onChunk 多次回调
    // 参数：
    //   n    = 段数（默认 50，上限 500）
    //   size = 每段字节数（默认 1024，上限 8192）
    $n = isset($_GET['n']) ? max(1, min(500, (int)$_GET['n'])) : 50;
    $size = isset($_GET['size']) ? max(1, min(8192, (int)$_GET['size'])) : 1024;

    header('Content-Type: text/plain');
    header('X-Stream-Test: true');

    // 关闭输出缓冲，确保每段数据立即发送（模拟流式响应）
    while (ob_get_level() > 0) {
        ob_end_flush();
    }

    for ($i = 0; $i < $n; $i++) {
        // 每段：序号 + 冒号 + 填充数据 + 换行，总长度约 size 字节
        $prefix = "segment-{$i}:";
        $padding = str_repeat('x', max(0, $size - strlen($prefix) - 1));
        echo $prefix . $padding . "\n";
        // 刷新输出缓冲到客户端，使 reqwest 能分块接收
        flush();
    }
    return;
}

// 默认：404
http_response_code(404);
echo json_encode(['error' => 'not found', 'path' => $path]);
