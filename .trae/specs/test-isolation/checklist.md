# Checklist

## mock_server 端点调整
- [x] mock_server.php 移除 /hang 端点
- [x] 注释更新说明 /hang 由独立 socat 进程提供（18400 端口）
- [x] /get /post /cookies /stream 端点保持不变

## 测试文件 URL 更新
- [x] php_silent_failure_and_usability_test.php: /hang → 18400
- [x] php_align_threadpool_and_cookie_safety_test.php: /hang → 18400
- [x] php_fix_threadpool_reuse_and_each_timeout_test.php: /hang → 18400
- [x] php_multi_each_test.php: 已使用 18400（无需改动）
- [x] php_each_test.php: 负值 clamp 测试更新为负值抛异常断言

## CI 脚本改造
- [x] 启动 socat 在 18400 端口提供 /hang（fork 模式不阻塞）
- [x] 每个测试文件运行前重启 mock_server（干净环境）
- [x] 单文件 timeout 120s 超时保护
- [x] 测试完成后清理所有后台进程（trap cleanup EXIT）
- [x] mock_server 就绪检查（curl /get 验证）

## 验证
- [x] 本地串行运行所有 13 个 php_*.php 测试全部通过（374 用例）
- [x] /hang 请求不阻塞 mock_server 的其他端点（socat fork 模式）
- [x] 无残留进程（trap 清理）
- [x] EXIT_CODE=0

> 验证脚本：`bash rust/tests/run_ci_locally.sh` 完整模拟 CI 流程
