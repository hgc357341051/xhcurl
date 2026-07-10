# 批量上限与配置一致性修复 Spec

## Why

第三轮审查发现 2 个高危缺陷：(1) `XHCurl::gather/each` 路径通过 `extract_requests` 解析请求数组，但完全没有 `MAX_REQUESTS_PER_BATCH` 检查（`XHMulti::add`/`XHThreadPool::add` 都有），用户传入超大数组会导致全部 clone + spawn，OOM 风险；(2) `GlobalConfig.max_response_size` 注释写"0 表示无限制"，但 `executor.rs` 将 0 当作"0 字节上限"，导致用户设置 0 后 fiber/单请求路径所有非空响应都失败，而 Multi 路径因归一化为 DEFAULT 不受影响——配置语义在三条路径间不一致。

## What Changes

### P0:为 gather/each 路径添加 MAX_REQUESTS_PER_BATCH 检查

- 在 `extract_requests`（php_ext.rs:114）中，解析完成后检查 `requests.len() > MAX_REQUESTS_PER_BATCH`，超过则返回错误。
- 这样 `XHCurl::gather`、`XHCurl::each`、`XHCurl::await`（单个不触发，但走同一函数）都受保护，与 `XHMulti::add`/`XHThreadPool::add` 的上限一致（10000）。

### P1:统一 max_response_size=0 的语义为"无限制"

- 修正 `executor.rs` 的 `collect_response_body`：当 `max_response_size == 0` 时视为无限制（跳过大小检查），与 `curl.rs:27` 注释"0 表示无限制"一致。
- 修正 `curl.rs:27` 注释保持不变（已正确）。
- 不改动 Multi 路径的归一化逻辑（`max_response_size(0)` → DEFAULT，这是 builder API 的合理行为，与全局配置语义不冲突）。
- 修正 fiber.rs:666 和 php_ext.rs:844 读取全局配置后的归一化：当全局 `max_response_size == 0` 时不归一化（传 0 给 executor，executor 视为无限制）。

## Impact

- 受影响文件：
  - `rust/src/php_ext.rs`（P0: extract_requests 加上限检查）
  - `rust/src/executor.rs`（P1: collect_response_body 当 max_response_size==0 时跳过大小检查）
  - `rust/tests/`（P0/P1 新增测试）
- 不改动：fiber.rs、curl.rs、multi.rs、threadpool.rs、response.rs
- 不影响现有 API 签名
- P0 修复后：gather/each 传入超过 10000 个请求时返回明确错误（之前会全部 spawn 导致 OOM）
- P1 修复后：用户设置 `max_response_size=0` 表示无限制，三条路径行为一致（之前 fiber/单请求路径会全部失败）

## ADDED Requirements

### Requirement: gather/each 批量上限检查

`extract_requests` SHALL 在解析请求数组后检查 `requests.len() > MAX_REQUESTS_PER_BATCH`，超过时返回错误，与 `XHMulti::add`/`XHThreadPool::add` 的上限保护一致。

#### Scenario: 超过上限返回错误
- **WHEN** 用户调用 `XHCurl::gather()` 传入 10001 个请求
- **THEN** 返回错误信息说明上限为 10000
- **AND** 不执行任何 spawn（避免 OOM）

#### Scenario: 正常数量通过
- **WHEN** 用户传入 10000 个请求（等于上限）
- **THEN** 正常执行

### Requirement: max_response_size=0 表示无限制

`collect_response_body` SHALL 将 `max_response_size == 0` 视为无限制（跳过大小检查），与 `GlobalConfig.max_response_size` 注释"0 表示无限制"一致。

#### Scenario: 设置 0 后大响应不报错
- **WHEN** 用户调用 `XHCurl::setConfig(['max_response_size' => 0])`
- **AND** 发起请求收到 1MB 响应
- **THEN** 请求成功（不报"响应体超过最大限制 0 字节"）

#### Scenario: 三条路径行为一致
- **WHEN** 全局配置 `max_response_size=0`
- **THEN** fiber 路径（await/gather/each）、单请求（execute）、Multi 路径都不因大小限制失败

## MODIFIED Requirements

### Requirement: extract_requests 上限保护

`extract_requests` 解析后 SHALL 检查请求数量不超过 `MAX_REQUESTS_PER_BATCH`（10000），超过返回错误。

### Requirement: collect_response_body 大小检查

`collect_response_body` 当 `max_response_size == 0` 时 SHALL 跳过大小检查（无限制），仅当 `max_response_size > 0` 时执行大小限制检查。
