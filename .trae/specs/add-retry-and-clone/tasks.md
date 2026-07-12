# Tasks

## 阶段 1：Rust 侧 XhRequest 新增 retry 字段与 setter

- [x] Task 1: 在 `rust/src/request.rs` 的 XhRequest 新增 retry 配置字段
  - [x] SubTask 1.1: 新增 `retry_times: u32` 字段（默认 0 = 不重试），初始化为 0
  - [x] SubTask 1.2: 新增 `retry_delay_ms: u64` 字段（默认 0 = 立即重试），初始化为 0
  - [x] SubTask 1.3: 在 `XhRequest::new()` 与 Default 中初始化默认值
- [x] Task 2: 在 `rust/src/request.rs` 新增 `retry()` setter 方法
  - [x] SubTask 2.1: 新增 `pub fn retry(mut self, times: u32, delay_ms: u64) -> Self`
  - [x] SubTask 2.2: delay_ms 无需校验（u64 不可能为负，0 = 立即重试是合法值）
  - [x] SubTask 2.3: 新增 `pub fn get_retry_times(&self) -> u32` 与 `pub fn get_retry_delay_ms(&self) -> u64` getter

## 阶段 2：PHP 绑定 retry/clone 方法

- [x] Task 3: 在 `rust/src/php_ext.rs` 新增 `retry()` PHP 绑定与 `__clone()` 魔术方法
  - [x] SubTask 3.1: 新增 `pub fn retry(self_, times: i64, delay_ms: Option<i64>) -> Result<...>`（delay_ms 可选默认 0），负值抛异常
  - [x] SubTask 3.2: 新增 `pub fn __clone(&mut self)`，PhpXhRequest 添加 `#[derive(Clone)]`
  - [x] SubTask 3.3: 新增 `pub fn get_retry(&self) -> Result<ZBox<ZendHashTable>, String>` getter
  - [x] SubTask 3.4: 在 withOptions() 中新增支持 `retry` key（关联数组，times 必填非负，delay_ms 可选默认 0）
- [x] Task 3.5: 修复 HeaderManager 浅拷贝 bug（`Arc<RwLock<HashMap>>` derive Clone 共享内部 HashMap）
  - [x] 手动实现 `impl Clone for HeaderManager`，深拷贝内部 HashMap
  - [x] 此修复确保 `clone $req` 后修改 header 不影响原对象

## 阶段 3：execute() 实现重试循环

- [x] Task 4: 在 `rust/src/php_ext.rs` 的 `execute()` 实现重试循环
  - [x] SubTask 4.1: 预取 retry_times/retry_delay_ms，loop 内每次 clone 请求模板
  - [x] SubTask 4.2: block_on 内循环：首次执行 + 最多 retry_times 次重试
  - [x] SubTask 4.3: 重试条件：execute_single 返回 Err（网络错误，HTTP 4xx/5xx 是 Ok(resp) 不触发）
  - [x] SubTask 4.4: HTTP 错误（4xx/5xx）不重试（Ok(resp) 路径直接返回）
  - [x] SubTask 4.5: 重试间隔用 `tokio::time::sleep(Duration::from_millis(retry_delay_ms))`（delay_ms=0 时跳过）
  - [x] SubTask 4.6: 每次尝试递增 attempts 计数器
  - [x] SubTask 4.7: 结果数组新增 `attempts` 字段（int，1 = 首次未重试）
- [x] Task 5: 在 `rust/src/php_ext.rs` 的 `executeJson()` 透传 attempts
  - [x] SubTask 5.1: executeJson() 内部调用 execute()，结果数组已含 attempts
  - [x] SubTask 5.2: executeJson() 失败异常信息含 attempts（"请求失败（HTTP {}，尝试 {} 次）"）

## 阶段 4：响应数组字段统一

- [x] Task 6: 在所有 HTTP 响应路径新增 `attempts` 字段
  - [x] SubTask 6.1: `result_to_php_array()` 新增 attempts=1（XHMulti/XHThreadPool/协程路径固定为 1）
  - [x] SubTask 6.2: `response_to_php_array()` 新增 attempts=1（成功路径默认）
  - [x] SubTask 6.3: execute() 单请求路径：重试时用实际 attempts 值覆盖默认 1
  - [x] SubTask 6.4: 字段集从 11 扩展到 12（含 attempts）

## 阶段 5：扩展 mock_server 测试基础设施

- [x] Task 7: 在 `rust/tests/mock_server.php` 新增 `/flaky` 与 `/echo-attempts` 端点
  - [x] SubTask 7.1: `/flaky?fail=N` 端点：用文件计数器模拟前 N 次返回 503，第 N+1 次返回 200
  - [x] SubTask 7.2: `/echo-attempts` 端点：回显请求 headers

## 阶段 6：文档更新

- [x] Task 8: 更新 README.md
  - [x] SubTask 8.1: XHRequest 方法表新增 `retry()` 行
  - [x] SubTask 8.2: XHRequest 方法表新增 `getRetry()` 行
  - [x] SubTask 8.3: 结果数组字段表新增 `attempts` 行
  - [x] SubTask 8.4: 新增「请求重试 retry()」小节
  - [x] SubTask 8.5: 新增「请求克隆 clone」小节
  - [x] SubTask 8.6: withOptions 选项 key 表新增 `retry` 行
- [x] Task 9: 升级版本号 1.5.0 → 1.6.0
  - [x] SubTask 9.1: `rust/Cargo.toml` version = "1.6.0"
  - [x] SubTask 9.2: `CHANGELOG.md` 新增 [1.6.0] 条目

## 阶段 7：测试

- [x] Task 10: 创建 `rust/tests/php_add_retry_and_clone_test.php`
  - [x] SubTask 10.1: retry(0) 默认不重试，attempts=1
  - [x] SubTask 10.2: retry(2) + socat /hang → 重试后仍失败，attempts=3
  - [x] SubTask 10.3: retry(2) + 正常请求 → attempts=1
  - [x] SubTask 10.4: retry(3) + /flaky?fail=0 → attempts=1
  - [x] SubTask 10.5: retry(3) + /flaky?fail=2 (503) → 不重试，attempts=1
  - [x] SubTask 10.6: retry(-1) 抛异常
  - [x] SubTask 10.7: retry(1, -100) 抛异常
  - [x] SubTask 10.8: retry(1, 50) + 正常请求 → delay 不影响成功路径
  - [x] SubTask 10.9: executeJson() + retry(2) → 成功
  - [x] SubTask 10.10: clone $req → 修改克隆对象不影响原对象
  - [x] SubTask 10.11: clone $req → 保留所有配置
  - [x] SubTask 10.12: clone 后链式调用正常
  - [x] SubTask 10.13: withOptions(['retry' => [...]]) 等价 retry()
  - [x] SubTask 10.14: withOptions retry 非数组抛异常
  - [x] SubTask 10.15: withOptions retry null 跳过保持原值

## 阶段 8：编译与全套件验证

- [x] Task 11: Rust 侧验证
  - [x] SubTask 11.1: `cargo fmt --check` 通过
  - [x] SubTask 11.2: `cargo clippy --all-targets --features php -- -D warnings` 通过
  - [x] SubTask 11.3: `cargo test --lib --features php` 98 用例通过
- [x] Task 12: 编译 release .so 并同步到 PHP 扩展目录
  - [x] SubTask 12.1: `cargo build --release --features php`
  - [x] SubTask 12.2: `cp target/release/libxhcurl.so` 到 PHP 扩展目录
  - [x] SubTask 12.3: `php -d extension=xhcurl -r 'echo XHCurl::version();'` 输出 `1.6.0`
- [x] Task 13: 运行新测试文件
  - [x] SubTask 13.1: 启动 socat(18400) + mock_server(18399)
  - [x] SubTask 13.2: `php -d extension=xhcurl rust/tests/php_add_retry_and_clone_test.php` 全通过（17/17）
- [x] Task 14: 运行全套件 26 个 PHP 测试文件
  - [x] SubTask 14.1: 串行运行所有 `rust/tests/php_*.php`（含本轮新增 1 个，共 26 个）
  - [x] SubTask 14.2: 确认全部 PASS（EXIT_CODE=0）

# Task Dependencies

- Task 1 → Task 2（setter 依赖字段定义）
- Task 1, 2 → Task 3（PHP 绑定依赖 Rust 实现）
- Task 3 → Task 4（execute 重试循环依赖 retry setter 与字段）
- Task 4 → Task 5（executeJson 依赖 execute 逻辑）
- Task 4 → Task 6（结果数组字段统一依赖 attempts 值来源）
- Task 7（mock_server）独立，可与 Task 1-6 并行
- Task 1-7 → Task 8（文档依赖实现完成）
- Task 8 → Task 9（CHANGELOG 依赖文档与实现完成）
- Task 1-7 全部 → Task 10（测试依赖实现完成）
- Task 10 → Task 11, 12, 13, 14（编译验证依赖测试代码就绪）
