# 验证检查清单

## 全局 base_uri 配置

- [x] GlobalConfig 新增 `base_uri: Option<String>` 字段
- [x] setConfig 校验 base_uri（类型/空字符串/URL 格式）
- [x] 请求 URL 以 `/` 开头时与 base_uri 拼接
- [x] 绝对 URL（http/https）不拼接 base_uri
- [x] base_uri 末尾斜杠处理（无双斜杠）
- [x] base_uri 为 null 时清除
- [x] base_uri 变更触发 Client 重建（配置指纹比对包含）
- [x] getConfig 返回 base_uri

## 全局 base_headers 配置

- [x] GlobalConfig 新增 `base_headers: HashMap<String, String>` 字段
- [x] setConfig 校验 base_headers（类型/元素标量）
- [x] 所有请求自动携带 base_headers
- [x] 请求级同名 header 覆盖全局 base_headers
- [x] base_headers 为 null/空数组时清除
- [x] base_headers 变更触发 Client 重建
- [x] getConfig 返回 base_headers

## withOptions() 方法

- [x] 支持批量设置多个选项（timeout/headers/query/accept 等）
- [x] 未知 key 抛异常（fail-fast，含 key 名）
- [x] null 值跳过（不调用对应 setter）
- [x] headers 数组中 null 值跳过
- [x] 多次调用 withOptions 累加
- [x] 与链式 setter 混用正常（后调用覆盖）
- [x] 支持的 key 涵盖常见选项（timeout/timeout_ms/connect_timeout/headers/query/accept/content_type/body/json/form/user_agent/referer/encoding/range/proxy/verify_ssl/follow_redirects/max_redirects）

## mock_server 新端点

- [x] `/base-test` 端点：返回 200 + JSON `{"url": REQUEST_URI, "headers": getallheaders()}`

## README 文档

- [x] XHRequest 方法表新增 `withOptions()` 行
- [x] setConfig 配置项表新增 `base_uri` 行
- [x] setConfig 配置项表新增 `base_headers` 行
- [x] 新增微服务场景示例小节
- [x] withOptions 支持的选项 key 表
- [x] base_uri 拼接规则说明

## 版本与 CHANGELOG

- [x] `rust/Cargo.toml` version = "1.5.0"
- [x] `CHANGELOG.md` 包含 [1.5.0] 条目

## 测试覆盖

- [x] withOptions() 批量设置多个选项
- [x] withOptions() 未知 key 抛异常
- [x] withOptions() null 值跳过
- [x] withOptions() 与链式 setter 混用
- [x] withOptions() headers 中 null 值跳过
- [x] withOptions() 多次调用累加
- [x] base_uri 相对 URL 拼接
- [x] base_uri 绝对 URL 优先
- [x] base_uri 末尾斜杠处理
- [x] base_uri 为 null 时清除
- [x] base_headers 自动携带
- [x] base_headers 请求级覆盖
- [x] base_headers 为 null 时清除
- [x] base_uri + base_headers 组合

## 编译与运行

- [x] `cargo fmt --check` 通过
- [x] `cargo clippy --all-targets --features php -- -D warnings` 通过
- [x] `cargo test --lib --features php` 98+ 用例通过
- [x] `cargo build --release --features php` 成功
- [x] .so 已同步到 PHP 扩展目录
- [x] `php -d extension=xhcurl -r 'echo XHCurl::version();'` 输出 `1.5.0`

## 测试套件

- [x] `rust/tests/php_add_withoptions_and_base_config_test.php` 创建并全部通过（14/14）
- [x] 全部 25 个 PHP 测试文件 PASS（含本轮新增 1 个，EXIT_CODE=0）
