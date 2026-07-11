# Changelog

本项目所有重要变更均记录于此文件。格式参考 [Keep a Changelog](https://keepachangelog.com/zh-CN/)，
版本号遵循 [语义化版本](https://semver.org/lang/zh-CN/)。

## [Unreleased]


## [1.0.8] - 2026-07-11

本版本新增**响应体分块流式回调**（`onChunk`/`onHeaders`），使 PHP 用户能在 HTTP 请求过程中
实时处理响应体数据块和响应头，无需等待整个请求完成。适用于大文件流式下载、SSE、NDJSON 流式解析。

### 新增
- **`XHMulti::executeEach` 支持 `onChunk`/`onHeaders`**：新增两个可选参数
  `?callable $onChunk = null` 和 `?callable $onHeaders = null`（向后兼容，已有代码不受影响）。
  - `$onChunk(string $requestId, string $chunk): void` —— 每收到一块响应体时触发（二进制安全）
  - `$onHeaders(string $requestId, int $status, array $headers): void` —— 收到响应头时触发
  - 所有 chunk 拼接后等于完整响应体（与 `$result['body']` 一致）
- **`XHThreadPool::executeEach` 同步支持 `onChunk`/`onHeaders`**：签名与 `XHMulti::executeEach` 一致。
- **核心层流式能力暴露**：`StreamEvent`（Headers/Chunk/Complete/Error）通过线程安全 mpsc channel
  从 tokio 工作线程传递到 PHP 线程，PHP 回调仅在 `block_on` 当前线程调用，确保线程安全。
- **`mock_server.php` 新增 `/stream` 端点**：分块输出大响应体（`flush()` 确保分段发送），
  供 `onChunk` 多次触发验证。参数：`n`（段数）、`size`（每段字节数）。
- **`php_streaming_test.php` 新增**：28 项测试覆盖 onChunk/onHeaders 触发、chunk 拼接完整性、
  XHMulti 和 XHThreadPool 两条路径、向后兼容回归、回调异常中止。

### 增强
- **流式事件 drain 机制**：主收集循环结束后 `try_recv` 排空 stream channel 残留事件，
  确保用户回调收到完整的分块数据（避免尾部 chunk 丢失）。
- **null 参数处理**：`Option<&Zval>` 参数正确处理 PHP `null`（视为未传），用户可显式传 `null`
  跳过 `onChunk` 只用 `onHeaders`。

### 文档
- README 核心特性表新增"流式回调"行。
- 新增"流式回调类型"小节，明确区分请求级（`onResult`）与响应体分块级（`onChunk`/`onHeaders`）。
- 更新 `executeEach` 签名表（XHMulti + XHThreadPool）。
- 补充 `onChunk`/`onHeaders` 使用示例。
- `each()` 章节澄清协程仅支持请求级流式。
- 故障排查新增"流式回调不触发"条目。


## [1.0.7] - 2026-07-11

本版本聚焦**错误处理健壮性与 CI 质量保障**：消除所有 panic 路径（改返回 PHP 异常或结果数组）、
统一失败路径字段、CI 启用 `--features php` 全量检查。无破坏性变更。

### 修复
- **`execute()` 统一返回结果数组**：网络/DNS/TLS 错误原抛 PHP 异常，与 `XHMulti`/fiber 路径不一致。
  现包装为 `success=false` 结果数组（含 `status: 0`、`error` 字段），用户统一检查 `$r['success']` 即可。
- **`global_client()`/`global_runtime()` 不再 panic**：初始化失败（如代理无效）原 `expect` 直接
  panic 杀死 PHP 进程（FPM worker 崩溃重启）。改为返回 `Result`，错误以 PHP 异常形式抛出，
  用户可 try/catch 并修正配置后重试。
- **RwLock 中毒恢复**：`curl.rs`（7 处）和 `header.rs`（8 处）的 `.read().unwrap()`/`.write().unwrap()`
  在锁中毒时 panic。改为 `unwrap_or_else(|e| e.into_inner())`，取中毒锁中的数据继续执行，避免 panic。
- **fiber.rs `expect` 改为优雅传播**：5 处 `.expect("调度器未初始化")` 改为 `if let Some` + 提前返回错误；
  `XHThreadPool::execute`/`execute_each` 的 `.expect("线程池已初始化")` 改为 `?` 传播。
- **失败路径字段补齐**：`result_to_php_array` 失败分支原仅有 `status/elapsed_ms/body/error`，
  补充 `headers => []`、`body_size => 0`、`url => ""`，确保失败路径字段集与成功路径完全一致。
- **`setConfig` 接受 null proxy**：`getConfig()` 返回 `proxy => null` 后 `setConfig($orig)` 往返
  报类型不匹配错误。`setConfig` 现接受 null（视为清除代理），与 `getConfig` 对称。

### 增强
- **`getConfig()` 的 `proxy` 始终返回**：原 proxy 为 None 时 `getConfig()` 不含 `proxy` 键，
  用户无法区分"未设置"和"获取失败"。现始终插入（None 时为 `null`），与 `setConfig` 接受 `null` 对称。
- **新增 `XHRequest::options()` 快捷方法**：与 `get()`/`post()`/`put()`/`delete()` 等一致，
  补齐 HTTP OPTIONS 方法的链式快捷方法。

### 文档
- README setConfig 示例补充 `http2_enabled => true`。
- 新增"错误处理统一"说明：`execute()` 网络错误返回 `success=false` 而非抛异常。
- 故障排查补充 3 条目：请求超时/连接失败、代理配置无效、响应体超限。

### CI 质量保障
- **clippy/test 启用 `--features php`**：原 CI 未启用 php feature，`php_ext.rs`/`fiber.rs`
  约 2400 行代码不参与编译检查。现 clippy 改为 `--all-targets --features php -- -D warnings`，
  test 改为 `--lib --features php`。
- **扩展加载验证有效化**：移除 `|| true` 容忍失败，改为断言式验证。
- **新增 PHP 测试套件执行**：CI 编译扩展后运行 `rust/tests/php_*.php`。
  新增 `mock_server.php`（PHP 内置服务器）提供 `/get`、`/post`、`/hang` 端点，
  CI 启动后供网络相关测试使用。
- macOS PHP 版本注释修正为 8.1~8.5。

### 测试
- **`test_drop_aborts_tasks` 改为真测试**：原仅验证空 multi 的 Drop 不 panic。改为添加 TEST-NET-1
  请求 + `spawn_all` + drop，用超时保护 `recv()` 验证 channel 关闭（任务被 abort）。
- **`test_global_manager_config` 避免触碰全局单例**：改用 `XhCurlManager::new(GlobalConfig::default())`
  独立实例，避免并行测试间全局状态污染。`XhCurlManager::new` 改为 `pub` 供测试使用。


## [1.0.6] - 2026-07-11

本版本从**使用者视角**全面审查并优化代码与文档，聚焦链式调用体验、API 命名一致性、
配置字段实际生效、失败路径字段完整性与文档对齐实现。无破坏性变更。

### 修复
- **fiber_each 并发上限读取配置**：`fiber_each` 硬编码 `total.min(64)`，导致用户
  `setConfig(['fiber_max_concurrency' => 128])` 后 `gather()` 生效但 `each()` 仍为 64。
  统一读取 `GlobalConfig.fiber_max_concurrency`，与 `gather()` 行为一致。
- **XhMulti 实现 Drop 防任务泄漏**：`XhMulti` 持有 `tasks: Vec<JoinHandle<()>>` 但无 Drop，
  `spawn_all` 后 panic/早期返回时后台任务继续运行泄漏连接。新增 Drop 实现调用 `abort_tasks()`，
  参考 `XhThreadPool` 的 Drop 模式。
- **id 字段默认值统一为 URL**：fiber 路径 `await/gather/each` 默认 `"task-{N}"` 与同步 `execute()`
  默认 URL 不一致。统一为未设置 `setId()` 时默认为请求 URL（与文档一致）。
- **失败响应补 `status: 0` 字段**：请求失败时 `result_to_php_array` 不写 status 字段，用户访问
  `$r['status']` 触发未定义索引警告。补 `status => 0`（哨兵值）和 `body => ""`，确保失败路径
  字段集与成功路径一致。

### 增强
- **链式 setter 统一返回 `&mut Self`**：`method()`/`json()`/`form()`/`multipart()`/`setUserData()`
  原返回 `Result<&mut Self, String>` 破坏链式调用（PHP 端需 `?` 或 `unwrap`）。改为失败时跳过本次
  设置并返回 `&mut Self`，用户可写 `createRequest($url)->get()->json([...])->timeout(10)->execute()`。
- **新增 `id()`/`userData()` 无前缀别名**：与其余 18 个无 `set` 前缀的链式 setter 风格一致。
  保留 `setId`/`setUserData` 旧名为别名，向后兼容。
- **负值处理统一为跳过**：`timeout`/`connectTimeout`/`maxRedirects`/`XHMulti::timeout`/
  `maxConcurrency`/`maxResponseSize`/`XHThreadPool::__construct` 负值原 clamp 到 0（语义混乱），
  统一为跳过本次设置（保留原值），与 `setConfig` 现有行为一致。
- **http2_enabled 实际生效**：`GlobalConfig.http2_enabled` 字段存在但 `create_client_builder` 从不读取，
  用户通过 `setConfig` 以为可配置实则无效。现 `false` 时显式 `.http1_only()` 禁用 HTTP/2，
  `true` 时保持默认协商。

### 重构
- **移除 `use_multi_thread` 死字段**：`GlobalConfig.use_multi_thread` 仅 `default()` 和测试出现，
  `set_config`/`get_config` 未暴露，`create_client_builder` 不读。运行时类型由 `sapi_is_cli()` 决定，
  此字段无实际作用，直接删除。

### 文档
- **README 补全 4 个漏列方法**：`XHCurl::each()`、`XHMulti::timeout()`、`XHMulti::executeEach()`、
  `XHThreadPool::executeEach()`，含签名、回调签名、返回值、示例。
- **修正 FPM/CLI 能力表**：协程 `run/await/gather/each` 仅 CLI 可用（实现中 FPM 显式拒绝），
  README 表格原声称 FPM 支持协程是错误的。协程章节顶部加 CLI-only 警告。
- **响应字段表区分成功/失败路径**：新增"失败路径字段说明"小节，标注 `status` 失败时为 0（哨兵）、
  `body` 为空字符串、`id` 未设置时默认为 URL（所有路径统一）。
- **方法表补充双名格式**：`setId`/`id`、`setUserData`/`userData` 均列出，示例改用新名。
- **故障排查新增 FPM 下调用 run() 报错条目**。

### 测试
- 新增 `test_error_result_response_none_ensures_status_zero_sentinel` 固化失败路径数据契约
  （`response.is_none()` → `status: 0` 哨兵逻辑的前提条件）。
- 请求级 Client 缓存测试改为 `contains_key()` 断言 + `unwrap_or_else` 处理中毒 Mutex，
  确保并行测试安全（原 `len()` 断言在并行运行时因共享全局缓存而 flaky）。
- PHP 运行时冒烟验证：链式调用无需 `?`、`id()`/`userData()` 新别名可用、旧名向后兼容、
  `fiber_max_concurrency` 配置生效、`http2_enabled=false` 生效、负值跳过不 crash、
  失败 `status=0`、`id` 默认为 URL 均通过。


## [1.0.5] - 2026-07-10

本版本完成代码审计剩余 4 项优化（P2.4 / P2.5 / P3.11 / P3.16），无破坏性变更。

### 重构
- **execute_each 代码去重**（P2.4）：抽取 `XhMulti::spawn_all()` / `abort_tasks()` /
  `join_tasks()` 公共方法，`PhpXhMulti::execute_each` 改为委托调用，消除约 130 行与
  `XhMulti::execute` 重复的 spawn/collect 逻辑。`PhpXhThreadPool::execute_each` 因采用
  worker+ResultMessage 模型（不共享 spawn 逻辑）保持不变。
- **请求级 Client 连接复用**（P3.16）：`request.rs::build_request_client` 新增按
  `OverrideKey`（follow_redirects/max_redirects/verify_ssl/proxy/connect_timeout 组合）缓存
  Client 的机制，同类请求复用同一 Client（含连接池），避免每次新建 Client 丢失连接复用。
  `reqwest::Client` 内部为 Arc，clone 廉价。`setConfig()` 变更后通过
  `clear_request_client_cache()` 主动失效缓存。

### 安全
- **xhrun shell 模式参数转义**（P2.5）：`shell => true` 时对每个 arg 按平台转义后再拼接：
  Unix 用单引号包裹 + `'\''` 转义内嵌单引号；Windows 用双引号包裹 + `^` 抑制
  `& | < > ^ ( ) %` 元字符，杜绝命令注入。新增 6 个单元测试覆盖转义逻辑。

### 增强
- **fiber gather/each 并发上限可配置**（P3.11）：`GlobalConfig` 新增
  `fiber_max_concurrency`（默认 64，0 = 不限制），`gather()`/`each()` 读取该配置决定
  Semaphore 容量，与 `XhMulti` 行为一致。可通过
  `XHCurl::setConfig(['fiber_max_concurrency' => N])` 调整，`getConfig()` 同步返回。

### 测试
- 新增 3 个请求级 Client 缓存单元测试（命中/未命中/清空）。
- 新增 6 个 xhrun shell 转义单元测试（Unix/Windows 双平台）。
- PHP 运行时冒烟验证：扩展加载、`fiber_max_concurrency` 配置读写、xhrun shell 转义、
  Client 缓存端到端均通过。

### 文档
- README 补充 `fiber_max_concurrency` 配置项说明。


## [1.0.4] - 2026-07-10

### 安全
- **响应体大小硬上限**：`spawn_output_reader` 使用 `checked_add` + `is_none_or` 防止字节累加
  整数溢出，超过 `max_response_size` 时截断并标记，防止恶意超大响应导致 OOM。

### 修复
- **fiber_await 上下文校验前置**：`XHCurl::await()` 在 spawn HTTP 任务前即校验是否处于
  Fiber 上下文，避免无谓的 tokio 任务创建后才报错。
- **execute_all 陈旧结果清理**：`XhThreadPool::execute_all` 在提交请求前 drain 上次调用
  残留的 `result_rx` 消息，避免陈旧结果污染本次返回（已修复 drain 位置错放导致测试挂起）。
- **task_handles 无界增长**：`run_event_loop` 单次轮询结束后 `retain(|h| !h.is_finished())`
  回收已完成 tokio 任务句柄，防止长轮询场景句柄数组无限增长。
- **get_config 缺字段**：`getConfig()` 补充返回 `tcp_keepalive_interval`，与 `setConfig`
  可配置项对齐。
- **extract_requests 提前检查上限**：在克隆元素前用 `len()` 检查
  `MAX_REQUESTS_PER_BATCH`，避免先克隆全部元素再拒绝导致内存浪费/OOM。
- **setConfig 类型不匹配静默忽略**：12 个配置项收集类型不匹配项，统一返回错误信息，
  而非静默忽略错误配置。
- **负数配置项处理**：`connect_timeout`/`request_timeout`/`max_response_size`/
  `max_redirects` 等数值配置项负值被跳过（保留原值），不触发类型转换异常或 panic。

### 重构
- **PhpXhResponse 死代码移除**：删除从未对外暴露的 `PhpXhResponse` 类及 4 个仅被其调用的
  `json_*` helper 函数（`json_to_php_array` 等）。所有 PHP API 仍直接返回关联数组。
- **迭代风格统一**：`extract_requests`/`opt_string_vec` 等手动 for 循环改用项目已有的
  `for_each_kv` 辅助函数，规避 ext-php-rs `Iter` 终止路径的空指针风险。

### 文档
- README 补充 `tcp_keepalive_interval` / `max_connections` 配置项。
- README 更新 `setConfig` 类型校验与负数处理说明。
- README 移除已删除的 `XHResponse` 内部类型说明。
- 新增本 CHANGELOG。

### 测试
- 新增 4 项 PHP 测试：`test_get_config_has_tcp_keepalive_interval`、
  `test_oversized_array_rejected_before_clone`、
  `test_set_config_wrong_type_returns_error`、`test_set_config_correct_type_applies`，
  PHP 测试套件总计 39 passed。
