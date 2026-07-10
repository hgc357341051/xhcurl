# Tasks

- [x] Task 1: 在 fiber.rs 新增 fiber_each 函数
  - [x] SubTask 1.1: 镜像 fiber_gather 结构,复用取 current_fiber、Fiber 上下文校验、runtime/client 获取、Semaphore(min(N,64))、spawn 循环、pending 注册循环
  - [x] SubTask 1.2: 循环体改为 suspend 后调用 ZendCallable 触发用户回调(不插入 results),回调异常用 ? 向上传播
  - [x] SubTask 1.3: 空请求列表(total==0)提前返回 Ok(0),不 spawn 不 suspend
  - [x] SubTask 1.4: 返回 Ok(total as i64)
  - [x] SubTask 1.5: 编译验证 cargo build --features php

- [x] Task 2: 在 php_ext.rs 新增 coroutine_each PHP 方法
  - [x] SubTask 2.1: 新增 #[php(name = "each")] pub fn coroutine_each(requests: &ZendHashTable, callback: &Zval) -> Result<i64, String>
  - [x] SubTask 2.2: 复用 coroutine_gather 的请求数组解析逻辑(提取 extract_requests 辅助函数共用)
  - [x] SubTask 2.3: 调用 crate::fiber::fiber_each(req_list, callback)
  - [x] SubTask 2.4: 在模块注册(get_module)中确认 each 方法已自动注册
  - [x] SubTask 2.5: 编译验证 cargo build --features php

- [x] Task 3: 编写 PHP 运行时测试 tests/php_each_test.php
  - [x] SubTask 3.1: 测试正常流式处理(多个请求,每个回调收到完整字段)
  - [x] SubTask 3.2: 测试结果按完成顺序触发(非提交顺序)
  - [x] SubTask 3.3: 测试空请求列表返回 0
  - [x] SubTask 3.4: 测试单个请求返回 1
  - [x] SubTask 3.5: 测试回调抛异常终止 each
  - [x] SubTask 3.6: 测试失败请求仍触发回调(success=false, error 非空, body 空字符串)
  - [x] SubTask 3.7: 测试在 run() 外调用返回错误
  - [x] SubTask 3.8: 测试字段一致性(与 gather 对比)

- [x] Task 4: 编写 Rust 集成测试(跳过,PHP 测试已充分覆盖)

- [x] Task 5: 全量验证
  - [x] SubTask 5.1: cargo fmt --check + cargo clippy --all-targets --features php -- -D warnings
  - [x] SubTask 5.2: cargo test --lib + cargo test --test integration_test + cargo test --test executor_async_test
  - [x] SubTask 5.3: cargo build --release --features php 编译 PHP 扩展
  - [x] SubTask 5.4: PHP 运行时测试(php_runtime_test.php 36 + php_network_test.php 42 + php_invalid_proxy_test.php + php_each_test.php 18)

- [x] Task 6(额外): 修复 result_to_php_array 失败路径缺 body 字段
  - 失败请求(response None)补充 body 空字符串,与成功路径字段一致

# Task Dependencies

- Task 1(fiber.rs)先于 Task 2(php_ext.rs 调用 fiber_each)
- Task 2 先于 Task 3(PHP 测试需要扩展编译成功)
- Task 4 跳过(PHP 测试已充分覆盖)
- Task 5 依赖所有前序任务完成
