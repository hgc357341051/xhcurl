# Tasks

- [x] Task 1: set_config 收集并反馈类型不匹配项（php_ext.rs）
  - [x] SubTask 1.1: 在 set_config 的 modify_config 闭包中，为 12 个配置项（6 数值/4 布尔/2 字符串）增加类型不匹配收集逻辑
  - [x] SubTask 1.2: modify_config 后检查收集列表，非空时返回 Err 列出所有不匹配项
  - [x] SubTask 1.3: cargo build --features php 编译通过

- [x] Task 2: run_event_loop 回收已完成任务句柄（fiber.rs）
  - [x] SubTask 2.1: 在 run_event_loop 的 Ok(msg) 分支、resume + take_php_exception 之后，调用 task_handles.retain(|h| !h.is_finished()) 回收已完成句柄
  - [x] SubTask 2.2: cargo build --features php 编译通过

- [x] Task 3: 新增测试
  - [x] SubTask 3.1: 新增 PHP 测试：setConfig 传字符串给数值配置项返回错误
  - [x] SubTask 3.2: 新增 PHP 测试：setConfig 正确类型正常应用（回归）

- [x] Task 4: 全量验证
  - [x] SubTask 4.1: cargo fmt --check + cargo clippy -- -D warnings（非 php）+ cargo clippy --all-targets --features php -- -D warnings
  - [x] SubTask 4.2: cargo test --lib（84 passed）+ cargo test --test integration_test（7 passed）+ cargo test --test executor_async_test（4 passed）
  - [x] SubTask 4.3: cargo build --release --features php 编译成功
  - [x] SubTask 4.4: PHP 运行时测试全量回归（each 39 + multi_each 18 + threadpool_each 16 + runtime 36 + network 42，共 151 项通过）

# Task Dependencies

- Task 1（php_ext.rs）和 Task 2（fiber.rs）互相独立，可并行
- Task 3 依赖 Task 1
- Task 4 依赖所有前序任务
