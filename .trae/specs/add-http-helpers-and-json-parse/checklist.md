# 验证检查清单

## mock_server 新端点

- [x] `/echo-query` 端点：返回 200 + JSON `{"query": $_GET}`（回显查询参数）
- [x] `/echo-json` 端点：返回 200 + Content-Type: application/json + 固定 JSON body
- [x] `/text` 端点：返回 200 + Content-Type: text/plain + "plain text"

## query() 方法

- [x] `XhRequest` 结构体新增 `query_params` 字段
- [x] `query()` 方法追加参数到 `query_params`（非覆盖）
- [x] `to_reqwest()` 中通过 `Url::query_pairs_mut()` 合并查询参数
- [x] 已有 URL 查询参数保留，新参数追加
- [x] 多次调用 `query()` 累加参数
- [x] 标量值（int/float/bool/null）自动转字符串
- [x] 空数组 `query([])` 不抛异常
- [x] 嵌套数组/对象元素抛异常（fail-fast）
- [x] `get_query_params()` getter 返回已设置参数

## accept() 方法

- [x] 设置 `Accept` header
- [x] 等价于 `header('Accept', $type)`
- [x] 空字符串抛异常（fail-fast）
- [x] 多次调用覆盖

## contentType() 方法

- [x] 设置 `Content-Type` header
- [x] 等价于 `header('Content-Type', $type)`
- [x] 空字符串抛异常（fail-fast）
- [x] 多次调用覆盖

## executeJson() 方法

- [x] 成功解析 JSON 响应返回 PHP 值（关联数组）
- [x] Content-Type 非 `application/json` 抛异常（含实际类型）
- [x] JSON 解析失败抛异常（含 json_last_error_msg）
- [x] 请求失败（success=false）抛异常（含 error 字段）
- [x] 不影响 `execute()` 的返回数组结构

## README 文档

- [x] XHRequest 方法表新增 `query()` 行
- [x] XHRequest 方法表新增 `accept()` 行
- [x] XHRequest 方法表新增 `contentType()` 行
- [x] XHRequest 方法表新增 `executeJson()` 行
- [x] query() 合并语义说明
- [x] executeJson() 失败/非 JSON 抛异常说明

## 版本与 CHANGELOG

- [x] `rust/Cargo.toml` version = "1.4.0"
- [x] `CHANGELOG.md` 包含 [1.4.0] 条目

## 测试覆盖

- [x] query() 追加参数到无参数 URL
- [x] query() 合并已有 URL 查询参数
- [x] query() 多次调用累加
- [x] query() 标量值转换（bool→1/0, null→空, int/float→字符串）
- [x] query([]) 空数组不抛异常
- [x] query() 嵌套数组抛异常
- [x] accept() 设置 Accept header
- [x] accept('') 空字符串抛异常
- [x] contentType() 设置 Content-Type header
- [x] contentType('') 空字符串抛异常
- [x] accept()/contentType() 多次调用覆盖
- [x] executeJson() 成功解析 JSON
- [x] executeJson() 非 JSON Content-Type 抛异常
- [x] executeJson() 请求失败抛异常

## 编译与运行

- [x] `cargo fmt --check` 通过
- [x] `cargo clippy --all-targets --features php -- -D warnings` 通过
- [x] `cargo test --lib --features php` 98+ 用例通过
- [x] `cargo build --release --features php` 成功
- [x] .so 已同步到 PHP 扩展目录
- [x] `php -d extension=xhcurl -r 'echo XHCurl::version();'` 输出 `1.4.0`

## 测试套件

- [x] `rust/tests/php_add_http_helpers_test.php` 创建并全部通过
- [x] 全部 24 个 PHP 测试文件 PASS（含本轮新增 1 个）
