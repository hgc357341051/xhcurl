# Checklist

## 错误处理统一化
- [ ] `XHRequest::execute()` 网络错误返回 `success=false` 数组（不抛异常）
- [ ] `global_client()`/`global_runtime()` 初始化失败返回错误（不 panic）
- [ ] `setConfig` 提前校验代理格式（fail-fast）
- [ ] `curl.rs` 7 处 RwLock unwrap 改为 unwrap_or_else
- [ ] `header.rs` 7 处 RwLock unwrap 改为 unwrap_or_else
- [ ] `fiber.rs` 5 处 expect 改为 ? 传播
- [ ] `php_ext.rs` 2 处 expect 改为 ? 传播

## 失败路径字段完整性
- [ ] `result_to_php_array` 失败分支含 `headers => []`
- [ ] `result_to_php_array` 失败分支含 `body_size => 0`
- [ ] `result_to_php_array` 失败分支含 `url => ""`
- [ ] 失败路径字段集与成功路径完全一致
- [ ] `getConfig()` proxy 为 None 时返回 null（非省略键）

## API 补全
- [ ] `XHRequest::options()` 方法已实现
- [ ] README 方法表列出 options()

## 文档对齐
- [ ] README setConfig 示例含 `http2_enabled`
- [ ] README 说明 execute() 错误处理语义
- [ ] README body() 签名为 `body(string $data)`
- [ ] README 故障排查含请求超时/代理无效/响应体超限

## CI 质量保障
- [ ] CI clippy 启用 `--features php`
- [ ] CI test 启用 `--features php`
- [ ] "验证扩展可加载"无 `|| true`，失败时 CI 红
- [ ] CI 运行 `rust/tests/php_*.php` 测试
- [ ] macOS PHP 版本注释为 8.1~8.5

## 测试改进
- [ ] `test_drop_aborts_tasks` 验证 abort 行为（非空 Vec）
- [ ] `test_global_manager_config` 用独立实例（不触碰全局单例）

## 验证
- [ ] cargo fmt --check 通过
- [ ] cargo clippy --all-targets --features php -- -D warnings 通过
- [ ] cargo test --lib --features php 全部通过
- [ ] PHP 冒烟：execute 网络错误返回 success=false、options()、getConfig proxy null、失败路径字段完整
- [ ] CHANGELOG 新增 1.0.7 条目
