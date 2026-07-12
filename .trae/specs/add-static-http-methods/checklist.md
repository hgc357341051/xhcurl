# 验证检查清单

## 静态方法实现
- [ ] `XHCurl::get($url, $options=[])` 静态方法实现正确
- [ ] `XHCurl::post($url, $body=null, $options=[])` 静态方法实现正确
- [ ] `XHCurl::put($url, $body=null, $options=[])` 静态方法实现正确
- [ ] `XHCurl::delete($url, $options=[])` 静态方法实现正确
- [ ] `XHCurl::patch($url, $body=null, $options=[])` 静态方法实现正确
- [ ] `XHCurl::head($url, $options=[])` 静态方法实现正确

## body 参数处理
- [ ] post/put/patch 的 $body 为 string → 调用 body()（原始字节体）
- [ ] post/put/patch 的 $body 为 array → 调用 json()（自动序列化）
- [ ] post/put/patch 的 $body 为 null → 无请求体

## options 语义
- [ ] $options 复用 withOptions 的 18 个 key
- [ ] $options 未知 key 抛异常（fail-fast）
- [ ] $options 为空数组（默认）正常工作

## 返回值
- [ ] 返回与 execute() 一致的结果数组（12 字段）
- [ ] 含 attempts 字段（默认 1）
- [ ] 含 truncated 字段（默认 false）

## mock_server
- [ ] `/echo-method` 端点回显请求方法与请求体

## 文档与版本
- [ ] README 新增「静态便捷方法」小节
- [ ] Cargo.toml version = "1.7.0"
- [ ] CHANGELOG.md 包含 [1.7.0] 条目

## 测试
- [ ] `php_add_static_http_methods_test.php` 创建并全部通过
- [ ] 全部 27 个 PHP 测试文件 PASS

## 编译与运行
- [ ] `cargo fmt --check` 通过
- [ ] `cargo clippy --all-targets --features php -- -D warnings` 通过
- [ ] `cargo test --lib --features php` 通过
- [ ] `cargo build --release --features php` 成功
- [ ] .so 已同步到 PHP 扩展目录
- [ ] `php -d extension=xhcurl -r 'echo XHCurl::version();'` 输出 `1.7.0`
