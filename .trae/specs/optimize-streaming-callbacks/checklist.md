# Checklist

## P1: 回调调用辅助函数

- [x] php_ext.rs 新增 `invoke_streaming_callback` 私有函数（封装 try_call + map_err 异常提取）
- [x] XhMulti::execute_each 超时分支使用 invoke_streaming_callback
- [x] XhMulti::execute_each 无超时分支使用 invoke_streaming_callback
- [x] XHThreadPool::execute_each 使用 invoke_streaming_callback
- [x] 无重复的 try_call + map_err 回调错误处理代码

## P2: XhMulti::executeEach 回调异常任务泄漏修复

- [x] 超时分支回调异常时，abort 剩余 tasks（handle.abort()）
- [x] 无超时分支回调异常时，abort 剩余 tasks
- [x] 回调异常后 execute_each 返回 Err（含异常 message）
- [x] 回调异常后剩余 tokio 任务不再继续执行（被 abort）

## P3: XHThreadPool::executeEach 完整性检查

- [x] 收集循环退出后检查 count == submitted（用 as usize 比较）
- [x] 不等时返回 Err（错误信息含预期/实际数量）
- [x] 正常完成时仍返回 Ok(count)

## P4: 测试覆盖

- [x] php_multi_each_test.php: 回调抛异常终止测试（返回 Err + 回调次数 < 请求总数）
- [x] php_multi_each_test.php: 超时测试（短超时返回超时 Err，hanging server on 18400）
- [x] php_threadpool_each_test.php: 回调抛异常终止测试（返回 Err + 回调次数 < 请求总数）

## 验证

- [x] cargo fmt --check 通过
- [x] cargo clippy --all-targets --features php -- -D warnings 通过
- [x] cargo test --lib 全部通过（84 通过）
- [x] cargo test --test integration_test 全部通过（7 通过）
- [x] cargo test --test executor_async_test 全部通过（4 通过）
- [x] cargo build --release --features php 编译成功
- [x] php_each_test.php 全部通过（21 通过，回归）
- [x] php_multi_each_test.php 全部通过（18 通过，含新增异常 + 超时测试）
- [x] php_threadpool_each_test.php 全部通过（16 通过，含新增异常测试）
- [x] php_runtime_test.php 全部通过（36 通过，回归）
- [x] php_network_test.php 全部通过（42 通过，回归）
