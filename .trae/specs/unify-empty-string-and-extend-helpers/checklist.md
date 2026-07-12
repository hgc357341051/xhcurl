# 验证检查清单

## 空字符串 fail-fast 校验

- [x] `userAgent('')` 抛异常，消息含 `userAgent` 与 `传 null 清除 User-Agent 覆盖`
- [x] `userAgent(null)` 不抛异常，清除请求级 UA 覆盖
- [x] `encoding('')` 抛异常，消息含 `encoding` 与 `传 null 清除 Accept-Encoding 覆盖`
- [x] `encoding(null)` 不抛异常
- [x] `range('')` 抛异常，消息含 `range` 与 `传 null 清除 Range 覆盖`
- [x] `range(null)` 不抛异常
- [x] `cookies('')` 抛异常，消息含 `cookies` 与 `传 null 清除 Cookie 覆盖`
- [x] `cookies(null)` 不抛异常

## 错误消息格式

- [x] `bearerToken('')` 异常消息含 `bearerToken 不能为空字符串，传 null 清除 Bearer Token`
- [x] `maxRedirects(-1)` 异常消息含 `maxRedirects 不能为负值，0 = 不跟随重定向`
- [x] `xhrun('echo', [], [], -1)` 异常消息含 `xhrun timeout 不能为负值，0 = 无超时`
- [x] `xhrun('echo', [], [], 0, -1)` 异常消息含 `xhrun max_output 不能为负值，0 = 无限制`

## 新增 setter：referer

- [x] `->referer('https://example.com/page')` 后 execute() 请求头含 `Referer: https://example.com/page`
- [x] `->referer('')` 抛异常，消息含 `referer` 与 `传 null 清除 Referer 覆盖`
- [x] `->referer(null)` 不抛异常
- [x] `getReferer()` 返回 referer() 设置的值
- [x] `getReferer()` 未设置时返回 null

## 新增 setter：cookie 增量

- [x] `->cookie('session', 'abc')->cookie('token', 'xyz')` 后 execute() Cookie 头含 `session=abc` 与 `token=xyz`
- [x] `->cookie('', 'value')` 抛异常，消息含 `cookie` 与 `name 不能为空`
- [x] `->cookie('name', '')` 抛异常，消息含 `cookie` 与 `value 不能为空`
- [x] cookie 增量添加不覆盖原有 cookies 字符串内容

## 新增 setter：jsonStr

- [x] `->jsonStr('{"k":"v"}')` 后 execute() 请求头含 `Content-Type: application/json`
- [x] `->jsonStr('{"k":"v"}')` 后 execute() body 为 `{"k":"v"}`
- [x] `->jsonStr('{"invalid":}')` 抛异常，消息含 `jsonStr` 与 `无效 JSON`

## 新增 getter：getHeader

- [x] `->header('Content-Type', 'application/json')` 后 `getHeader('content-type')` 返回 `application/json`
- [x] `->header('Content-Type', 'application/json')` 后 `getHeader('CONTENT-TYPE')` 返回 `application/json`（大小写不敏感）
- [x] 未设置的 header `getHeader('X-Not-Set')` 返回 null

## 新增 getter：getMultipart

- [x] `->multipart([['name' => 'file', 'contents' => 'data', 'filename' => 'test.txt']])` 后 `getMultipart()` 返回数组
- [x] 返回的数组每个元素含 `name`/`contents`/`filename` 三键
- [x] 未设置 multipart 时 `getMultipart()` 返回 null
- [x] 设置了 `json()` 后 `getMultipart()` 返回 null

## 扩展 getter：getBody

- [x] `->body('raw bytes')` 后 `getBody()` 返回 `raw bytes`（行为不变）
- [x] `->json(['k' => 'v'])` 后 `getBody()` 返回 `{"k":"v"}`
- [x] `->form(['k' => 'v'])` 后 `getBody()` 返回 `k=v`
- [x] `->multipart([...])` 后 `getBody()` 返回 null

## maxRedirects(0) 文档化与测试

- [x] README 明确说明 `maxRedirects(0)` 等价于 `followRedirects(false)`
- [x] 测试验证 `maxRedirects(0)` 对 `/redirect` 端点返回 3xx 而非跟随

## 编译与运行

- [x] `cargo fmt --check` 通过
- [x] `cargo clippy --all-targets --features php -- -D warnings` 通过
- [x] `cargo test --lib --features php` 98+ 用例通过
- [x] `cargo build --release --features php` 成功
- [x] .so 已同步到 PHP 扩展目录
- [x] `php -d extension=xhcurl -r 'echo XHCurl::version();'` 输出 `1.2.0`

## 文档与版本

- [x] `rust/Cargo.toml` version = "1.2.0"
- [x] `CHANGELOG.md` 包含 [1.2.0] 条目，标注 BREAKING 与新增方法
- [x] `README.md` 新增 `referer()`/`cookie()`/`jsonStr()` 方法表行
- [x] `README.md` 新增 `getHeader()`/`getMultipart()`/`getReferer()` 方法表行
- [x] `README.md` 修改 `getBody()` 说明为返回 body/json/form 序列化字符串
- [x] `README.md` 说明 `maxRedirects(0)` 等价于 `followRedirects(false)`

## 测试套件

- [x] `rust/tests/php_unify_empty_and_helpers_test.php` 创建并全部通过（约 30 用例）
- [x] 全部 22 个 PHP 测试文件 PASS（含本轮新增 1 个）
- [x] 现有测试不受 BREAKING 影响（若受影响已修复为 null 清除或断言抛异常）
