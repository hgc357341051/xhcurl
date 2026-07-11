# Tasks

- [x] Task 1: 修复 `XHThreadPool::execute_each` 回调异常时不中止剩余任务
  - [x] SubTask 1.1: 在 `rust/src/php_ext.rs` 的 `PhpXhThreadPool::execute_each` 中，将回调异常分支 `(pool, Err(msg))` 改为 `(None, Err(msg))`，让 pool 在 `block_on` 闭包结束时被 drop（`Drop` 实现 abort dispatcher + workers）
  - [x] SubTask 1.2: 确认 `self.pool = None` 后，下次调用 `execute_each`/`execute` 时 `pool.is_none()` 分支重建 pool
  - [x] SubTask 1.3: 确认正常路径（无回调异常）仍存回 pool 复用，行为不变
  - [x] SubTask 1.4: 添加代码注释说明"回调异常不存回 pool，与协程 each / XhMulti::execute_each 行为一致"
  - [x] SubTask 1.5: 编译验证 `cargo build --features php`

- [x] Task 2: 新增测试验证行为一致性
  - [x] SubTask 2.1: 在 `rust/tests/php_streaming_test.php` 或新测试文件中，验证 XHThreadPool 回调异常后剩余请求不触发回调（提交 N 个请求，第 1 个回调抛异常，断言回调触发次数 < N）
  - [x] SubTask 2.2: 验证协程 `each` 回调异常后剩余请求不触发回调（对比一致性）
  - [x] SubTask 2.3: 验证 `XHMulti::executeEach` 回调异常后剩余请求不触发回调（对比一致性）
  - [x] SubTask 2.4: 验证 XHThreadPool 回调异常后再次调用 `executeEach` 能正常工作（pool 重建）

- [x] Task 3: README 文档澄清
  - [x] SubTask 3.1: 新增"请求级流式回调行为契约"小节，明确三种模式（`each` / `executeEach`）都支持请求级流式回调
  - [x] SubTask 3.2: 列出统一行为契约：每完成一个就回调、成功/失败都触发、不累积、回调异常中止剩余任务
  - [x] SubTask 3.3: 补充三者对比表（调用方式、并发模型、SAPI 限制、是否支持 onChunk/onHeaders）

- [x] Task 4: 验证
  - [x] SubTask 4.1: `cargo fmt --check`
  - [x] SubTask 4.2: `cargo clippy --features php -- -D warnings`
  - [x] SubTask 4.3: `cargo test --lib --features php`
  - [x] SubTask 4.4: `cargo build --features php` 编译成功
  - [x] SubTask 4.5: 启动 mock 服务器，运行全部 `php_*.php` 测试通过

# Task Dependencies
- Task 1（修复）先于 Task 2（测试依赖修复）
- Task 3（文档）独立，可与 Task 1/2 并行
- Task 4（验证）依赖所有前序任务完成
