# Tasks

## Phase 1: 资源安全（P1）

- [x] Task 1: 修复 XHThreadPool::executeEach take 顺序（P1-1）
  - [x] SubTask 1.1: `rust/src/php_ext.rs` `XHThreadPool::execute_each` 中将 `std::mem::take(&mut self.requests)` 移到回调校验（callback/on_chunk/on_headers）之后
  - [x] SubTask 1.2: 在新测试文件新增 `test_threadpool_execute_each_invalid_callback_preserves_requests` 用例

## Phase 2: 响应字段稳定性（P2）

- [x] Task 2: result_to_php_array 成功路径补 error_type（P2-1）
  - [x] SubTask 2.1: `rust/src/php_ext.rs` `result_to_php_array` 成功路径插入 `error_type` 空字符串
  - [x] SubTask 2.2: 修复 `response_to_php_array` 也补 `error_type`（单请求 execute 路径）
  - [x] SubTask 2.3: 在新测试文件新增 `test_response_success_has_error_type` 用例

- [x] Task 3: fill_response_fields error 无条件插入（P2-2）
  - [x] SubTask 3.1: `rust/src/php_ext.rs` `fill_response_fields` 中 `error` 改为无条件插入（None 时为空字符串）
  - [x] SubTask 3.2: 在新测试文件新增 `test_response_success_has_error_field` 用例

## Phase 3: setConfig 校验（P2-P3）

- [x] Task 4: setConfig 负值抛异常（P2-3）**BREAKING**
  - [x] SubTask 4.1: `rust/src/php_ext.rs` `set_config` 中 7 处负值收集到 `negative_errors` 向量，与 type_mismatches 一起返回 Err
  - [x] SubTask 4.2: 在新测试文件新增 `test_set_config_negative_throws`、`test_set_config_multiple_negatives`、`test_set_config_zero_ok` 用例

- [x] Task 5: setConfig 原子性应用（P3-1）
  - [x] SubTask 5.1: `rust/src/php_ext.rs` `set_config` 改为两阶段：先全量校验（类型 + 负值），全部通过后才统一应用
  - [x] SubTask 5.2: 在新测试文件新增 `test_set_config_type_error_no_partial_apply` 用例

## Phase 4: 验证

- [x] Task 6: 完整验证流水线
  - [x] SubTask 6.1: `cd /workspace/rust && cargo fmt --check` 通过
  - [x] SubTask 6.2: `cargo clippy --all-targets --features php -- -D warnings` 0 warning
  - [x] SubTask 6.3: `cargo test --lib --features php` 99 passed
  - [x] SubTask 6.4: `cargo build --release --features php` 并同步到 PHP 扩展目录
  - [x] SubTask 6.5: 启动 mock_server + socat，串行运行全部 PHP 测试文件（18 个文件 / 458 用例 / 0 失败）
  - [x] SubTask 6.6: 更新 php_each_test.php 中 test_set_config_negative_skipped 断言为 test_set_config_negative_throws

- [x] Task 7: 合理性评估报告
  - [x] SubTask 7.1: 在最终回复中给出每项 P1/P2/P3 的"修复前/修复后/PHP 用户受益"评估
  - [x] SubTask 7.2: 标注破坏性变更清单（1 项：setConfig 负值抛异常）和迁移建议
