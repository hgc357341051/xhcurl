# Checklist

## P0: 提交失败反馈
- [x] `XHThreadPool::execute_all` 提交失败时返回 `Err`（含失败数量）
- [x] `XHThreadPool::execute_each` 同样处理
- [x] PHP 端错误传播为异常
- [x] 测试：大批量超过队列容量时 execute 抛异常

## P1: 0 值统一
- [x] `timeout(0)`/`timeoutMs(0)` 不立即超时（跳过设置）
- [x] `connectTimeout(0)`/`connectTimeoutMs(0)` 不触发 Client 重建
- [x] 全局 `connect_timeout=0`/`request_timeout=0` 跳过设置
- [x] 测试：timeout(0) 请求成功（不立即超时）
- [x] 测试：connectTimeout(0) 不重建 Client

## P1: 残留静默跳过修复
- [x] `cookies()` 数组形式支持整型/浮点/布尔（对齐 form）
- [x] `cookies()` 数组/对象/资源值抛异常
- [x] `multipart()` 空 name 抛异常
- [x] `multipart()` 非数组元素抛异常
- [x] `body()` 非字符串输入抛异常
- [x] `headers()` 列表数组抛异常
- [x] `xhrun` env 标量值转换（非跳过）
- [x] 测试：cookies 整型/布尔转换
- [x] 测试：multipart 空 name 抛异常
- [x] 测试：body 非字符串抛异常
- [x] 测试：headers 列表数组抛异常
- [x] 测试：xhrun env 整型值

## P1: Getter 对称
- [x] `getTimeout()`/`getConnectTimeout()`
- [x] `getHeaders()`/`getCookies()`
- [x] `getProxy()`/`getVerifySsl()`/`getUserAgent()`
- [x] `getId()`/`getUserData()`
- [x] 未设置返回 null/默认值
- [x] 测试：setter 后 getter 返回设置值

## P1: count/isEmpty
- [x] `XHMulti::count()`/`isEmpty()`
- [x] `XHThreadPool::count()`/`isEmpty()`
- [x] 测试：add 后 count 正确；isEmpty 布尔正确

## 文档
- [x] 新增 getter 方法说明
- [x] `count()`/`isEmpty()` 说明
- [x] onHeaders 回调 headers 键名小写说明
- [x] timeout 类 0 值语义说明
- [x] cookies/multipart/body/headers 校验说明更新
- [x] XHThreadPool 提交失败抛异常说明

## 向后兼容
- [x] 现有合法调用不受影响
- [x] 现有测试全部通过（除行为变更的测试需更新断言）

## 验证
- [x] `cargo fmt --check` 通过
- [x] `cargo clippy --features php -- -D warnings` 通过
- [x] `cargo test --lib --features php` 全部通过（95 passed）
- [x] `cargo build --features php` 编译成功
- [x] 全部 `php_*.php` 测试通过（12 文件 318 用例全通过）
