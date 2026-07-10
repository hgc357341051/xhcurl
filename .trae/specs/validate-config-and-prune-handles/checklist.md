# Checklist

## Task 1: set_config 类型不匹配反馈

- [x] 数值配置项（connect_timeout/request_timeout/max_response_size/max_redirects/tcp_keepalive_interval/max_connections）类型不匹配时收集键名
- [x] 布尔配置项（follow_redirects/verify_ssl/http2_enabled/tcp_keepalive）类型不匹配时收集键名
- [x] 字符串配置项（user_agent/proxy）类型不匹配时收集键名
- [x] 收集列表非空时返回 Err 列出所有不匹配项
- [x] 正确类型正常应用（不影响现有行为）

## Task 2: run_event_loop 句柄回收

- [x] resume + take_php_exception 之后调用 task_handles.retain(|h| !h.is_finished())
- [x] 已完成句柄被回收，task_handles 不无限增长

## Task 3: 新增测试

- [x] setConfig 字符串给数值配置项返回错误
- [x] setConfig 正确类型正常应用（回归）

## 验证

- [x] cargo fmt --check 通过
- [x] cargo clippy -- -D warnings（非 php）通过
- [x] cargo clippy --all-targets --features php -- -D warnings 通过
- [x] cargo test --lib 全部通过（84 passed）
- [x] cargo test --test integration_test 全部通过（7 passed）
- [x] cargo test --test executor_async_test 全部通过（4 passed）
- [x] cargo build --release --features php 编译成功
- [x] php_each_test.php 全部通过（39 passed，含 2 项新增）
- [x] php_multi_each_test.php 全部通过（18 passed）
- [x] php_threadpool_each_test.php 全部通过（16 passed）
- [x] php_runtime_test.php 全部通过（36 passed）
- [x] php_network_test.php 全部通过（42 passed）
