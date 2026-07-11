# Tasks

## Phase 1: P1 一致性核心修复

- [x] Task 1: cookies() 数组整数键改为抛异常（P1-1）
  - [x] SubTask 1.1: `rust/src/php_ext.rs` 中 `cookies()` 数组分支 `if key.is_long() { return Ok(()); }` 改为 `return Err("cookies() 不支持列表数组（整数键），请使用关联数组 ['name' => 'value'] 形式".to_string())`
  - [x] SubTask 1.2: 更新 `rust/tests/php_harden_edges_and_getters_test.php` 等现有测试中受影响的 cookies 用例（如有列表数组形式调用需改抛异常断言）
  - [x] SubTask 1.3: 在 `rust/tests/php_align_php_user_api_test.php`（新文件）新增 `test_cookies_integer_keys_throw` 用例验证抛异常

- [x] Task 2: maxConcurrency(0) 语义文档对齐实现（P1-2）
  - [x] SubTask 2.1: `rust/src/php_ext.rs` `max_concurrency` setter 文档字符串注释从"0 = 无限制"改为"0 = 使用默认（CPU 核心数）"
  - [x] SubTask 2.2: 检查 README 是否有"0 = 无限制"措辞，若有则修正为"0 = 使用默认（CPU 核心数）"
  - [x] SubTask 2.3: 验证实现不变（max_concurrency=0 时 worker_count 走 ThreadPoolConfig::default()）

- [x] Task 3: XHThreadPool 构造函数负值改为抛异常（P1-3）
  - [x] SubTask 3.1: `rust/src/php_ext.rs` `__construct` 中 `match workers { Some(n) if n >= 0 => n as usize, _ => 0 }` 改为：负值返回 `Err`（需把 `__construct` 签名改为 `-> Result<Self, String>`，ext-php-rs 把 Err 转 PHP 异常）
  - [x] SubTask 3.2: 错误信息 `"XHThreadPool workers 不能为负值，0 = 使用默认（CPU 核心数）"`
  - [x] SubTask 3.3: 更新 `rust/tests/php_each_test.php` 的 `test_threadpool_negative_workers_clamped` → `test_threadpool_negative_workers_throws`，断言 `new XHThreadPool(-4)` 抛异常
  - [x] SubTask 3.4: 在新测试文件新增 `test_threadpool_construct_zero_ok`、`test_threadpool_construct_negative_throws` 用例

- [x] Task 4: 补全 6 个缺失 getter（P1-4）
  - [x] SubTask 4.1: `rust/src/php_ext.rs` `PhpXhRequest impl` 新增 `get_basic_auth(&self) -> Option<String>` 返回 `self.request.auth`（先在 `rust/src/request.rs` 暴露 `auth` 字段为 pub 或加 `auth()` getter 方法）
  - [x] SubTask 4.2: 同样新增 `get_bearer_token`、`get_follow_redirects`、`get_max_redirects`（u32 → i64 转换）、`get_encoding`、`get_range`
  - [x] SubTask 4.3: 在新测试文件新增 `test_get_basic_auth`、`test_get_bearer_token`、`test_get_follow_redirects`、`test_get_max_redirects`、`test_get_encoding`、`test_get_range` 6 个用例（每个用例含"设置后能取回"和"未设置返回 null"两个断言）

- [x] Task 5: xhrun 失败路径补 error_type 枚举（P1-5）
  - [x] SubTask 5.1: `rust/src/php_ext.rs` `xhrun` 函数返回数组构建处（约 3314 行），在 `timed_out` 分支插入 `error_type => "timeout"`，在 `truncated` 分支插入 `error_type => "output_too_large"`
  - [x] SubTask 5.2: 在 `success=false` 但非超时/截断的分支（exit_code != 0）插入 `error_type => "exit_error"`
  - [x] SubTask 5.3: 成功路径不插入 `error_type`
  - [x] SubTask 5.4: 在新测试文件新增 `test_xhrun_timeout_error_type`、`test_xhrun_exit_error_error_type`、`test_xhrun_success_no_error_type` 3 个用例
  - [x] SubTask 5.5: README xhrun 返回字段表补充 `error_type` 行

## Phase 2: P2 配套改进（同批落地）

- [x] Task 6: bearerToken 空值校验（P2-1）
  - [x] SubTask 6.1: `rust/src/php_ext.rs` `bearer_token` 签名改为 `-> Result<&mut ZendClassObject<PhpXhRequest>, String>`，空字符串返回 `Err("bearerToken 不能为空字符串".to_string())`
  - [x] SubTask 6.2: 在新测试文件新增 `test_bearer_token_empty_throws` 用例

- [x] Task 7: xhrun 超时清理进程组（P2-2）
  - [x] SubTask 7.1: `rust/src/php_ext.rs` xhrun 的 `Command::new(...)` 在 Unix 平台调用 `.process_group(0)`（如 std::process::Command 支持；如不支持，使用 `pre_exec` + `libc::setpgid(0, 0)`）
  - [x] SubTask 7.2: 超时 kill 时改为 `killpg(pgid, SIGKILL)`（取 `child.id() as i32` 作为 pgid，因 process_group(0) 使子进程 PID = PGID）
  - [x] SubTask 7.3: 在新测试文件新增 `test_xhrun_shell_timeout_kills_grandchildren` 用例：`xhrun('sleep 60 & sleep 120', ['shell'=>true,'timeout'=>1])`，验证 1 秒后无残留 sleep 进程（用 `pgrep` 或 `ps` 检查）
  - [x] SubTask 7.4: Windows 平台保持原 `child.kill()` 行为，README 注明 Unix 专有增强

- [x] Task 8: executeEach 空请求与 execute 行为对齐（P2-3）
  - [x] SubTask 8.1: `rust/src/php_ext.rs` `XHMulti::execute_each` 中 `if self.requests.is_empty() { return Ok(0); }` 改为 `return Err("XHMulti 没有待执行请求，请先调用 add() 添加请求".to_string())`
  - [x] SubTask 8.2: 同样修改 `XHThreadPool::execute_each`
  - [x] SubTask 8.3: 在新测试文件新增 `test_multi_execute_each_empty_throws`、`test_threadpool_execute_each_empty_throws` 用例

- [x] Task 9: classify_error_type 优先级顺序注释 + 单元测试（P2-4）
  - [x] SubTask 9.1: `rust/src/php_ext.rs` `classify_error_type` 函数补充文档注释说明优先级顺序与顺序敏感性
  - [x] SubTask 9.2: 在 `rust/src/php_ext.rs` 末尾或 `#[cfg(test)]` 模块新增 4 个单元测试：`test_classify_dns_priority_over_timeout`、`test_classify_ssl_priority_over_connection`、`test_classify_unknown_fallback`、`test_classify_connection_keywords`

- [x] Task 10: format_request_error_message 错误分类完善（P2-5）
  - [x] SubTask 10.1: sub-agent 实施时检查 `format_request_error_message` 实现，若仅特判 timeout 而忽略其他 error_type，改为先 `classify_error_type` 再格式化分支
  - [x] SubTask 10.2: 确保改动不破坏现有 timeout 错误消息（含 "timed out" 关键词）

- [x] Task 11: README 文档修正（P2-6 + P2-7）
  - [x] SubTask 11.1: README:592 "成功路径不含此字段（或为空字符串）" 修正为 "成功路径不含此字段"
  - [x] SubTask 11.2: README:597 "（如 HTTP 0xx）" 删除（HTTP 状态码规范最小为 100，不存在 0xx）

## Phase 3: 验证与合理性评估

- [x] Task 12: 完整验证流水线
  - [x] SubTask 12.1: `cd /workspace/rust && cargo fmt --check`
  - [x] SubTask 12.2: `cargo clippy --all-targets --features php -- -D warnings`
  - [x] SubTask 12.3: `cargo test --lib --features php`（99 个单元测试通过）
  - [x] SubTask 12.4: 编译扩展 `cargo build --release --features php`
  - [x] SubTask 12.5: 启动 mock_server + socat，串行运行全部 15 个 PHP 测试文件（含新增 `php_align_php_user_api_test.php`），统计 pass/fail
  - [x] SubTask 12.6: 失败用例逐一修复直至全通过（396/396 通过；根因是 PHP 扩展目录中 xhcurl.so 旧版本未同步，复制新编译产物后全部通过）

- [x] Task 13: 合理性评估报告
  - [x] SubTask 13.1: 在最终回复中按 P1-1 ~ P1-5 + P2-1/2/3/4/5/6/7 给出每项的"修复前/修复后/PHP 用户受益"评估
  - [x] SubTask 13.2: 标注破坏性变更清单（4 项）和迁移建议（PHP 用户应如何调整代码）

# Task Dependencies

- Task 1（cookies 整数键）独立，可并行
- Task 2（maxConcurrency 文档）独立，可并行
- Task 3（构造函数负值）独立，但需更新 `php_each_test.php`，与 Task 4（getter）改 `php_ext.rs` 同文件不冲突可并行
- Task 4（getter）改 `php_ext.rs`，与 Task 1/3 同文件有冲突风险，建议顺序执行或 sub-agent 内协调
- Task 5（xhrun error_type）改 `php_ext.rs` xhrun 函数，与 Task 1/3/4 不同函数，可并行
- Task 6（bearerToken）改 `php_ext.rs` bearer_token setter，与 Task 4（getter）同函数区域，建议顺序执行
- Task 7（xhrun 进程组）改 `php_ext.rs` xhrun 的 Command 构建与 kill 逻辑，与 Task 5（xhrun error_type）同函数，建议合并到一个 sub-agent 顺序执行
- Task 8（executeEach 空请求）改 `php_ext.rs` 两个 execute_each，独立
- Task 9（classify_error_type）改 `php_ext.rs` 注释 + 新增单测，独立
- Task 10（format_request_error_message）改 `php_ext.rs` 错误格式化，独立
- Task 11（README 文档）纯 README，完全独立
- Task 12（验证）依赖所有前序任务完成
- Task 13（评估）依赖 Task 12 通过

**建议并行分组**：
- Group A（php_ext.rs cookies + getter）：Task 1 + Task 4（顺序执行）
- Group B（php_ext.rs XHThreadPool）：Task 2 + Task 3 + Task 8（顺序执行）
- Group C（php_ext.rs xhrun）：Task 5 + Task 7 + Task 6（顺序执行）
- Group D（php_ext.rs 错误分类）：Task 9 + Task 10（顺序执行）
- Group E（README）：Task 11（独立）
- Group F（验证）：Task 12 + Task 13（最后）
