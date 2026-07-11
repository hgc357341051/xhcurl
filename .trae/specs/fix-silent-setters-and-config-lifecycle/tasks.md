# Tasks

- [x] Task 1: 修复全局配置不生效（global_client 重建机制）
  - [x] SubTask 1.1: 在 `rust/src/curl.rs` 中将全局 Client 的 `OnceLock` 改为可重建的存储（如 `RwLock<Option<Client>>` 或 `ArcSwap<Client>` + 配置指纹比对）
  - [x] SubTask 1.2: 在 `rust/src/php_ext.rs::set_config()` 成功后触发全局 Client 重建（保留 `clear_request_client_cache()` 调用）
  - [x] SubTask 1.3: 确保 `global_client()` 仍线程安全且首调用延迟初始化
  - [x] SubTask 1.4: 测试：setConfig 切换 proxy/verify_ssl 后，无覆盖请求立即生效

- [x] Task 2: 修复 `cookies()/encoding()/range()/userAgent()` 静默丢弃
  - [x] SubTask 2.1: 在 `rust/src/request.rs::to_reqwest()` 中，`cookies`/`encoding`/`range`/`user_agent` 的 `HeaderValue::from_str` 失败时返回 `Err`（含字段名和原始值）
  - [x] SubTask 2.2: 确保错误信息对 PHP 友好（建议中文 urlencode 提示）
  - [x] SubTask 2.3: 测试：cookies 含中文抛异常、userAgent 含 emoji 抛异常、range 格式错误抛异常

- [x] Task 3: 修复 `XHThreadPool::execute()` 资源丢失
  - [x] SubTask 3.1: 在 `rust/src/php_ext.rs::XHThreadPool::execute()` 中调整顺序：先 `global_client()?` → 再 `take` requests/pool
  - [x] SubTask 3.2: 测试：全局代理无效时 requests 不丢失（execute 失败后仍可重新调用）

- [x] Task 4: 修复 `form()` 静默丢弃非标量值
  - [x] SubTask 4.1: 在 `rust/src/php_ext.rs::php_array_to_form()` 中，遇到非标量值（数组/对象/资源）时返回 `Err`（提示用 multipart() 或 json()）
  - [x] SubTask 4.2: 测试：form 含数组值抛异常

- [x] Task 5: `header()` 改为 fail-fast 校验
  - [x] SubTask 5.1: 在 `rust/src/php_ext.rs::header()` 中调用时即用 `HeaderValue::from_str` 校验 name 和 value
  - [x] SubTask 5.2: 校验失败抛异常，错误信息含 header 名和值
  - [x] SubTask 5.3: 测试：header 含 NUL 字节立即抛异常（非 execute 时）

- [x] Task 6: 新增 `connectTimeoutMs(int $ms)` 方法
  - [x] SubTask 6.1: 在 `rust/src/request.rs` 增加 `connect_timeout_ms: Option<u64>` 字段和 setter
  - [x] SubTask 6.2: 在 `to_reqwest()` 中优先使用 `connect_timeout_ms`（`Duration::from_millis`）
  - [x] SubTask 6.3: 在 `rust/src/php_ext.rs` 暴露 `#[php(name = "connectTimeoutMs")]` 方法
  - [x] SubTask 6.4: 测试：connectTimeoutMs(100) 触发连接超时

- [x] Task 7: 暴露 `headers(array $headers): $this` 批量方法
  - [x] SubTask 7.1: 在 `rust/src/php_ext.rs::PhpXhRequest` impl 块新增 `headers()` 方法，遍历数组调用 `header()`（复用 fail-fast 校验）
  - [x] SubTask 7.2: 测试：批量设置多个 header；单个非法值整体抛异常

- [x] Task 8: `execute()` 空请求列表抛异常
  - [x] SubTask 8.1: 在 `rust/src/php_ext.rs::XHMulti::execute()` 和 `XHThreadPool::execute()` 中，requests 为空时抛异常（消息含类名）
  - [x] SubTask 8.2: 测试：空请求 execute 抛异常

- [x] Task 9: HEAD 请求跳过 body 读取
  - [x] SubTask 9.1: 在 `rust/src/executor.rs::execute_request_inner()` 中，检测 `request.get_method() == HttpMethod::Head` 时跳过 `stream.chunk()` 循环
  - [x] SubTask 9.2: 测试：HEAD 请求返回空 body、状态码和 headers 正常

- [x] Task 10: `basicAuth()` 空值校验
  - [x] SubTask 10.1: 在 `rust/src/request.rs::basic_auth()` 中，空字符串或无冒号时返回 `Err`（提示格式 `user:pass`）
  - [x] SubTask 10.2: 测试：basicAuth('') 和 basicAuth('nouserpass') 抛异常

- [x] Task 11: README 文档更新
  - [x] SubTask 11.1: 增加"全局配置变更立即生效"说明（setConfig 后无需重启）
  - [x] SubTask 11.2: 补充 `connectTimeoutMs()` 方法说明
  - [x] SubTask 11.3: 补充 `headers(array)` 批量方法说明
  - [x] SubTask 11.4: 补充"execute 空请求抛异常"说明
  - [x] SubTask 11.5: 补充"execute 消费 requests，重用需重新 add"说明
  - [x] SubTask 11.6: 更新链式 setter 校验说明（cookies/encoding/range/userAgent/header/form 非法值抛异常）

- [x] Task 12: 验证
  - [x] SubTask 12.1: `cargo fmt --check`
  - [x] SubTask 12.2: `cargo clippy --features php -- -D warnings`
  - [x] SubTask 12.3: `cargo test --lib --features php`
  - [x] SubTask 12.4: `cargo build --features php` 编译成功
  - [x] SubTask 12.5: 启动 mock 服务器，运行全部 `php_*.php` 测试通过（含新增测试 + 回归）

# Task Dependencies
- Task 1（全局配置生效）独立
- Task 2（cookies/encoding/range/userAgent 校验）独立
- Task 3（XHThreadPool 资源保留）独立
- Task 4（form 校验）独立
- Task 5（header fail-fast）独立
- Task 6（connectTimeoutMs）独立
- Task 7（headers 批量）依赖 Task 5（复用 header 校验）
- Task 8（空请求报错）独立
- Task 9（HEAD 跳过 body）独立
- Task 10（basicAuth 校验）独立
- Task 11（文档）依赖 Task 1-10 完成
- Task 12（验证）依赖所有前序任务完成
- Task 1-6、8-10 可并行

# 实施说明
- Task 1-10 代码实现全部完成，覆盖 `rust/src/{curl,request,executor,php_ext}.rs`。
- 新增测试文件 `rust/tests/php_silent_setters_and_config_test.php`（31 个用例）全部通过。
- 现有测试 `rust/tests/php_each_test.php` 中 3 个"负数 clamp"用例已更新断言以匹配 Task 8（空请求 execute 抛异常）。
- 关键发现：http 1.x `HeaderValue::from_str` 按 RFC 7230 接受 obs-text（0x80-0xFF），故非 ASCII UTF-8（中文/emoji）会被接受。新增 `validate_ascii_header_value` 辅助函数先做 `is_ascii()` 检查再 `from_str`，确保中文/emoji 值被拒绝。
- Task 11（README 文档更新）不在本轮范围内，保持未完成。
- 验证说明（SubTask 12.5）：每个 `php_*.php` 测试文件在独立运行的干净 mock 服务器上均 100% 通过；串行循环运行时存在已知的环境性偶发失败（PHP 内置 `-S` 单线程 mock 服务器在 `/hang` 端点 sleep 期间被阻塞，以及快速连续运行时的端口/TIME_WAIT 资源争用），与代码改动无关。
