# Checklist

## XHThreadPool 回调异常修复
- [x] `PhpXhThreadPool::execute_each` 回调异常分支返回 `(None, Err(msg))`（不存回 pool）
- [x] pool 被 drop 后 dispatcher + workers 被 abort（HTTP 请求被中止）
- [x] 下次调用 `executeEach`/`execute` 时 `pool.is_none()` → 重建 pool
- [x] 正常路径（无回调异常）行为不变（pool 存回复用）
- [x] 代码注释说明与协程 each / XhMulti::execute_each 行为一致

## 行为一致性（三者对齐）
- [x] 协程 `each` 回调异常 → abort 剩余任务（`SchedulerGuard::drop`）
- [x] `XHMulti::executeEach` 回调异常 → `abort_tasks()`
- [x] `XHThreadPool::executeEach` 回调异常 → drop pool → abort dispatcher + workers
- [x] 三者回调收到的 `$result` 字段一致（共享 `result_to_php_array`）
- [x] 三者失败请求都触发回调（`success=false, status=0, body="", error=...`）
- [x] 三者都不累积结果（内存恒定）

## 测试
- [x] XHThreadPool 回调异常后剩余请求不触发回调
- [x] 协程 each 回调异常后剩余请求不触发回调
- [x] XHMulti executeEach 回调异常后剩余请求不触发回调
- [x] XHThreadPool 回调异常后再次调用 executeEach 能正常工作（pool 重建）

## 文档
- [x] README 新增"请求级流式回调行为契约"小节
- [x] 明确三种模式都支持请求级流式回调
- [x] 列出统一行为契约（每完成一个就回调、成功/失败都触发、不累积、回调异常中止）
- [x] 三者对比表（调用方式、并发模型、SAPI 限制、onChunk/onHeaders 支持）

## 验证
- [x] `cargo fmt --check` 通过
- [x] `cargo clippy --features php -- -D warnings` 通过
- [x] `cargo test --lib --features php` 全部通过
- [x] `cargo build --features php` 编译成功
- [x] 全部 `php_*.php` 测试通过
