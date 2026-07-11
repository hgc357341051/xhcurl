# Checklist

## P1: pool 复用时配置变更生效
- [x] execute() 检测 maxConcurrency/maxResponseSize 变更并重建 pool
- [x] execute_each() 同样检测并重建
- [x] 未修改配置时 pool 复用不重建（无额外开销）
- [x] 测试：两次 execute 间修改 maxConcurrency 生效
- [x] 测试：未修改配置时 pool 复用行为一致
- [x] 测试：修改 maxResponseSize 后重建生效（超限失败）

## P1: executeEach 强制 timeout
- [x] execute_each() 中 timeout > 0 时计算 deadline
- [x] 非流式分支：recv() 用 tokio::time::timeout 包裹
- [x] 流式分支：select! 加 sleep_until(deadline) 分支
- [x] 超时后中止任务并返回错误（含已处理数）
- [x] timeout=0 时无超时（行为不变）
- [x] 测试：executeEach + timeout(2) + /hang 约 2 秒返回错误

## P2: 负值抛异常
- [x] XHMulti timeout/maxResponseSize/maxConcurrency 负值抛异常
- [x] XHThreadPool timeout/maxResponseSize/maxConcurrency 负值抛异常
- [x] 0 值不抛异常（保持"无超时/使用默认"语义）
- [x] 错误消息含字段名和合法范围提示
- [x] 测试：负值抛异常；0 值不抛异常

## 文档
- [x] README 注明 execute 间修改配置会重建 pool 生效
- [x] README 注明 executeEach 也强制 timeout
- [x] README 注明负值抛异常

## 向后兼容
- [x] 0 值语义不变（无超时/使用默认）
- [x] 合法正值不受影响
- [x] 未修改配置的复用场景行为不变

## 验证
- [x] `cargo fmt --check` 通过
- [x] `cargo clippy --features php -- -D warnings` 通过
- [x] `cargo test --lib --features php` 全部通过（95 passed）
- [x] `cargo build --features php` 编译成功
- [x] 新增测试 17 用例全通过
- [x] 第四轮测试 38 用例全通过（更新负值断言后）
- [x] 回归测试 64+31+28 全通过

> 注：`php_*.php` 测试在独立运行的干净 mock 服务器上均 100% 通过。串行循环运行时受 PHP 内置 `-S` 单进程服务器限制（`/hang` 端点阻塞、端口/TIME_WAIT 资源争用）偶发失败，属测试基础设施的既有局限，与代码改动无关。
