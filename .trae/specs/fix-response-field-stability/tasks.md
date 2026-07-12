# Tasks

## Phase 1: 响应数组字段稳定性

- [x] Task 1: fill_response_fields 中 remote_addr/version 稳定插入（P2-1）
  - [x] SubTask 1.1: `rust/src/php_ext.rs` `fill_response_fields` 中 `remote_addr` 的 `if let Some(addr) = ...` 改为无条件插入：`unwrap_or("")`
  - [x] SubTask 1.2: 同样修改 `version` 字段
  - [x] SubTask 1.3: 在新测试文件新增 `test_response_always_has_remote_addr`、`test_response_always_has_version` 用例

## Phase 2: xhrun 失败路径字段完整性

- [x] Task 2: failure_result 补 error_type 字段（P2-2）
  - [x] SubTask 2.1: `rust/src/php_ext.rs` `failure_result` 函数新增 `error_type` 参数，插入 `error_type` 字段
  - [x] SubTask 2.2: 三处调用点更新：白/黑名单传 `"denied"`、启动失败传 `"spawn_failed"`
  - [x] SubTask 2.3: 在新测试文件新增 `test_xhrun_denied_error_type`、`test_xhrun_spawn_failed_error_type` 用例

- [x] Task 3: xhrun exit_error 路径补 command/error 字段（P2-3）
  - [x] SubTask 3.1: `rust/src/php_ext.rs` xhrun 末尾 `else if exit_code != 0` 分支补充 `error` 和 `command` 字段
  - [x] SubTask 3.2: 在新测试文件新增 `test_xhrun_exit_error_has_command`、`test_xhrun_exit_error_has_error` 用例

## Phase 3: execute 失败路径耗时

- [x] Task 4: XHRequest::execute 失败路径记录真实 elapsed_ms（P2-4）
  - [x] SubTask 4.1: `rust/src/php_ext.rs` `execute` 方法入口记录 `let start = std::time::Instant::now();`
  - [x] SubTask 4.2: 失败分支 `Duration::from_secs(0)` 改为 `start.elapsed()`
  - [x] SubTask 4.3: 在新测试文件新增 `test_execute_failure_elapsed_ms_positive` 用例（用 socat /hang + timeoutMs(300) 触发超时失败）

## Phase 4: XHMulti clear 方法

- [x] Task 5: XHMulti 暴露 clear() 方法（P2-5）
  - [x] SubTask 5.1: `rust/src/php_ext.rs` `impl PhpXhMulti` 新增 `pub fn clear(&mut self)` 调用 `self.requests.clear()`
  - [x] SubTask 5.2: 在新测试文件新增 `test_multi_clear`、`test_multi_clear_then_isEmpty`、`test_multi_clear_reuse` 用例

## Phase 5: 错误措辞与文档

- [x] Task 6: xhrun 错误措辞统一 + body 文档修正（P3-1, P3-2）
  - [x] SubTask 6.1: `rust/src/php_ext.rs` xhrun 中 `"不能为负数"` 改为 `"不能为负值"`
  - [x] SubTask 6.2: `rust/src/php_ext.rs` execute() 文档注释 body 改为 `（二进制安全，可能非 UTF-8）`

## Phase 6: 验证

- [x] Task 7: 完整验证流水线
  - [x] SubTask 7.1: `cd /workspace/rust && cargo fmt --check` 通过
  - [x] SubTask 7.2: `cargo clippy --all-targets --features php -- -D warnings` 0 warning
  - [x] SubTask 7.3: `cargo test --lib --features php` 99 passed
  - [x] SubTask 7.4: `cargo build --release --features php` 并同步到 PHP 扩展目录
  - [x] SubTask 7.5: 启动 mock_server + socat，串行运行全部 PHP 测试文件（17 个文件 / 448 用例 / 0 失败）
  - [x] SubTask 7.6: 一次通过，仅更新 php_runtime_test.php 第 110 行断言适配 P3-1 措辞变更

- [x] Task 8: 合理性评估报告
  - [x] SubTask 8.1: 在最终回复中给出每项 P2/P3 的"修复前/修复后/PHP 用户受益"评估
  - [x] SubTask 8.2: 标注破坏性变更清单（无破坏性变更）和迁移建议
