# Checklist

## Task 1: extract_requests 提前检查批量上限

- [x] extract_requests 在克隆循环前用 requests.len() 检查上限
- [x] 超大数组在克隆前即返回错误（内存安全）

## Task 2: fiber_await 先校验 Fiber 上下文

- [x] Fiber::getCurrent 调用移到 runtime.spawn 之前
- [x] 非 Fiber 上下文调用 await 不 spawn tokio task

## Task 3: get_config 补全字段

- [x] get_config 返回 tcp_keepalive_interval 字段
- [x] setConfig 后 getConfig 可完整回读

## Task 4: spawn_output_reader 防溢出

- [x] buf.len() + n 改用 checked_add
- [x] 溢出时停止读取并标记 exceeded=true

## 验证

- [x] cargo fmt --check 通过
- [x] cargo clippy -- -D warnings（非 php）通过
- [x] cargo clippy --all-targets --features php -- -D warnings 通过
- [x] cargo test --lib 全部通过（84 passed）
- [x] cargo test --test integration_test 全部通过（7 passed）
- [x] cargo test --test executor_async_test 全部通过（4 passed）
- [x] cargo build --release --features php 编译成功
- [x] php_each_test.php 全部通过（37 passed，含 2 项新增）
- [x] php_multi_each_test.php 全部通过（18 passed）
- [x] php_threadpool_each_test.php 全部通过（16 passed）
- [x] php_runtime_test.php 全部通过（36 passed）
- [x] php_network_test.php 全部通过（42 passed）
- [x] 新增 get_config tcp_keepalive_interval 字段测试通过
- [x] 新增大数组 gather 提前拒绝测试通过
