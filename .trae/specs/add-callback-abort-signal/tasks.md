# Tasks

- [x] Task 1: 扩展 `invoke_streaming_callback` 支持返回值控制
  - [x] SubTask 1.1: 修改 `rust/src/php_ext.rs` 的 `invoke_streaming_callback` 函数签名，返回值从 `Result<(), String>` 改为 `Result<bool, String>`
  - [x] SubTask 1.2: 调用 `callback.try_call(...)` 后获取返回的 `Zval`，检查是否严格 `=== false`（用 `zval.is_false()` 判定），是则返回 `Ok(false)`，否则返回 `Ok(true)`
  - [x] SubTask 1.3: 异常分支保持 `Err(extract_callback_error(e))` 不变
  - [x] SubTask 1.4: 编译验证 `cargo build --features php`

- [x] Task 2: 修改 `PhpXhMulti::execute_each` 处理 `Ok(false)` 中止
  - [x] SubTask 2.1: 在 4 处 `invoke_streaming_callback` 调用点（无超时分支 + 超时分支 + streaming 分支），将 `Ok(false)` 视为"使用者请求中止"：退出循环，调用 `multi.abort_tasks()` 中止剩余任务，返回 `Ok(count)`（已处理数，**不视为错误**）
  - [x] SubTask 2.2: 区分 `Ok(false)`（正常中止，返回 Ok）和 `Err(msg)`（异常中止，返回 Err）
  - [x] SubTask 2.3: 编译验证

- [x] Task 3: 修改 `PhpXhThreadPool::execute_each` 处理 `Ok(false)` 中止
  - [x] SubTask 3.1: 在 2 处 `invoke_streaming_callback` 调用点（streaming 分支 + 非 streaming 分支），将 `Ok(false)` 视为"使用者请求中止"：退出循环，返回 `(None, Ok(count))`（不存回 pool，drop 触发 abort，与异常中止一致的清理）返回 `Ok(count)`
  - [x] SubTask 3.2: 区分 `Ok(false)`（正常中止，返回 Ok）和 `Err(msg)`（异常中止，返回 Err）
  - [x] SubTask 3.3: 编译验证

- [x] Task 4: 修改 `fiber_each` 支持返回值控制
  - [x] SubTask 4.1: 在 `rust/src/fiber.rs` 的 `fiber_each` 中，`callback_callable.try_call(...)` 后获取返回的 `Zval`，检查是否 `is_false()`
  - [x] SubTask 4.2: 若 `is_false()`，提前退出循环（不再 suspend 剩余），返回 `Ok(count as i64)`（已处理数，不视为错误）
  - [x] SubTask 4.3: 异常分支保持不变（`?` 传播）
  - [x] SubTask 4.4: 编译验证

- [x] Task 5: 新增测试验证返回值控制
  - [x] SubTask 5.1: 在 `rust/tests/php_callback_abort_test.php` 或新文件中，验证 XHMulti 回调返回 `false` 后剩余请求不触发回调，方法返回已处理数（非错误）
  - [x] SubTask 5.2: 验证 XHThreadPool 回调返回 `false` 后剩余请求不触发回调，方法返回已处理数
  - [x] SubTask 5.3: 验证协程 `each` 回调返回 `false` 后剩余请求不触发回调
  - [x] SubTask 5.4: 验证回调返回 `true`/`null`/`void`/`0`/`''` 时继续处理（向后兼容 + 弱类型陷阱避免）
  - [x] SubTask 5.5: 验证回调抛异常仍中止（回归，已有测试覆盖）

- [x] Task 6: README 文档补充
  - [x] SubTask 6.1: 新增"回调返回值控制中止"小节，说明 `$onResult` 返回 `false` 中止、其他值继续、抛异常仍中止
  - [x] SubTask 6.2: 补充使用示例（如"遇到 success=false 时返回 false 中止"业务场景）
  - [x] SubTask 6.3: 在三者对比表中补充"返回 false 中止"行

- [x] Task 7: 验证
  - [x] SubTask 7.1: `cargo fmt --check`
  - [x] SubTask 7.2: `cargo clippy --features php -- -D warnings`
  - [x] SubTask 7.3: `cargo test --lib --features php`
  - [x] SubTask 7.4: `cargo build --features php` 编译成功
  - [x] SubTask 7.5: 启动 mock 服务器，运行全部 `php_*.php` 测试通过（含新增返回值控制测试 + 回归测试）

# Task Dependencies
- Task 1（辅助函数）先于 Task 2/3（调用方修改）
- Task 4（fiber_each）独立，可与 Task 1-3 并行
- Task 5（测试）依赖 Task 1-4 完成
- Task 6（文档）独立，可与 Task 1-5 并行
- Task 7（验证）依赖所有前序任务完成
