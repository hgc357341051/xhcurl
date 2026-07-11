#!/bin/bash
# 本地模拟 CI 流程：验证测试隔离方案
set +e  # 不因单条命令失败而退出

pkill -f "socat TCP-LISTEN" 2>/dev/null
pkill -f "php -S 127.0.0.1" 2>/dev/null
sleep 2
echo "=== killed old processes ==="

# 启动 socat /hang server
socat TCP-LISTEN:18400,fork,reuseaddr SYSTEM:'sleep 60' &
SOCAT_PID=$!
sleep 2
if kill -0 $SOCAT_PID 2>/dev/null; then
  echo "=== socat /hang server started (PID $SOCAT_PID) ==="
else
  echo "=== ERROR: socat failed to start ==="
  exit 1
fi

# 串行运行所有 PHP 测试，每个文件独立 mock_server
EXIT_CODE=0
for f in /workspace/rust/tests/php_*.php; do
  FNAME=$(basename "$f")
  echo ""
  echo "=== Running $FNAME ==="

  # 重启 mock_server
  pkill -f "php -S 127.0.0.1:18399" 2>/dev/null
  sleep 1
  php -S 127.0.0.1:18399 /workspace/rust/tests/mock_server.php > /tmp/mock_18399.log 2>&1 &
  MOCK_PID=$!
  sleep 1

  # 验证 mock_server 就绪
  if ! curl -sf http://127.0.0.1:18399/get > /dev/null 2>&1; then
    echo "::error::mock_server failed to start for $FNAME"
    cat /tmp/mock_18399.log
    EXIT_CODE=1
    continue
  fi

  # 运行测试（120s 超时保护）
  OUTPUT=$(timeout 120 php -d extension=xhcurl "$f" 2>&1)
  RC=$?
  echo "$OUTPUT" | tail -3
  if [ $RC -ne 0 ]; then
    echo "::error::$FNAME failed (exit $RC)"
    EXIT_CODE=1
  fi

  # 清理 mock_server
  kill $MOCK_PID 2>/dev/null
  sleep 1
done

echo ""
echo "=========================================="
echo "=== 最终结果: EXIT_CODE=$EXIT_CODE ==="
echo "=========================================="

kill $SOCAT_PID 2>/dev/null
pkill -f "php -S 127.0.0.1" 2>/dev/null
exit $EXIT_CODE
