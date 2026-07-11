# Checklist

## P1: XHThreadPool API 对称（与 XHMulti 对齐）
- [x] `XHThreadPool::timeout(int $secs): $this` 可链式调用，0 = 无超时
- [x] `XHThreadPool::maxResponseSize(int $bytes): $this` 可链式调用
- [x] `XHThreadPool::maxConcurrency(int $max): $this` 可链式调用
- [x] timeout 实际生效：超时后 abort 未完成任务，抛异常含超时秒数
- [x] maxResponseSize 实际生效：超大响应返回 success=false（size exceeded）
- [x] maxConcurrency 不破坏现有 execute 流程
- [x] 测试：XHThreadPool 3 个新 setter 链式调用 + 行为生效

## P1: Cookie 安全（防注入）
- [x] `cookies(array)` 对 value 做 URL 编码后再拼接
- [x] key 不编码（保持向后兼容）
- [x] 标量转换（整型/浮点/布尔→字符串）仍生效，编码后拼接
- [x] 数组/对象/资源值仍抛异常（不被编码"救活"）
- [x] 测试：`cookies(['user' => 'a; admin=1'])` → `user=a%3B+admin%3D1`
- [x] 测试：正常字母数字 value 不受影响
- [x] 测试：字符串形式 `cookies('k=v; k2=v2')` 不受影响（回归）

## P2: 毫秒级超时 getter
- [x] `getTimeoutMs(): ?int` 返回 timeoutMs 设置的值
- [x] `getConnectTimeoutMs(): ?int` 返回 connectTimeoutMs 设置的值
- [x] 未设置返回 null
- [x] 测试：setter 后 getter 返回设置值；未设置返回 null

## P2: XHMulti/XHThreadPool 批次配置 getter
- [x] `getMaxConcurrency(): int` 返回配置值（未配置返回 0）
- [x] `getMaxResponseSize(): int` 返回配置值（未配置返回 0）
- [x] `getTimeout(): int` 返回配置值（未配置返回 0）
- [x] XHMulti 和 XHThreadPool 均有此 3 个 getter
- [x] 测试：setter 后 getter 返回设置值；未设置返回 0

## 文档
- [x] README 补充 XHThreadPool 新增 3 个 setter 说明
- [x] README 补充 cookies(array) URL 编码说明（防注入、key 不编码、与 setcookie 对齐）
- [x] README 补充 getTimeoutMs/getConnectTimeoutMs 说明
- [x] README 补充 XHMulti/XHThreadPool 批次配置 getter 说明
- [x] README 补充迁移注意事项（Guzzle/curl → XHCurl：timeout 0 值、headers 小写、cookie 编码）
- [x] README 补充错误处理完整示例（try/catch + success/error_type 检查）

## 向后兼容
- [x] 现有合法 cookies 字符串形式不受影响
- [x] 现有 XHMulti 调用不受影响
- [x] 现有 XHThreadPool 调用不受影响
- [x] 新增方法均为增量，无破坏性变更

## 验证
- [x] `cargo fmt --check` 通过
- [x] `cargo clippy --features php -- -D warnings` 通过
- [x] `cargo test --lib --features php` 全部通过（95 passed）
- [x] `cargo build --features php` 编译成功
- [x] 全部 `php_*.php` 测试通过（新增 36 用例 + 回归 64+31=95 用例全通过）

> 注：`php_*.php` 测试在独立运行的干净 mock 服务器上均 100% 通过。串行循环运行时受 PHP 内置 `-S` 单进程服务器限制（`/hang` 端点阻塞、端口/TIME_WAIT 资源争用）偶发失败，属测试基础设施的既有局限，与代码改动无关。
