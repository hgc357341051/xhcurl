# Tasks

- [x] Task 1: timeout 类 0 值统一为"跳过/使用默认"
  - [x] SubTask 1.1: 在 `rust/src/request.rs::to_reqwest()` 中，对 `request_timeout`/`request_timeout_ms`/`connect_timeout`/`connect_timeout_ms` 增加 `if > 0` 判断（0/负值跳过设置）
  - [x] SubTask 1.2: 在 `rust/src/curl.rs::create_client()` 中，对全局 `connect_timeout=0`/`request_timeout=0` 增加 `if > 0` 判断
  - [x] SubTask 1.3: 测试：`timeout(0)` 不立即超时（用 mock_server /get，请求成功）

- [x] Task 2: connectTimeout(0) 不触发 Client 重建
  - [x] SubTask 2.1: 在 `rust/src/request.rs::needs_request_client()` 中，对 `connect_timeout` 用 `.filter(|&s| s > 0)` 过滤（与 OverrideKey 的 filter 一致）
  - [x] SubTask 2.2: 同样处理 `connect_timeout_ms`（若已加入 needs_request_client 判断）
  - [x] SubTask 2.3: 测试：`connectTimeout(0)` 后请求仍走全局 Client（验证不抛异常 + 请求成功）

- [x] Task 3: XHThreadPool 提交失败返回错误
  - [x] SubTask 3.1: 在 `rust/src/threadpool.rs::execute_all()` 中，当 `submitted_count < requests.len()` 时返回 `Err`（含失败数量）
  - [x] SubTask 3.2: 在 `rust/src/threadpool.rs::execute_each()` 中同样处理
  - [x] SubTask 3.3: 在 `rust/src/php_ext.rs` 的 XHThreadPool execute 路径确保错误传播为 PHP 异常
  - [x] SubTask 3.4: 测试：大批量请求超过队列容量时 execute 抛异常（含失败数量）

- [x] Task 4: cookies() 数组形式对齐 form() 类型转换
  - [x] SubTask 4.1: 在 `rust/src/php_ext.rs::cookies()` 数组分支中，整型/浮点→to_string，布尔→"1"/"0"
  - [x] SubTask 4.2: 数组/对象/资源值抛异常
  - [x] SubTask 4.3: 测试：cookies 整型/布尔值正确转换；数组值抛异常

- [x] Task 5: multipart() 字段校验
  - [x] SubTask 5.1: 在 `rust/src/php_ext.rs::multipart()` 中，字段解析后校验 `!name.is_empty()`，为空抛异常
  - [x] SubTask 5.2: 非数组元素抛异常（当前 `continue` 改为 `return Err`）
  - [x] SubTask 5.3: 测试：multipart 空 name 抛异常；非数组元素抛异常

- [x] Task 6: body() 非字符串抛异常
  - [x] SubTask 6.1: 在 `rust/src/php_ext.rs::body()` 的 else 分支改为 `return Err("body 参数必须是字符串")`
  - [x] SubTask 6.2: 测试：body(null)/body([])/body(123) 抛异常

- [x] Task 7: headers() 列表数组校验
  - [x] SubTask 7.1: 在 `rust/src/php_ext.rs::headers()` 遍历前检测所有键是否为整数（纯列表数组），若是抛异常
  - [x] SubTask 7.2: 测试：headers 列表数组抛异常；关联数组正常

- [x] Task 8: xhrun env 非字符串值转换
  - [x] SubTask 8.1: 在 `rust/src/php_ext.rs` xhrun env 选项遍历中，整型/浮点/布尔→字符串
  - [x] SubTask 8.2: 测试：env 整型值被设置（可验证子进程收到 VERBOSE=1）

- [x] Task 9: XHRequest 补充 getter
  - [x] SubTask 9.1: 在 `rust/src/php_ext.rs::PhpXhRequest` impl 块新增 getter：`getTimeout()`/`getConnectTimeout()`/`getHeaders()`/`getCookies()`/`getProxy()`/`getVerifySsl()`/`getUserAgent()`/`getId()`/`getUserData()`，委托 Rust 端已有 getter
  - [x] SubTask 9.2: 未设置的字段返回 null 或默认值（如 getTimeout 返回 null 表示无请求级覆盖）
  - [x] SubTask 9.3: 测试：setter 后 getter 返回设置的值；未设置返回 null/默认

- [x] Task 10: XHMulti/XHThreadPool 补充 count/isEmpty
  - [x] SubTask 10.1: 在 `rust/src/php_ext.rs::PhpXhMulti` 新增 `count(): int` 和 `isEmpty(): bool`
  - [x] SubTask 10.2: 在 `rust/src/php_ext.rs::PhpXhThreadPool` 新增同样方法
  - [x] SubTask 10.3: 测试：add 后 count 返回正确数；isEmpty 在空/非空时返回正确布尔

- [x] Task 11: README 文档更新
  - [x] SubTask 11.1: 补充新增 getter 方法说明
  - [x] SubTask 11.2: 补充 `count()`/`isEmpty()` 说明
  - [x] SubTask 11.3: 补充 onHeaders 回调 headers 键名小写说明
  - [x] SubTask 11.4: 补充 timeout 类 0 值语义说明（0 = 使用默认值，非立即超时）
  - [x] SubTask 11.5: 更新 cookies/multipart/body/headers 校验说明
  - [x] SubTask 11.6: 补充 XHThreadPool 提交失败抛异常说明

- [x] Task 12: 验证
  - [x] SubTask 12.1: `cargo fmt --check`
  - [x] SubTask 12.2: `cargo clippy --features php -- -D warnings`
  - [x] SubTask 12.3: `cargo test --lib --features php`（95 passed）
  - [x] SubTask 12.4: `cargo build --features php` 编译成功
  - [x] SubTask 12.5: 启动 mock 服务器，运行全部 `php_*.php` 测试通过（12 文件 318 用例全通过）

# Task Dependencies
- Task 1（0 值统一）独立
- Task 2（connectTimeout 不重建）独立
- Task 3（提交失败反馈）独立
- Task 4（cookies 类型转换）独立
- Task 5（multipart 校验）独立
- Task 6（body 校验）独立
- Task 7（headers 列表校验）独立
- Task 8（xhrun env）独立
- Task 9（getter）独立
- Task 10（count/isEmpty）独立
- Task 11（文档）依赖 Task 1-10 完成
- Task 12（验证）依赖所有前序任务完成
- Task 1-10 可并行
