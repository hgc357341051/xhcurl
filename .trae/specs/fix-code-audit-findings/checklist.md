# Checklist

## 高优先级修复

- [x] header.rs to_header_map() 对非法头部名返回 InvalidArgument 错误(非静默跳过)
- [x] header.rs to_header_map() 对非法头部值返回 InvalidArgument 错误(非静默跳过)
- [x] header.rs 合法头部转换不受影响(原有功能正常)
- [x] php_ext.rs fill_response_fields 在 body 为 None 时插入空字符串 ""
- [x] php_ext.rs PHP 端失败响应 $resp['body'] 不再触发 Undefined index
- [x] fiber.rs fiber_gather 信号量 acquire() 失败时发送错误结果并 return(不执行请求)
- [x] php_ext.rs php_array_to_form 非 UTF-8 表单值不被静默丢弃

## 中优先级修复

- [x] php_ext.rs set_config 支持 http2_enabled 字段
- [x] php_ext.rs set_config 支持 tcp_keepalive 字段
- [x] php_ext.rs set_config 支持 max_connections 字段
- [x] php_ext.rs set_config 支持 tcp_keepalive_interval 字段
- [x] php_ext.rs set_config 后 getConfig() 返回值一致
- [x] request.rs build_request_client 的 build() 错误走 Request 变体(非 Generic)
- [x] request.rs 原始 reqwest::Error 可通过 Error::source() 取回
- [x] php_ext.rs elapsed_ms 由单处写入(无双重插入覆盖)
- [x] php_ext.rs error 字段由单处写入(无双重插入覆盖)
- [x] fiber.rs 嵌套 run() 返回明确错误(非 panic)
- [x] fiber.rs 外层调度器状态不受嵌套影响

## 验证

- [x] cargo fmt --check 通过
- [x] cargo clippy --all-targets --features php -- -D warnings 通过
- [x] cargo test --lib 全部通过(84 passed)
- [x] cargo test --test integration_test 全部通过(7 passed)
- [x] cargo test --test executor_async_test 全部通过(4 passed)
- [x] cargo build --features php 编译成功
- [x] PHP 运行时测试 php_runtime_test.php 36 通过
- [x] PHP 网络测试 php_network_test.php 42 通过
- [x] PHP 无效代理测试 exit code 134(panic)
