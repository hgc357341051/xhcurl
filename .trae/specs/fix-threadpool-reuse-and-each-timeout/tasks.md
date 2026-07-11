# Tasks

- [x] Task 1: pool 复用时检测配置变更并重建
  - [x] SubTask 1.1: 在 `rust/src/php_ext.rs::PhpXhThreadPool` 中新增配置指纹记录（记录 pool 创建时的 `max_concurrency`/`max_response_size` 值，可用两个 `usize` 字段或一个 hash）
  - [x] SubTask 1.2: 在 `execute()` 中，当 `pool.is_some()` 时比对当前配置与 pool 创建时的指纹，不一致则 drop 旧 pool 重建（用新配置）
  - [x] SubTask 1.3: 在 `execute_each()` 中同样处理
  - [x] SubTask 1.4: 测试：`execute()` → `maxConcurrency(16)` → `execute()`，第二次并发数生效（可通过 worker_count 间接验证或行为差异验证）
  - [x] SubTask 1.5: 测试：未修改配置时 pool 复用不重建（无异常、行为一致）

- [x] Task 2: XHThreadPool::execute_each 强制 timeout
  - [x] SubTask 2.1: 在 `rust/src/php_ext.rs::PhpXhThreadPool::execute_each()` 中，删除 `_timeout` 未用代码，改为计算 deadline（`Instant::now() + Duration::from_secs(timeout)`），当 timeout > 0 时
  - [x] SubTask 2.2: 在非流式分支的 `result_rx.recv().await` 处用 `tokio::time::timeout(remaining, result_rx.recv())` 包裹，超时后 drop pool（中止任务）并返回错误（含已处理数）
  - [x] SubTask 2.3: 在流式分支的 `select!` 中加 `tokio::time::sleep_until(deadline)` 分支，超时后中止
  - [x] SubTask 2.4: 测试：`timeout(2)` + `/hang` 请求 + `executeEach`，约 2 秒后返回错误（elapsed < 10s）

- [x] Task 3: 负值改为抛异常
  - [x] SubTask 3.1: 修改 `PhpXhMulti::timeout()`/`max_response_size()`/`max_concurrency()`，负值返回 `Err`（0 保持合法）
  - [x] SubTask 3.2: 修改 `PhpXhThreadPool::timeout()`/`max_response_size()`/`max_concurrency()`，负值返回 `Err`（0 保持合法）
  - [x] SubTask 3.3: 错误消息含字段名和合法范围提示（如 "timeout 不能为负值，0 = 无超时"）
  - [x] SubTask 3.4: 测试：`timeout(-1)`/`maxResponseSize(-1)`/`maxConcurrency(-1)` 抛异常
  - [x] SubTask 3.5: 回归测试：`timeout(0)`/`maxResponseSize(0)`/`maxConcurrency(0)` 不抛异常

- [x] Task 4: README 文档更新
  - [x] SubTask 4.1: 更新 XHThreadPool 方法说明，注明"execute 间修改配置会重建线程池生效"
  - [x] SubTask 4.2: 注明"executeEach 也强制 timeout"（移除"仅 execute 生效"的旧说明）
  - [x] SubTask 4.3: 注明"负值抛异常，0 = 无超时/使用默认"

- [x] Task 5: 验证
  - [x] SubTask 5.1: `cargo fmt --check`
  - [x] SubTask 5.2: `cargo clippy --features php -- -D warnings`
  - [x] SubTask 5.3: `cargo test --lib --features php`
  - [x] SubTask 5.4: `cargo build --features php` 编译成功
  - [x] SubTask 5.5: 启动 mock 服务器，运行新增测试（17）+ 第四轮测试（38）+ 回归测试（64+31+28）全通过

# Task Dependencies
- Task 1（pool 重建）独立
- Task 2（executeEach timeout）独立
- Task 3（负值抛异常）独立
- Task 4（文档）依赖 Task 1-3 完成
- Task 5（验证）依赖所有前序任务完成
- Task 1-3 可并行
