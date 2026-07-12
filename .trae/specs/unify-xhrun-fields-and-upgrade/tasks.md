# Tasks

## Phase 1: xhrun 字段集统一（P2）

- [x] Task 1: xhrun 成功路径补字段（P2-1）
  - [x] SubTask 1.1: `rust/src/php_ext.rs` xhrun 成功路径插入 `error_type=""`、`error=""`、`command` 字段
  - [x] SubTask 1.2: 修正成功路径错误注释为"成功路径插入空 error_type/error，与 execute() 字段集一致"
  - [x] SubTask 1.3: 在新测试文件新增 `test_xhrun_success_has_error_type`、`test_xhrun_success_has_error`、`test_xhrun_success_has_command` 用例

## Phase 2: 文档同步（P2）

- [x] Task 2: README error_type 说明更新（P2-2）
  - [x] SubTask 2.1: `README.md` 中 `error_type` 字段说明从"成功路径不含此字段"改为"成功时为空字符串"
  - [x] SubTask 2.2: 更新 xhrun 字段表，补充成功路径字段集说明（`error`/`error_type`/`command` 三字段"始终存在"）

## Phase 3: 行为一致性（P3）

- [x] Task 3: fiber_each 空请求抛异常（P3-1）**BREAKING**
  - [x] SubTask 3.1: `rust/src/fiber.rs` `fiber_each` 中 `if total == 0 { return Ok(0) }` 改为 `return Err("XHCurl::each 没有待执行请求".to_string())`
  - [x] SubTask 3.2: 更新 `rust/tests/php_each_test.php` 中 `each(array(), ...)` 返回 0 的断言为期望抛异常
  - [x] SubTask 3.3: 在新测试文件新增 `test_fiber_each_empty_throws` 用例

- [x] Task 4: 删除 to_info_map 死代码（P3-2）
  - [x] SubTask 4.1: `rust/src/response.rs` 删除 `to_info_map` 方法及其测试 `test_to_info_map`
  - [x] SubTask 4.2: 确认无其他引用（grep 确认，仅自身测试引用）

- [x] Task 5: URL 空字符串校验（P3-3）
  - [x] SubTask 5.1: `rust/src/php_ext.rs` `create_request` 中增加 `if url.is_empty() { return Err(...) }`
  - [x] SubTask 5.2: 同样修改 `PhpXhRequest::__construct`（签名从 `-> Self` 改为 `-> Result<Self, String>`）
  - [x] SubTask 5.3: 在新测试文件新增 `test_create_request_empty_url_throws`、`test_xhrequest_construct_empty_url_throws` 用例

## Phase 4: 版本升级

- [x] Task 6: 版本升级 1.0.7 → 1.0.8
  - [x] SubTask 6.1: `rust/Cargo.toml` version = "1.0.7" → "1.0.8"
  - [x] SubTask 6.2: `CHANGELOG.md` 新增 `[1.0.8]` 条目，记录本轮所有变更
  - [x] SubTask 6.3: 检查 `rust/tests/php_invalid_proxy_test.php` 中 v1.0.7 注释（历史性注释，保留不动）

## Phase 5: 验证

- [x] Task 7: 完整验证流水线
  - [x] SubTask 7.1: `cd /workspace/rust && cargo fmt --check`
  - [x] SubTask 7.2: `cargo clippy --all-targets --features php -- -D warnings`
  - [x] SubTask 7.3: `cargo test --lib --features php`（98 个单元测试通过，减 1 个删除的 test_to_info_map）
  - [x] SubTask 7.4: `cargo build --release --features php` 并同步到 PHP 扩展目录
  - [x] SubTask 7.5: 启动 mock_server + socat，串行运行全部 PHP 测试文件（19 个文件 / 441 用例 / 0 失败）
  - [x] SubTask 7.6: 一次通过，无需修复

- [x] Task 8: 合理性评估报告
  - [x] SubTask 8.1: 在最终回复中给出每项 P2/P3 的"修复前/修复后/PHP 用户受益"评估
  - [x] SubTask 8.2: 标注破坏性变更清单（1 项：fiber_each 空请求抛异常）和迁移建议

# Task Dependencies
- Task 1（xhrun 字段）独立
- Task 2（README）独立，纯文档
- Task 3（fiber_each）独立
- Task 4（to_info_map 删除）独立
- Task 5（URL 校验）独立
- Task 6（版本升级）依赖所有前序任务
- Task 7（验证）依赖 Task 6
- Task 8（评估）依赖 Task 7

**建议并行分组**：
- Group A（php_ext.rs xhrun + create_request + fiber.rs）：Task 1 + Task 3 + Task 5（顺序执行）
- Group B（response.rs）：Task 4（独立）
- Group C（README + CHANGELOG + Cargo.toml）：Task 2 + Task 6（顺序执行）
- Group D（验证）：Task 7 + Task 8（最后）
