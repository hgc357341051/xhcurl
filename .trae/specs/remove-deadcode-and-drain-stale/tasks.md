# Tasks

- [x] Task 1: 移除 PhpXhResponse 死代码（php_ext.rs）
  - [x] SubTask 1.1: 移除 PhpXhResponse struct 定义 + #[php_class] 属性
  - [x] SubTask 1.2: 移除 PhpXhResponse 的 #[php_impl] impl 块
  - [x] SubTask 1.3: 移除 #[php_module] 中的 .class::<PhpXhResponse>() 注册
  - [x] SubTask 1.4: 移除 4 个仅被 PhpXhResponse::json() 调用的未使用 json_* helper 函数
  - [x] SubTask 1.5: cargo build --features php 编译通过（无警告）

- [x] Task 2: execute_all drain 陈旧结果（threadpool.rs）
  - [x] SubTask 2.1: 在提交请求前 drain result_rx 残留消息（独立作用域避免借用冲突）
  - [x] SubTask 2.2: cargo build --features php 编译通过

- [x] Task 3: 统一手动迭代为 for_each_kv（php_ext.rs）
  - [x] SubTask 3.1: extract_requests 改用 for_each_kv
  - [x] SubTask 3.2: opt_string_vec 改用 for_each_kv
  - [x] SubTask 3.3: cargo build --features php 编译通过

- [x] Task 4: 全量验证
  - [x] SubTask 4.1: cargo fmt --check + cargo clippy -- -D warnings（非 php）+ cargo clippy --all-targets --features php -- -D warnings
  - [x] SubTask 4.2: cargo test --lib（84 passed）+ cargo test --test integration_test（7 passed）+ cargo test --test executor_async_test（4 passed）
  - [x] SubTask 4.3: cargo build --release --features php 编译成功
  - [x] SubTask 4.4: PHP 运行时测试全量回归（each 39 + multi_each 18 + threadpool_each 16 + runtime 36 + network 42，共 151 项通过）

# Task Dependencies

- Task 1（php_ext.rs 移除）、Task 2（threadpool.rs）、Task 3（php_ext.rs 迭代改写）中 Task 1 和 Task 3 改同一文件但不同位置，建议串行避免冲突
- Task 4 依赖所有前序任务
