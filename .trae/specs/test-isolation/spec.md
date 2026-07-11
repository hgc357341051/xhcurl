# 测试基础设施隔离 Spec

## Why

当前 CI 在 `Run PHP tests` 步骤用单个 PHP 内置服务器（`php -S 127.0.0.1:18399 mock_server.php`）承载所有测试。`/hang` 端点 `sleep(60)` 会阻塞单进程服务器，导致后续测试文件的请求挂起或失败，CI 偶发失败，阻塞 GitHub Actions 自动发布流水线。

独立运行各测试文件均 100% 通过，证实是测试基础设施问题，与代码改动无关。

## What Changes

### 修改 CI 脚本的 PHP 测试步骤
- `mock_server.php` 只保留非阻塞端点（`/get`/`/post`/`/cookies`/`/stream`）
- `/hang` 端点改用独立的 socat/python 进程承载（`sleep` 不阻塞 mock_server）
- 每个 PHP 测试文件运行前重启 mock_server，确保干净环境（彻底隔离）
- 测试超时保护：单文件超过 120 秒强制 kill

### mock_server.php 端点调整
- 移除 `/hang` 端点（改由独立 socat 进程提供 18400 端口的 hang 服务）
- 保留 `/get`/`/post`/`/cookies`/`/stream`

## Impact
- Affected code: `.github/workflows/build-rust.yml`（Run PHP tests 步骤）、`rust/tests/mock_server.php`
- 不影响 Rust 源码
- 不影响 PHP 测试逻辑（`/hang` 请求改打 18400 端口）

## ADDED Requirements

### Requirement: 测试环境隔离
每个 PHP 测试文件 SHALL 在干净的 mock_server 实例上运行，前序测试的状态（连接、`/hang` 阻塞）不影响后续测试。

#### Scenario: 串行运行所有测试文件
- **WHEN** CI runs all `php_*.php` files sequentially
- **THEN** each file gets a fresh mock_server (no cross-file contamination)

### Requirement: /hang 端点不阻塞服务器
The `/hang` endpoint SHALL be served by an independent process that does not block other mock_server requests.

#### Scenario: /hang 请求不阻塞其他端点
- **WHEN** a test calls `/hang` then another test calls `/get`
- **THEN** `/get` responds normally (not blocked by the sleeping `/hang` handler)

## MODIFIED Requirements
（无）

## REMOVED Requirements
（无）
