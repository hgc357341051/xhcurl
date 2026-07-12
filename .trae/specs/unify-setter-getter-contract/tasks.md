# Tasks

## Phase 1: P1 一致性核心修复

- [x] Task 1: 修复 `getMethod()` 在 customMethod 设置后返回误导值（P1-1）
  - [x] SubTask 1.1: `rust/src/php_ext.rs` `get_method` 修改：当 `self.request.get_custom_method()` 返回 Some 时返回该自定义方法名，否则返回 `self.request.get_method().to_string()`
  - [x] SubTask 1.2: `rust/src/php_ext.rs` 在 `impl PhpXhRequest` 新增 `pub fn get_custom_method(&self) -> Option<String>` 桥接 `self.request.get_custom_method().map(|s| s.to_string())`
  - [x] SubTask 1.3: 在 `rust/tests/php_unify_setter_getter_test.php`（新文件）新增 `test_get_method_after_custom_method`、`test_get_method_without_custom_method`、`test_get_custom_method` 用例

## Phase 2: P2 配套改进

- [x] Task 2: XHRequest 数值 setter 负值改为抛异常（P2-1）**BREAKING**
  - [x] SubTask 2.1: `rust/src/php_ext.rs` `timeout` setter 签名改为 `-> Result<&mut ZendClassObject<PhpXhRequest>, String>`，负值返回 `Err("timeout 不能为负值，0 = 使用全局默认".to_string())`
  - [x] Task 2.2: 同样修改 `timeoutMs`/`connectTimeout`/`connectTimeoutMs`/`maxRedirects`，错误信息含字段名与"负值"
  - [x] SubTask 2.3: 检查 `rust/tests/php_harden_edges_and_getters_test.php`、`php_each_test.php` 等现有测试中是否有传负值的用例，若有则更新断言为 expect throw
  - [x] SubTask 2.4: 在新测试文件新增 `test_timeout_negative_throws`、`test_timeoutMs_negative_throws`、`test_connect_timeout_negative_throws`、`test_max_redirects_negative_throws` 用例

- [x] Task 3: timeout(0) 语义差异文档化（P2-2）
  - [x] SubTask 3.1: `rust/src/php_ext.rs` `timeout` setter 文档注释补充 "0 = 使用全局默认"
  - [x] SubTask 3.2: `rust/src/php_ext.rs` XHMulti/XHThreadPool 的 `timeout` setter 文档注释补充 "0 = 无批量超时"
  - [x] SubTask 3.3: README 中 timeout 章节补充两类 timeout(0) 语义差异说明

- [x] Task 4: ASCII 校验前置到 setter（P2-3）**BREAKING**
  - [x] SubTask 4.1: `rust/src/request.rs` `user_agent`/`encoding`/`range`/`cookies` builder 方法中调用 `validate_ascii_header_value`（已有函数），setter 时校验而非延迟到 to_reqwest
  - [x] SubTask 4.2: 确认 `rust/src/php_ext.rs` 对应 PHP setter 透传 Err（Result 返回类型已支持）
  - [x] SubTask 4.3: 检查 `rust/tests/php_silent_setters_and_config_test.php` 中验证"延迟到 execute 报错"的用例，更新为"setter 时抛异常"
  - [x] SubTask 4.4: 在新测试文件新增 `test_user_agent_non_ascii_throws`、`test_encoding_non_ascii_throws`、`test_range_non_ascii_throws`、`test_cookies_non_ascii_throws` 用例

## Phase 3: P3 易用性改进

- [x] Task 5: null-clear 语义对齐（P3-1）
  - [x] SubTask 5.1: `rust/src/php_ext.rs` `basic_auth`/`bearer_token`/`user_agent`/`encoding`/`range`/`cookies` setter 签名改为接受 `Option<String>`，null 时调用对应的 clear 方法（需在 `rust/src/request.rs` 新增 `clear_user_agent`/`clear_encoding`/`clear_range`/`clear_cookies`/`clear_basic_auth`/`clear_bearer_token`，如不存在）
  - [x] SubTask 5.2: 在新测试文件新增 `test_basic_auth_null_clears`、`test_user_agent_null_clears`、`test_encoding_null_clears` 等用例

- [x] Task 6: 暴露 getBody getter（P3-2）
  - [x] SubTask 6.1: `rust/src/php_ext.rs` 新增 `pub fn get_body(&self) -> Option<String>` 桥接 `self.request.get_body().map(|s| s.to_string())`
  - [x] SubTask 6.2: 在新测试文件新增 `test_get_body` 用例

- [x] Task 7: 暴露 url() setter（P3-3）
  - [x] SubTask 7.1: `rust/src/php_ext.rs` 新增 `pub fn url(self_: &mut ZendClassObject<PhpXhRequest>, url: String) -> &mut ZendClassObject<PhpXhRequest>` 桥接 `self_.request.clone().url(url)`
  - [x] SubTask 7.2: 在新测试文件新增 `test_url_setter` 用例

## Phase 4: 验证

- [x] Task 8: 完整验证流水线
  - [x] SubTask 8.1: `cd /workspace/rust && cargo fmt --check`
  - [x] SubTask 8.2: `cargo clippy --all-targets --features php -- -D warnings`
  - [x] SubTask 8.3: `cargo test --lib --features php`（99 个单元测试通过）
  - [x] SubTask 8.4: `cargo build --release --features php` 并同步到 PHP 扩展目录
  - [x] SubTask 8.5: 启动 mock_server + socat，串行运行全部 PHP 测试文件（16 个文件 / 433 用例 / 0 失败）
  - [x] SubTask 8.6: 一次通过，无需修复

- [x] Task 9: 合理性评估报告
  - [x] SubTask 9.1: 在最终回复中按 P1-1 + P2-1/2/3 + P3-1/2/3 给出每项的"修复前/修复后/PHP 用户受益"评估
  - [x] SubTask 9.2: 标注破坏性变更清单（2 项：负值抛异常、ASCII 校验前置）和迁移建议

# Task Dependencies
- Task 1（getMethod 修复）独立
- Task 2（负值抛异常）改 timeout 等 setter，与 Task 1 同文件但不同方法，可并行
- Task 3（timeout(0) 文档）独立，纯文档
- Task 4（ASCII 校验前置）改 request.rs builder + php_ext.rs setter，与 Task 2 同文件需协调
- Task 5（null-clear）改 setter 签名为 Option，与 Task 2/4 同函数区域，建议顺序执行
- Task 6（getBody）独立，新增 getter
- Task 7（url setter）独立，新增 setter
- Task 8（验证）依赖所有前序任务
- Task 9（评估）依赖 Task 8

**建议并行分组**：
- Group A（php_ext.rs getter）：Task 1 + Task 6 + Task 7（顺序执行，同 impl 块）
- Group B（php_ext.rs setter 签名 + request.rs 校验）：Task 2 + Task 4 + Task 5（顺序执行，同函数区域）
- Group C（文档）：Task 3（独立）
- Group D（验证）：Task 8 + Task 9（最后）
