# Tasks

- [x] Task 1: Fiber 路径添加 SAPI 检查（P0 防无限循环）
  - [x] SubTask 1.1: 在 fiber.rs 的 fiber_run 入口（already_running 检查后）添加 `if !sapi_is_cli() { return Err("XHCurl::run 仅在 CLI 模式下可用（FPM 请用 XHMulti）".to_string()); }`，复用 php_ext.rs 的 sapi_is_cli
  - [x] SubTask 1.2: 确保 sapi_is_cli 对 fiber.rs 可见（pub(crate) 或在 fiber.rs 内调用 php_ext::sapi_is_cli）
  - [x] SubTask 1.3: 新增 PHP 测试：CLI 模式下 run 正常执行（回归）
  - [x] SubTask 1.4: cargo build --features php 编译通过

- [x] Task 2: 修复 XhMulti::execute 超时 break 路径（P1 防无限阻塞）
  - [x] SubTask 2.1: 在 multi.rs 的 execute 超时循环 `if now >= deadline { break; }` 处，改为 abort 所有 self.tasks 并返回超时错误（与 execute_each 一致）
  - [x] SubTask 2.2: cargo build --features php 编译通过

- [x] Task 3: PHP 入口方法添加负数校验（P2）
  - [x] SubTask 3.1: php_ext.rs 中 timeout/connect_timeout/max_redirects 方法：负值 clamp 到 0
  - [x] SubTask 3.2: PhpXhMulti 的 max_concurrency/max_response_size 方法：负值 clamp 到 0
  - [x] SubTask 3.3: PhpXhThreadPool::__construct 的 workers：负值 clamp 到 0
  - [x] SubTask 3.4: set_config 中各数值配置项：负值跳过（不设置）
  - [x] SubTask 3.5: 新增 PHP 测试：timeout(-1) 等 7 项负数校验测试
  - [x] SubTask 3.6: cargo build --features php 编译通过

- [x] Task 4: create_fiber 的 __construct 后检查 PHP 异常（P3）
  - [x] SubTask 4.1: 在 fiber.rs 的 create_fiber 中，__construct 调用后、返回前调用 take_php_exception()?
  - [x] SubTask 4.2: cargo build --features php 编译通过

- [x] Task 5: 全量验证
  - [x] SubTask 5.1: cargo fmt --check + cargo clippy -- -D warnings（非 php）+ cargo clippy --all-targets --features php -- -D warnings
  - [x] SubTask 5.2: cargo test --lib + cargo test --test integration_test + cargo test --test executor_async_test
  - [x] SubTask 5.3: cargo build --release --features php
  - [x] SubTask 5.4: PHP 运行时测试（php_each_test.php 35 + php_multi_each_test.php 18 + php_threadpool_each_test.php 16 + php_runtime_test.php 36 + php_network_test.php 42 + 新增 7 项负数校验测试，共 147 项通过）

# Task Dependencies

- Task 1-4 互相独立，可并行（但都改 fiber.rs/multi.rs/php_ext.rs，建议串行避免冲突）
- Task 5 依赖所有前序任务
