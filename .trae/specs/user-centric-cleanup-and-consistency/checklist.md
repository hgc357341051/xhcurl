# Checklist

## 文档完整性
- [x] README 列出全部公开 PHP 方法（XHCurl::each、XHMulti::timeout、XHMulti::executeEach、XHThreadPool::executeEach）
- [x] README FPM/CLI 能力表与实现一致（协程仅 CLI 可用）
- [x] README 协程章节顶部有 CLI-only 警告
- [x] README 响应字段表区分成功/失败路径字段差异
- [x] README `id` 字段默认值说明正确（未设置时为 URL，所有路径统一）
- [x] README 有 each() 与 gather() 对比示例
- [x] README 有 executeEach/each 回调签名说明

## Bug 修复
- [x] fiber_each 读取 fiber_max_concurrency 配置（不再硬编码 64）
- [x] XhMulti 实现 Drop，drop 时 abort 所有未完成任务
- [x] id 字段在 fiber 路径默认为 URL（而非 task-N）

## API 一致性
- [x] 所有 XHRequest 链式 setter 返回 &mut Self（不返回 Result）
- [x] 链式调用 `->get()->json([...])->timeout(10)->execute()` 无需 ? 或 unwrap
- [x] 新增 id()/userData() 无前缀方法
- [x] setId()/setUserData() 保留为别名（向后兼容）
- [x] 负值处理统一为跳过+保留原值（不再 clamp 到 0）

## 死配置清理
- [x] http2_enabled 在 create_client_builder 中实际读取（false 时 http1_only）
- [x] use_multi_thread 字段已移除
- [x] curl.rs 编译无 use_multi_thread 引用残留

## 字段一致性
- [x] 失败响应包含 status => 0（哨兵值）
- [x] 失败响应包含 body => ""（空字符串）
- [x] 成功/失败路径字段集一致（都有 status/body）

## 验证
- [x] cargo fmt --check 通过
- [x] cargo clippy -- -D warnings 通过（非 php feature）
- [x] cargo clippy --all-targets --features php -- -D warnings 通过
- [x] cargo test --lib 全部通过（89 passed）
- [x] PHP 运行时冒烟：扩展加载、链式调用、each 配置生效、id 默认为 URL、失败时 status=0
- [x] CHANGELOG 新增 1.0.6 条目
