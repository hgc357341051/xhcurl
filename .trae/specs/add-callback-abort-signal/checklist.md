# Checklist

## invoke_streaming_callback 扩展
- [x] 返回值从 `Result<(), String>` 改为 `Result<bool, String>`
- [x] 回调返回严格 `=== false`（`is_false()`）→ `Ok(false)`
- [x] 回调返回其他值 → `Ok(true)`
- [x] 回调抛异常 → `Err(extract_callback_error(e))`（不变）

## PhpXhMulti::execute_each
- [x] 4 处 `invoke_streaming_callback` 调用点处理 `Ok(false)`
- [x] `Ok(false)` → abort_tasks + 返回 `Ok(count)`（已处理数，非错误）
- [x] `Err(msg)` → abort_tasks + 返回 `Err(msg)`（不变）
- [x] drain 残留 stream channel 事件（Ok(false) 也需 drain）

## PhpXhThreadPool::execute_each
- [x] 2 处 `invoke_streaming_callback` 调用点处理 `Ok(false)`
- [x] `Ok(false)` → 返回 `(None, Ok(count))`（drop pool 中止，非错误）
- [x] `Err(msg)` → 返回 `(None, Err(msg))`（不变）
- [x] drain 残留 stream channel 事件（Ok(false) 也需 drain）

## fiber_each
- [x] `callback_callable.try_call` 后检查返回值 `is_false()`
- [x] `is_false()` → 提前退出循环，返回 `Ok(count as i64)`
- [x] 异常分支保持 `?` 传播（不变）

## 测试
- [x] XHMulti 回调返回 `false` 中止剩余任务，返回已处理数（非错误）
- [x] XHThreadPool 回调返回 `false` 中止剩余任务，返回已处理数
- [x] 协程 each 回调返回 `false` 中止剩余任务
- [x] 回调返回 `true`/`null`/`void`/`0`/`''`/`[]` 时继续处理（向后兼容 + 弱类型避免）
- [x] 回调抛异常仍中止（回归）

## 文档
- [x] README 新增"回调返回值控制中止"小节
- [x] 说明 `false` 中止、其他值继续、抛异常仍中止
- [x] 使用示例（业务异常中止场景）
- [x] 三者对比表补充"返回 false 中止"行

## 向后兼容
- [x] 现有回调返回 `void`/`null` → 继续处理（行为不变）
- [x] 现有回调抛异常 → 中止（行为不变）
- [x] 现有测试全部通过（无回归）

## 验证
- [x] `cargo fmt --check` 通过
- [x] `cargo clippy --features php -- -D warnings` 通过
- [x] `cargo test --lib --features php` 全部通过
- [x] `cargo build --features php` 编译成功
- [x] 全部 `php_*.php` 测试通过（含新增返回值控制测试）
