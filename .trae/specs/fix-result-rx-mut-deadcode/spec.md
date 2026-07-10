# 修复 result_rx_mut dead_code 警告 Spec

## Why

CI 在不带 `--features php` 构建 lib 时（`cargo clippy` 默认），`result_rx_mut` 方法因唯一调用者（php_ext.rs 的 executeEach）在 `#[cfg(feature = "php")]` 下而不存在，触发 `dead_code` 警告，CI 用 `-D warnings` 将其当作错误导致构建失败。

## What Changes

- 给 `threadpool.rs` 的 `result_rx_mut` 方法添加 `#[cfg(feature = "php")]` 属性，使该方法仅在 php feature 启用时编译（与其唯一调用者保持一致）。

## Impact

- 受影响文件：`rust/src/threadpool.rs`（1 行属性添加）
- 不影响现有 API 和行为（php 构建时该方法仍存在且被使用；非 php 构建时该方法不存在，避免 dead_code）
- 修复 CI 的非 php feature 构建路径

## MODIFIED Requirements

### Requirement: result_rx_mut 编译条件

`result_rx_mut` 方法 SHALL 添加 `#[cfg(feature = "php")]` 属性，仅在 php feature 启用时编译，与其唯一调用者（php_ext.rs executeEach）的编译条件一致。
