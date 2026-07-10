# Tasks

- [x] Task 1: 修复事件泵异常传播缺陷(P0)
  - [x] SubTask 1.1: 在 fiber.rs 顶部引入 `use ext_php_rs::zend::ExecutorGlobals;`
  - [x] SubTask 1.2: 在 run_event_loop 的 resume 调用后,检查 ExecutorGlobals::take_exception(),若 Some(exc) 提取 message 返回 Err
  - [x] SubTask 1.3: 对 start 和 getReturn 调用也加异常检查
  - [x] SubTask 1.4: 编译验证 cargo build --features php
  - [x] SubTask 1.5: PHP 运行时测试验证异常正确传播(gather 后抛异常、each 回调抛异常都应立即返回错误而非超时)

- [x] Task 2: XhMulti::executeEach 流式回调(P1)
  - [x] SubTask 2.1: 在 php_ext.rs 的 PhpXhMulti impl 块新增 #[php(name = "executeEach")] 方法
  - [x] SubTask 2.2: 复用 execute 的 XhMulti 创建 + spawn 逻辑,但收集循环改为 recv 一个调一次回调(ZendCallable::try_call)
  - [x] SubTask 2.3: 空请求列表提前返回 0
  - [x] SubTask 2.4: 返回处理总数
  - [x] SubTask 2.5: 编译验证 cargo build --features php
  - [x] SubTask 2.6: PHP 运行时测试(正常流式、空列表、失败请求、字段一致性) → 见 Task 5

- [x] Task 3: XHThreadPool::executeEach 流式回调(P2)
  - [x] SubTask 3.1: 在 php_ext.rs 的 PhpXhThreadPool impl 块新增 #[php(name = "executeEach")] 方法
  - [x] SubTask 3.2: 复用 execute 的 ThreadPool 创建 + submit 逻辑,但收集循环改为 recv 一个调一次回调
  - [x] SubTask 3.3: 沿用 sapi_is_cli() 检查(与 execute 一致,FPM 下报错)
  - [x] SubTask 3.4: 空请求列表提前返回 0
  - [x] SubTask 3.5: 返回处理总数
  - [x] SubTask 3.6: 编译验证 cargo build --features php
  - [x] SubTask 3.7: PHP 运行时测试(正常流式、空列表、失败请求、字段一致性、FPM 守卫) → 见 Task 5

- [x] Task 4: 更新 each 异常测试(P0 修复后回归)
  - [x] SubTask 4.1: 更新 php_each_test.php 的异常测试断言,验证异常 message 正确传播(不再只是"run() 返回错误")
  - [x] SubTask 4.2: 新增 gather 异常传播测试(gather 后抛异常,验证 run() 返回异常 message)

- [x] Task 5: 全量验证
  - [x] SubTask 5.1: cargo fmt --check + cargo clippy --all-targets --features php -- -D warnings
  - [x] SubTask 5.2: cargo test --lib + cargo test --test integration_test + cargo test --test executor_async_test
  - [x] SubTask 5.3: cargo build --release --features php 编译 PHP 扩展
  - [x] SubTask 5.4: PHP 运行时测试(php_runtime_test.php + php_network_test.php + php_invalid_proxy_test.php + php_each_test.php + php_multi_each_test.php + php_threadpool_each_test.php)

# Task Dependencies

- Task 1(P0 异常修复)独立,可最先做
- Task 2(multi each)和 Task 3(threadpool each)互相独立,可并行
- Task 4 依赖 Task 1(异常修复后才能更新测试断言)
- Task 5 依赖所有前序任务完成
