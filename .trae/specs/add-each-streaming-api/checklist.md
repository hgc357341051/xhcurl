# Checklist

## fiber.rs fiber_each 实现

- [x] fiber_each 函数存在,签名正确(接收 Vec<XhRequest> + &Zval 回调,返回 Result<i64, String>)
- [x] 复用 gather 的 current_fiber 获取与上下文校验(必须在 Fiber 内调用)
- [x] 复用 gather 的 runtime/client 获取逻辑
- [x] 复用 Semaphore::new(min(total, 64)) 并发上限
- [x] 复用 spawn 循环 + pending 注册循环
- [x] 循环体:suspend 后调用用户回调,不累积到 results
- [x] 空请求列表(total==0)提前返回 Ok(0)
- [x] 回调异常用 ? 向上传播(不静默吞)
- [x] 返回 Ok(total as i64)

## php_ext.rs coroutine_each 实现

- [x] #[php(name = "each")] 方法注册
- [x] 参数:requests: &ZendHashTable, callback: &Zval
- [x] 返回值:Result<i64, String>
- [x] 复用 coroutine_gather 的请求数组解析逻辑(提取 extract_requests 辅助函数共用)
- [x] 调用 crate::fiber::fiber_each
- [x] 模块注册中 each 方法可见

## PHP 运行时测试

- [x] 正常流式处理:多个请求,每个回调收到完整字段(id/success/status/body/headers/elapsed_ms)
- [x] 结果按完成顺序触发(非提交顺序)
- [x] 空请求列表返回 0
- [x] 单个请求返回 1
- [x] 回调抛异常终止 each(run() 返回错误,回调次数 < 总数)
- [x] 失败请求仍触发回调(success=false, error 非空, body 空字符串)
- [x] 在 run() 外调用返回错误
- [x] 字段一致性:回调收到的 $result 与 gather 返回元素字段一致

## 额外修复

- [x] result_to_php_array 失败路径(response None)补充 body 空字符串字段(与成功路径一致)

## 验证

- [x] cargo fmt --check 通过
- [x] cargo clippy --all-targets --features php -- -D warnings 通过
- [x] cargo test --lib 全部通过(84 passed)
- [x] cargo test --test integration_test 全部通过(7 passed)
- [x] cargo test --test executor_async_test 全部通过(4 passed)
- [x] cargo build --release --features php 编译成功
- [x] PHP 运行时测试 php_runtime_test.php 36 通过
- [x] PHP 网络测试 php_network_test.php 42 通过
- [x] PHP 无效代理测试 panic 正确触发
- [x] PHP each 测试 php_each_test.php 18 通过

## 已知限制(非本次 spec 范围)

- [ ] 事件泵在 Fiber 内抛异常时,resume() 吞异常导致事件泵超时报"空闲"而非传播原始异常(gather 也有同样问题,预存缺陷,超出 each spec 范围)
