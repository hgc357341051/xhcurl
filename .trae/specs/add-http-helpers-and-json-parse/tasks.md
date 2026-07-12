# Tasks

## 阶段 1：扩展 mock_server 测试基础设施

- [x] Task 1: 在 `rust/tests/mock_server.php` 新增 `/echo-query` 端点
  - [x] SubTask 1.1: 在路由分发逻辑中添加 `/echo-query` 路径处理
  - [x] SubTask 1.2: 返回 200 + JSON `{"query": $_GET}`（回显所有查询参数，便于断言合并结果）
- [x] Task 2: 在 `rust/tests/mock_server.php` 新增 `/echo-json` 端点
  - [x] SubTask 2.1: 在路由分发逻辑中添加 `/echo-json` 路径处理
  - [x] SubTask 2.2: 返回 200 + Content-Type: application/json + body `{"received": true, "method": "GET"}`（固定 JSON 响应）
- [x] Task 2b: 在 `rust/tests/mock_server.php` 新增 `/text` 端点
  - [x] SubTask 2b.1: 返回 200 + Content-Type: text/plain + body "plain text"（用于测试 executeJson 非 JSON 抛异常）

## 阶段 2：query() URL 查询参数方法

- [x] Task 3: 在 `rust/src/request.rs` 的 `XhRequest` 结构体新增 `query_params` 字段
  - [x] SubTask 3.1: 在 `XhRequest` 结构体（约 290 行附近）新增 `query_params: Vec<(String, String)>` 字段
  - [x] SubTask 3.2: 在 `new()` 与 `with_config()` 初始化为空 Vec
  - [x] SubTask 3.3: 在 `Clone`/`PartialEq` derive 中自动包含
- [x] Task 4: 在 `rust/src/request.rs` 实现 `query()` 方法
  - [x] SubTask 4.1: 新增 `pub fn query(mut self, params: I) -> Self` 方法，接受 `IntoIterator<Item = (String, String)>` 或类似
  - [x] SubTask 4.2: 将 params 追加到 `self.query_params`（非覆盖）
  - [x] SubTask 4.3: 新增 `pub fn get_query_params(&self) -> &[(String, String)]` getter
- [x] Task 5: 在 `rust/src/request.rs` 的 `to_reqwest()` 合并查询参数
  - [x] SubTask 5.1: 在构建 reqwest::RequestBuilder 前，解析 URL，用 `Url::query_pairs_mut()` 追加 `query_params`
  - [x] SubTask 5.2: 确保已有 URL 查询参数保留，新参数追加（非覆盖）
  - [x] SubTask 5.3: 更新 `OverrideKey` 哈希计算（若 query_params 影响客户端缓存键需包含）
- [x] Task 6: 在 `rust/src/php_ext.rs` 新增 `query()` PHP 绑定方法
  - [x] SubTask 6.1: 读取 `XHRequest::header()` 方法作为模板（约 692-700 行）
  - [x] SubTask 6.2: 新增 `pub fn query(self_, params: &ZendHashTable) -> Result<&mut ZendClassObject<PhpXhRequest>, String>`
  - [x] SubTask 6.3: 遍历 ZendHashTable，标量值转字符串，非标量抛异常（fail-fast）
  - [x] SubTask 6.4: 空数组 `query([])` 不抛异常，返回 `$this`
  - [x] SubTask 6.5: 调用 `self_.request = self_.request.clone().query(params_vec)`

## 阶段 3：accept() / contentType() 便捷方法

- [x] Task 7: 在 `rust/src/php_ext.rs` 实现 `accept()` 方法
  - [x] SubTask 7.1: 读取 `header()` 方法作为模板
  - [x] SubTask 7.2: 新增 `pub fn accept(self_, type: &str) -> Result<&mut ZendClassObject<PhpXhRequest>, String>`
  - [x] SubTask 7.3: 空字符串抛异常（fail-fast，与 `userAgent('')` 一致）
  - [x] SubTask 7.4: 内部调用 `self_.request = self_.request.clone().header("Accept", type)`
- [x] Task 8: 在 `rust/src/php_ext.rs` 实现 `contentType()` 方法
  - [x] SubTask 8.1: 新增 `pub fn content_type(self_, type: &str) -> Result<&mut ZendClassObject<PhpXhRequest>, String>`
  - [x] SubTask 8.2: 空字符串抛异常（fail-fast）
  - [x] SubTask 8.3: 内部调用 `self_.request = self_.request.clone().header("Content-Type", type)`

## 阶段 4：executeJson() JSON 自动解析方法

- [x] Task 9: 在 `rust/src/php_ext.rs` 实现 `executeJson()` 方法
  - [x] SubTask 9.1: 读取 `execute()` 方法作为模板（约 1697-1744 行）
  - [x] SubTask 9.2: 新增 `pub fn execute_json(&mut self) -> Result<Zval, String>`
  - [x] SubTask 9.3: 内部调用 `self.execute()` 获取结果数组
  - [x] SubTask 9.4: 若 `success=false` 抛异常（含 error 字段）
  - [x] SubTask 9.5: 检查 Content-Type header 是否含 `application/json`，否则抛异常
  - [x] SubTask 9.6: 对 body 执行 JSON 解析，失败抛异常（含 json_last_error_msg）
  - [x] SubTask 9.7: 返回解析后的 Zval（数组/标量/null）

## 阶段 5：文档更新

- [x] Task 10: 更新 README.md
  - [x] SubTask 10.1: 在 XHRequest 方法表新增 `query()` 行
  - [x] SubTask 10.2: 在 XHRequest 方法表新增 `accept()` 行
  - [x] SubTask 10.3: 在 XHRequest 方法表新增 `contentType()` 行
  - [x] SubTask 10.4: 在 XHRequest 方法表新增 `executeJson()` 行
  - [x] SubTask 10.5: 补充 query() 合并语义说明（已有参数保留，新参数追加）
  - [x] SubTask 10.6: 补充 executeJson() 失败/非 JSON 抛异常说明
- [x] Task 11: 升级版本号 1.3.0 → 1.4.0
  - [x] SubTask 11.1: `rust/Cargo.toml` version = "1.4.0"
  - [x] SubTask 11.2: `CHANGELOG.md` 新增 [1.4.0] 条目

## 阶段 6：测试

- [x] Task 12: 创建 `rust/tests/php_add_http_helpers_test.php`
  - [x] SubTask 12.1: query() 追加参数到无参数 URL（断言 /echo-query 回显含新参数）
  - [x] SubTask 12.2: query() 合并已有 URL 查询参数（断言已有+新参数都在）
  - [x] SubTask 12.3: query() 多次调用累加（断言两组参数都在）
  - [x] SubTask 12.4: query() 标量值转换（bool/null/int/float）
  - [x] SubTask 12.5: query([]) 空数组不抛异常
  - [x] SubTask 12.6: query() 嵌套数组抛异常（fail-fast）
  - [x] SubTask 12.7: accept() 设置 Accept header
  - [x] SubTask 12.8: accept('') 空字符串抛异常
  - [x] SubTask 12.9: contentType() 设置 Content-Type header
  - [x] SubTask 12.10: contentType('') 空字符串抛异常
  - [x] SubTask 12.11: executeJson() 成功解析 JSON 响应
  - [x] SubTask 12.12: executeJson() 非 JSON Content-Type 抛异常
  - [x] SubTask 12.13: executeJson() 请求失败抛异常
  - [x] SubTask 12.14: accept()/contentType() 多次调用覆盖

## 阶段 7：编译与全套件验证

- [x] Task 13: Rust 侧验证
  - [x] SubTask 13.1: `cargo fmt --check` 通过
  - [x] SubTask 13.2: `cargo clippy --all-targets --features php -- -D warnings` 通过
  - [x] SubTask 13.3: `cargo test --lib --features php` 98+ 用例通过
- [x] Task 14: 编译 release .so 并同步到 PHP 扩展目录
  - [x] SubTask 14.1: `cargo build --release --features php`
  - [x] SubTask 14.2: `cp target/release/libxhcurl.so` 到 PHP 扩展目录
  - [x] SubTask 14.3: `php -d extension=xhcurl -r 'echo XHCurl::version();'` 输出 `1.4.0`
- [x] Task 15: 运行新测试文件
  - [x] SubTask 15.1: 启动 socat(18400) + mock_server(18399)
  - [x] SubTask 15.2: `php -d extension=xhcurl rust/tests/php_add_http_helpers_test.php` 全通过
- [x] Task 16: 运行全套件 23+ 个 PHP 测试文件
  - [x] SubTask 16.1: 串行运行所有 `rust/tests/php_*.php`（含本轮新增 1 个，共 24 个）
  - [x] SubTask 16.2: 确认全部 PASS

# Task Dependencies

- Task 1, 2, 2b → Task 12（测试依赖新端点）
- Task 3, 4, 5 → Task 6（PHP 绑定依赖 Rust 实现）
- Task 7, 8 独立（accept/contentType 仅依赖 header()，已有）
- Task 9 依赖 execute() 已有实现
- Task 6, 7, 8, 9 → Task 10（文档依赖方法实现）
- Task 10 → Task 11（CHANGELOG 依赖文档与实现完成）
- Task 1-9 全部 → Task 12（测试依赖实现完成）
- Task 12 → Task 13, 14, 15, 16（编译验证依赖测试代码就绪）
- Task 1, 2, 2b（mock_server）与 Task 3-5（request.rs）可并行
- Task 7, 8（accept/contentType）与 Task 6（query）可并行
