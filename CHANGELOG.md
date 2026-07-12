# Changelog

本项目所有重要变更均记录于此文件。格式参考 [Keep a Changelog](https://keepachangelog.com/zh-CN/)，
版本号遵循 [语义化版本](https://semver.org/lang/zh-CN/)。

## [Unreleased]


## [1.6.0] - 2026-07-12

本版本为**次版本号升级**（非 BREAKING），聚焦**补齐生产环境网络抖动场景的重试能力**
与**批量场景下的请求克隆能力**。新增 `retry()` 方法与 `__clone()` 魔术方法，
结果数组新增 `attempts` 字段。

### 新增
- **`retry(int $times, int $delay_ms = 0): $this`**：设置失败重试次数与重试间隔。
  times=0 不重试（默认），times>0 失败时最多重试 N 次（总尝试 N+1）。
  delay_ms 重试间隔（毫秒），0=立即重试；负值抛异常（fail-fast）。
  **重试条件**：仅重试网络错误（请求未到达服务器：DNS/连接/超时/SSL），
  HTTP 错误（4xx/5xx）不重试（服务器已响应，属业务逻辑），与 Guzzle 默认行为一致。
  影响 execute() 与 executeJson()，不影响 XHMulti/XHThreadPool/协程路径。
- **`__clone()` 魔术方法**：支持 PHP `clone $req` 安全深拷贝 XHRequest 对象。
  保留所有配置（headers/body/timeout/retry/query 等）。典型场景：批量调用不同 URL，配置相同。
- **`getRetry(): array`**：返回 `['times' => int, 'delay_ms' => int]`。
- **结果数组 `attempts` 字段**：实际尝试次数（1 = 首次未重试，2 = 重试 1 次）。
  所有 HTTP 响应路径（execute/XHMulti/XHThreadPool/协程）均含此字段。
  非重试场景固定为 1。字段集从 11 扩展到 12。
- **withOptions 支持 `retry` key**：`withOptions(['retry' => ['times' => 2, 'delay_ms' => 100]])`
  等价于 `retry(2, 100)`。times 必填非负，delay_ms 可选默认 0。
- **mock_server 新增 `/flaky?fail=N` 端点**：前 N 次返回 503，第 N+1 次返回 200，
  用文件计数器模拟间歇性失败，用于测试「HTTP 错误不重试」场景。
- **mock_server 新增 `/echo-attempts` 端点**：回显请求 headers，便于验证重试行为。

### 测试
- 新增 `php_add_retry_and_clone_test.php`（约 14 项），覆盖：
  retry(0) 默认不重试、retry(2) + 网络错误重试后仍失败 attempts=3、
  retry(2) + 正常请求 attempts=1、retry(3) + /flaky?fail=2 (503) 不重试 attempts=1、
  retry(-1) 抛异常、retry(1, -100) 抛异常、retry(1, 50) + 正常请求 delay 不影响成功路径、
  executeJson() + retry(2) attempts=1、
  clone $req 独立修改不影响原对象、clone 保留所有配置、clone 后链式调用、
  withOptions(['retry' => [...]]) 等价 retry()、withOptions retry 非数组抛异常。


## [1.5.0] - 2026-07-12

本版本为**次版本号升级**（非 BREAKING），聚焦**提升微服务与多环境部署的配置灵活性**：
新增请求级批量设置方法 `withOptions()`、全局基础 URI `base_uri` 与全局默认 headers `base_headers`。

### 新增
- **`withOptions(array $options): $this`**：请求级批量设置多个选项，内部按 key 分发到对应 setter。
  支持 18 个常见选项 key（timeout/timeout_ms/connect_timeout/headers/query/accept/content_type/
  body/json/form/user_agent/referer/encoding/range/proxy/verify_ssl/follow_redirects/max_redirects）。
  未知 key 抛异常（fail-fast，避免拼写错误静默忽略）。null 值跳过（不调用对应 setter）。
  headers 数组中 null 值跳过。多次调用累加（后调用覆盖同名选项）。与链式 setter 混用正常工作。
- **全局 `base_uri` 配置**：`setConfig(['base_uri' => 'https://user-svc.internal'])`，
  请求 URL 以 `/` 开头时自动拼接为完整 URL。绝对 URL（http/https 开头）优先，不拼接。
  base_uri 末尾斜杠自动处理（避免双斜杠）。变更触发 Client 重建。
  微服务场景下避免每个请求重复写 host，环境切换只需修改一处。
- **全局 `base_headers` 配置**：`setConfig(['base_headers' => ['Authorization' => 'Bearer xxx']])`，
  所有请求自动携带这些公共 header。请求级同名 header 覆盖全局（请求级优先）。
  非标量值抛异常（fail-fast）。变更触发 Client 重建。
  解决认证 Token、TraceID 等公共 header 无法全局默认的问题。
- **mock_server 新增 `/base-test` 端点**：返回 200 + JSON `{"url": REQUEST_URI, "headers": getallheaders()}`，
  回显实际请求 URL 与请求头，用于测试 base_uri 拼接与 base_headers 合并。

### 测试
- 新增 `php_add_withoptions_and_base_config_test.php`（约 14 项），覆盖：
  withOptions() 批量设置/未知 key 抛异常/null 值跳过/与链式 setter 混用/headers 中 null 值跳过/多次调用累加、
  base_uri 相对 URL 拼接/绝对 URL 优先/末尾斜杠处理/null 清除、
  base_headers 自动携带/请求级覆盖/null 清除、
  base_uri + base_headers 组合使用。


## [1.4.0] - 2026-07-12

本版本为**次版本号升级**（非 BREAKING），聚焦**补齐现代 HTTP 客户端标准便捷方法**，
提升 PHP 使用者日常开发体验。新增 4 个向后兼容方法，覆盖 URL 查询参数构建、
Accept/Content-Type 设置、JSON 响应自动解析四类高频场景。

### 新增
- **`query(array $params): $this`**：增量追加 URL 查询参数，与已有 URL 查询参数合并（非覆盖）。
  多次调用累加参数。支持 int/float/bool/null 标量值（自动转字符串：bool→1/0，null→空字符串）。
  空数组 query([]) 不抛异常（无操作）。非标量元素（嵌套数组/对象）抛异常（fail-fast）。
  补齐 Guzzle/Symfony HTTP Client/Axios 均有的 query 参数构建功能。
- **`accept(string $type): $this`**：设置 Accept header（header('Accept', $type) 的语义化别名）。
  空字符串抛异常（fail-fast，与 userAgent('') 一致）。多次调用覆盖。
- **`contentType(string $type): $this`**：设置 Content-Type header（header('Content-Type', $type) 的语义化别名）。
  空字符串抛异常（fail-fast）。多次调用覆盖。与 json()/form()/multipart() 的自动 Content-Type 设置不冲突。
- **`executeJson(): mixed`**：执行请求并自动 json_decode 响应体（关联数组形式）。
  Content-Type 不含 application/json 时抛异常（含实际类型）。JSON 解析失败抛异常（含错误信息）。
  请求失败（success=false）抛异常（含 error 字段）。不影响 execute() 的返回数组结构。

### 测试
- 新增 `php_add_http_helpers_test.php`，覆盖：
  query() 追加/合并/累加/标量转换/空数组/嵌套数组抛异常、
  accept()/contentType() 设置 header 与空字符串抛异常与多次调用覆盖、
  executeJson() 成功解析/非 JSON Content-Type 抛异常/请求失败抛异常。
- mock_server 新增 /echo-query（回显查询参数）、/echo-json（固定 JSON 响应）、
  /text（text/plain 响应）三个端点。


## [1.3.0] - 2026-07-12

本版本为**次版本号升级**（非 BREAKING），聚焦**修正 P1 文档错误**
（README 截断行为描述与实现不符）、**HTTP 响应超限专属 error_type 分类**
（与 xhrun 的 output_too_large 对齐）、**HTTP 结果数组新增 truncated 字段**
（与 xhrun 字段集对齐）、**扩展 mock_server 测试基础设施**
（新增 /redirect 与 /large 端点）、**补齐重定向与超限测试覆盖**。

### 修复
- **README 截断行为描述与实现矛盾**：原描述"截断时 `success` 仍为 `true`，
  但 `body` 不完整"与实现不符。实际行为：响应体超过 `max_response_size` 时，
  请求被视为失败（`success=false`、`body=""`、`body_size=0`、
  `error` 含"超过最大限制"、`error_type="response_too_large"`、`truncated=true`）。
  部分读取的响应体不返回给 PHP（实现上返回 Err，部分 body 被丢弃）。
  README 已修正为准确描述。
- **`maxRedirects(0)` 返回错误而非 3xx 响应**：原实现使用
  `reqwest::redirect::Policy::limited(0)`，该策略在遇到重定向时返回
  "too many redirects" 错误（status=0），而非返回 3xx 响应。
  修正为 `Policy::none()`（与 `followRedirects(false)` 等价），
  正确返回 302 响应体。影响 `maxRedirects(0)` 和
  `followRedirects(true)->maxRedirects(0)` 两种调用路径。
  README 中"maxRedirects(0) 等价于 followRedirects(false)"的描述现真正成立。

### 新增
- **HTTP 响应超限专属 error_type**：`classify_error_type` 增加对"超过最大限制"或
  "响应体"关键词的识别，返回 `"response_too_large"`。与 `xhrun` 的
  `"output_too_large"` 风格对齐。error_type 值集扩展为
  `dns/timeout/ssl/connection/response_too_large/unknown`，成功路径仍为 `""`。
- **HTTP 结果数组 truncated 字段**：所有 HTTP 响应构建路径
  （execute/XHMulti/XHThreadPool/fiber_await/gather/each）新增 `truncated` 布尔字段，
  默认 `false`。仅当响应体超限时设为 `true`。与 `xhrun` 的 `truncated` 字段对齐。
  HTTP 结果数组字段集从 10 扩展到 11。
- **mock_server 新增 /redirect?n=N 端点**：n>0 返回 302 + Location: /redirect?n=N-1，
  n=0 返回 200 + JSON `{"redirected":true}`。解锁 maxRedirects/followRedirects 测试。
- **mock_server 新增 /large?size=N 端点**：返回 200 + 指定字节数响应体
  （上限 10MB 防止 mock_server OOM）。解锁 HTTP 响应超限测试。

### 测试
- 新增 `php_unify_truncation_and_redirect_test.php`（16 项），覆盖：
  HTTP 响应超限（success=false/body=""/error_type="response_too_large"/truncated=true）、
  成功路径 truncated=false、maxRedirects(0)/maxRedirects(5)/followRedirects(false)/
  followRedirects(true) 对 /redirect 端点的行为、
  error_type 值集（dns/timeout/connection/response_too_large/""）、
  body_size 与 strlen(body) 一致性。
  注意：`maxResponseSize()` 仅存在于 XHMulti/XHThreadPool，XHRequest 通过全局
  `setConfig(['max_response_size' => N])` 设置；DNS 测试兼容沙箱代理环境
  （代理可能拦截 DNS 失败返回 502 而非真正的 DNS 错误）。


## [1.2.0] - 2026-07-12

本版本为**次版本号升级**（含 BREAKING 变更），完成**空字符串 fail-fast 校验扩展**
（userAgent/encoding/range/cookies 与第七轮 proxy 对齐）、**错误消息格式统一**
（bearerToken/maxRedirects/xhrun）、**新增 3 个便捷 setter 与 4 个 getter**
（referer/cookie/jsonStr + getHeader/getMultipart/getReferer/getBody 扩展）、
**maxRedirects(0) 文档化**与测试覆盖补齐。

### BREAKING：空字符串 fail-fast 校验扩展
- **`userAgent('')` 抛异常**：与第七轮 `proxy('')` 一致，空字符串不再静默接受。
  错误消息含"传 null 清除 User-Agent 覆盖"。
- **`encoding('')` 抛异常**：同上，错误消息含"传 null 清除 Accept-Encoding 覆盖"。
- **`range('')` 抛异常**：同上，错误消息含"传 null 清除 Range 覆盖"。
- **`cookies('')` 字符串路径抛异常**：同上，错误消息含"传 null 清除 Cookie 覆盖"。
  （数组路径不受影响，因数组形式无空字符串歧义）
- **迁移**：清除请求级覆盖请用 `null`（如 `->userAgent(null)`），不要传空字符串。

### 新增：3 个便捷 setter
- **`referer(?string $referer): $this`**：设置 Referer header（CURLOPT_REFERER 等价）。
  null 清除，空字符串抛异常，ASCII 校验。补齐 PHP cURL 用户的习惯用法。
- **`cookie(string $name, string $value): $this`**：增量添加单个 cookie，不覆盖已有 cookies。
  与 `cookies()` 的"整体覆盖"语义不同，`cookie()` 追加到现有 Cookie 字符串末尾。
  value 自动 URL 编码（防注入）。name/value 空字符串抛异常。
- **`jsonStr(string $json): $this`**：传入预序列化的 JSON 字符串作为请求体。
  与 `json()` 不同：`json()` 接受 PHP 数组并内部序列化；`jsonStr()` 直接使用字符串，
  避免双重序列化。自动设置 Content-Type: application/json。无效 JSON 抛异常。

### 新增：4 个 getter
- **`getHeader(string $name): ?string`**：大小写不敏感查询单个 header 值。
- **`getMultipart(): ?array`**：返回已设置的 multipart 字段数组（含 name/value/filename/content_type 四键）。
- **`getReferer(): ?string`**：返回 referer() 设置的值。
- **扩展 `getBody(): ?string`**：对 `json()` 返回序列化 JSON 字符串，对 `form()` 返回 `k=v&k=v` 格式字符串。
  `body()` 行为不变；`multipart()` 返回 null（含二进制文件内容，无法安全序列化）。

### 改进：错误消息格式统一
- **`bearerToken('')` 错误消息追加"传 null 清除 Bearer Token"**：与 `proxy('')` 风格对齐。
- **`maxRedirects` 负值错误消息追加"0 = 不跟随重定向"**：与其他数值 setter 的"0 = ..."提示一致。
- **`xhrun` 的 `timeout`/`max_output` 负值错误消息改为"0 = ..."格式**：去掉冗余的"得到 {}"值回显，
  与其他 setter 错误消息风格统一。

### 文档
- README 新增 `referer()`/`cookie()`/`jsonStr()` 方法表行
- README 新增 `getHeader()`/`getMultipart()`/`getReferer()` 方法表行
- README 修改 `getBody()` 说明为返回 body/json/form 序列化字符串
- README 明确说明 `maxRedirects(0)` 等价于 `followRedirects(false)`

### 测试
- 新增 `php_unify_empty_and_helpers_test.php`（约 30 用例），覆盖：
  4 个 setter 空字符串抛异常、错误消息格式验证、3 个新 setter 正常工作与边界、
  4 个新 getter 返回值验证、maxRedirects(0) 行为验证。


## [1.1.0] - 2026-07-12

本版本为**次版本号升级**（含 BREAKING 变更），聚焦**响应字段集最终统一**
（失败路径补 `remote_addr`/`version`）、**fail-fast 校验扩展**
（`proxy`/`command`/`cwd` 空字符串、`allow`/`deny` 标量转换）、
**空请求行为统一**（`fiber_gather([])` 抛异常）与**错误信息优化**。

### 修复
- **失败路径补 `remote_addr`/`version` 字段**：`result_to_php_array` 失败路径
  （无响应分支）原缺这两个字段，导致 PHP 端访问 `$result['remote_addr']` 或
  `$result['version']` 触发 `Undefined index` 警告。现无条件插入空字符串，
  成功/失败路径字段集完全一致（10 个字段全部存在）。
  影响：`execute()`/`XHMulti`/`XHThreadPool`/`fiber_await`/`gather`/`each` 所有失败结果。
- **`proxy('')` 空字符串抛异常**：请求级 `proxy()` setter 原接受空字符串，
  延迟到 `execute()` 时 `reqwest::Proxy::all("")` 报错。现 setter 时校验
  （与 `bearerToken`/`customMethod`/`setId` 一致），fail-fast。
- **`setConfig(['proxy' => ''])` 空字符串校验**：全局 `setConfig` 原接受空代理，
  延迟到下次请求时报错，违反两阶段原子校验。现校验阶段收集到 type_mismatches。
- **`xhrun('')` 空 command 抛异常**：原 `xhrun('', [], [])` 走到 `Command::new("")`
  → `spawn()` 失败，错误信息含 OS 错误细节不友好。现函数开头校验，返回
  `"command 不能为空字符串"`。
- **`xhrun cwd` 空字符串校验**：原 `cwd => ''` 传给 `current_dir("")`，
  OS 行为未定义（Linux 下 `chdir("")` 报 `ENOENT`）。现校验空字符串抛异常。
- **`fiber_gather([])` 空请求抛异常**：原返回空数组 `Ok(ZendHashTable::new())`，
  与 `fiber_each([])`/`XHMulti::execute([])`/`XHThreadPool::execute([])` 抛异常
  行为不一致。现抛 `"XHCurl::gather 没有待执行请求"` 异常，四种执行模式行为统一 **BREAKING**。

### 改进
- **`allow`/`deny` 数组支持标量转换**：原仅接受字符串元素，int/float/bool 元素
  被静默跳过（与 `args` 行为不一致）。现支持标量转换（int/float/bool → string），
  与 `args` 一致；空字符串元素被跳过（无意义的白名单/黑名单条目）。
- **`args` 错误信息含索引**：原 `"args 数组元素必须是标量类型"` 不含具体位置，
  现改为 `"args 数组第 N 个元素必须是标量类型"`，便于定位。
- **`setConfig` 负值错误信息去冗余**：原输出 `"以下配置项为负值: connect_timeout 不能为负值"`
  （"为负值"与"不能为负值"语义重复），现改为 `"以下配置项为负值: connect_timeout"`
  （仅字段名列表，与类型错误格式一致）。

### 文档
- **README setConfig 示例对齐修正**：`'http2_enabled'` 行多余缩进已修正。
- **README `body()` 行为说明**：补充空字符串行为（合法，POST 空 body）。
- **README `getBody()` 返回范围说明**：明确仅返回 `body()` 设置的原始字节请求体，
  `json()`/`form()`/`multipart()` 不在此返回。

### 测试
- 新增 `php_unify_field_and_safety_test.php`（16 项），覆盖：
  失败路径含 `remote_addr`/`version` 空字符串、`proxy('')` 抛异常、
  `setConfig(['proxy' => ''])` 校验失败、`xhrun('')` 空 command 抛异常、
  `xhrun cwd` 空字符串抛异常、`fiber_gather([])` 空请求抛异常、
  `allow`/`deny` 标量转换与空字符串跳过、`args` 错误信息含索引、
  `setConfig` 负值错误信息无冗余。
- 更新 `php_invalid_proxy_test.php`：原测试期望 `setConfig(['proxy' => ''])`
  静默通过、在 `execute()` 时报错（v1.0.7 行为）。本版 `setConfig` 对空字符串
  fail-fast 抛异常，测试相应拆分为两组：空字符串 → `setConfig` 立即抛异常；
  非空但非法代理（`"://"`）→ `execute()` 抛异常（保留"不 panic"保证）。

### 破坏性变更与迁移
- **`XHCurl::gather([])` 现抛异常**：原返回空数组，现抛 `"XHCurl::gather 没有待执行请求"` 异常。
  迁移：调用前 `if (count($requests) > 0)` 检查，或用 `try/catch` 捕获。
  与 `each`/`execute`/`execute_each` 空请求行为一致。
- **`setConfig(['proxy' => ''])` 现抛异常**：原静默接受空代理（跳过赋值），
  延迟到下次 `execute()` 时 `global_client()` 报错；现 `setConfig` 校验阶段
  将空字符串收集到 `type_mismatches`，与类型不匹配一致 fail-fast。
  迁移：清除代理请用 `setConfig(['proxy' => null])`。
- 其余变更（`proxy('')`/`xhrun('')`/`cwd` 校验、失败路径补字段）为字段集/校验时机的
  向后兼容增强，不影响已通过 `isset()` 判断的代码。


## [1.0.9] - 2026-07-12

本版本聚焦**API 对称性补齐**（XHThreadPool 新增 `clear()`/`isRunning()`）、
**fail-fast 校验扩展**（`url`/`setId`/`customMethod` 空字符串、`max_output` 负值）与
**文档同步**。包含 2 项破坏性变更。

### 新增
- **`XHThreadPool::clear()` 方法**：与 `XHMulti::clear()` 对齐，清空已添加的请求列表，
  允许复用同一 `XHThreadPool` 对象。此前 `XHThreadPool` 无此方法，PHP 用户无法手动清空。
- **`XHThreadPool::isRunning()` 方法**：暴露内部线程池工作状态，
  `execute()`/`executeEach()` 执行期间为 true，未启动/已完成时为 false。
  PHP 用户可在异步场景判断是否可安全修改配置。

### 修复
- **`url('')`/`setId('')`/`id('')` 空字符串抛异常**：原 `url('')`/`setId('')` 接受空字符串
  并存储为 `Some("")`，与"未设置"（`None`）不可区分。`getId()` 返回空字符串而非 null，
  README 文档说"未设置返回 null"但实际返回空字符串。现空字符串抛异常
  （与 `bearerToken`/`basicAuth` 一致），fail-fast **BREAKING**。
- **`customMethod('')` 空字符串与控制字符抛异常**：原 `customMethod('')` 接受空字符串，
  `customMethod("GET POST")` 含空格不被拒绝。现空字符串抛异常（与 `bearerToken` 一致），
  含空格/控制字符抛异常（RFC 7230 要求 method 为 token）**BREAKING**。
- **`xhrun` `max_output` 负值抛异常**：原 `max_output: -1` 静默变为无限制（`usize::MAX`），
  与 `timeout` 负值抛异常不一致。现负值抛异常（与 `timeout` 一致），0 仍表示无限制 **BREAKING**。

### 文档
- **README XHMulti/XHThreadPool 方法表补全 `clear()`**：原 `XHMulti::clear()` 代码存在
  但 README 未列出。现两个类的方法表均列出 `clear()`，`XHThreadPool` 表新增 `isRunning()`。
- **README 失败路径字段描述修正**：原称 `url`/`remote_addr`/`version` "可能为空或缺失"，
  但代码中这些字段无条件插入（None 时为空字符串）。现改为"为空字符串（字段始终存在）"，
  与实际行为一致。
- **README `getBody()` 描述修正**：原称获取 `body()`/`json()`/`form()` 设置的请求体，
  但代码仅返回 `body()` 设置的原始字节体。现明确说明 `json()`/`form()`/`multipart()`
  设置的请求体不在此返回。
- **README xhrun `error_type` 补全 `spawn_failed`**：原列出 4 个枚举值，
  缺少 `spawn_failed`（子进程 spawn 失败时返回）。现补全为 5 个值。

### 测试
- 新增 `php_unify_clear_and_validation_test.php`（12 项），覆盖：
  `XHThreadPool::clear()` 清空请求、`XHThreadPool::isRunning()` 状态查询、
  `url('')`/`setId('')`/`id('')`/`customMethod('')` 空字符串抛异常、
  `customMethod("GET POST")` 含空格抛异常、`xhrun` `max_output` 负值抛异常。

### 破坏性变更与迁移
- **`url('')`/`setId('')`/`id('')`/`customMethod('')` 现抛异常**：原接受空字符串。
  迁移：调用前检查字符串非空，或用 `try/catch` 捕获。实际场景中空字符串往往是上游 bug。
- **`customMethod` 含空格/控制字符现抛异常**：原不校验。
  迁移：确保传入的 method 为合法 RFC 7230 token（无空格/控制字符）。
- **`xhrun` `max_output` 负值现抛异常**：原静默变为无限制。
  迁移：用 `0` 表示无限制（语义不变），负值无意义应移除。


## [1.0.8] - 2026-07-12

本版本聚焦**响应数组字段集在所有路径（HTTP 成功/失败、xhrun 成功/失败、单请求/批量/协程）的最终统一**
与**入口参数 fail-fast 校验**，并清理死代码。包含 1 项破坏性变更。

### 修复
- **xhrun 成功路径补 `error_type`/`error`/`command` 字段**：原 xhrun 成功路径仅返回
  8 个字段（`success`/`exit_code`/`stdout`/`stderr`/`elapsed_ms`/`pid`/`timed_out`/`truncated`），
  失败路径有 11 个（多 `error`/`command`/`error_type`），字段集不一致。
  现成功路径插入空 `error_type=""`、空 `error=""` 与 `command`，使成功/失败字段集完全一致，
  与 HTTP API（`execute()`/`XHMulti`/`XHThreadPool`/协程 `gather`/`each`）字段集统一的风格对齐。
  修正源码错误注释（原声称"成功路径不插入 error_type（与 execute() 一致）"实际不一致）。
- **`fiber_each` 空请求抛异常**：原 `XHCurl::each([], $cb)` 返回 `Ok(0)`，与
  `XHMulti::executeEach`/`XHThreadPool::executeEach` 空请求抛异常行为不一致。
  现抛出 `"XHCurl::each 没有待执行请求"` 异常，三种执行模式空请求行为对齐 **BREAKING**。
- **`createRequest('')`/`new XHRequest('')` 空字符串 URL 抛异常**：原空字符串 URL 延迟到
  `execute()` 才报错（fail-late），现 `createRequest` 与 `XHRequest::__construct` 入口即校验
  （fail-fast），错误信息含"url"与"空"，避免用户构造了无效请求后链式调用多个 setter 才发现问题。

### 重构
- **删除 `XhResponse::to_info_map` 死代码**：该方法仅被其自身的单元测试调用，
  生产代码无任何引用。删除方法及其测试 `test_to_info_map`。

### 文档
- **README `error_type` 说明修正**：原称"成功路径不含此字段"，实际第四轮已改为插入空字符串。
  现更新为"成功时为空字符串（字段始终存在）"，并补充 `error` 字段"成功时为空字符串"说明。
- **README xhrun 字段表同步**：更新 `error`/`error_type`/`command` 三字段说明，
  标注"始终存在，成功/失败路径字段集一致"，并补充 `denied` 错误类型枚举值。

### 测试
- 新增 `php_unify_xhrun_fields_and_upgrade_test.php`（10 项），覆盖：
  xhrun 成功路径含 `error_type=""`/`error=""`/`command`、xhrun 失败路径字段集与成功一致、
  `XHCurl::each` 空请求抛异常、`XHCurl::createRequest('')` 与 `new XHRequest('')` 抛异常。
- 更新 `php_each_test.php` 中 `test_each_empty_returns_0` → `test_each_empty_throws`
  （断言改为 expect throws，与新行为一致）。

### 破坏性变更与迁移
- **`XHCurl::each([], $cb)` 现抛异常**：原返回 `0`，现抛 `"XHCurl::each 没有待执行请求"` 异常。
  迁移：调用前 `if (count($requests) > 0)` 检查，或用 `try/catch` 捕获。
- 其余变更（xhrun 成功路径补字段、`createRequest('')` 抛异常）为字段集/校验时机的向后兼容增强，
  不影响已通过 `isset($r['error_type'])` 判断的代码（字段从无到有，`isset` 仍返回 `true`）。


## [1.0.7] - 2026-07-11

本版本为三种执行模式（协程 `each` / `XHMulti::executeEach` / `XHThreadPool::executeEach`）的
流式回调补齐了**响应体分块级流式**（`onChunk`/`onHeaders`）、**行为契约对齐**和**返回值控制中止**三大能力，
让 PHP 使用者能根据业务情况显式控制回调处理流程。

### 新增
- **`onChunk`/`onHeaders` 响应体分块级流式回调**：`XHMulti::executeEach` 和 `XHThreadPool::executeEach`
  新增两个可选参数 `?callable $onChunk = null` 和 `?callable $onHeaders = null`（向后兼容）。
  - `$onChunk(string $requestId, string $chunk): void` —— 每收到一块响应体时触发（二进制安全）
  - `$onHeaders(string $requestId, int $status, array $headers): void` —— 收到响应头时触发
  - 所有 chunk 拼接后等于完整响应体（与 `$result['body']` 一致）
- **回调返回值控制中止**：`$onResult` 回调返回 `false`（严格 `=== false`）时中止剩余任务，
  方法返回已处理数（`int`，**不视为错误**）。返回 `true`/`null`/`void`/其他值继续处理。
  抛异常仍中止（向后兼容）。让使用者能显式决定遇到业务异常时继续还是中断，无需用异常控制流程。
- **核心层流式能力暴露**：`StreamEvent`（Headers/Chunk/Complete/Error）通过线程安全 mpsc channel
  从 tokio 工作线程传递到 PHP 线程，PHP 回调仅在 `block_on` 当前线程调用，确保线程安全。
- **`mock_server.php` 新增 `/stream` 端点**：分块输出大响应体（`flush()` 确保分段发送），
  供 `onChunk` 多次触发验证。参数：`n`（段数）、`size`（每段字节数）。
- **测试新增**：`php_streaming_test.php`（28 项）、`php_callback_abort_test.php`（11 项）、
  `php_return_value_abort_test.php`（15 项），覆盖流式分块、行为契约一致性、返回值控制中止。

### 修复
- **`XHThreadPool::executeEach` 回调异常不中止剩余任务**：原仅 `break` 退出循环，pool 被存回复用，
  已提交的请求继续在 worker 上执行。修复为回调异常时不存回 pool（让 pool 被 drop，`Drop` 实现 abort
  dispatcher + workers），与协程 `each`（`SchedulerGuard::drop`）和 `XhMulti::executeEach`（`abort_tasks()`）
  行为一致。

### 增强
- **流式事件 drain 机制**：主收集循环结束后 `try_recv` 排空 stream channel 残留事件，
  确保用户回调收到完整的分块数据（避免尾部 chunk 丢失）。
- **null 参数处理**：`Option<&Zval>` 参数正确处理 PHP `null`（视为未传），用户可显式传 `null`
  跳过 `onChunk` 只用 `onHeaders`。
- **`invoke_streaming_callback` 返回 `Result<bool, String>`**：`Ok(true)` 继续、`Ok(false)` 使用者请求中止、
  `Err(msg)` 异常中止，复用于 `XhMulti` 和 `XHThreadPool` 两条路径。
- **`fiber_each` 检查回调返回值**：协程 `each()` 的回调返回 `is_false()` 时提前退出循环返回 `Ok(count)`。

### 文档
- README 核心特性表新增"流式回调"行。
- 新增"流式回调类型"小节，明确区分请求级（`onResult`）与响应体分块级（`onChunk`/`onHeaders`）。
- 新增"请求级流式回调行为契约"小节，明确三种模式都支持请求级流式回调 + 统一行为契约 + 三者对比表。
- 新增"回调返回值控制中止"子小节，说明 `false` 中止、其他值继续、抛异常仍中止 + 业务场景示例。
- 更新 `executeEach` 签名表（XHMulti + XHThreadPool）。
- 补充 `onChunk`/`onHeaders` 使用示例。
- `each()` 章节澄清协程仅支持请求级流式。
- 故障排查新增"流式回调不触发"条目。


## [1.0.7] - 2026-07-11

本版本聚焦**错误处理健壮性与 CI 质量保障**：消除所有 panic 路径（改返回 PHP 异常或结果数组）、
统一失败路径字段、CI 启用 `--features php` 全量检查。无破坏性变更。

### 修复
- **`execute()` 统一返回结果数组**：网络/DNS/TLS 错误原抛 PHP 异常，与 `XHMulti`/fiber 路径不一致。
  现包装为 `success=false` 结果数组（含 `status: 0`、`error` 字段），用户统一检查 `$r['success']` 即可。
- **`global_client()`/`global_runtime()` 不再 panic**：初始化失败（如代理无效）原 `expect` 直接
  panic 杀死 PHP 进程（FPM worker 崩溃重启）。改为返回 `Result`，错误以 PHP 异常形式抛出，
  用户可 try/catch 并修正配置后重试。
- **RwLock 中毒恢复**：`curl.rs`（7 处）和 `header.rs`（8 处）的 `.read().unwrap()`/`.write().unwrap()`
  在锁中毒时 panic。改为 `unwrap_or_else(|e| e.into_inner())`，取中毒锁中的数据继续执行，避免 panic。
- **fiber.rs `expect` 改为优雅传播**：5 处 `.expect("调度器未初始化")` 改为 `if let Some` + 提前返回错误；
  `XHThreadPool::execute`/`execute_each` 的 `.expect("线程池已初始化")` 改为 `?` 传播。
- **失败路径字段补齐**：`result_to_php_array` 失败分支原仅有 `status/elapsed_ms/body/error`，
  补充 `headers => []`、`body_size => 0`、`url => ""`，确保失败路径字段集与成功路径完全一致。
- **`setConfig` 接受 null proxy**：`getConfig()` 返回 `proxy => null` 后 `setConfig($orig)` 往返
  报类型不匹配错误。`setConfig` 现接受 null（视为清除代理），与 `getConfig` 对称。

### 增强
- **`getConfig()` 的 `proxy` 始终返回**：原 proxy 为 None 时 `getConfig()` 不含 `proxy` 键，
  用户无法区分"未设置"和"获取失败"。现始终插入（None 时为 `null`），与 `setConfig` 接受 `null` 对称。
- **新增 `XHRequest::options()` 快捷方法**：与 `get()`/`post()`/`put()`/`delete()` 等一致，
  补齐 HTTP OPTIONS 方法的链式快捷方法。

### 文档
- README setConfig 示例补充 `http2_enabled => true`。
- 新增"错误处理统一"说明：`execute()` 网络错误返回 `success=false` 而非抛异常。
- 故障排查补充 3 条目：请求超时/连接失败、代理配置无效、响应体超限。

### CI 质量保障
- **clippy/test 启用 `--features php`**：原 CI 未启用 php feature，`php_ext.rs`/`fiber.rs`
  约 2400 行代码不参与编译检查。现 clippy 改为 `--all-targets --features php -- -D warnings`，
  test 改为 `--lib --features php`。
- **扩展加载验证有效化**：移除 `|| true` 容忍失败，改为断言式验证。
- **新增 PHP 测试套件执行**：CI 编译扩展后运行 `rust/tests/php_*.php`。
  新增 `mock_server.php`（PHP 内置服务器）提供 `/get`、`/post`、`/hang` 端点，
  CI 启动后供网络相关测试使用。
- macOS PHP 版本注释修正为 8.1~8.5。

### 测试
- **`test_drop_aborts_tasks` 改为真测试**：原仅验证空 multi 的 Drop 不 panic。改为添加 TEST-NET-1
  请求 + `spawn_all` + drop，用超时保护 `recv()` 验证 channel 关闭（任务被 abort）。
- **`test_global_manager_config` 避免触碰全局单例**：改用 `XhCurlManager::new(GlobalConfig::default())`
  独立实例，避免并行测试间全局状态污染。`XhCurlManager::new` 改为 `pub` 供测试使用。


## [1.0.6] - 2026-07-11

本版本从**使用者视角**全面审查并优化代码与文档，聚焦链式调用体验、API 命名一致性、
配置字段实际生效、失败路径字段完整性与文档对齐实现。无破坏性变更。

### 修复
- **fiber_each 并发上限读取配置**：`fiber_each` 硬编码 `total.min(64)`，导致用户
  `setConfig(['fiber_max_concurrency' => 128])` 后 `gather()` 生效但 `each()` 仍为 64。
  统一读取 `GlobalConfig.fiber_max_concurrency`，与 `gather()` 行为一致。
- **XhMulti 实现 Drop 防任务泄漏**：`XhMulti` 持有 `tasks: Vec<JoinHandle<()>>` 但无 Drop，
  `spawn_all` 后 panic/早期返回时后台任务继续运行泄漏连接。新增 Drop 实现调用 `abort_tasks()`，
  参考 `XhThreadPool` 的 Drop 模式。
- **id 字段默认值统一为 URL**：fiber 路径 `await/gather/each` 默认 `"task-{N}"` 与同步 `execute()`
  默认 URL 不一致。统一为未设置 `setId()` 时默认为请求 URL（与文档一致）。
- **失败响应补 `status: 0` 字段**：请求失败时 `result_to_php_array` 不写 status 字段，用户访问
  `$r['status']` 触发未定义索引警告。补 `status => 0`（哨兵值）和 `body => ""`，确保失败路径
  字段集与成功路径一致。

### 增强
- **链式 setter 统一返回 `&mut Self`**：`method()`/`json()`/`form()`/`multipart()`/`setUserData()`
  原返回 `Result<&mut Self, String>` 破坏链式调用（PHP 端需 `?` 或 `unwrap`）。改为失败时跳过本次
  设置并返回 `&mut Self`，用户可写 `createRequest($url)->get()->json([...])->timeout(10)->execute()`。
- **新增 `id()`/`userData()` 无前缀别名**：与其余 18 个无 `set` 前缀的链式 setter 风格一致。
  保留 `setId`/`setUserData` 旧名为别名，向后兼容。
- **负值处理统一为跳过**：`timeout`/`connectTimeout`/`maxRedirects`/`XHMulti::timeout`/
  `maxConcurrency`/`maxResponseSize`/`XHThreadPool::__construct` 负值原 clamp 到 0（语义混乱），
  统一为跳过本次设置（保留原值），与 `setConfig` 现有行为一致。
- **http2_enabled 实际生效**：`GlobalConfig.http2_enabled` 字段存在但 `create_client_builder` 从不读取，
  用户通过 `setConfig` 以为可配置实则无效。现 `false` 时显式 `.http1_only()` 禁用 HTTP/2，
  `true` 时保持默认协商。

### 重构
- **移除 `use_multi_thread` 死字段**：`GlobalConfig.use_multi_thread` 仅 `default()` 和测试出现，
  `set_config`/`get_config` 未暴露，`create_client_builder` 不读。运行时类型由 `sapi_is_cli()` 决定，
  此字段无实际作用，直接删除。

### 文档
- **README 补全 4 个漏列方法**：`XHCurl::each()`、`XHMulti::timeout()`、`XHMulti::executeEach()`、
  `XHThreadPool::executeEach()`，含签名、回调签名、返回值、示例。
- **修正 FPM/CLI 能力表**：协程 `run/await/gather/each` 仅 CLI 可用（实现中 FPM 显式拒绝），
  README 表格原声称 FPM 支持协程是错误的。协程章节顶部加 CLI-only 警告。
- **响应字段表区分成功/失败路径**：新增"失败路径字段说明"小节，标注 `status` 失败时为 0（哨兵）、
  `body` 为空字符串、`id` 未设置时默认为 URL（所有路径统一）。
- **方法表补充双名格式**：`setId`/`id`、`setUserData`/`userData` 均列出，示例改用新名。
- **故障排查新增 FPM 下调用 run() 报错条目**。

### 测试
- 新增 `test_error_result_response_none_ensures_status_zero_sentinel` 固化失败路径数据契约
  （`response.is_none()` → `status: 0` 哨兵逻辑的前提条件）。
- 请求级 Client 缓存测试改为 `contains_key()` 断言 + `unwrap_or_else` 处理中毒 Mutex，
  确保并行测试安全（原 `len()` 断言在并行运行时因共享全局缓存而 flaky）。
- PHP 运行时冒烟验证：链式调用无需 `?`、`id()`/`userData()` 新别名可用、旧名向后兼容、
  `fiber_max_concurrency` 配置生效、`http2_enabled=false` 生效、负值跳过不 crash、
  失败 `status=0`、`id` 默认为 URL 均通过。


## [1.0.5] - 2026-07-10

本版本完成代码审计剩余 4 项优化（P2.4 / P2.5 / P3.11 / P3.16），无破坏性变更。

### 重构
- **execute_each 代码去重**（P2.4）：抽取 `XhMulti::spawn_all()` / `abort_tasks()` /
  `join_tasks()` 公共方法，`PhpXhMulti::execute_each` 改为委托调用，消除约 130 行与
  `XhMulti::execute` 重复的 spawn/collect 逻辑。`PhpXhThreadPool::execute_each` 因采用
  worker+ResultMessage 模型（不共享 spawn 逻辑）保持不变。
- **请求级 Client 连接复用**（P3.16）：`request.rs::build_request_client` 新增按
  `OverrideKey`（follow_redirects/max_redirects/verify_ssl/proxy/connect_timeout 组合）缓存
  Client 的机制，同类请求复用同一 Client（含连接池），避免每次新建 Client 丢失连接复用。
  `reqwest::Client` 内部为 Arc，clone 廉价。`setConfig()` 变更后通过
  `clear_request_client_cache()` 主动失效缓存。

### 安全
- **xhrun shell 模式参数转义**（P2.5）：`shell => true` 时对每个 arg 按平台转义后再拼接：
  Unix 用单引号包裹 + `'\''` 转义内嵌单引号；Windows 用双引号包裹 + `^` 抑制
  `& | < > ^ ( ) %` 元字符，杜绝命令注入。新增 6 个单元测试覆盖转义逻辑。

### 增强
- **fiber gather/each 并发上限可配置**（P3.11）：`GlobalConfig` 新增
  `fiber_max_concurrency`（默认 64，0 = 不限制），`gather()`/`each()` 读取该配置决定
  Semaphore 容量，与 `XhMulti` 行为一致。可通过
  `XHCurl::setConfig(['fiber_max_concurrency' => N])` 调整，`getConfig()` 同步返回。

### 测试
- 新增 3 个请求级 Client 缓存单元测试（命中/未命中/清空）。
- 新增 6 个 xhrun shell 转义单元测试（Unix/Windows 双平台）。
- PHP 运行时冒烟验证：扩展加载、`fiber_max_concurrency` 配置读写、xhrun shell 转义、
  Client 缓存端到端均通过。

### 文档
- README 补充 `fiber_max_concurrency` 配置项说明。


## [1.0.4] - 2026-07-10

### 安全
- **响应体大小硬上限**：`spawn_output_reader` 使用 `checked_add` + `is_none_or` 防止字节累加
  整数溢出，超过 `max_response_size` 时截断并标记，防止恶意超大响应导致 OOM。

### 修复
- **fiber_await 上下文校验前置**：`XHCurl::await()` 在 spawn HTTP 任务前即校验是否处于
  Fiber 上下文，避免无谓的 tokio 任务创建后才报错。
- **execute_all 陈旧结果清理**：`XhThreadPool::execute_all` 在提交请求前 drain 上次调用
  残留的 `result_rx` 消息，避免陈旧结果污染本次返回（已修复 drain 位置错放导致测试挂起）。
- **task_handles 无界增长**：`run_event_loop` 单次轮询结束后 `retain(|h| !h.is_finished())`
  回收已完成 tokio 任务句柄，防止长轮询场景句柄数组无限增长。
- **get_config 缺字段**：`getConfig()` 补充返回 `tcp_keepalive_interval`，与 `setConfig`
  可配置项对齐。
- **extract_requests 提前检查上限**：在克隆元素前用 `len()` 检查
  `MAX_REQUESTS_PER_BATCH`，避免先克隆全部元素再拒绝导致内存浪费/OOM。
- **setConfig 类型不匹配静默忽略**：12 个配置项收集类型不匹配项，统一返回错误信息，
  而非静默忽略错误配置。
- **负数配置项处理**：`connect_timeout`/`request_timeout`/`max_response_size`/
  `max_redirects` 等数值配置项负值被跳过（保留原值），不触发类型转换异常或 panic。

### 重构
- **PhpXhResponse 死代码移除**：删除从未对外暴露的 `PhpXhResponse` 类及 4 个仅被其调用的
  `json_*` helper 函数（`json_to_php_array` 等）。所有 PHP API 仍直接返回关联数组。
- **迭代风格统一**：`extract_requests`/`opt_string_vec` 等手动 for 循环改用项目已有的
  `for_each_kv` 辅助函数，规避 ext-php-rs `Iter` 终止路径的空指针风险。

### 文档
- README 补充 `tcp_keepalive_interval` / `max_connections` 配置项。
- README 更新 `setConfig` 类型校验与负数处理说明。
- README 移除已删除的 `XHResponse` 内部类型说明。
- 新增本 CHANGELOG。

### 测试
- 新增 4 项 PHP 测试：`test_get_config_has_tcp_keepalive_interval`、
  `test_oversized_array_rejected_before_clone`、
  `test_set_config_wrong_type_returns_error`、`test_set_config_correct_type_applies`，
  PHP 测试套件总计 39 passed。
