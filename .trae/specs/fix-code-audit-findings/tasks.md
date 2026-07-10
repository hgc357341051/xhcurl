# Tasks

- [ ] Task 1: 修复 header.rs to_header_map() 静默丢头部
  - [ ] SubTask 1.1: 将 to_header_map() 签名改为 `-> XhCurlResult<reqwest::header::HeaderMap>`,非法头部返回 InvalidArgument 错误
  - [ ] SubTask 1.2: 更新所有调用方(request.rs to_reqwest 中的 to_header_map 调用)处理 Result
  - [ ] SubTask 1.3: 补充测试:非法头部名/值返回错误、合法头部正常转换
  - [ ] SubTask 1.4: 编译验证 cargo build + cargo test --lib

- [ ] Task 2: 修复 php_ext.rs 错误响应缺 body 字段
  - [ ] SubTask 2.1: fill_response_fields 中 body 为 None 时插入空字符串 ""
  - [ ] SubTask 2.2: 验证 PHP 端失败响应 $resp['body'] 返回空字符串而非 Undefined index
  - [ ] SubTask 2.3: 编译验证 cargo build --features php + PHP 运行时测试

- [ ] Task 3: 修复 fiber.rs fiber_gather 信号量获取失败仍执行请求
  - [ ] SubTask 3.1: acquire().await 返回 Err 时发送 RequestResult::error 到 channel 并 return,不执行 execute_http_task
  - [ ] SubTask 3.2: 编译验证 cargo build --features php

- [ ] Task 4: 修复 php_ext.rs php_array_to_form 非二进制安全
  - [ ] SubTask 4.1: 表单 value 优先用 binary::<u8>() 读取,非 UTF-8 字节保留
  - [ ] SubTask 4.2: 编译验证 cargo build --features php

- [ ] Task 5: 修复 php_ext.rs set_config/get_config 字段不对称
  - [ ] SubTask 5.1: set_config 补齐 http2_enabled、tcp_keepalive、max_connections、tcp_keepalive_interval 字段读取
  - [ ] SubTask 5.2: 编译验证 cargo build --features php + PHP 运行时测试 setConfig/getConfig

- [ ] Task 6: 修复 request.rs build_request_client 错误类型丢失
  - [ ] SubTask 6.1: builder.build() 失败改用 .map_err(XhCurlError::from) 走 Request 变体
  - [ ] SubTask 6.2: 编译验证 cargo test --lib

- [ ] Task 7: 修复 php_ext.rs elapsed_ms/error 字段双重插入冗余
  - [ ] SubTask 7.1: fill_response_fields 不再写入 elapsed_ms 和 error,统一由 result_to_php_array 顶层负责
  - [ ] SubTask 7.2: 编译验证 cargo build --features php + PHP 运行时测试验证字段值一致

- [ ] Task 8: 修复 fiber.rs 嵌套 run() 破坏调度器
  - [ ] SubTask 8.1: fiber_run 入口检测是否已在事件泵内(调度器已初始化),拒绝嵌套返回明确错误
  - [ ] SubTask 8.2: 编译验证 cargo build --features php

- [ ] Task 9: 全量验证
  - [ ] SubTask 9.1: cargo fmt --check + cargo clippy --all-targets --features php -- -D warnings
  - [ ] SubTask 9.2: cargo test --lib + cargo test --test integration_test + cargo test --test executor_async_test
  - [ ] SubTask 9.3: cargo build --features php 编译 PHP 扩展
  - [ ] SubTask 9.4: PHP 运行时测试(php_runtime_test.php + php_network_test.php + php_invalid_proxy_test.php)

# Task Dependencies

- Task 1(header.rs)独立,可并行
- Task 2/4/5/7(php_ext.rs)有依赖:都改同一文件,需串行避免冲突
- Task 3/8(fiber.rs)有依赖:都改同一文件,需串行
- Task 6(request.rs)独立,可并行
- Task 9 依赖所有前序任务完成
- 可并行分组:Task 1 + Task 6 可并行;Task 2→4→5→7 串行;Task 3→8 串行
