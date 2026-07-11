# Tasks

- [x] Task 1: mock_server.php 移除 /hang 端点
  - [x] SubTask 1.1: 删除 `/hang` 端点（约第 47-52 行），保留 `/get`/`/post`/`/cookies`/`/stream`
  - [x] SubTask 1.2: 更新文件顶部注释，说明 `/hang` 改由独立 socat 进程提供（端口 18400）

- [x] Task 2: 更新测试文件中的 /hang URL
  - [x] SubTask 2.1: 搜索所有 `php_*.php` 中使用 `/hang` 端点的测试，将 URL 从 `$BASE . '/hang'`（127.0.0.1:18399）改为 `http://127.0.0.1:18400/hang`
  - [x] SubTask 2.2: 验证改动后测试逻辑仍正确（/hang 仍返回 200 但由独立进程提供）
  - [x] SubTask 2.3: 更新 php_each_test.php 中负值 clamp 测试为负值抛异常断言（第五轮行为变更）

- [x] Task 3: CI 脚本测试步骤改造
  - [x] SubTask 3.1: 修改 `.github/workflows/build-rust.yml` 的 `Run PHP tests` 步骤
  - [x] SubTask 3.2: 启动 socat 进程在 18400 端口提供 `/hang`（`socat TCP-LISTEN:18400,fork,reuseaddr SYSTEM:'sleep 60'`），fork 模式不阻塞
  - [x] SubTask 3.3: 每个 PHP 测试文件运行前重启 mock_server（kill + sleep 1 + 重启），确保干净环境
  - [x] SubTask 3.4: 单文件测试加 timeout 120s 超时保护（`timeout 120 php -d extension=xhcurl "$f"`）
  - [x] SubTask 3.5: 测试完成后清理 mock_server + socat 进程

- [x] Task 4: 验证
  - [x] SubTask 4.1: 本地模拟 CI 流程：启动 mock_server + socat，串行运行所有 php_*.php 测试（13 文件 374 用例）全通过
  - [x] SubTask 4.2: 验证 /hang 请求不阻塞 mock_server 的 /get 端点

# Task Dependencies
- Task 1（移除 /hang）独立
- Task 2（更新测试 URL）依赖 Task 1
- Task 3（CI 脚本）依赖 Task 1
- Task 4（验证）依赖 Task 1-3
