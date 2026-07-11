# Checklist

## P0: 静默失败修复
- [x] `json()` 序列化失败抛 PHP 异常（含原始错误原因）
- [x] `setUserData()` / `userData()` 序列化失败抛异常
- [x] `multipart()` 单字段非数组时跳过该字段（不退出整个方法）
- [x] `method()` 无效方法名抛异常（提示用 customMethod()）

## P1: 易用性优化
- [x] `proxy()` 接受 `?string`，传 null 清除请求级代理
- [x] 新增 `timeoutMs(int $ms)` 毫秒级超时
- [x] `cookies()` 接受数组（关联数组自动拼接为 cookie 字符串）
- [x] 结果数组失败路径含 `error_type` 字段（dns/timeout/ssl/connection/http）

## 错误分类逻辑
- [x] DNS 解析失败 → `error_type = 'dns'`
- [x] 请求超时 → `error_type = 'timeout'`
- [x] SSL/TLS 错误 → `error_type = 'ssl'`
- [x] 连接拒绝/失败 → `error_type = 'connection'`
- [x] HTTP 错误状态码 → `error_type = 'http'`（或不含 error_type，因 success=true）
- [x] 成功路径 → 不含 `error_type`（或空字符串）

## 文档
- [x] README 区分"配置类错误抛异常"vs"请求级失败返回数组"
- [x] `proxy(null)` 清除代理说明
- [x] `timeoutMs()` 方法说明
- [x] `cookies()` 数组形式说明
- [x] `error_type` 字段说明
- [x] `method()` 抛异常说明 + `customMethod()` 用途

## 测试
- [x] `json()` 序列化失败抛异常
- [x] `method()` 无效方法名抛异常
- [x] `multipart()` 单字段错误跳过该字段
- [x] `proxy(null)` 清除代理
- [x] `timeoutMs(500)` 毫秒级超时
- [x] `cookies(['k'=>'v'])` 数组形式
- [x] `error_type` 字段（dns/timeout/成功路径）

## 向后兼容
- [x] `cookies(string)` 字符串形式仍可用（向后兼容）
- [x] `timeout(int)` 秒级仍可用（向后兼容）
- [x] 现有测试全部通过（除静默失败行为变更的测试需更新断言）

## 验证
- [x] `cargo fmt --check` 通过
- [x] `cargo clippy --features php -- -D warnings` 通过
- [x] `cargo test --lib --features php` 全部通过（95 passed）
- [x] `cargo build --features php` 编译成功
- [x] 全部 `php_*.php` 测试通过（10 文件 223 用例全通过）
