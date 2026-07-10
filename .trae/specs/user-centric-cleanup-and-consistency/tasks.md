# Tasks

## Phase 1: 文档对齐实现（用户最先感知）

- [x] Task 1: README 补全 4 个漏列方法文档
  - [x] SubTask 1.1: 在 XHCurl 方法表（README:329-338）补充 `each(array $requests, callable $callback): int` 行
  - [x] SubTask 1.2: 在 XHMulti 方法表（README:475-481）补充 `timeout(int $seconds): self` 和 `executeEach(callable $callback): int` 行
  - [x] SubTask 1.3: 在 XHThreadPool 方法表（README:499-503）补充 `executeEach(callable $callback): int` 行
  - [x] SubTask 1.4: 在协程章节补充 `each()` 用法示例（对比 `gather()` 累积返回 vs `each()` 流式回调）
  - [x] SubTask 1.5: 补充 `executeEach`/`each` 回调签名说明 `function(array $result): void`，返回值为处理结果总数

- [x] Task 2: 修正 README FPM/CLI 能力表与协程章节限制说明
  - [x] SubTask 2.1: 更正 README FPM/CLI 表（行 660-663）：协程 `run/await/gather/each` 仅 CLI 可用
  - [x] SubTask 2.2: 在协程章节顶部（README:217 附近）加 CLI-only 警告 blockquote
  - [x] SubTask 2.3: 在 README 故障排查章节（行 679-712）补充"FPM 下调用 run() 报错"条目

- [x] Task 3: README 响应字段表区分成功/失败路径
  - [x] SubTask 3.1: 在响应字段表（README:448-466）补充"失败时（success=false）字段说明"小节
  - [x] SubTask 3.2: 标注 `status` 失败时为 0（哨兵）、`body` 失败时为空字符串、`status/body_size/headers/url/remote_addr/version` 失败时存在但为默认值
  - [x] SubTask 3.3: 修正 `id` 字段说明（README:455）：未设置时默认为 URL（所有路径统一）

## Phase 2: Bug 修复（用户可感知的行为差异）

- [x] Task 4: 修复 fiber_each 并发上限硬编码 bug
  - [x] SubTask 4.1: 在 `fiber.rs` 找到 `fiber_each` 中 `total.min(64)` 硬编码处
  - [x] SubTask 4.2: 改为读取 `XhCurlManager::global().config().fiber_max_concurrency`，与 `fiber_gather` 一致
  - [x] SubTask 4.3: 添加单测验证配置生效（若可测）

- [x] Task 5: 为 XhMulti 实现 Drop 防止任务泄漏
  - [x] SubTask 5.1: 在 `multi.rs` 的 `XhMulti` struct 上实现 `Drop`
  - [x] SubTask 5.2: Drop 实现中调用 `self.abort_tasks()`（即 `for handle in self.tasks.drain(..) { handle.abort(); }`）
  - [x] SubTask 5.3: 添加测试验证 drop 后任务被 abort

- [x] Task 6: 统一 id 字段默认值为 URL
  - [x] SubTask 6.1: 在 `fiber.rs` 的 `execute_http_task`（约 677-680 行）将 `task-{task_id}` fallback 改为 `request.get_url()`
  - [x] SubTask 6.2: 验证 `await/gather/each` 返回的 `id` 默认为 URL

## Phase 3: API 一致性（链式调用体验）

- [x] Task 7: 链式 setter 返回类型统一为 &mut Self
  - [x] SubTask 7.1: `php_ext.rs` 中 `method()` 改为失败时跳过设置返回 `&mut Self`（而非 `Result<&mut Self, String>`）
  - [x] SubTask 7.2: `json()` 同上改造
  - [x] SubTask 7.3: `form()` 改为直接返回 `&mut Self`（本就不会失败）
  - [x] SubTask 7.4: `multipart()` 改为失败时跳过返回 `&mut Self`
  - [x] SubTask 7.5: `setUserData()` 改为失败时跳过返回 `&mut Self`
  - [x] SubTask 7.6: 验证 PHP 端链式 `->get()->json([...])->timeout(10)->execute()` 无需 `?`

- [x] Task 8: 移除 set 前缀，新增无前缀方法并保留旧名为别名
  - [x] SubTask 8.1: `php_ext.rs` 新增 `id(string)` 方法（与 `setId` 等价）
  - [x] SubTask 8.2: `php_ext.rs` 新增 `userData(array)` 方法（与 `setUserData` 等价）
  - [x] SubTask 8.3: 保留 `setId`/`setUserData` 不删除（向后兼容）
  - [x] SubTask 8.4: README 更新示例用新名 `id()`/`userData()`

- [x] Task 9: 负值处理统一为"跳过+保留原值"
  - [x] SubTask 9.1: `php_ext.rs` 中 `XHRequest::timeout(-1)` 等改为跳过（不 clamp 到 0）
  - [x] SubTask 9.2: `XHMulti::timeout/maxConcurrency/maxResponseSize` 负值改为跳过
  - [x] SubTask 9.3: `XHThreadPool::__construct` 负值改为跳过（用默认）
  - [x] SubTask 9.4: 验证与 `setConfig` 现有"负值跳过"行为一致

## Phase 4: 死配置清理与字段一致性

- [x] Task 10: http2_enabled 实际生效
  - [x] SubTask 10.1: `curl.rs::create_client_builder` 读取 `http2_enabled`：`false` 时 `.http1_only()`，`true` 时保持默认
  - [x] SubTask 10.2: 验证 `setConfig(['http2_enabled' => false])` 后请求走 HTTP/1.1

- [x] Task 11: 移除 use_multi_thread 死字段
  - [x] SubTask 11.1: `curl.rs` 删除 `GlobalConfig.use_multi_thread` 字段
  - [x] SubTask 11.2: 删除 `default()` 中的初始化
  - [x] SubTask 11.3: 检查并清理任何引用（测试中的 `assert!(config.use_multi_thread)`）

- [x] Task 12: 失败响应补 status: 0 字段
  - [x] SubTask 12.1: `php_ext.rs::result_to_php_array`（约 1496-1504 行）失败路径补 `status => 0`
  - [x] SubTask 12.2: 确保成功/失败路径字段集一致（都有 status/body）
  - [x] SubTask 12.3: 添加测试验证失败时 `status` 字段存在且为 0

## Phase 5: 验证与文档收尾

- [x] Task 13: 运行完整验证流水线
  - [x] SubTask 13.1: `cargo fmt --check`
  - [x] SubTask 13.2: `cargo clippy -- -D warnings`（非 php）
  - [x] SubTask 13.3: `cargo clippy --all-targets --features php -- -D warnings`
  - [x] SubTask 13.4: `cargo test --lib`
  - [x] SubTask 13.5: PHP 运行时冒烟测试（扩展加载、链式调用、each 配置、id 默认值、失败 status 字段）

- [x] Task 14: 更新 CHANGELOG 并提交
  - [x] SubTask 14.1: CHANGELOG 新增 `[1.0.6]` 条目记录本次变更
  - [x] SubTask 14.2: 跑 fmt 后 git commit
  - [ ] SubTask 14.3: （可选）打 tag v1.0.6 并推送

# Task Dependencies

- Task 4（fiber_each bug）独立，可并行
- Task 5（XhMulti Drop）独立，可并行
- Task 6（id 默认值）独立，可并行
- Task 7（链式返回类型）与 Task 8（set 前缀）都改 php_ext.rs 的 XHRequest impl，建议顺序执行避免冲突
- Task 9（负值处理）改 php_ext.rs 多处 setter，与 Task 7/8 有文件冲突风险，建议 Task 7/8 完成后再做
- Task 10/11（死配置）改 curl.rs，独立于 php_ext.rs，可并行
- Task 12（status 字段）改 php_ext.rs 的 result_to_php_array，与 Task 7/8/9 不同函数，可并行
- Task 1/2/3（文档）纯 README，完全独立，可并行
- Task 13（验证）依赖所有前序任务完成
- Task 14（提交）依赖 Task 13 通过
