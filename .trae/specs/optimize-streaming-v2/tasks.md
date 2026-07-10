# Tasks

- [x] Task 1: 修复 fiber_run 调度器泄漏（P0 RAII guard）
  - [x] SubTask 1.1: 在 fiber.rs 新增 `struct SchedulerGuard;` + `impl Drop for SchedulerGuard { fn drop(&mut self) { drop_scheduler(); } }`
  - [x] SubTask 1.2: 重构 fiber_run：init_scheduler 后构造 `_guard = SchedulerGuard;`，移除末尾手动 drop_scheduler（guard 离开作用域自动清理）
  - [x] SubTask 1.3: 确认 create_fiber/start/take_php_exception/run_event_loop 的所有 Err 路径都触发 guard Drop
  - [x] SubTask 1.4: 新增 PHP 测试：run() 抛异常后再次调用 run() 应成功（验证不再误报"不支持嵌套调用"）
  - [x] SubTask 1.5: cargo build --features php 编译通过

- [x] Task 2: 修复 fiber 路径 tokio 任务泄漏（P1 保存 + abort JoinHandle）
  - [x] SubTask 2.1: Scheduler 结构体新增 `task_handles: Vec<tokio::task::JoinHandle<()>>` 字段，new() 初始化为空 Vec
  - [x] SubTask 2.2: fiber_await 的 runtime.spawn(...) 返回值存入 task_handles（通过 SCHEDULER.with 取 Scheduler.task_handles）
  - [x] SubTask 2.3: fiber_gather 的 spawn 返回值存入 task_handles
  - [x] SubTask 2.4: fiber_each 的 spawn 返回值存入 task_handles
  - [x] SubTask 2.5: drop_scheduler 中先 `for h in task_handles.drain(..) { h.abort(); }` 再 drop Scheduler
  - [x] SubTask 2.6: cargo build --features php 编译通过

- [x] Task 3: 修正 create_fiber refcount 注释（P2）
  - [x] SubTask 3.1: 修正 create_fiber 中 set_object 的注释，说明 inc_count + obj Drop dec_count 净 refcount=1 的语义
  - [x] SubTask 3.2: cargo build --features php 编译通过

- [x] Task 4: 全量验证
  - [x] SubTask 4.1: cargo fmt --check + cargo clippy --all-targets --features php -- -D warnings
  - [x] SubTask 4.2: cargo test --lib + cargo test --test integration_test + cargo test --test executor_async_test
  - [x] SubTask 4.3: cargo build --release --features php
  - [x] SubTask 4.4: PHP 运行时测试（php_each_test.php + php_multi_each_test.php + php_threadpool_each_test.php + php_runtime_test.php + php_network_test.php + 新增 run 失败恢复测试）

# Task Dependencies

- Task 1（RAII guard）和 Task 2（task_handles）都改 fiber.rs，存在修改重叠，建议串行：先 Task 1 再 Task 2
- Task 3（注释）独立，可与 Task 2 并行或之后
- Task 4 依赖所有前序任务
