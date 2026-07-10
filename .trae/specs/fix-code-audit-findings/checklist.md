# Checklist

## 高优先级修复

- [ ] header.rs to_header_map() 对非法头部名返回 InvalidArgument 错误(非静默跳过)
- [ ] header.rs to_header_map() 对非法头部值返回 InvalidArgument 错误(非静默跳过)
- [ ] header.rs 合法头部转换不受影响(原有功能正常)
- [ ] php_ext.rs fill_response_fields 在 body 为 None 时插入空字符串 ""
- [ ] php_ext.rs PHP 端失败响应 $resp['body'] 不再触发 Undefined index
- [ ] fiber.rs fiber_gather 信号量 acquire() 失败时发送错误结果并 return(不执行请求)
- [ ] php_ext.rs php_array_to_form 非 UTF-8 表单值不被静默丢弃

## 中优先级修复

- [ ] php_ext.rs set_config 支持 http2_enabled 字段
- [ ] php_ext.rs set_config 支持 tcp_keepalive 字段
- [ ] php_ext.rs set_config 支持 max_connections 字段
- [ ] php_ext.rs set_config 支持 tcp_keepalive_interval 字段
- [ ] php_ext.rs set_config 后 getConfig() 返回值一致
- [ ] request.rs build_request_client 的 build() 错误走 Request 变体(非 Generic)
- [ ] request.rs 原始 reqwest::Error 可通过 Error::source() 取回
- [ ] php_ext.rs elapsed_ms 由单处写入(无双重插入覆盖)
- [ ] php_ext.rs error 字段由单处写入(无双重插入覆盖)
- [ ] fiber.rs 嵌套 run() 返回明确错误(非 panic)
- [ ] fiber.rs 外层调度器状态不受嵌套影响

## 验证

- [ ] cargo fmt --check 通过
- [ ] cargo clippy --all-targets --features php -- -D warnings 通过
- [ ] cargo test --lib 全部通过
- [ ] cargo test --test integration_test 全部通过
- [ ] cargo test --test executor_async_test 全部通过
- [ ] cargo build --features php 编译成功
- [ ] PHP 运行时测试 php_runtime_test.php 36 通过
- [ ] PHP 网络测试 php_network_test.php 42 通过
- [ ] PHP 无效代理测试 exit code 134(panic)
