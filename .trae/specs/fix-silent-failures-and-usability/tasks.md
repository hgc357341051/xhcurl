# Tasks

- [x] Task 1: 修复 `json()` 静默失败
  - [x] SubTask 1.1: 在 `rust/src/php_ext.rs` 的 `json()` 方法中，`php_array_to_json` 失败时抛 PHP 异常（返回 `Result` 或用 `throw` 等价机制）
  - [x] SubTask 1.2: `body_json_str` 失败时同样抛异常
  - [x] SubTask 1.3: 异常 message 含"JSON 序列化失败"和原始错误原因

- [x] Task 2: 修复 `setUserData()` / `userData()` 静默失败
  - [x] SubTask 2.1: 在 `set_user_data()` 和 `user_data()` 中，`php_array_to_json` 失败时抛异常
  - [x] SubTask 2.2: 异常 message 含"userData 序列化失败"

- [x] Task 3: 修复 `multipart()` 单字段错误处理
  - [x] SubTask 3.1: 在 `rust/src/php_ext.rs` 的 `multipart()` 中，将 `None => return self_` 改为 `None => continue`（跳过该字段继续处理）
  - [x] SubTask 3.2: 确保至少一个有效字段时正常设置 multipart body

- [x] Task 4: 修复 `method()` 无效方法名静默跳过
  - [x] SubTask 4.1: 在 `rust/src/php_ext.rs` 的 `method()` 中，`HttpMethod::from_str` 失败时抛异常
  - [x] SubTask 4.2: 异常 message 含"无效的 HTTP 方法"和提示使用 `customMethod()`

- [x] Task 5: `proxy()` 接受 `?string` 参数
  - [x] SubTask 5.1: 修改 `proxy()` 签名为 `proxy(?string $proxy)`，传 null 时清除请求级代理覆盖
  - [x] SubTask 5.2: 确认 `XhRequest` 的 proxy 字段是 `Option<String>`，设为 None 即清除

- [x] Task 6: 新增 `timeoutMs(int $ms)` 方法
  - [x] SubTask 6.1: 在 `PhpXhRequest` impl 块新增 `#[php(name = "timeoutMs")]` 方法
  - [x] SubTask 6.2: 将毫秒转为秒（`ms / 1000`，注意 reqwest 超时精度，若 reqwest 支持 Duration 可直接用）
  - [x] SubTask 6.3: 确认 reqwest 的 timeout 是否支持亚秒精度（reqwest 用 Duration，支持毫秒）

- [x] Task 7: `cookies()` 接受数组
  - [x] SubTask 7.1: 修改 `cookies()` 签名接受 `&Zval`，判断是字符串还是数组
  - [x] SubTask 7.2: 若为数组，遍历键值对拼接为 `"key=value; key2=value2"` 格式
  - [x] SubTask 7.3: 若为字符串，保持原有行为（向后兼容）

- [x] Task 8: 结果数组新增 `error_type` 字段
  - [x] SubTask 8.1: 在 `result_to_php_array` 的失败路径，根据错误字符串分类错误类型（dns/timeout/ssl/connection/http）
  - [x] SubTask 8.2: 在失败结果数组中插入 `error_type` 字段
  - [x] SubTask 8.3: 成功路径不含 `error_type`（或为空字符串）
  - [x] SubTask 8.4: 错误分类逻辑：检查 reqwest 错误字符串关键词（如 "dns"/"resolve" → dns，"timeout"/"timed out" → timeout，"ssl"/"tls"/"certificate" → ssl，"connection refused"/"connect" → connection）

- [x] Task 9: 新增测试
  - [x] SubTask 9.1: 测试 `json()` 序列化失败抛异常（传含资源的数组）
  - [x] SubTask 9.2: 测试 `method()` 无效方法名抛异常
  - [x] SubTask 9.3: 测试 `multipart()` 单字段错误跳过该字段
  - [x] SubTask 9.4: 测试 `proxy(null)` 清除代理
  - [x] SubTask 9.5: 测试 `timeoutMs(500)` 毫秒级超时
  - [x] SubTask 9.6: 测试 `cookies(['k'=>'v'])` 数组形式
  - [x] SubTask 9.7: 测试 `error_type` 字段（dns 失败、超时、成功路径）

- [x] Task 10: README 文档更新
  - [x] SubTask 10.1: 更新错误处理说明：明确区分"配置类错误抛异常"vs"请求级失败返回 success=false 数组"
  - [x] SubTask 10.2: 补充 `proxy(null)` 清除代理说明
  - [x] SubTask 10.3: 补充 `timeoutMs()` 方法说明
  - [x] SubTask 10.4: 补充 `cookies()` 数组形式说明
  - [x] SubTask 10.5: 补充 `error_type` 字段说明
  - [x] SubTask 10.6: 补充 `method()` 抛异常说明 + `customMethod()` 用途

- [x] Task 11: 验证
  - [x] SubTask 11.1: `cargo fmt --check`
  - [x] SubTask 11.2: `cargo clippy --features php -- -D warnings`
  - [x] SubTask 11.3: `cargo test --lib --features php`（95 passed）
  - [x] SubTask 11.4: `cargo build --features php` 编译成功
  - [x] SubTask 11.5: 启动 mock 服务器，运行全部 `php_*.php` 测试通过（10 文件 223 用例全通过）

# Task Dependencies
- Task 1-4（静默失败修复）独立，可并行
- Task 5-7（易用性优化）独立，可并行
- Task 8（error_type）独立
- Task 9（测试）依赖 Task 1-8 完成
- Task 10（文档）独立，可与 Task 1-8 并行
- Task 11（验证）依赖所有前序任务完成
