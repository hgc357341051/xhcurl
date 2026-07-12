# Tasks

## 阶段 1：全局 base_uri 与 base_headers 配置

- [x] Task 1: 在 `rust/src/curl.rs` 的 GlobalConfig 新增 base_uri 与 base_headers 字段
  - [x] SubTask 1.1: 读取 GlobalConfig 结构体定义（约 20-70 行）
  - [x] SubTask 1.2: 新增 `base_uri: Option<String>` 字段，初始化为 None
  - [x] SubTask 1.3: 新增 `base_headers: HashMap<String, String>` 字段，初始化为空 HashMap
  - [x] SubTask 1.4: 在 Default impl 中初始化默认值
- [x] Task 2: 在 `rust/src/curl.rs` 的 setConfig 校验与处理 base_uri/base_headers
  - [x] SubTask 2.1: 读取 setConfig 的校验逻辑（搜索 type_mismatches/negative_errors 等模式）
  - [x] SubTask 2.2: base_uri 校验：非字符串收集到 type_mismatches；空字符串允许（清除）；非法 URL 格式收集到错误
  - [x] SubTask 2.3: base_headers 校验：非数组收集到 type_mismatches；元素非标量收集到错误
  - [x] SubTask 2.4: 在应用阶段将 base_uri/base_headers 写入 GlobalConfig
  - [x] SubTask 2.5: 在配置指纹比对（proxy/verify_ssl 等变更触发 Client 重建）中包含 base_uri/base_headers
- [x] Task 3: 在 `rust/src/request.rs` 的 to_reqwest() 实现 base_uri URL 拼接与 base_headers 合并
  - [x] SubTask 3.1: 读取 to_reqwest() 中 URL 处理逻辑（约 800-850 行）
  - [x] SubTask 3.2: 在 URL 解析前检查：若 URL 以 `/` 开头且全局 base_uri 存在，拼接为完整 URL
  - [x] SubTask 3.3: base_uri 末尾斜杠处理（避免双斜杠）
  - [x] SubTask 3.4: 绝对 URL（http:// 或 https:// 开头）不拼接 base_uri
  - [x] SubTask 3.5: 在 header 构建阶段，先合并全局 base_headers，再合并请求级 headers（请求级覆盖）
- [x] Task 4: 在 `rust/src/php_ext.rs` 的 getConfig 输出 base_uri/base_headers
  - [x] SubTask 4.1: 读取 getConfig 方法（搜索 `pub fn get_config` 或类似）
  - [x] SubTask 4.2: 在返回的配置数组中新增 base_uri（string 或 null）与 base_headers（array）

## 阶段 2：withOptions 请求级批量设置方法

- [x] Task 5: 在 `rust/src/request.rs` 实现 with_options() 方法
  - [x] SubTask 5.1: 读取现有 setter 方法了解参数类型与签名
  - [x] SubTask 5.2: 新增 `pub fn with_options(mut self, options: ...) -> Result<Self, String>`，接受 options map
  - [x] SubTask 5.3: 遍历 options，按 key 分发到对应 setter（timeout/headers/query/accept 等）
  - [x] SubTask 5.4: 未知 key 返回 Err（fail-fast）
  - [x] SubTask 5.5: null 值跳过（不调用对应 setter）
- [x] Task 6: 在 `rust/src/php_ext.rs` 新增 withOptions() PHP 绑定方法
  - [x] SubTask 6.1: 读取 header() 或 query() 方法作为模板
  - [x] SubTask 6.2: 新增 `pub fn with_options(self_, options: &ZendHashTable) -> Result<&mut ZendClassObject<PhpXhRequest>, String>`
  - [x] SubTask 6.3: 遍历 ZendHashTable，按 key 分发到对应 PHP setter
  - [x] SubTask 6.4: 支持的 key 与对应 setter（参考 spec 中的支持选项表）
  - [x] SubTask 6.5: 未知 key 抛异常（含 key 名）
  - [x] SubTask 6.6: null 值跳过（Zval is_null 检查）
  - [x] SubTask 6.7: headers 数组中 header 值为 null 时跳过

## 阶段 3：扩展 mock_server 测试基础设施

- [x] Task 7: 在 `rust/tests/mock_server.php` 新增 `/base-test` 端点
  - [x] SubTask 7.1: 在路由分发逻辑中添加 `/base-test` 路径处理
  - [x] SubTask 7.2: 返回 200 + JSON `{"url": $_SERVER['REQUEST_URI'], "headers": getallheaders()}`（回显实际请求 URL 与 headers，便于断言 base_uri 拼接与 base_headers 合并）

## 阶段 4：文档更新

- [x] Task 8: 更新 README.md
  - [x] SubTask 8.1: 在 XHRequest 方法表新增 `withOptions(array $options): $this` 行
  - [x] SubTask 8.2: 在 setConfig 配置项表新增 `base_uri` 行
  - [x] SubTask 8.3: 在 setConfig 配置项表新增 `base_headers` 行
  - [x] SubTask 8.4: 新增"微服务场景示例"小节（base_uri + base_headers 用法）
  - [x] SubTask 8.5: 补充 withOptions 支持的选项 key 表
  - [x] SubTask 8.6: 补充 base_uri 拼接规则说明（相对 URL 拼接，绝对 URL 优先）
- [x] Task 9: 升级版本号 1.4.0 → 1.5.0
  - [x] SubTask 9.1: `rust/Cargo.toml` version = "1.5.0"
  - [x] SubTask 9.2: `CHANGELOG.md` 新增 [1.5.0] 条目

## 阶段 5：测试

- [x] Task 10: 创建 `rust/tests/php_add_withoptions_and_base_config_test.php`
  - [x] SubTask 10.1: withOptions() 批量设置多个选项（timeout/headers/query）
  - [x] SubTask 10.2: withOptions() 未知 key 抛异常
  - [x] SubTask 10.3: withOptions() null 值跳过
  - [x] SubTask 10.4: withOptions() 与链式 setter 混用（后调用覆盖）
  - [x] SubTask 10.5: withOptions() headers 数组中 null 值跳过
  - [x] SubTask 10.6: withOptions() 多次调用累加
  - [x] SubTask 10.7: base_uri 相对 URL 拼接
  - [x] SubTask 10.8: base_uri 绝对 URL 优先（不拼接）
  - [x] SubTask 10.9: base_uri 末尾斜杠处理
  - [x] SubTask 10.10: base_uri 为 null 时清除
  - [x] SubTask 10.11: base_headers 自动携带全局 header
  - [x] SubTask 10.12: base_headers 请求级覆盖全局
  - [x] SubTask 10.13: base_headers 为 null 时清除
  - [x] SubTask 10.14: base_uri + base_headers 组合使用

## 阶段 6：编译与全套件验证

- [x] Task 11: Rust 侧验证
  - [x] SubTask 11.1: `cargo fmt --check` 通过
  - [x] SubTask 11.2: `cargo clippy --all-targets --features php -- -D warnings` 通过
  - [x] SubTask 11.3: `cargo test --lib --features php` 98+ 用例通过
- [x] Task 12: 编译 release .so 并同步到 PHP 扩展目录
  - [x] SubTask 12.1: `cargo build --release --features php`
  - [x] SubTask 12.2: `cp target/release/libxhcurl.so` 到 PHP 扩展目录
  - [x] SubTask 12.3: `php -d extension=xhcurl -r 'echo XHCurl::version();'` 输出 `1.5.0`
- [x] Task 13: 运行新测试文件
  - [x] SubTask 13.1: 启动 socat(18400) + mock_server(18399)
  - [x] SubTask 13.2: `php -d extension=xhcurl rust/tests/php_add_withoptions_and_base_config_test.php` 全通过（14/14）
- [x] Task 14: 运行全套件 25 个 PHP 测试文件
  - [x] SubTask 14.1: 串行运行所有 `rust/tests/php_*.php`（含本轮新增 1 个，共 25 个）
  - [x] SubTask 14.2: 确认全部 PASS（EXIT_CODE=0）

# Task Dependencies

- Task 1, 2 → Task 3（to_reqwest 依赖 GlobalConfig 字段）
- Task 1, 2 → Task 4（getConfig 依赖字段定义）
- Task 5 → Task 6（PHP 绑定依赖 Rust 实现）
- Task 1, 2, 3（base 配置）与 Task 5, 6（withOptions）可并行
- Task 7（mock_server）独立，可与 Task 1-6 并行
- Task 1-7 → Task 8（文档依赖实现完成）
- Task 8 → Task 9（CHANGELOG 依赖文档与实现完成）
- Task 1-7 全部 → Task 10（测试依赖实现完成）
- Task 10 → Task 11, 12, 13, 14（编译验证依赖测试代码就绪）
