# Tasks

- [x] Task 1: XHThreadPool 新增 `timeout(int $secs): $this`
  - [x] SubTask 1.1: 在 `rust/src/php_ext.rs::PhpXhThreadPool` impl 块新增 `timeout()` setter，存储到 `request_timeout` 字段（与 XHMulti 对齐）
  - [x] SubTask 1.2: 在 `rust/src/threadpool.rs::execute_all/execute_each` 中透传 `request_timeout` 到 batch 执行（用 `tokio::time::timeout` 包裹整体 future；0 = 无超时）
  - [x] SubTask 1.3: 测试：`$pool->timeout(1)` + 添加 hang 请求，execute 在约 1 秒后返回（hang 请求 success=false）

- [x] Task 2: XHThreadPool 新增 `maxResponseSize(int $bytes): $this`
  - [x] SubTask 2.1: 在 `rust/src/php_ext.rs::PhpXhThreadPool` impl 块新增 `maxResponseSize()` setter，存储到 `max_response_size` 字段
  - [x] SubTask 2.2: 在 `rust/src/threadpool.rs` execute 路径透传 `max_response_size` 到单请求执行（复用 executor 现有 max_response_size 逻辑）
  - [x] SubTask 2.3: 测试：`$pool->maxResponseSize(100)` + 请求 /stream（大 body），对应结果 success=false（size exceeded）

- [x] Task 3: XHThreadPool 新增 `maxConcurrency(int $max): $this`
  - [x] SubTask 3.1: 在 `rust/src/php_ext.rs::PhpXhThreadPool` impl 块新增 `maxConcurrency()` setter，更新 `max_concurrency` 字段（线程池已有该字段）
  - [x] SubTask 3.2: 在 execute 路径确保 `max_concurrency` 被用于限制并发信号量（验证现有 Semaphore 已绑定该值）
  - [x] SubTask 3.3: 测试：`$pool->maxConcurrency(1)` + 2 个请求，验证可执行不抛异常（并发上限生效的细粒度断言可选）

- [x] Task 4: `cookies(array)` 对 value 做 URL 编码（防注入）
  - [x] SubTask 4.1: 在 `rust/src/php_ext.rs::cookies()` 数组分支中，用 `urlencoding::encode(&value_str)` 或 `percent_encoding` 对 value 编码后再 `format!("{}={}", k, encoded_v)`
  - [x] SubTask 4.2: 保留现有标量转换逻辑（整型/浮点/布尔→字符串→URL 编码）
  - [x] SubTask 4.3: key 不编码（cookie name 通常为字母数字，编码会破坏向后兼容）
  - [x] SubTask 4.4: 测试：`cookies(['user' => 'a; admin=1'])` → Cookie 头为 `user=a%3B+admin%3D1`（非 `user=a; admin=1`）
  - [x] SubTask 4.5: 测试：正常字母数字 value 不受影响（`['session' => 'abc123']` → `session=abc123`）
  - [x] SubTask 4.6: 回归测试：现有 `cookies('k=v; k2=v2')` 字符串形式不受影响

- [x] Task 5: XHRequest 新增 `getTimeoutMs()`/`getConnectTimeoutMs()`
  - [x] SubTask 5.1: 在 `rust/src/request.rs` 新增 `timeout_ms`/`connect_timeout_ms` 的 getter（若 Rust 端尚无 getter 则新增）
  - [x] SubTask 5.2: 在 `rust/src/php_ext.rs::PhpXhRequest` impl 块新增 `getTimeoutMs(): ?int` 和 `getConnectTimeoutMs(): ?int`，委托 Rust getter
  - [x] SubTask 5.3: 测试：`timeoutMs(500)->getTimeoutMs()` 返回 500；未设置返回 null

- [x] Task 6: XHMulti/XHThreadPool 新增批次配置 getter
  - [x] SubTask 6.1: 在 `rust/src/php_ext.rs::PhpXhMulti` impl 块新增 `getMaxConcurrency(): int`、`getMaxResponseSize(): int`、`getTimeout(): int`（未配置返回 0）
  - [x] SubTask 6.2: 在 `rust/src/php_ext.rs::PhpXhThreadPool` impl 块新增同样 3 个 getter
  - [x] SubTask 6.3: 测试：`$multi->maxConcurrency(10)->timeout(30)->getMaxConcurrency()` 返回 10；`getTimeout()` 返回 30；未设置返回 0
  - [x] SubTask 6.4: 测试：XHThreadPool 同样

- [x] Task 7: README 文档更新
  - [x] SubTask 7.1: 补充 XHThreadPool 新增 `timeout()`/`maxResponseSize()`/`maxConcurrency()` 方法说明（与 XHMulti 对齐）
  - [x] SubTask 7.2: 补充 `cookies(array)` 对 value URL 编码的说明（防注入、与 setcookie 行为对齐、key 不编码）
  - [x] SubTask 7.3: 补充 `getTimeoutMs()`/`getConnectTimeoutMs()` 方法说明
  - [x] SubTask 7.4: 补充 XHMulti/XHThreadPool 批次配置 getter 说明
  - [x] SubTask 7.5: 补充迁移注意事项（从 Guzzle/curl 迁移：timeout 0 值语义、headers 键名小写、cookie 数组形式自动 URL 编码）
  - [x] SubTask 7.6: 补充错误处理完整示例（try/catch + `if (!$result['success'])` 检查 error_type）

- [x] Task 8: 验证
  - [x] SubTask 8.1: `cargo fmt --check`
  - [x] SubTask 8.2: `cargo clippy --features php -- -D warnings`
  - [x] SubTask 8.3: `cargo test --lib --features php`
  - [x] SubTask 8.4: `cargo build --features php` 编译成功
  - [x] SubTask 8.5: 启动 mock 服务器，运行全部 `php_*.php` 测试通过（含新增测试 + 回归）

# Task Dependencies
- Task 1（XHThreadPool timeout）独立
- Task 2（XHThreadPool maxResponseSize）独立
- Task 3（XHThreadPool maxConcurrency）独立
- Task 4（cookies URL 编码）独立
- Task 5（ms getter）独立
- Task 6（批次 getter）依赖 Task 1-3（验证 setter 存在）
- Task 7（文档）依赖 Task 1-6 完成
- Task 8（验证）依赖所有前序任务完成
- Task 1-5 可并行
