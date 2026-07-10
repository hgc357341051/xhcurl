# Checklist

## P0: fiber_run 调度器 RAII 清理

- [x] fiber.rs 新增 SchedulerGuard 结构体 + impl Drop（调用 drop_scheduler）
- [x] fiber_run 用 RAII guard 替代手动 drop_scheduler
- [x] create_fiber 失败时调度器被清理（guard Drop）
- [x] start 失败时调度器被清理
- [x] take_php_exception 失败时调度器被清理
- [x] run_event_loop 返回 Err 时调度器被清理
- [x] run() 失败后再次调用 run() 不报"不支持嵌套调用"（新增测试验证）

## P1: fiber 路径 tokio 任务 abort

- [x] Scheduler 结构体新增 task_handles: Vec<JoinHandle<()>> 字段
- [x] fiber_await spawn 的 JoinHandle 存入 task_handles
- [x] fiber_gather spawn 的 JoinHandle 存入 task_handles
- [x] fiber_each spawn 的 JoinHandle 存入 task_handles
- [x] drop_scheduler 先 abort 所有 task_handles 再 drop Scheduler
- [x] 主 Fiber 异常退出后残留 tokio 任务被 abort（逻辑正确性）

## P2: create_fiber refcount 注释修正

- [x] set_object 注释修正为准确的 refcount 语义（inc_count + Drop dec_count，净 refcount=1）

## 验证

- [x] cargo fmt --check 通过
- [x] cargo clippy --all-targets --features php -- -D warnings 通过
- [x] cargo test --lib 全部通过（84 通过）
- [x] cargo test --test integration_test 全部通过（7 通过）
- [x] cargo test --test executor_async_test 全部通过（4 通过）
- [x] cargo build --release --features php 编译成功
- [x] php_each_test.php 全部通过（24 通过，含新增 run 失败恢复测试 3 项）
- [x] php_multi_each_test.php 全部通过（18 通过，回归）
- [x] php_threadpool_each_test.php 全部通过（16 通过，回归）
- [x] php_runtime_test.php 全部通过（36 通过，回归）
- [x] php_network_test.php 全部通过（42 通过，回归）
- [x] 新增 run 失败恢复测试通过（run 抛异常后再次 run 成功）
