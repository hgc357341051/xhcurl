# Tasks

## 阶段 1：扩展 mock_server 端点（解锁测试基础设施）

- [x] Task 1: 在 `rust/tests/mock_server.php` 新增 `/redirect?n=N` 端点
  - [x] SubTask 1.1: 在路由分发逻辑中添加 `/redirect` 路径处理
  - [x] SubTask 1.2: 解析 `n` 参数（默认 1），n>0 时返回 302 + `Location: /redirect?n=N-1`，n=0 时返回 200 + JSON `{"redirected":true}`
- [x] Task 2: 在 `rust/tests/mock_server.php` 新增 `/large?size=N` 端点
  - [x] SubTask 2.1: 在路由分发逻辑中添加 `/large` 路径处理
  - [x] SubTask 2.2: 解析 `size` 参数（默认 1024），返回 200 + 指定字节数的响应体（用 `str_repeat('a', $size)` 或类似生成）

## 阶段 2：HTTP 响应超限 error_type 分类

- [x] Task 3: 在 `rust/src/php_ext.rs` 的 `classify_error_type` 函数增加 response_too_large 分类
  - [x] SubTask 3.1: 读取 `classify_error_type` 函数当前实现（约 3001-3025 行）
  - [x] SubTask 3.2: 在关键词匹配中增加"超过最大限制"或"响应体"的识别，返回 `"response_too_large"`
  - [x] SubTask 3.3: 确保 error_type 值集文档同步更新

## 阶段 3：HTTP 结果数组新增 truncated 字段

- [x] Task 4: 在 `rust/src/php_ext.rs` 的 `result_to_php_array` 增加 truncated 字段
  - [x] SubTask 4.1: 读取 `result_to_php_array` 函数当前实现
  - [x] SubTask 4.2: 在成功路径插入 `truncated => false`
  - [x] SubTask 4.3: 在失败路径根据 error_type 判断：若 `error_type == "response_too_large"` 则 `truncated => true`，否则 `truncated => false`
- [x] Task 5: 在 `rust/src/php_ext.rs` 的 `response_to_php_array` 增加 truncated 字段（如有独立函数）
  - [x] SubTask 5.1: 读取 `response_to_php_array` 函数当前实现
  - [x] SubTask 5.2: 成功路径插入 `truncated => false`（HTTP 响应对象转换路径，无超限场景）

## 阶段 4：文档更新

- [x] Task 6: 修正 README.md 截断行为描述（P1）
  - [x] SubTask 6.1: 查找第 1208 行附近的"截断时 success 仍为 true"描述
  - [x] SubTask 6.2: 替换为实际行为：`success=false`、`body=""`、`body_size=0`、`error_type="response_too_large"`、`truncated=true`
  - [x] SubTask 6.3: 同步修正字段表与 note 块（如有相关描述）
- [x] Task 7: 更新 README.md 的 error_type 值集与字段表
  - [x] SubTask 7.1: 在 error_type 值集描述中增加 `response_too_large`
  - [x] SubTask 7.2: 在 HTTP 结果数组字段表中增加 `truncated` 行
- [x] Task 8: 升级版本号 1.2.0 → 1.3.0
  - [x] SubTask 8.1: `rust/Cargo.toml` version = "1.3.0"
  - [x] SubTask 8.2: `CHANGELOG.md` 新增 [1.3.0] 条目

## 阶段 5：测试

- [x] Task 9: 创建 `rust/tests/php_unify_truncation_and_redirect_test.php`
  - [x] SubTask 9.1: HTTP 响应超限：`/large?size=8192` + `maxResponseSize(1024)` 验证 `success=false`/`body=""`/`error_type="response_too_large"`/`truncated=true`
  - [x] SubTask 9.2: HTTP 成功路径：`/get` 验证 `truncated=false`
  - [x] SubTask 9.3: maxRedirects(0)：`/redirect?n=1` 返回 status=302 不跟随
  - [x] SubTask 9.4: maxRedirects(5)：`/redirect?n=3` 跟随到 status=200
  - [x] SubTask 9.5: followRedirects(false)：`/redirect?n=1` 返回 status=302
  - [x] SubTask 9.6: followRedirects(true)->maxRedirects(5)：`/redirect?n=3` 跟随到 status=200
  - [x] SubTask 9.7: error_type 值集：dns（`http://nonexistent.invalid`）、timeout（`/hang` + timeoutMs）、connection（连接拒绝）、response_too_large、`""`（成功）
  - [x] SubTask 9.8: body_size 与 strlen(body) 一致性断言

## 阶段 6：编译与全套件验证

- [x] Task 10: Rust 侧验证
  - [x] SubTask 10.1: `cargo fmt --check` 通过
  - [x] SubTask 10.2: `cargo clippy --all-targets --features php -- -D warnings` 通过
  - [x] SubTask 10.3: `cargo test --lib --features php` 98+ 用例通过
- [x] Task 11: 编译 release .so 并同步到 PHP 扩展目录
  - [x] SubTask 11.1: `cargo build --release --features php`
  - [x] SubTask 11.2: `cp target/release/libxhcurl.so` 到 PHP 扩展目录
  - [x] SubTask 11.3: `php -d extension=xhcurl -r 'echo XHCurl::version();'` 输出 `1.3.0`
- [x] Task 12: 运行新测试文件
  - [x] SubTask 12.1: 启动 socat(18400) + mock_server(18399)
  - [x] SubTask 12.2: `php -d extension=xhcurl rust/tests/php_unify_truncation_and_redirect_test.php` 全通过
- [x] Task 13: 运行全套件 23+ 个 PHP 测试文件
  - [x] SubTask 13.1: 串行运行所有 `rust/tests/php_*.php`
  - [x] SubTask 13.2: 确认 23 个文件全部 PASS（含本轮新增的 1 个）
- [x] Task 14: 检查现有测试是否受字段集扩展影响
  - [x] SubTask 14.1: 现有测试若断言"字段集恰好为 10 项"需更新为 11 项
  - [x] SubTask 14.2: 现有测试若做字段集严格比较需加入 truncated

# Task Dependencies

- Task 1, 2 → Task 9（测试依赖新端点）
- Task 3 → Task 4, 5（error_type 分类先于 truncated 字段判断）
- Task 3, 4, 5 → Task 6, 7（文档依赖最终 error_type 值集与字段集）
- Task 6, 7 → Task 8（CHANGELOG 依赖最终变更清单）
- Task 1-8 全部 → Task 9（测试依赖实现完成）
- Task 9 → Task 10, 11, 12, 13, 14（编译验证依赖测试代码就绪）
- Task 1, 2（mock_server）与 Task 3, 4, 5（PHP 边界）可并行
