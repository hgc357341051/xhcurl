# Tasks

- [x] Task 1: extract_requests 提前检查批量上限（P1 防 OOM）
  - [x] SubTask 1.1: 在 php_ext.rs 的 extract_requests 函数开头（克隆循环之前）添加 `if requests.len() > MAX_REQUESTS_PER_BATCH` 检查，返回错误；移除循环后的重复检查
  - [x] SubTask 1.2: cargo build --features php 编译通过

- [x] Task 2: fiber_await 先校验 Fiber 上下文再 spawn（P2 防孤立任务）
  - [x] SubTask 2.1: 在 fiber.rs 的 fiber_await 中，将 `Fiber::getCurrent` 调用 + null/对象校验移到 `runtime.spawn` 之前，校验失败时直接返回 Err，不 spawn tokio task
  - [x] SubTask 2.2: cargo build --features php 编译通过

- [x] Task 3: get_config 补全 tcp_keepalive_interval 字段（P2 配置一致性）
  - [x] SubTask 3.1: 在 php_ext.rs 的 get_config 中，在 tcp_keepalive 之后插入 `tcp_keepalive_interval` 字段
  - [x] SubTask 3.2: cargo build --features php 编译通过

- [x] Task 4: spawn_output_reader 防整数溢出（P3 checked_add）
  - [x] SubTask 4.1: 在 php_ext.rs 的 spawn_output_reader 中，将 `buf.len() + n <= limit` 改为 `checked_add + is_none_or`，溢出时标记 exceeded=true 并 break
  - [x] SubTask 4.2: cargo build --features php 编译通过

- [x] Task 5: 全量验证 + 新增测试
  - [x] SubTask 5.1: cargo fmt --check + cargo clippy -- -D warnings（非 php）+ cargo clippy --all-targets --features php -- -D warnings
  - [x] SubTask 5.2: cargo test --lib + cargo test --test integration_test + cargo test --test executor_async_test
  - [x] SubTask 5.3: cargo build --release --features php
  - [x] SubTask 5.4: PHP 运行时测试全量回归（php_each 37 + multi_each 18 + threadpool_each 16 + runtime 36 + network 42，共 149 项通过）
  - [x] SubTask 5.5: 新增测试：get_config 含 tcp_keepalive_interval 字段、超大数组 gather 提前拒绝（不克隆）— 2 项均通过

# Task Dependencies

- Task 1-4 互相独立，可并行（Task 1/3/4 改 php_ext.rs 不同位置，Task 2 改 fiber.rs）
- Task 5 依赖所有前序任务
