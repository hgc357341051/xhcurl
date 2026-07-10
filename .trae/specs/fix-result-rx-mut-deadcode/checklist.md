# Checklist

## 修复 result_rx_mut dead_code

- [x] threadpool.rs result_rx_mut 方法添加 #[cfg(feature = "php")]
- [x] 非 php 构建无 dead_code 警告
- [x] php 构建仍正常（executeEach 可调用 result_rx_mut）

## 验证

- [x] cargo fmt --check 通过
- [x] cargo clippy -- -D warnings（非 php）通过
- [x] cargo clippy --all-targets --features php -- -D warnings 通过
- [x] cargo test --lib 全部通过（84 通过）
- [x] cargo test --test integration_test 全部通过（7 通过）
- [x] cargo test --test executor_async_test 全部通过（4 通过）
- [x] cargo build --release --features php 编译成功
- [x] php_each_test.php 全部通过（28 通过，回归）
- [x] php_multi_each_test.php 全部通过（18 通过，回归）
- [x] php_threadpool_each_test.php 全部通过（16 通过，回归，含 executeEach 测试）
- [x] php_runtime_test.php 全部通过（36 通过，回归）
- [x] php_network_test.php 全部通过（42 通过，回归）
