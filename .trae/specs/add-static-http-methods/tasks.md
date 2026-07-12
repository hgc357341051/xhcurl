# Tasks

- [ ] Task 1: 在 php_ext.rs 的 XHCurl 类实现中新增 6 个静态方法（get/post/put/delete/patch/head）
  - [ ] SubTask 1.1: 实现 `get(url, options=[])` 静态方法：内部调用 createRequest(url)->get()->withOptions(options)->execute()
  - [ ] SubTask 1.2: 实现 `post(url, body=null, options=[])` 静态方法：body 为 string→body()，array→json()，null→无 body
  - [ ] SubTask 1.3: 实现 `put(url, body=null, options=[])` 静态方法（与 post 同 body 处理逻辑）
  - [ ] SubTask 1.4: 实现 `delete(url, options=[])` 静态方法（无 body 参数，与 get 一致）
  - [ ] SubTask 1.5: 实现 `patch(url, body=null, options=[])` 静态方法（与 post 同 body 处理逻辑）
  - [ ] SubTask 1.6: 实现 `head(url, options=[])` 静态方法（无 body 参数，与 get 一致）
- [ ] Task 2: mock_server.php 新增 `/echo-method` 端点
  - [ ] SubTask 2.1: 新增 `/echo-method` 端点，回显请求方法（GET/POST/PUT/DELETE/PATCH/HEAD）与请求体
- [ ] Task 3: 文档与版本更新
  - [ ] SubTask 3.1: README.md 新增「静态便捷方法」小节（含方法表 + 示例）
  - [ ] SubTask 3.2: Cargo.toml 版本 1.6.0 → 1.7.0
  - [ ] SubTask 3.3: CHANGELOG.md 新增 [1.7.0] 条目
- [ ] Task 4: 创建测试文件 `php_add_static_http_methods_test.php`
  - [ ] SubTask 4.1: 测试 6 个静态方法的基本调用（get/post/put/delete/patch/head）
  - [ ] SubTask 4.2: 测试 $body 参数三种类型（string/array/null）
  - [ ] SubTask 4.3: 测试 $options 复用 withOptions 语义（timeout/headers/query/retry）
  - [ ] SubTask 4.4: 测试未知 options key 抛异常
  - [ ] SubTask 4.5: 测试返回数组字段集与 execute() 一致（含 attempts/truncated）
- [ ] Task 5: 编译验证
  - [ ] SubTask 5.1: cargo fmt/clippy/test 通过
  - [ ] SubTask 5.2: cargo build --release --features php 编译成功
  - [ ] SubTask 5.3: .so 同步到 PHP 扩展目录，版本输出 1.7.0
  - [ ] SubTask 5.4: 运行新测试文件 php_add_static_http_methods_test.php
  - [ ] SubTask 5.5: 运行全套件 PHP 测试（预期 27 个文件）

# Task Dependencies
- Task 2 独立（mock_server 端点）
- Task 3 独立（文档与版本）
- Task 4 依赖 Task 1 与 Task 2（测试需要实现与端点）
- Task 5 依赖 Task 1-4 全部完成
