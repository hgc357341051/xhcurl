# Tasks

- [x] Task 1: 为 extract_requests 添加 MAX_REQUESTS_PER_BATCH 检查（P0）
  - [x] SubTask 1.1: 在 php_ext.rs 的 extract_requests 函数末尾（return Ok(req_list) 前）添加 `if req_list.len() > MAX_REQUESTS_PER_BATCH { return Err(format!(...)); }`
  - [x] SubTask 1.2: 新增 PHP 测试：gather/each 传入超过上限的数组返回错误
  - [x] SubTask 1.3: cargo build --features php 编译通过

- [x] Task 2: 统一 max_response_size=0 语义为"无限制"（P1）
  - [x] SubTask 2.1: 在 executor.rs 的 collect_response_body 中，将比较改为 `max_response_size > 0 && new_size > max_response_size`（0 时跳过大小检查）
  - [x] SubTask 2.2: 检查 fiber.rs 和 php_ext.rs 读取全局 max_response_size 后是否透传 0（确认无需修改，直接透传）
  - [x] SubTask 2.3: 新增 PHP 测试：setConfig max_response_size=0 后请求不报大小超限
  - [x] SubTask 2.4: cargo build --features php 编译通过

- [x] Task 3: 全量验证
  - [x] SubTask 3.1: cargo fmt --check + cargo clippy --all-targets --features php -- -D warnings
  - [x] SubTask 3.2: cargo test --lib + cargo test --test integration_test + cargo test --test executor_async_test
  - [x] SubTask 3.3: cargo build --release --features php
  - [x] SubTask 3.4: PHP 运行时测试（php_each_test.php + php_multi_each_test.php + php_threadpool_each_test.php + php_runtime_test.php + php_network_test.php + 新增上限/无限制测试）

# Task Dependencies

- Task 1 和 Task 2 互相独立，可并行
- Task 3 依赖所有前序任务
