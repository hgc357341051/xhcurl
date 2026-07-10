# Checklist

## Task 1: 移除 PhpXhResponse 死代码

- [x] PhpXhResponse struct + #[php_class] 移除
- [x] PhpXhResponse #[php_impl] impl 块移除
- [x] #[php_module] 中 .class::<PhpXhResponse>() 注册移除
- [x] 4 个未使用 json_* helper 函数移除
- [x] 编译通过，无 XHResponse 类注册、无未使用警告

## Task 2: execute_all drain 陈旧结果

- [x] execute_all 提交请求前 drain result_rx 残留消息
- [x] 陈旧 WorkerShutdown 消息不污染当前结果集
- [x] drain 在独立作用域，避免与 self.submit() 借用冲突

## Task 3: 统一手动迭代为 for_each_kv

- [x] extract_requests 改用 for_each_kv
- [x] opt_string_vec 改用 for_each_kv
- [x] 迭代风格统一

## 验证

- [x] cargo fmt --check 通过
- [x] cargo clippy -- -D warnings（非 php）通过
- [x] cargo clippy --all-targets --features php -- -D warnings 通过
- [x] cargo test --lib 全部通过（84 passed）
- [x] cargo test --test integration_test 全部通过（7 passed）
- [x] cargo test --test executor_async_test 全部通过（4 passed）
- [x] cargo build --release --features php 编译成功
- [x] php_each_test.php 全部通过（39 passed）
- [x] php_multi_each_test.php 全部通过（18 passed）
- [x] php_threadpool_each_test.php 全部通过（16 passed）
- [x] php_runtime_test.php 全部通过（36 passed）
- [x] php_network_test.php 全部通过（42 passed）
