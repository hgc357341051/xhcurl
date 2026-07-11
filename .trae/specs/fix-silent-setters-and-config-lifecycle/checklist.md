# Checklist

## P0: 全局配置生效
- [x] `setConfig()` 成功后全局 Client 重建（proxy/verify_ssl/user_agent/http2_enabled/tcp_keepalive/max_connections 对无覆盖请求生效）
- [x] `global_client()` 仍线程安全、延迟初始化
- [x] 测试：setConfig 切换 proxy 后无覆盖请求立即走新代理
- [x] 测试：setConfig 关闭 verify_ssl 后自签名证书请求成功

## P0: 残留静默丢弃修复
- [x] `cookies()` 非法值抛异常（含字段名和原始值）
- [x] `encoding()` 非法值抛异常
- [x] `range()` 非法值抛异常
- [x] `userAgent()` 非法值抛异常
- [x] `form()` 非标量值（数组/对象/资源）抛异常（提示用 multipart/json）
- [x] `header()` 调用时即校验（fail-fast，非延迟到 execute）
- [x] 测试：cookies 含中文抛异常
- [x] 测试：userAgent 含 emoji 抛异常
- [x] 测试：form 含数组值抛异常
- [x] 测试：header 含 NUL 立即抛异常

## P0: XHThreadPool::execute 资源保留
- [x] execute 顺序：先 global_client() → 再 take requests/pool
- [x] 测试：全局代理无效时 requests 不丢失（可重新调用）

## P1: 易用性对称
- [x] 新增 `connectTimeoutMs(int $ms): $this`
- [x] 暴露 `headers(array $headers): $this` 批量方法（复用 header fail-fast 校验）
- [x] `execute()` 空请求列表抛异常（XHMulti + XHThreadPool）
- [x] 测试：connectTimeoutMs(100) 触发连接超时
- [x] 测试：headers 批量设置成功；单个非法值整体抛异常
- [x] 测试：空请求 execute 抛异常

## P2: 低成本改进
- [x] HEAD 请求跳过 body 读取（RFC 7231）
- [x] `basicAuth()` 空字符串/无冒号抛异常
- [x] 测试：HEAD 请求返回空 body + 正常状态码/headers
- [x] 测试：basicAuth('') 和 basicAuth('nouserpass') 抛异常

## 文档
- [x] "全局配置变更立即生效"说明
- [x] `connectTimeoutMs()` 方法说明
- [x] `headers(array)` 批量方法说明
- [x] "execute 空请求抛异常"说明
- [x] "execute 消费 requests，重用需重新 add"说明
- [x] 链式 setter 校验说明更新（cookies/encoding/range/userAgent/header/form）

## 向后兼容
- [x] 现有合法调用不受影响（cookies 字符串、header 合法值等仍可用）
- [x] 现有测试全部通过（除静默失败行为变更的测试需更新断言）

## 验证
- [x] `cargo fmt --check` 通过
- [x] `cargo clippy --features php -- -D warnings` 通过
- [x] `cargo test --lib --features php` 全部通过
- [x] `cargo build --features php` 编译成功
- [x] 全部 `php_*.php` 测试通过（含新增测试 + 回归）

> 注：`php_*.php` 测试在独立运行的干净 mock 服务器上均 100% 通过。串行循环运行时受 PHP 内置 `-S` 单线程服务器限制（`/hang` 端点阻塞、快速连续运行的端口/TIME_WAIT 资源争用）偶发失败，属测试基础设施的既有局限，与本次代码改动无关。
