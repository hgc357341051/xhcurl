# 验证检查清单

## retry() 方法

- [x] XhRequest 新增 `retry_times: u32` + `retry_delay_ms: u64` 字段
- [x] `retry(int $times, int $delay_ms = 0): $this` setter 实现（delay_ms 可选默认 0）
- [x] `retry(0)` 默认不重试（保持 v1.5.0 行为）
- [x] `retry(-1)` 抛异常（fail-fast）
- [x] `retry(1, -100)` 抛异常（delay_ms 负值）
- [x] `getRetry(): array` 返回 `['times' => int, 'delay_ms' => int]`
- [x] retry 配置变更不触发 Client 重建（retry 是请求级，不影响 Client）

## execute() 重试循环

- [x] 重试条件：execute_single 返回 Err（网络错误，status==0）
- [x] HTTP 错误（4xx/5xx，Ok(resp)）不重试
- [x] 重试间隔 `tokio::time::sleep`（delay_ms=0 时跳过）
- [x] 重试次数 = retry_times，总尝试次数 = retry_times + 1
- [x] 结果数组新增 `attempts` 字段（1 = 首次未重试）
- [x] 成功路径 attempts = 1（无失败无需重试）
- [x] executeJson() 透传 attempts（失败异常含 attempts 信息）

## __clone() 魔术方法

- [x] `clone $req` 不抛异常
- [x] 克隆对象与原对象独立（修改克隆不影响原对象）
- [x] 克隆保留所有配置（headers/body/timeout/retry/query 等）
- [x] 克隆后可继续链式调用
- [x] 修复 HeaderManager 浅拷贝 bug（手动实现 Clone 深拷贝内部 HashMap）

## withOptions 集成

- [x] `withOptions(['retry' => ['times' => 2, 'delay_ms' => 100]])` 等价于 `retry(2, 100)`
- [x] `withOptions(['retry' => ['times' => 2]])` delay_ms 默认 0
- [x] `withOptions(['retry' => ['times' => -1]])` 抛异常
- [x] `withOptions(['retry' => 'invalid'])` 抛异常（非数组）
- [x] `withOptions(['retry' => null])` 跳过（保持原值）

## 结果数组字段统一

- [x] execute() 结果含 attempts 字段
- [x] XHMulti::execute() 结果含 attempts=1（固定值）
- [x] XHThreadPool::execute() 结果含 attempts=1（固定值）
- [x] 协程 gather/each 结果含 attempts=1（固定值）
- [x] 字段集从 11 扩展到 12

## mock_server 新端点

- [x] `/flaky?fail=N` 端点：前 N 次返回 503，第 N+1 次返回 200
- [x] `/echo-attempts` 端点：回显请求 headers

## README 文档

- [x] XHRequest 方法表新增 `retry()` 行
- [x] XHRequest 方法表新增 `getRetry()` 行
- [x] 结果数组字段表新增 `attempts` 行
- [x] 新增「请求重试 retry()」小节
- [x] 新增「请求克隆 clone」小节
- [x] withOptions 选项 key 表新增 `retry` 行

## 版本与 CHANGELOG

- [x] `rust/Cargo.toml` version = "1.6.0"
- [x] `CHANGELOG.md` 包含 [1.6.0] 条目

## 测试覆盖

- [x] retry(0) 默认不重试，attempts=1
- [x] retry(2) + 网络错误 → 重试后仍失败，attempts=3
- [x] retry(2) + 正常请求 → attempts=1
- [x] retry(3) + /flaky?fail=2 (503) → 不重试，attempts=1
- [x] retry(-1) 抛异常
- [x] retry(1, -100) 抛异常
- [x] retry(1, 50) + 正常请求 → delay 不影响成功路径
- [x] executeJson() + retry(2) → 成功
- [x] clone $req → 独立修改不影响原对象
- [x] clone $req → 保留所有配置
- [x] clone 后链式调用正常
- [x] withOptions(['retry' => [...]]) 等价 retry()
- [x] withOptions retry 非数组抛异常
- [x] withOptions retry null 跳过保持原值

## 编译与运行

- [x] `cargo fmt --check` 通过
- [x] `cargo clippy --all-targets --features php -- -D warnings` 通过
- [x] `cargo test --lib --features php` 98 用例通过
- [x] `cargo build --release --features php` 成功
- [x] .so 已同步到 PHP 扩展目录
- [x] `php -d extension=xhcurl -r 'echo XHCurl::version();'` 输出 `1.6.0`

## 测试套件

- [x] `rust/tests/php_add_retry_and_clone_test.php` 创建并全部通过（17/17）
- [x] 全部 26 个 PHP 测试文件 PASS（含本轮新增 1 个，EXIT_CODE=0）
