# Tasks

## Phase 1: 错误处理统一化（用户最直接感知）

- [ ] Task 1: `XHRequest::execute()` 统一返回结果数组（不抛异常）
  - [ ] SubTask 1.1: `php_ext.rs` 中 `execute()` 将 `execute_single` 的 `Err` 包装为 `success=false` 结果数组（参考 `fiber.rs::execute_http_task` 的处理方式）
  - [ ] SubTask 1.2: 失败时返回 `result_to_php_array` 路径（含 status:0、body:""、error 字段）
  - [ ] SubTask 1.3: 添加测试验证网络错误时返回 `success=false` 而非抛异常

- [ ] Task 2: `global_client()`/`global_runtime()` 不再 panic
  - [ ] SubTask 2.1: 改为返回 `Result<&'static reqwest::Client, String>` / `Result<&'static tokio::runtime::Runtime, String>`
  - [ ] SubTask 2.2: 所有调用方（execute/await/gather/each/multi/threadpool）传播错误为 PHP 异常
  - [ ] SubTask 2.3: `setConfig` 中提前校验代理格式（fail-fast，避免延迟到首次请求才报错）

- [ ] Task 3: `RwLock.unwrap()` 统一为 `unwrap_or_else(|e| e.into_inner())`
  - [ ] SubTask 3.1: `curl.rs` 7 处 unwrap 改为 unwrap_or_else
  - [ ] SubTask 3.2: `header.rs` 7 处 unwrap 改为 unwrap_or_else（保持与 `get()` 的 `.ok()?` 一致风格）

- [ ] Task 4: `fiber.rs` 的 `.expect()` 改为 `?` 传播错误
  - [ ] SubTask 4.1: 5 处 `.expect("调度器未初始化")` 改为 `ok_or_else(|| "...".to_string())?`
  - [ ] SubTask 4.2: `PhpXhMulti::execute`/`execute_each` 的 `.expect("线程池已初始化")` 改为 `?`（2 处）

## Phase 2: 失败路径字段完整性

- [ ] Task 5: `result_to_php_array` 失败分支补齐字段
  - [ ] SubTask 5.1: 失败分支插入 `headers => []`、`body_size => 0`、`url => ""`
  - [ ] SubTask 5.2: 添加测试验证失败路径字段集与成功路径一致

- [ ] Task 6: `getConfig()` 的 `proxy` 为 None 时插入 null
  - [ ] SubTask 6.1: `php_ext.rs` getConfig 中 proxy 分支改为始终插入（None 时插 null）
  - [ ] SubTask 6.2: 验证 `setConfig(['proxy' => null])` 后 `getConfig()['proxy']` 为 null

## Phase 3: API 补全与文档对齐

- [ ] Task 7: 新增 `XHRequest::options()` 快捷方法
  - [ ] SubTask 7.1: `php_ext.rs` PhpXhRequest impl 新增 `options()` 方法（与 get/post 一致）
  - [ ] SubTask 7.2: README 方法表补充 `options()` 行

- [ ] Task 8: README 文档对齐
  - [ ] SubTask 8.1: setConfig 示例和配置说明补 `http2_enabled => true`
  - [ ] SubTask 8.2: 说明 `execute()` 错误处理语义（统一返回结果数组，检查 success）
  - [ ] SubTask 8.3: 修正 `body()` 签名为 `body(string $data)`
  - [ ] SubTask 8.4: 故障排查补充请求超时、代理无效、响应体超限条目

## Phase 4: CI 质量保障

- [ ] Task 9: CI clippy/test 启用 `--features php`
  - [ ] SubTask 9.1: `build-rust.yml` lint-and-test job 安装 PHP 开发头文件
  - [ ] SubTask 9.2: clippy 改为 `cargo clippy --all-targets --features php -- -D warnings`
  - [ ] SubTask 9.3: test 改为 `cargo test --lib --features php`

- [ ] Task 10: CI 扩展加载验证有效
  - [ ] SubTask 10.1: 移除 "Verify extension loads" 的 `|| true`
  - [ ] SubTask 10.2: 改为断言式验证（`php -d extension=xhcurl -r "echo XHCurl::version();"` 失败时 exit 1）

- [ ] Task 11: CI 增加 PHP 测试套件执行
  - [ ] SubTask 11.1: build-linux job 编译扩展后运行 `rust/tests/php_*.php`
  - [ ] SubTask 11.2: 测试失败时 CI 红

- [ ] Task 12: CI 注释修正
  - [ ] SubTask 12.1: macOS PHP 版本注释改为 8.1~8.5

## Phase 5: 测试改进

- [ ] Task 13: `test_drop_aborts_tasks` 改为真测试
  - [ ] SubTask 13.1: 添加请求 + spawn_all 后 drop，用原子计数器验证任务被 abort

- [ ] Task 14: `test_global_manager_config` 避免触碰全局单例
  - [ ] SubTask 14.1: 改用 `XhCurlManager::new(GlobalConfig::default())` 独立实例

## Phase 6: 验证与提交

- [ ] Task 15: 运行完整验证流水线
  - [ ] SubTask 15.1: `cargo fmt --check`
  - [ ] SubTask 15.2: `cargo clippy --all-targets --features php -- -D warnings`
  - [ ] SubTask 15.3: `cargo test --lib --features php`
  - [ ] SubTask 15.4: PHP 运行时冒烟测试（execute 网络错误返回 success=false、options()、getConfig proxy null、失败路径字段完整）

- [ ] Task 16: 更新 CHANGELOG 并提交
  - [ ] SubTask 16.1: CHANGELOG 新增 `[1.0.7]` 条目
  - [ ] SubTask 16.2: Cargo.toml 版本 → 1.0.7
  - [ ] SubTask 16.3: git commit + tag v1.0.7

# Task Dependencies

- Task 1（execute 错误处理）独立，改 php_ext.rs
- Task 2（global_client/runtime）改 php_ext.rs，与 Task 1 同文件建议顺序执行
- Task 3（RwLock）改 curl.rs/header.rs，独立可并行
- Task 4（fiber expect）改 fiber.rs + php_ext.rs，独立可并行
- Task 5（失败字段）改 php_ext.rs result_to_php_array，与 Task 1 同函数附近，建议 Task 1 后做
- Task 6（getConfig proxy）改 php_ext.rs，独立
- Task 7（options()）改 php_ext.rs，独立
- Task 8（README）纯文档，完全独立，可并行
- Task 9-12（CI）改 build-rust.yml，独立可并行
- Task 13-14（测试改进）独立可并行
- Task 15（验证）依赖所有前序任务
- Task 16（提交）依赖 Task 15 通过
