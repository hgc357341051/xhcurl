# Checklist

## P0: 事件泵异常传播修复

- [x] fiber.rs 引入 ExecutorGlobals
- [x] run_event_loop resume 调用后检查 take_exception,Some 时提取 message 返回 Err
- [x] run_event_loop start 调用后检查 take_exception
- [x] run_event_loop getReturn 调用后检查 take_exception
- [x] 回调抛异常时 run() 立即返回错误(含异常 message)
- [x] gather 后用户代码抛异常时 run() 立即返回错误(含异常 message)
- [x] 正常执行无异常时行为不变

## P1: XhMulti::executeEach

- [x] #[php(name = "executeEach")] 方法注册
- [x] 参数:callback: &Zval
- [x] 返回值:Result<i64, String>(处理总数)
- [x] 复用 XhMulti 的 spawn + channel 机制
- [x] 收集循环改为 recv 一个调一次回调,不累积
- [x] 空请求列表返回 0
- [x] 回调收到的 $result 字段与 execute 一致(复用 result_to_php_array)
- [x] 失败请求仍触发回调(success=false)

## P2: XHThreadPool::executeEach

- [x] #[php(name = "executeEach")] 方法注册
- [x] 参数:callback: &Zval
- [x] 返回值:Result<i64, String>(处理总数)
- [x] 复用 ThreadPool 的 submit + dispatcher 机制
- [x] 收集循环改为 recv 一个调一次回调,不累积
- [x] 空请求列表返回 0
- [x] sapi_is_cli() 检查(FPM 下报错,与 execute 一致)
- [x] 回调收到的 $result 字段与 execute 一致
- [x] 失败请求仍触发回调(success=false)

## P0 回归测试

- [x] php_each_test.php 异常测试断言更新(验证异常 message 传播)
- [x] 新增 gather 异常传播测试

## 新增 PHP 测试

- [x] php_multi_each_test.php:正常流式、空列表、失败请求、字段一致性
- [x] php_threadpool_each_test.php:正常流式、空列表、失败请求、字段一致性、FPM 守卫

## 验证

- [x] cargo fmt --check 通过
- [x] cargo clippy --all-targets --features php -- -D warnings 通过
- [x] cargo test --lib 全部通过(84 通过)
- [x] cargo test --test integration_test 全部通过(7 通过)
- [x] cargo test --test executor_async_test 全部通过(4 通过)
- [x] cargo build --release --features php 编译成功
- [x] PHP 运行时测试 php_runtime_test.php 36 通过
- [x] PHP 网络测试 php_network_test.php 42 通过
- [x] PHP 无效代理测试 panic 正确触发
- [x] PHP each 测试 php_each_test.php 通过(21 通过,含异常传播断言更新 + gather 异常测试)
- [x] PHP multi each 测试 php_multi_each_test.php 通过(13 通过)
- [x] PHP threadpool each 测试 php_threadpool_each_test.php 通过(13 通过)
