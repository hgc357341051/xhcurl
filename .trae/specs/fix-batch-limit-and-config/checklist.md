# Checklist

## P0: gather/each 批量上限检查

- [x] extract_requests 末尾检查 req_list.len() > MAX_REQUESTS_PER_BATCH 返回错误
- [x] gather 传入超过上限的数组返回错误（不 spawn）
- [x] each 传入超过上限的数组返回错误（不 spawn）
- [x] 正常数量（<= 10000）仍正常执行

## P1: max_response_size=0 表示无限制

- [x] executor.rs collect_response_body 当 max_response_size==0 时跳过大小检查
- [x] fiber 路径读取全局 max_response_size=0 后透传 0（不归一化为 DEFAULT）
- [x] 单请求 execute 路径读取全局 max_response_size=0 后透传 0
- [x] setConfig max_response_size=0 后大响应不报错（新增测试验证）
- [x] 三条路径（fiber/execute/multi）行为一致

## 验证

- [x] cargo fmt --check 通过
- [x] cargo clippy --all-targets --features php -- -D warnings 通过
- [x] cargo test --lib 全部通过（84 通过）
- [x] cargo test --test integration_test 全部通过（7 通过）
- [x] cargo test --test executor_async_test 全部通过（4 通过）
- [x] cargo build --release --features php 编译成功
- [x] php_each_test.php 全部通过（28 通过，含新增上限检查 3 项 + 无限制测试 1 项）
- [x] php_multi_each_test.php 全部通过（18 通过，回归）
- [x] php_threadpool_each_test.php 全部通过（16 通过，回归）
- [x] php_runtime_test.php 全部通过（36 通过，回归）
- [x] php_network_test.php 全部通过（42 通过，回归）
- [x] 新增上限检查测试通过
- [x] 新增 max_response_size=0 无限制测试通过
