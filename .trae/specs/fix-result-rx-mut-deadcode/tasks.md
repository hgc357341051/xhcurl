# Tasks

- [x] Task 1: 给 result_rx_mut 添加 #[cfg(feature = "php")] 属性
  - [x] SubTask 1.1: 在 threadpool.rs 的 result_rx_mut 方法上方添加 `#[cfg(feature = "php")]`
  - [x] SubTask 1.2: 验证非 php 构建无 dead_code 警告：`cargo clippy -- -D warnings`（不带 --features php）
  - [x] SubTask 1.3: 验证 php 构建仍正常：`cargo build --features php`

- [x] Task 2: 全量验证（含非 php 和 php 两种构建）
  - [x] SubTask 2.1: `cargo fmt --check`
  - [x] SubTask 2.2: `cargo clippy -- -D warnings`（非 php 构建无警告）
  - [x] SubTask 2.3: `cargo clippy --all-targets --features php -- -D warnings`（php 构建无警告）
  - [x] SubTask 2.4: `cargo test --lib` + `cargo test --test integration_test` + `cargo test --test executor_async_test`
  - [x] SubTask 2.5: `cargo build --release --features php`
  - [x] SubTask 2.6: PHP 运行时测试（php_each_test.php + php_multi_each_test.php + php_threadpool_each_test.php + php_runtime_test.php + php_network_test.php）

# Task Dependencies

- Task 2 依赖 Task 1 完成
