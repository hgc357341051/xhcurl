# Tasks

- [x] Task 1: 提取回调调用辅助函数（P1 消除重复）
  - [x] SubTask 1.1: 在 php_ext.rs 新增私有辅助函数 `invoke_streaming_callback(callback: &ZendCallable, result_array: &ZBox<ZendHashTable>) -> Result<(), String>`，封装 try_call + map_err（匹配 Error::Exception 提取 message、其他错误格式化）
  - [x] SubTask 1.2: XhMulti::execute_each 超时分支的回调调用替换为 invoke_streaming_callback
  - [x] SubTask 1.3: XhMulti::execute_each 无超时分支的回调调用替换为 invoke_streaming_callback
  - [x] SubTask 1.4: XHThreadPool::execute_each 的回调调用替换为 invoke_streaming_callback
  - [x] SubTask 1.5: cargo build --features php 编译通过

- [x] Task 2: 修复 XhMulti::executeEach 回调异常任务泄漏（P2）
  - [x] SubTask 2.1: 超时分支：回调异常 `?` 返回前，先 `for handle in tasks.drain(..) { handle.abort(); }` 中止剩余任务
  - [x] SubTask 2.2: 无超时分支：回调异常 `?` 返回前，同样 abort 剩余任务
  - [x] SubTask 2.3: cargo build --features php 编译通过

- [x] Task 3: 补全 XHThreadPool::executeEach 完整性检查（P3）
  - [x] SubTask 3.1: 收集循环退出后，检查 `count < submitted`（用 as usize），不等时返回 Err（与 execute_all 一致）
  - [x] SubTask 3.2: cargo build --features php 编译通过

- [x] Task 4: 补充测试覆盖（P4）
  - [x] SubTask 4.1: php_multi_each_test.php 新增回调抛异常终止测试（10 请求，第 2 个回调抛异常，验证返回 Err + 回调次数 < 10）
  - [x] SubTask 4.2: php_multi_each_test.php 新增超时测试（短超时 + hanging server on 18400，验证返回超时 Err）
  - [x] SubTask 4.3: php_threadpool_each_test.php 新增回调抛异常终止测试（10 请求，第 2 个回调抛异常，验证返回 Err + 回调次数 < 10）

- [x] Task 5: 全量验证
  - [x] SubTask 5.1: cargo fmt --check + cargo clippy --all-targets --features php -- -D warnings
  - [x] SubTask 5.2: cargo test --lib + cargo test --test integration_test + cargo test --test executor_async_test
  - [x] SubTask 5.3: cargo build --release --features php
  - [x] SubTask 5.4: PHP 运行时测试（php_each_test.php + php_multi_each_test.php + php_threadpool_each_test.php 全部通过，含新增测试）

# Task Dependencies

- Task 1（辅助函数）独立，最先做
- Task 2（任务 abort）和 Task 3（完整性检查）互相独立，可并行，但都依赖 Task 1 完成后才能替换调用点（避免冲突）
- Task 4（测试）依赖 Task 1-3 完成
- Task 5（全量验证）依赖所有前序任务
