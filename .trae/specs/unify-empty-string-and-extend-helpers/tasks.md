# Tasks

## 阶段 1：Rust 核心层准备

- [x] Task 1: 在 `request.rs` 新增 `referer` 字段与 setter/getter，并在 `to_reqwest`/`build_request_client` 中应用 Referer header
  - [x] SubTask 1.1: 在 `XhRequest` 结构体增加 `referer: Option<String>` 字段，更新 `Clone`/`Debug` 派生
  - [x] SubTask 1.2: 实现 `referer()`/`get_referer()` 方法，与 `user_agent` 风格一致
  - [x] SubTask 1.3: 在 `to_reqwest` 与 `build_request_client` 中，若 `referer` 非空则 `builder = builder.header("Referer", r)`
  - [x] SubTask 1.4: 暴露 `body_json_str` 已存在的 API 给 PHP 层调用（无需新增 Rust 代码，确认可见性即可）
- [x] Task 2: 在 `request.rs` 新增 `get_multipart_fields()` 访问器，返回 `Option<&Vec<MultipartField>>` 供 PHP 层构建返回数组
  - [x] SubTask 2.1: 确认 `MultipartField` 结构体字段（name/contents/filename）可见性为 `pub`
  - [x] SubTask 2.2: 实现 `get_multipart()` 返回 `Option<&Vec<MultipartField>>`
  - [x] SubTask 2.3: 新增 `add_cookie_pair(name, value)` 方法，增量追加 cookie 到 `cookies` 字段（用 `; ` 分隔）

## 阶段 2：PHP 边界空字符串校验扩展

- [x] Task 3: 在 `php_ext.rs` 为 4 个 setter 增加空字符串 fail-fast 校验
  - [x] SubTask 3.1: `userAgent` 在 ASCII 校验前增加空字符串检查，错误消息含"传 null 清除 User-Agent 覆盖"
  - [x] SubTask 3.2: `encoding` 同上，错误消息含"传 null 清除 Accept-Encoding 覆盖"
  - [x] SubTask 3.3: `range` 同上，错误消息含"传 null 清除 Range 覆盖"
  - [x] SubTask 3.4: `cookies` 字符串路径（`Some(s) if !is_array`）增加空字符串检查，错误消息含"传 null 清除 Cookie 覆盖"

## 阶段 3：错误消息格式统一

- [x] Task 4: 修正错误消息以符合契约
  - [x] SubTask 4.1: `bearerToken('')` 错误消息追加"，传 null 清除 Bearer Token"
  - [x] SubTask 4.2: `maxRedirects` 负值错误消息追加"，0 = 不跟随重定向"
  - [x] SubTask 4.3: `xhrun` `timeout` 负值错误消息改为 `"xhrun timeout 不能为负值，0 = 无超时"`
  - [x] SubTask 4.4: `xhrun` `max_output` 负值错误消息改为 `"xhrun max_output 不能为负值，0 = 无限制"`

## 阶段 4：新增 PHP 便捷 setter

- [x] Task 5: 在 `php_ext.rs` 新增 `referer()` PHP 方法
  - [x] SubTask 5.1: 方法签名 `referer(?string $referer): Result<&mut PhpXhRequest, String>`，null 清除，空字符串抛异常，ASCII 校验
  - [x] SubTask 5.2: 调用 `self_.request = self_.request.clone().referer(r)` 并返回 `Ok(self_)`
- [x] Task 6: 在 `php_ext.rs` 新增 `cookie()` 增量添加方法
  - [x] SubTask 6.1: 方法签名 `cookie(string $name, string $value): Result<&mut PhpXhRequest, String>`
  - [x] SubTask 6.2: 校验 name 与 value 非空字符串、ASCII 合法
  - [x] SubTask 6.3: 调用 `self_.request.add_cookie_pair(name, value)` 增量追加，返回 `Ok(self_)`
- [x] Task 7: 在 `php_ext.rs` 新增 `jsonStr()` 方法
  - [x] SubTask 7.1: 方法签名 `jsonStr(string $json): Result<&mut PhpXhRequest, String>`
  - [x] SubTask 7.2: 调用 `self_.request = self_.request.clone().body_json_str(&json).map_err(|e| format!("jsonStr 无效 JSON: {}", e))?` 并返回 `Ok(self_)`

## 阶段 5：新增 PHP getter

- [x] Task 8: 在 `php_ext.rs` 新增 `getHeader(name)` 单值查询方法
  - [x] SubTask 8.1: 方法签名 `getHeader(string $name): ?string`
  - [x] SubTask 8.2: 调用 `self_.request.get_headers()`，按小写键查询（headers 已统一为小写存储），返回 `Option<String>`
- [x] Task 9: 在 `php_ext.rs` 新增 `getMultipart()` 方法
  - [x] SubTask 9.1: 方法签名 `getMultipart(): ?array`
  - [x] SubTask 9.2: 调用 `self_.request.get_multipart()`，若 `None` 返回 PHP `null`；若 `Some(fields)` 构建关联数组（每个元素含 `name`/`contents`/`filename` 三键，`filename` 缺省为 null）
- [x] Task 10: 在 `php_ext.rs` 新增 `getReferer()` 方法
  - [x] SubTask 10.1: 方法签名 `getReferer(): ?string`，调用 `self_.request.get_referer()`
- [x] Task 11: 扩展 `getBody()` 支持返回 JSON/form 序列化字符串
  - [x] SubTask 11.1: 修改 `get_body()`，根据 `BodyType` 分支：`Bytes` 返回 `String::from_utf8_lossy`；`Json` 用 `serde_json::to_string` 序列化；`Form` 拼 `k=v&k=v`；`Multipart` 返回 `None`

## 阶段 6：文档与版本

- [x] Task 12: 更新 README.md
  - [x] SubTask 12.1: 在 XHRequest 方法表新增 `referer()`/`cookie()`/`jsonStr()` 行
  - [x] SubTask 12.2: 在 getter 表新增 `getHeader()`/`getMultipart()`/`getReferer()` 行
  - [x] SubTask 12.3: 修改 `getBody()` 说明为"返回 body/json/form 序列化字符串，multipart 返回 null"
  - [x] SubTask 12.4: 在 `maxRedirects` 说明中明确"0 = 不跟随重定向，等价于 followRedirects(false)"
  - [x] SubTask 12.5: 在空字符串契约说明中，将 `userAgent`/`encoding`/`range`/`cookies`/`referer` 一并列出
- [x] Task 13: 升级版本号 1.1.0 → 1.2.0
  - [x] SubTask 13.1: `rust/Cargo.toml` version = "1.2.0"
  - [x] SubTask 13.2: `CHANGELOG.md` 新增 [1.2.0] 条目，包含 BREAKING 说明（5 个 setter 空字符串行为变更）与新增方法列表

## 阶段 7：测试

- [x] Task 14: 创建 `rust/tests/php_unify_empty_and_helpers_test.php`
  - [x] SubTask 14.1: 空字符串 setter 抛异常测试（userAgent/encoding/range/cookies 各 1 项，null 清除仍工作各 1 项 = 8 用例）
  - [x] SubTask 14.2: 错误消息格式验证（bearerToken/maxRedirects/xhrun timeout/max_output = 4 用例）
  - [x] SubTask 14.3: referer 正常 + 空字符串 + null 清除 + getReferer 往返 = 4 用例
  - [x] SubTask 14.4: cookie 增量添加 + 空名称抛异常 + getCookies 验证 = 3 用例
  - [x] SubTask 14.5: jsonStr 正常 + 无效 JSON 抛异常 + getBody 验证 = 3 用例
  - [x] SubTask 14.6: getHeader 大小写不敏感 + 未设置返回 null = 2 用例
  - [x] SubTask 14.7: getMultipart 已设置 + 未设置返回 null = 2 用例
  - [x] SubTask 14.8: getBody 对 JSON/form 返回序列化字符串 + 对 multipart 返回 null = 3 用例
  - [x] SubTask 14.9: maxRedirects(0) 不跟随重定向行为验证 = 1 用例

## 阶段 8：编译与全套件验证

- [x] Task 15: Rust 侧验证
  - [x] SubTask 15.1: `cargo fmt --check` 通过
  - [x] SubTask 15.2: `cargo clippy --all-targets --features php -- -D warnings` 通过
  - [x] SubTask 15.3: `cargo test --lib --features php` 98+ 用例通过
- [x] Task 16: 编译 release .so 并同步到 PHP 扩展目录
  - [x] SubTask 16.1: `cargo build --release --features php`
  - [x] SubTask 16.2: `cp target/release/libxhcurl.so` 到 `/root/.phpenv/versions/8.2snapshot/lib/php/extensions/no-debug-non-zts-20220829/xhcurl.so`
  - [x] SubTask 16.3: `php -d extension=xhcurl -r 'echo XHCurl::version();'` 输出 `1.2.0`
- [x] Task 17: 运行新测试文件
  - [x] SubTask 17.1: 启动 socat(18400) + mock_server(18399)
  - [x] SubTask 17.2: `php -d extension=xhcurl rust/tests/php_unify_empty_and_helpers_test.php` 全通过
- [x] Task 18: 运行全套件 21+ 个 PHP 测试文件
  - [x] SubTask 18.1: 串行运行所有 `rust/tests/php_*.php`（每文件独立 mock_server）
  - [x] SubTask 18.2: 确认 22 个文件全部 PASS（含本轮新增的 1 个）
- [x] Task 19: 检查现有测试是否受 BREAKING 影响
  - [x] SubTask 19.1: 全套件失败时定位是否因空字符串校验变更（userAgent/encoding/range/cookies）
  - [x] SubTask 19.2: 修复受影响测试（改为 null 清除或断言抛异常）

# Task Dependencies

- Task 1, 2 → Task 3, 4, 5, 6, 7, 8, 9, 10, 11（PHP 边界依赖 Rust 核心 API）
- Task 3, 4, 5, 6, 7, 8, 9, 10, 11 → Task 12（文档依赖最终 API 形态）
- Task 12 → Task 13（CHANGELOG 依赖最终变更清单）
- Task 1-13 全部 → Task 14（测试依赖实现完成）
- Task 14 → Task 15, 16, 17, 18, 19（编译验证依赖测试代码就绪）
- Task 3, 4（空字符串 + 错误消息）可与 Task 5-11（新增方法）并行
