# Checklist

## P0: Fiber 路径 SAPI 检查

- [x] fiber_run 入口添加 sapi_is_cli 检查，非 CLI 返回错误
- [x] sapi_is_cli 对 fiber.rs 可见
- [x] CLI 模式下 run 正常执行（回归测试）

## P1: XhMulti::execute 超时 break 路径 abort

- [x] multi.rs execute 超时 now >= deadline 时 abort 所有 tasks 并返回错误
- [x] 超时后不进入 handle.await 无限等待

## P2: PHP 入口方法负数校验

- [x] timeout/connect_timeout 负值 clamp 到 0
- [x] max_redirects 负值 clamp 到 0
- [x] PhpXhMulti max_concurrency/max_response_size 负值 clamp 到 0
- [x] PhpXhThreadPool __construct workers 负值 clamp 到 0
- [x] set_config 数值配置项负值跳过（6 项：connect_timeout/request_timeout/max_response_size/max_redirects/tcp_keepalive_interval/max_connections）

## P3: create_fiber 异常检查

- [x] create_fiber __construct 后调用 take_php_exception
- [x] 构造异常被正确传播

## 验证

- [x] cargo fmt --check 通过
- [x] cargo clippy -- -D warnings（非 php）通过
- [x] cargo clippy --all-targets --features php -- -D warnings 通过
- [x] cargo test --lib 全部通过（84 passed）
- [x] cargo test --test integration_test 全部通过（7 passed）
- [x] cargo test --test executor_async_test 全部通过（4 passed）
- [x] cargo build --release --features php 编译成功
- [x] php_each_test.php 全部通过（35 passed，含 7 项新增负数校验）
- [x] php_multi_each_test.php 全部通过（18 passed）
- [x] php_threadpool_each_test.php 全部通过（16 passed）
- [x] php_runtime_test.php 全部通过（36 passed）
- [x] php_network_test.php 全部通过（42 passed）
- [x] 新增 SAPI/超时/负数校验测试通过（7 项负数校验 + 回归测试）
