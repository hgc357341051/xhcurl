# 验证检查清单

## mock_server 新端点

- [x] `/redirect?n=N` 端点：n>0 返回 302 + Location: /redirect?n=N-1；n=0 返回 200 + JSON
- [x] `/large?size=N` 端点：返回 200 + 指定字节数的响应体

## HTTP 响应超限 error_type 分类

- [x] 响应体超过 max_response_size 时 `error_type = "response_too_large"`
- [x] 普通网络错误（dns/timeout/ssl/connection）error_type 不受影响
- [x] `classify_error_type` 函数正确识别"超过最大限制"关键词

## HTTP 结果数组 truncated 字段

- [x] 响应体超限时 `truncated = true`，`success = false`，`body = ""`
- [x] 正常响应 `truncated = false`（无论成功或失败）
- [x] execute() 结果数组含 truncated 字段
- [x] XHMulti::execute() 结果数组含 truncated 字段
- [x] XHThreadPool::execute() 结果数组含 truncated 字段
- [x] fiber_await()/gather()/each() 结果数组含 truncated 字段
- [x] xhrun 结果数组已有 truncated 字段（保持不变）

## README 文档修正

- [x] 第 1208 行附近"截断时 success 仍为 true"已删除
- [x] 替换为实际行为描述：success=false、body=""、error_type="response_too_large"、truncated=true
- [x] error_type 值集描述包含 `response_too_large`
- [x] HTTP 结果数组字段表包含 `truncated` 行

## 版本与 CHANGELOG

- [x] `rust/Cargo.toml` version = "1.3.0"
- [x] `CHANGELOG.md` 包含 [1.3.0] 条目

## 重定向测试覆盖

- [x] maxRedirects(0) 对 `/redirect?n=1` 返回 status=302 不跟随
- [x] maxRedirects(5) 对 `/redirect?n=3` 跟随到 status=200
- [x] followRedirects(false) 对 `/redirect?n=1` 返回 status=302
- [x] followRedirects(true)->maxRedirects(5) 对 `/redirect?n=3` 跟随到 status=200

## HTTP 响应超限测试

- [x] `/large?size=8192` + `maxResponseSize(1024)` 触发截断
- [x] 截断时 success=false、body=""、body_size=0
- [x] 截断时 error_type="response_too_large"
- [x] 截断时 truncated=true

## error_type 值集测试

- [x] DNS 失败 error_type="dns"
- [x] 超时失败 error_type="timeout"
- [x] 连接拒绝 error_type="connection"
- [x] 响应超限 error_type="response_too_large"
- [x] 成功路径 error_type=""

## body_size 一致性

- [x] 成功响应 body_size === strlen(body)

## 编译与运行

- [x] `cargo fmt --check` 通过
- [x] `cargo clippy --all-targets --features php -- -D warnings` 通过
- [x] `cargo test --lib --features php` 98+ 用例通过
- [x] `cargo build --release --features php` 成功
- [x] .so 已同步到 PHP 扩展目录
- [x] `php -d extension=xhcurl -r 'echo XHCurl::version();'` 输出 `1.3.0`

## 测试套件

- [x] `rust/tests/php_unify_truncation_and_redirect_test.php` 创建并全部通过
- [x] 全部 23 个 PHP 测试文件 PASS（含本轮新增 1 个）
- [x] 现有测试不受新增 truncated 字段影响（若做字段集严格比较已更新）
