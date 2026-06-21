/* +----------------------------------------------------------------------+
 * | XHCurl 扩展 - XHRequest 请求构建器类实现                             |
 * | 构建单个 HTTP 请求的配置，支持链式调用                                |
 * | 支持设置请求方法、头部、Cookie、请求体、超时、重定向、流式回调等      |
 * +----------------------------------------------------------------------+
 */

#include "xhcurl_priv.h"

/* 类入口指针 */
zend_class_entry *xhrequest_ce;

/* XHRequest 自定义对象操作函数表（用于注册 free_obj，确保对象销毁时释放资源） */
static zend_object_handlers xhrequest_object_handlers;

/* +----------------------------------------------------------------------+
 * | 对象生命周期函数                                                      |
 * +----------------------------------------------------------------------+
 */

/**
 * 释放 XHRequest 对象资源
 * @param object zend_object 指针
 */
static void xhrequest_free_obj(zend_object *object)
{
    /* 从 zend_object 获取 XHRequest 对象 */
    xhrequest_obj_t *obj = XHREQUEST_OBJ_FROM_ZOBJ(object);

    /* 释放 URL 字符串 */
    if (obj->url != NULL) {
        efree(obj->url);
        obj->url = NULL;
    }

    /* 释放 HTTP 方法字符串 */
    if (obj->method != NULL) {
        efree(obj->method);
        obj->method = NULL;
    }

    /* 释放请求级别头部链表 */
    if (obj->headers != NULL) {
        xhcurl_header_list_free(obj->headers);
        obj->headers = NULL;
    }

    /* 释放请求级别 Cookie 链表 */
    if (obj->cookies != NULL) {
        xhcurl_cookie_list_free(obj->cookies);
        obj->cookies = NULL;
    }

    /* 释放请求体数据 */
    if (obj->body != NULL) {
        efree(obj->body);
        obj->body = NULL;
    }

    /* 释放请求级代理地址 */
    if (obj->proxy != NULL) {
        efree(obj->proxy);
        obj->proxy = NULL;
    }

    /* 释放流式数据回调引用 */
    if (!Z_ISUNDEF(obj->chunk_callback)) {
        zval_ptr_dtor(&obj->chunk_callback);
        ZVAL_UNDEF(&obj->chunk_callback);
    }

    /* 释放响应头回调引用 */
    if (!Z_ISUNDEF(obj->header_callback)) {
        zval_ptr_dtor(&obj->header_callback);
        ZVAL_UNDEF(&obj->header_callback);
    }

    /* 调用标准对象释放函数 */
    zend_object_std_dtor(object);
}

/**
 * 创建 XHRequest 对象
 * @param class_type 类入口指针
 * @return zend_object 指针
 */
static zend_object *xhrequest_create_obj(zend_class_entry *class_type)
{
    /* 分配对象内存 */
    xhrequest_obj_t *obj = (xhrequest_obj_t *)zend_object_alloc(sizeof(xhrequest_obj_t), class_type);

    /* 初始化 PHP 标准对象 */
    zend_object_std_init(&obj->std, class_type);
    /* 初始化对象属性 */
    object_properties_init(&obj->std, class_type);

    /* 初始化自定义字段为默认值 */
    obj->url = NULL;                    /* URL 由构造函数设置 */
    obj->method = estrdup("GET");       /* 默认 GET 方法 */
    obj->headers = NULL;                /* 无请求级头部 */
    obj->cookies = NULL;                /* 无请求级 Cookie */
    obj->body = NULL;                   /* 无请求体 */
    obj->body_len = 0;                  /* 请求体长度为 0 */
    obj->timeout = 0;                   /* 0 表示使用全局默认值 */
    obj->connect_timeout = 0;           /* 0 表示使用全局默认值 */
    obj->follow_redirects = 1;          /* 默认跟随重定向 */
    obj->max_redirects = XHCURL_DEFAULT_MAX_REDIRECTS; /* 默认最大重定向次数 */
    obj->proxy = NULL;                  /* NULL 表示使用全局代理 */
    ZVAL_UNDEF(&obj->chunk_callback);   /* 无流式回调 */
    ZVAL_UNDEF(&obj->header_callback);  /* 无头部回调 */

    /* 设置自定义对象操作函数表（包含 free_obj，确保对象销毁时调用 xhrequest_free_obj） */
    obj->std.handlers = &xhrequest_object_handlers;

    return &obj->std;
}

/* +----------------------------------------------------------------------+
 * | PHP 方法实现                                                          |
 * +----------------------------------------------------------------------+
 */

/**
 * 构造函数
 * XHRequest::__construct(string $url)
 */
PHP_METHOD(XHRequest, __construct)
{
    char *url;          /* URL 参数 */
    size_t url_len;     /* URL 长度 */

    /* 解析参数 */
    ZEND_PARSE_PARAMETERS_START(1, 1)
        Z_PARAM_STRING(url, url_len)
    ZEND_PARSE_PARAMETERS_END();

    /* 获取当前对象 */
    xhrequest_obj_t *obj = XHREQUEST_OBJ_FROM_ZOBJ(Z_OBJ_P(getThis()));

    /* 释放已有的 URL（防止重复调用构造函数导致内存泄漏） */
    if (obj->url != NULL) {
        efree(obj->url);
    }

    /* 复制 URL 字符串 */
    obj->url = estrdup(url);
}

/**
 * 设置 HTTP 方法
 * XHRequest::setMethod(string $method): static
 */
PHP_METHOD(XHRequest, setMethod)
{
    char *method;       /* HTTP 方法参数 */
    size_t method_len;  /* 方法字符串长度 */

    /* 解析参数 */
    ZEND_PARSE_PARAMETERS_START(1, 1)
        Z_PARAM_STRING(method, method_len)
    ZEND_PARSE_PARAMETERS_END();

    /* 获取当前对象 */
    xhrequest_obj_t *obj = XHREQUEST_OBJ_FROM_ZOBJ(Z_OBJ_P(getThis()));

    /* 释放旧的方法字符串 */
    if (obj->method != NULL) {
        efree(obj->method);
    }

    /* 复制新的方法字符串 */
    obj->method = estrdup(method);

    /* 返回 $this 支持链式调用 */
    RETURN_ZVAL(getThis(), 1, 0);
}

/**
 * 设置请求级别头部
 * XHRequest::setHeader(string $name, string $value): static
 * 请求级头部会与全局头部合并，同名头部请求级优先
 */
PHP_METHOD(XHRequest, setHeader)
{
    char *name;         /* 头部名称 */
    size_t name_len;    /* 名称长度 */
    char *value;        /* 头部值 */
    size_t value_len;   /* 值长度 */

    /* 解析参数 */
    ZEND_PARSE_PARAMETERS_START(2, 2)
        Z_PARAM_STRING(name, name_len)
        Z_PARAM_STRING(value, value_len)
    ZEND_PARSE_PARAMETERS_END();

    /* 获取当前对象 */
    xhrequest_obj_t *obj = XHREQUEST_OBJ_FROM_ZOBJ(Z_OBJ_P(getThis()));

    /* 设置请求级头部（同名头部会替换旧值，避免重复） */
    xhcurl_header_set(&obj->headers, name, value);

    /* 返回 $this 支持链式调用 */
    RETURN_ZVAL(getThis(), 1, 0);
}

/**
 * 设置请求级别 Cookie
 * XHRequest::setCookie(string $name, string $value): static
 */
PHP_METHOD(XHRequest, setCookie)
{
    char *name;         /* Cookie 名称 */
    size_t name_len;    /* 名称长度 */
    char *value;        /* Cookie 值 */
    size_t value_len;   /* 值长度 */

    /* 解析参数 */
    ZEND_PARSE_PARAMETERS_START(2, 2)
        Z_PARAM_STRING(name, name_len)
        Z_PARAM_STRING(value, value_len)
    ZEND_PARSE_PARAMETERS_END();

    /* 获取当前对象 */
    xhrequest_obj_t *obj = XHREQUEST_OBJ_FROM_ZOBJ(Z_OBJ_P(getThis()));

    /* 添加到请求级 Cookie 链表 */
    xhcurl_cookie_add(&obj->cookies, name, value, NULL, NULL);

    /* 返回 $this 支持链式调用 */
    RETURN_ZVAL(getThis(), 1, 0);
}

/**
 * 设置请求体
 * XHRequest::setBody(string $body): static
 */
PHP_METHOD(XHRequest, setBody)
{
    char *body;         /* 请求体数据 */
    size_t body_len;    /* 请求体长度 */

    /* 解析参数 */
    ZEND_PARSE_PARAMETERS_START(1, 1)
        Z_PARAM_STRING(body, body_len)
    ZEND_PARSE_PARAMETERS_END();

    /* 获取当前对象 */
    xhrequest_obj_t *obj = XHREQUEST_OBJ_FROM_ZOBJ(Z_OBJ_P(getThis()));

    /* 释放旧的请求体数据 */
    if (obj->body != NULL) {
        efree(obj->body);
    }

    /* 复制新的请求体数据 */
    obj->body = estrndup(body, body_len);
    obj->body_len = body_len;

    /* 返回 $this 支持链式调用 */
    RETURN_ZVAL(getThis(), 1, 0);
}

/**
 * 设置请求级代理
 * XHRequest::setProxy(string $proxy): static
 *
 * 设置请求级代理，覆盖全局代理设置。
 * - 非空字符串：使用指定代理（如 "http://proxy:8080"）
 * - 空字符串 ""：禁用代理（即使全局设置了代理也不使用）
 * - 未调用此方法：使用全局代理（XHCurl::setProxy 设置的值）
 *
 * 与全局 Cookie / 请求级 Cookie 的优先级模式一致：
 * 请求级设置覆盖全局设置，空值表示"显式禁用"。
 *
 * @param string $proxy 代理地址（空字符串 = 禁用代理）
 */
PHP_METHOD(XHRequest, setProxy)
{
    char *proxy;        /* 代理地址参数 */
    size_t proxy_len;   /* 代理地址长度 */

    /* 解析参数 */
    ZEND_PARSE_PARAMETERS_START(1, 1)
        Z_PARAM_STRING(proxy, proxy_len)
    ZEND_PARSE_PARAMETERS_END();

    /* 获取当前对象 */
    xhrequest_obj_t *obj = XHREQUEST_OBJ_FROM_ZOBJ(Z_OBJ_P(getThis()));

    /* 释放旧的代理地址 */
    if (obj->proxy != NULL) {
        efree(obj->proxy);
        obj->proxy = NULL;
    }

    /* +--------------------------------------------------------------+
     * | 代理地址语义：                                               |
     * | - 非空字符串：复制为请求级代理（覆盖全局代理）               |
     * | - 空字符串 ""：复制为请求级代理（显式禁用代理）              |
     * |   空字符串在 xhcurl_context_create 中会被特殊处理：          |
     * |   检测到 proxy_len == 0 时不设置 CURLOPT_PROXY，             |
     * |   同时跳过全局代理，实现"禁用代理"语义                       |
     * +--------------------------------------------------------------+
     */
    obj->proxy = estrndup(proxy, proxy_len);

    /* 返回 $this 支持链式调用 */
    RETURN_ZVAL(getThis(), 1, 0);
}

/**
 * 设置 JSON 格式请求体
 * XHRequest::setJsonBody(array $data): static
 * 自动将数组转为 JSON 字符串并设置 Content-Type
 */
PHP_METHOD(XHRequest, setJsonBody)
{
    zval *data;         /* 数组数据参数 */
    zval json_ret;      /* json_encode 返回值 */
    zval json_args[1];  /* json_encode 参数数组 */

    /* 解析参数 */
    ZEND_PARSE_PARAMETERS_START(1, 1)
        Z_PARAM_ARRAY(data)
    ZEND_PARSE_PARAMETERS_END();

    /* 获取当前对象 */
    xhrequest_obj_t *obj = XHREQUEST_OBJ_FROM_ZOBJ(Z_OBJ_P(getThis()));

    /* 调用 PHP 内置函数 json_encode 进行编码 */
    ZVAL_COPY(&json_args[0], data);
    ZVAL_UNDEF(&json_ret);

    /* 优先使用 MINIT 阶段缓存的函数指针（避免每次调用都做哈希查找） */
    if (xhcurl_json_encode_func != NULL) {
        /* 使用缓存的函数指针直接调用，性能更优 */
        zend_call_known_function(xhcurl_json_encode_func, NULL, NULL,
                                  &json_ret, 1, json_args, NULL);
    } else {
        /* 回退方案：通过函数名查找调用（json 扩展未加载时） */
        zval func_name_zv;
        ZVAL_STRING(&func_name_zv, "json_encode");
        int call_result = call_user_function(EG(function_table), NULL,
                                              &func_name_zv, &json_ret,
                                              1, json_args);
        zval_ptr_dtor(&func_name_zv);
        if (call_result != SUCCESS) {
            zval_ptr_dtor(&json_args[0]);
            zend_throw_exception(xhcurl_exception_ce, "Failed to call json_encode", 0);
            return;
        }
    }

    /* 释放参数 */
    zval_ptr_dtor(&json_args[0]);

    /* 检查返回值是否为字符串（编码失败时返回 false 或抛出异常） */
    if (Z_TYPE(json_ret) != IS_STRING) {
        /* JSON 编码失败 */
        if (Z_TYPE(json_ret) != IS_UNDEF) {
            zval_ptr_dtor(&json_ret);
        }
        zend_throw_exception(xhcurl_exception_ce, "Failed to encode JSON", 0);
        return;
    }

    /* 释放旧的请求体数据 */
    if (obj->body != NULL) {
        efree(obj->body);
    }

    /* 复制 JSON 字符串作为请求体 */
    obj->body = estrndup(Z_STRVAL(json_ret), Z_STRLEN(json_ret));
    obj->body_len = Z_STRLEN(json_ret);

    /* 释放 json_encode 返回值 */
    zval_ptr_dtor(&json_ret);

    /* +--------------------------------------------------------------+
     * | 自动设置 Content-Type 为 application/json                    |
     * | 仅当用户未通过 setHeader 手动设置 Content-Type 时才自动设置，|
     * | 用户显式设置优先级高于便捷方法的自动设置。                   |
     * | 场景：                                                       |
     * |   setHeader('Content-Type', 'text/html') → setJsonBody()    |
     * |   → 保留 'text/html'，不覆盖为 'application/json'           |
     * |   setJsonBody() → setHeader('Content-Type', 'text/html')    |
     * |   → 覆盖为 'text/html'（用户最后设置者生效）                |
     * +--------------------------------------------------------------+
     */
    if (xhcurl_header_find(obj->headers, "Content-Type") == NULL) {
        /* 用户未手动设置 Content-Type，自动设置 */
        xhcurl_header_set(&obj->headers, "Content-Type", "application/json");
    }

    /* 返回 $this 支持链式调用 */
    RETURN_ZVAL(getThis(), 1, 0);
}

/**
 * 设置请求超时时间
 * XHRequest::setTimeout(int $seconds): static
 * 设置为 0 表示使用全局默认值
 */
PHP_METHOD(XHRequest, setTimeout)
{
    zend_long seconds;  /* 超时秒数 */

    /* 解析参数 */
    ZEND_PARSE_PARAMETERS_START(1, 1)
        Z_PARAM_LONG(seconds)
    ZEND_PARSE_PARAMETERS_END();

    /* 获取当前对象 */
    xhrequest_obj_t *obj = XHREQUEST_OBJ_FROM_ZOBJ(Z_OBJ_P(getThis()));

    /* +--------------------------------------------------------------+
     * | 验证超时值：负数会导致 curl 行为不确定                        |
     * | 负数超时在 curl 中可能被解释为无限等待或立即超时，            |
     * | 与用户预期不符。0 表示无限等待（curl 文档），                |
     * | 但通常用户期望 0 表示"使用默认值"，由 xhcurl_context_create  |
     * | 中的 (timeout > 0) 判断实现。                                |
     * +--------------------------------------------------------------+
     */
    if (seconds < 0) {
        zend_throw_exception(xhcurl_exception_ce, "Timeout must be non-negative", 0);
        return;
    }

    /* 设置超时时间 */
    obj->timeout = (long)seconds;

    /* 返回 $this 支持链式调用 */
    RETURN_ZVAL(getThis(), 1, 0);
}

/**
 * 设置连接超时时间
 * XHRequest::setConnectTimeout(int $seconds): static
 */
PHP_METHOD(XHRequest, setConnectTimeout)
{
    zend_long seconds;  /* 连接超时秒数 */

    /* 解析参数 */
    ZEND_PARSE_PARAMETERS_START(1, 1)
        Z_PARAM_LONG(seconds)
    ZEND_PARSE_PARAMETERS_END();

    /* 获取当前对象 */
    xhrequest_obj_t *obj = XHREQUEST_OBJ_FROM_ZOBJ(Z_OBJ_P(getThis()));

    /* 验证连接超时值：负数会导致 curl 行为不确定 */
    if (seconds < 0) {
        zend_throw_exception(xhcurl_exception_ce, "Connect timeout must be non-negative", 0);
        return;
    }

    /* 设置连接超时 */
    obj->connect_timeout = (long)seconds;

    /* 返回 $this 支持链式调用 */
    RETURN_ZVAL(getThis(), 1, 0);
}

/**
 * 设置是否跟随重定向
 * XHRequest::setFollowRedirects(bool $follow, int $max = 5): static
 */
PHP_METHOD(XHRequest, setFollowRedirects)
{
    zend_bool follow;       /* 是否跟随重定向 */
    zend_long max_redirects = XHCURL_DEFAULT_MAX_REDIRECTS; /* 最大重定向次数 */

    /* 解析参数 */
    ZEND_PARSE_PARAMETERS_START(1, 2)
        Z_PARAM_BOOL(follow)
        Z_PARAM_OPTIONAL
        Z_PARAM_LONG(max_redirects)
    ZEND_PARSE_PARAMETERS_END();

    /* 获取当前对象 */
    xhrequest_obj_t *obj = XHREQUEST_OBJ_FROM_ZOBJ(Z_OBJ_P(getThis()));

    /* 验证最大重定向次数：负数传给 CURLOPT_MAXREDIRS 会禁用限制 */
    if (max_redirects < 0) {
        zend_throw_exception(xhcurl_exception_ce, "Max redirects must be non-negative", 0);
        return;
    }

    /* 设置重定向跟随 */
    obj->follow_redirects = follow;
    obj->max_redirects = (long)max_redirects;

    /* 返回 $this 支持链式调用 */
    RETURN_ZVAL(getThis(), 1, 0);
}

/**
 * 注册流式数据回调
 * XHRequest::onChunk(callable $callback): static
 * 当接收到响应体数据时，会调用此回调函数
 * 回调签名：function(string $chunk): void
 * 注意：此回调在 XHThreadPool 模式下无效（线程安全限制）
 */
PHP_METHOD(XHRequest, onChunk)
{
    zval *callback;     /* 回调函数参数 */

    /* 解析参数：必须是可调用的 */
    ZEND_PARSE_PARAMETERS_START(1, 1)
        Z_PARAM_ZVAL(callback)
    ZEND_PARSE_PARAMETERS_END();

    /* 验证回调是否可调用 */
    if (!zend_is_callable(callback, 0, NULL)) {
        zend_throw_exception(xhcurl_exception_ce, "Callback is not callable", 0);
        return;
    }

    /* 获取当前对象 */
    xhrequest_obj_t *obj = XHREQUEST_OBJ_FROM_ZOBJ(Z_OBJ_P(getThis()));

    /* 释放旧的回调引用 */
    if (!Z_ISUNDEF(obj->chunk_callback)) {
        zval_ptr_dtor(&obj->chunk_callback);
    }

    /* 保存新的回调引用 */
    ZVAL_COPY(&obj->chunk_callback, callback);

    /* 返回 $this 支持链式调用 */
    RETURN_ZVAL(getThis(), 1, 0);
}

/**
 * 注册响应头回调
 * XHRequest::onHeader(callable $callback): static
 * 当接收到响应头数据时，会调用此回调函数
 * 回调签名：function(string $headerLine): void
 * 注意：此回调在 XHThreadPool 模式下无效（线程安全限制）
 */
PHP_METHOD(XHRequest, onHeader)
{
    zval *callback;     /* 回调函数参数 */

    /* 解析参数 */
    ZEND_PARSE_PARAMETERS_START(1, 1)
        Z_PARAM_ZVAL(callback)
    ZEND_PARSE_PARAMETERS_END();

    /* 验证回调是否可调用 */
    if (!zend_is_callable(callback, 0, NULL)) {
        zend_throw_exception(xhcurl_exception_ce, "Callback is not callable", 0);
        return;
    }

    /* 获取当前对象 */
    xhrequest_obj_t *obj = XHREQUEST_OBJ_FROM_ZOBJ(Z_OBJ_P(getThis()));

    /* 释放旧的回调引用 */
    if (!Z_ISUNDEF(obj->header_callback)) {
        zval_ptr_dtor(&obj->header_callback);
    }

    /* 保存新的回调引用 */
    ZVAL_COPY(&obj->header_callback, callback);

    /* 返回 $this 支持链式调用 */
    RETURN_ZVAL(getThis(), 1, 0);
}

/**
 * 设置表单格式的请求体（application/x-www-form-urlencoded）
 * XHRequest::setFormBody(array $data): static
 *
 * 将关联数组编码为 URL 编码的表单字符串，并自动设置 Content-Type。
 * 类似 PHP curl 中 CURLOPT_POSTFIELDS 传入数组的行为。
 *
 * 用法：
 *   $request->setMethod('POST')->setFormBody(['name' => 'test', 'age' => '25']);
 *   等效于 curl_setopt($ch, CURLOPT_POSTFIELDS, ['name' => 'test', 'age' => '25']);
 *
 * 编码规则：
 *   - 键和值都会通过 rawurlencode 编码（空格变为 +，特殊字符变为 %XX）
 *   - 数值索引的数组项会被编码为 key[0]=val0&key[1]=val1 格式
 *   - 嵌套数组暂不支持（建议使用 setJsonBody 发送复杂数据结构）
 *
 * @param array $data 表单数据（关联数组：键名 => 键值）
 */
PHP_METHOD(XHRequest, setFormBody)
{
    zval *data; /* 表单数据数组参数 */

    /* 解析参数：必须是数组 */
    ZEND_PARSE_PARAMETERS_START(1, 1)
        Z_PARAM_ARRAY(data)
    ZEND_PARSE_PARAMETERS_END();

    /* 获取当前对象 */
    xhrequest_obj_t *obj = XHREQUEST_OBJ_FROM_ZOBJ(Z_OBJ_P(getThis()));

    /* +--------------------------------------------------------------+
     * | 将 PHP 数组编码为 URL 编码的表单字符串                        |
     * | 格式：key1=value1&key2=value2                                 |
     * | 每个键值对都通过 rawurlencode 编码，确保特殊字符安全传输      |
     * | 参考：PHP curl 传入数组给 CURLOPT_POSTFIELDS 时的行为         |
     * +--------------------------------------------------------------+
     */
    zend_string *form_str = NULL; /* 编码后的表单字符串 */
    zend_string *key_str;         /* 当前遍历的键名 */
    zval *val_zv;                 /* 当前遍历的值 zval */
    zend_ulong num_idx;           /* 数值索引（用于非关联数组） */

    /* 遍历数组，逐个编码键值对 */
    ZEND_HASH_FOREACH_KEY_VAL(Z_ARRVAL_P(data), num_idx, key_str, val_zv) {
        /* 编码键名 */
        zend_string *encoded_key;
        if (key_str != NULL) {
            /* 关联数组：键名为字符串，rawurlencode 编码 */
            encoded_key = php_raw_url_encode(ZSTR_VAL(key_str), ZSTR_LEN(key_str));
        } else {
            /* 数值索引数组：将数字索引转为字符串后编码 */
            char idx_buf[32]; /* 足够存放 zend_ulong 的字符串表示 */
            int idx_len = snprintf(idx_buf, sizeof(idx_buf), ZEND_ULONG_FMT, num_idx);
            encoded_key = php_raw_url_encode(idx_buf, idx_len);
        }

        /* 编码键值（支持字符串、整数、浮点数、布尔值） */
        zend_string *encoded_val = NULL;
        if (Z_TYPE_P(val_zv) == IS_STRING) {
            /* 字符串值：直接 rawurlencode 编码 */
            encoded_val = php_raw_url_encode(Z_STRVAL_P(val_zv), Z_STRLEN_P(val_zv));
        } else if (Z_TYPE_P(val_zv) == IS_LONG) {
            /* 整数值：转为字符串后编码 */
            char val_buf[32];
            int val_len = snprintf(val_buf, sizeof(val_buf), ZEND_LONG_FMT, Z_LVAL_P(val_zv));
            encoded_val = php_raw_url_encode(val_buf, val_len);
        } else if (Z_TYPE_P(val_zv) == IS_DOUBLE) {
            /* 浮点数值：转为字符串后编码 */
            char val_buf[64];
            int val_len = snprintf(val_buf, sizeof(val_buf), "%.14g", Z_DVAL_P(val_zv));
            encoded_val = php_raw_url_encode(val_buf, val_len);
        } else if (Z_TYPE_P(val_zv) == IS_TRUE) {
            /* 布尔值 true：编码为 "1" */
            encoded_val = php_raw_url_encode("1", 1);
        } else if (Z_TYPE_P(val_zv) == IS_FALSE) {
            /* 布尔值 false：编码为 "0" */
            encoded_val = php_raw_url_encode("0", 1);
        } else if (Z_TYPE_P(val_zv) == IS_NULL) {
            /* null 值：编码为空字符串 */
            encoded_val = php_raw_url_encode("", 0);
        } else {
            /* 不支持的类型（数组、对象、资源），跳过 */
            zend_string_release(encoded_key);
            continue;
        }

        /* 拼接键值对到表单字符串 */
        if (form_str == NULL) {
            /* 第一个键值对：直接创建 "key=value" */
            size_t pair_len = ZSTR_LEN(encoded_key) + 1 + ZSTR_LEN(encoded_val);
            form_str = zend_string_alloc(pair_len, 0);
            snprintf(ZSTR_VAL(form_str), pair_len + 1, "%s=%s",
                     ZSTR_VAL(encoded_key), ZSTR_VAL(encoded_val));
        } else {
            /* 后续键值对：追加 "&key=value" */
            size_t old_len = ZSTR_LEN(form_str);
            size_t append_len = 1 + ZSTR_LEN(encoded_key) + 1 + ZSTR_LEN(encoded_val); /* &key=value */
            form_str = zend_string_extend(form_str, old_len + append_len, 0);
            /* 在末尾追加 "&key=value" */
            snprintf(ZSTR_VAL(form_str) + old_len, append_len + 1, "&%s=%s",
                     ZSTR_VAL(encoded_key), ZSTR_VAL(encoded_val));
        }

        /* 释放编码后的临时字符串 */
        zend_string_release(encoded_key);
        if (encoded_val != NULL) {
            zend_string_release(encoded_val);
        }
    } ZEND_HASH_FOREACH_END();

    /* 检查是否成功编码了数据 */
    if (form_str == NULL) {
        /* 空数组或所有值均为不支持的类型，设置空表单 */
        if (obj->body != NULL) {
            efree(obj->body);
        }
        obj->body = estrdup("");
        obj->body_len = 0;
        RETURN_ZVAL(getThis(), 1, 0);
    }

    /* 释放旧的请求体数据 */
    if (obj->body != NULL) {
        efree(obj->body);
    }

    /* 将编码后的表单字符串复制为请求体 */
    obj->body = estrndup(ZSTR_VAL(form_str), ZSTR_LEN(form_str));
    obj->body_len = ZSTR_LEN(form_str);

    /* 释放临时表单字符串 */
    zend_string_release(form_str);

    /* +--------------------------------------------------------------+
     * | 自动设置 Content-Type 为 application/x-www-form-urlencoded   |
     * | 仅当用户未通过 setHeader 手动设置 Content-Type 时才自动设置，|
     * | 用户显式设置优先级高于便捷方法的自动设置。                   |
     * | 场景：                                                       |
     * |   setHeader('Content-Type', 'text/plain') → setFormBody()   |
     * |   → 保留 'text/plain'，不覆盖                               |
     * |   setFormBody() → setHeader('Content-Type', 'text/plain')   |
     * |   → 覆盖为 'text/plain'（用户最后设置者生效）                |
     * +--------------------------------------------------------------+
     */
    if (xhcurl_header_find(obj->headers, "Content-Type") == NULL) {
        /* 用户未手动设置 Content-Type，自动设置表单编码类型 */
        xhcurl_header_set(&obj->headers, "Content-Type", "application/x-www-form-urlencoded");
    }

    /* 返回 $this 支持链式调用 */
    RETURN_ZVAL(getThis(), 1, 0);
}

/**
 * 获取请求 URL
 * XHRequest::getUrl(): string
 */
PHP_METHOD(XHRequest, getUrl)
{
    /* 无参数 */
    ZEND_PARSE_PARAMETERS_NONE();

    /* 获取当前对象 */
    xhrequest_obj_t *obj = XHREQUEST_OBJ_FROM_ZOBJ(Z_OBJ_P(getThis()));

    if (obj->url != NULL) {
        RETURN_STRING(obj->url);
    } else {
        RETURN_EMPTY_STRING();
    }
}

/* +----------------------------------------------------------------------+
 * | 方法注册表                                                            |
 * +----------------------------------------------------------------------+
 */

/* 构造函数参数信息 */
ZEND_BEGIN_ARG_INFO_EX(arginfo_xhrequest_construct, 0, 0, 1)
    ZEND_ARG_INFO(0, url)         /* URL 参数（必填） */
ZEND_END_ARG_INFO()

/* setMethod 参数信息 */
ZEND_BEGIN_ARG_INFO_EX(arginfo_xhrequest_setMethod, 0, 0, 1)
    ZEND_ARG_INFO(0, method)      /* HTTP 方法（必填） */
ZEND_END_ARG_INFO()

/* setHeader 参数信息 */
ZEND_BEGIN_ARG_INFO_EX(arginfo_xhrequest_setHeader, 0, 0, 2)
    ZEND_ARG_INFO(0, name)        /* 头部名称（必填） */
    ZEND_ARG_INFO(0, value)       /* 头部值（必填） */
ZEND_END_ARG_INFO()

/* setCookie 参数信息 */
ZEND_BEGIN_ARG_INFO_EX(arginfo_xhrequest_setCookie, 0, 0, 2)
    ZEND_ARG_INFO(0, name)        /* Cookie 名称（必填） */
    ZEND_ARG_INFO(0, value)       /* Cookie 值（必填） */
ZEND_END_ARG_INFO()

/* setBody 参数信息 */
ZEND_BEGIN_ARG_INFO_EX(arginfo_xhrequest_setBody, 0, 0, 1)
    ZEND_ARG_INFO(0, body)        /* 请求体（必填） */
ZEND_END_ARG_INFO()

/* setProxy 参数信息 */
ZEND_BEGIN_ARG_INFO_EX(arginfo_xhrequest_setProxy, 0, 0, 1)
    ZEND_ARG_INFO(0, proxy)       /* 代理地址（必填，空字符串=禁用代理） */
ZEND_END_ARG_INFO()

/* setJsonBody 参数信息 */
ZEND_BEGIN_ARG_INFO_EX(arginfo_xhrequest_setJsonBody, 0, 0, 1)
    ZEND_ARG_INFO(0, data)        /* 数组数据（必填） */
ZEND_END_ARG_INFO()

/* setFormBody 参数信息 */
ZEND_BEGIN_ARG_INFO_EX(arginfo_xhrequest_setFormBody, 0, 0, 1)
    ZEND_ARG_INFO(0, data)        /* 表单数据数组（必填） */
ZEND_END_ARG_INFO()

/* setTimeout 参数信息 */
ZEND_BEGIN_ARG_INFO_EX(arginfo_xhrequest_setTimeout, 0, 0, 1)
    ZEND_ARG_INFO(0, seconds)     /* 超时秒数（必填） */
ZEND_END_ARG_INFO()

/* setConnectTimeout 参数信息 */
ZEND_BEGIN_ARG_INFO_EX(arginfo_xhrequest_setConnectTimeout, 0, 0, 1)
    ZEND_ARG_INFO(0, seconds)     /* 连接超时秒数（必填） */
ZEND_END_ARG_INFO()

/* setFollowRedirects 参数信息 */
ZEND_BEGIN_ARG_INFO_EX(arginfo_xhrequest_setFollowRedirects, 0, 0, 1)
    ZEND_ARG_INFO(0, follow)      /* 是否跟随（必填） */
    ZEND_ARG_INFO(0, max)         /* 最大次数（可选） */
ZEND_END_ARG_INFO()

/* onChunk 参数信息 */
ZEND_BEGIN_ARG_INFO_EX(arginfo_xhrequest_onChunk, 0, 0, 1)
    ZEND_ARG_INFO(0, callback)    /* 回调函数（必填） */
ZEND_END_ARG_INFO()

/* onHeader 参数信息 */
ZEND_BEGIN_ARG_INFO_EX(arginfo_xhrequest_onHeader, 0, 0, 1)
    ZEND_ARG_INFO(0, callback)    /* 回调函数（必填） */
ZEND_END_ARG_INFO()

/* getUrl 参数信息（无参数） */
ZEND_BEGIN_ARG_INFO_EX(arginfo_xhrequest_getUrl, 0, 0, 0)
ZEND_END_ARG_INFO()

static const zend_function_entry xhrequest_methods[] = {
    /* 构造函数 */
    PHP_ME(XHRequest, __construct, arginfo_xhrequest_construct, ZEND_ACC_PUBLIC)
    /* 设置 HTTP 方法 */
    PHP_ME(XHRequest, setMethod, arginfo_xhrequest_setMethod, ZEND_ACC_PUBLIC)
    /* 设置请求头部 */
    PHP_ME(XHRequest, setHeader, arginfo_xhrequest_setHeader, ZEND_ACC_PUBLIC)
    /* 设置请求 Cookie */
    PHP_ME(XHRequest, setCookie, arginfo_xhrequest_setCookie, ZEND_ACC_PUBLIC)
    /* 设置请求体 */
    PHP_ME(XHRequest, setBody, arginfo_xhrequest_setBody, ZEND_ACC_PUBLIC)
    /* 设置请求级代理 */
    PHP_ME(XHRequest, setProxy, arginfo_xhrequest_setProxy, ZEND_ACC_PUBLIC)
    /* 设置 JSON 请求体 */
    PHP_ME(XHRequest, setJsonBody, arginfo_xhrequest_setJsonBody, ZEND_ACC_PUBLIC)
    /* 设置表单请求体（application/x-www-form-urlencoded） */
    PHP_ME(XHRequest, setFormBody, arginfo_xhrequest_setFormBody, ZEND_ACC_PUBLIC)
    /* 设置超时时间 */
    PHP_ME(XHRequest, setTimeout, arginfo_xhrequest_setTimeout, ZEND_ACC_PUBLIC)
    /* 设置连接超时 */
    PHP_ME(XHRequest, setConnectTimeout, arginfo_xhrequest_setConnectTimeout, ZEND_ACC_PUBLIC)
    /* 设置重定向跟随 */
    PHP_ME(XHRequest, setFollowRedirects, arginfo_xhrequest_setFollowRedirects, ZEND_ACC_PUBLIC)
    /* 注册流式数据回调 */
    PHP_ME(XHRequest, onChunk, arginfo_xhrequest_onChunk, ZEND_ACC_PUBLIC)
    /* 注册响应头回调 */
    PHP_ME(XHRequest, onHeader, arginfo_xhrequest_onHeader, ZEND_ACC_PUBLIC)
    /* 获取请求 URL */
    PHP_ME(XHRequest, getUrl, arginfo_xhrequest_getUrl, ZEND_ACC_PUBLIC)
    /* 结束标记 */
    PHP_FE_END
};

/* +----------------------------------------------------------------------+
 * | 类初始化函数                                                          |
 * +----------------------------------------------------------------------+
 */
PHP_MINIT_FUNCTION(xhrequest_class)
{
    /* 初始化类入口 */
    zend_class_entry ce;
    INIT_CLASS_ENTRY(ce, "XHRequest", xhrequest_methods);

    /* 注册类 */
    xhrequest_ce = zend_register_internal_class(&ce);

    /* 设置对象创建函数 */
    xhrequest_ce->create_object = xhrequest_create_obj;

    /* +--------------------------------------------------------------+
     * | 初始化自定义对象操作函数表                                     |
     * | 关键：设置 free_obj 回调，确保 PHP 对象销毁时释放 C 侧资源   |
     * | 不设置 free_obj 会导致 url/method/headers/cookies 等资源泄漏 |
     * +--------------------------------------------------------------+
     */
    memcpy(&xhrequest_object_handlers, zend_get_std_object_handlers(), sizeof(zend_object_handlers));
    /* 设置 free_obj 回调：PHP GC 回收对象时自动调用 xhrequest_free_obj */
    xhrequest_object_handlers.free_obj = xhrequest_free_obj;
    /* 设置 std 字段在结构体中的偏移量 */
    xhrequest_object_handlers.offset = XtOffsetOf(xhrequest_obj_t, std);

    return SUCCESS;
}
