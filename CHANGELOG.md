# Changelog

本项目所有重要变更均记录于此文件。格式参考 [Keep a Changelog](https://keepachangelog.com/zh-CN/)，
版本号遵循 [语义化版本](https://semver.org/lang/zh-CN/)。

## [Unreleased]


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
