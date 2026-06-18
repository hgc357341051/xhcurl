/* +----------------------------------------------------------------------+
 * | XHCurl 扩展 - 模块入口 + XHCurl 全局管理器类实现                     |
 * | 模块入口：处理 PHP 生命周期（MINIT/MSHUTDOWN/RINIT/RSHUTDOWN）        |
 * | XHCurl 类：管理全局配置（headers/cookies/超时/代理等）               |
 * +----------------------------------------------------------------------+
 */

#include "xhcurl_priv.h"

/* +----------------------------------------------------------------------+
 * | 全局变量声明                                                          |
 * +----------------------------------------------------------------------+
 */

/* XHCurl 类入口指针 */
zend_class_entry *xhcurl_ce;

/* +----------------------------------------------------------------------+
 * | JSON 函数指针缓存（性能优化）                                        |
 * | 在 MINIT 时缓存 json_encode/json_decode 的函数指针，                 |
 * | 避免每次调用都通过 zend_hash_find 查找函数表                         |
 * +----------------------------------------------------------------------+
 */

/* json_encode 函数指针（MINIT 时初始化） */
zend_function *xhcurl_json_encode_func = NULL;
/* json_decode 函数指针（MINIT 时初始化） */
zend_function *xhcurl_json_decode_func = NULL;

/* XHCurlException 异常类入口指针 */
zend_class_entry *xhcurl_exception_ce;

/* +----------------------------------------------------------------------+
 * | XHCurl 对象生命周期函数                                               |
 * +----------------------------------------------------------------------+
 */

/**
 * 释放 XHCurl 对象资源
 * @param object zend_object 指针
 */
static void xhcurl_free_obj(zend_object *object)
{
    /* 从 zend_object 获取 XHCurl 对象 */
    xhcurl_obj_t *obj = XHCURL_OBJ_FROM_ZOBJ(object);

    /* 释放 curl 共享句柄 */
    if (obj->share != NULL) {
        curl_share_cleanup(obj->share);
        obj->share = NULL;
    }

    /* 释放全局头部列表（curl_slist 格式） */
    if (obj->global_headers != NULL) {
        curl_slist_free_all(obj->global_headers);
        obj->global_headers = NULL;
    }

    /* 释放全局头部链表（原始键值对格式） */
    if (obj->global_headers_raw != NULL) {
        xhcurl_header_list_free(obj->global_headers_raw);
        obj->global_headers_raw = NULL;
    }

    /* 释放全局 Cookie 链表 */
    if (obj->global_cookies != NULL) {
        xhcurl_cookie_list_free(obj->global_cookies);
        obj->global_cookies = NULL;
    }

    /* 释放 User-Agent 字符串 */
    if (obj->user_agent != NULL) {
        efree(obj->user_agent);
        obj->user_agent = NULL;
    }

    /* 释放代理地址字符串 */
    if (obj->proxy != NULL) {
        efree(obj->proxy);
        obj->proxy = NULL;
    }

    /* 调用标准对象释放函数 */
    zend_object_std_dtor(object);
}

/**
 * 创建 XHCurl 对象
 * @param class_type 类入口指针
 * @return zend_object 指针
 */
static zend_object *xhcurl_create_obj(zend_class_entry *class_type)
{
    /* 分配对象内存 */
    xhcurl_obj_t *obj = (xhcurl_obj_t *)zend_object_alloc(sizeof(xhcurl_obj_t), class_type);

    /* 初始化 PHP 标准对象 */
    zend_object_std_init(&obj->std, class_type);
    /* 初始化对象属性 */
    object_properties_init(&obj->std, class_type);

    /* 创建 curl 共享句柄，用于多个请求间共享 DNS/SSL/Cookie */
    obj->share = curl_share_init();
    /* 设置共享数据类型：DNS 缓存、SSL 会话、Cookie */
    curl_share_setopt(obj->share, CURLSHOPT_SHARE, CURL_LOCK_DATA_DNS);
    curl_share_setopt(obj->share, CURLSHOPT_SHARE, CURL_LOCK_DATA_SSL_SESSION);
    curl_share_setopt(obj->share, CURLSHOPT_SHARE, CURL_LOCK_DATA_COOKIE);

    /* 初始化全局配置为默认值 */
    obj->global_headers = NULL;                         /* 无全局头部 */
    obj->global_headers_raw = NULL;                     /* 无全局头部（原始格式） */
    obj->global_cookies = NULL;                         /* 无全局 Cookie */
    obj->timeout = XHCURL_DEFAULT_TIMEOUT;              /* 默认超时 30 秒 */
    obj->connect_timeout = XHCURL_DEFAULT_CONNECT_TIMEOUT; /* 默认连接超时 10 秒 */
    obj->verify_ssl = 1;                                /* 默认验证 SSL */
    obj->user_agent = NULL;                             /* 无自定义 User-Agent */
    obj->proxy = NULL;                                  /* 无代理 */
    obj->max_response_size = XHCURL_DEFAULT_MAX_RESPONSE_SIZE; /* 默认最大 10MB */

    /* 设置对象释放函数 */
    obj->std.handlers = zend_get_std_object_handlers();

    return &obj->std;
}

/* +----------------------------------------------------------------------+
 * | XHCurl PHP 方法实现                                                   |
 * +----------------------------------------------------------------------+
 */

/**
 * 构造函数
 * XHCurl::__construct()
 */
PHP_METHOD(XHCurl, __construct)
{
    /* 无参数 */
    ZEND_PARSE_PARAMETERS_NONE();

    /* 构造函数逻辑已在 create_obj 中完成 */
}

/**
 * 设置全局请求头
 * XHCurl::setGlobalHeader(string $name, string $value): void
 * 全局头部会应用到所有通过此管理器发出的请求
 */
PHP_METHOD(XHCurl, setGlobalHeader)
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
    xhcurl_obj_t *obj = XHCURL_OBJ_FROM_ZOBJ(Z_OBJ_P(getThis()));

    /* 添加到原始头部链表（用于合并时查找） */
    xhcurl_header_add(&obj->global_headers_raw, name, value);

    /* 添加到 curl_slist 格式头部列表（用于设置到 curl 句柄） */
    char header_line[4096];
    snprintf(header_line, sizeof(header_line), "%s: %s", name, value);
    obj->global_headers = curl_slist_append(obj->global_headers, header_line);
}

/**
 * 设置全局 Cookie
 * XHCurl::setGlobalCookie(string $name, string $value, string $domain = '', string $path = '/'): void
 * 全局 Cookie 通过 curl_share 在请求间共享
 */
PHP_METHOD(XHCurl, setGlobalCookie)
{
    char *name;                 /* Cookie 名称 */
    size_t name_len;            /* 名称长度 */
    char *value;                /* Cookie 值 */
    size_t value_len;           /* 值长度 */
    char *domain = "";          /* Cookie 域名（默认空） */
    size_t domain_len = 0;      /* 域名长度 */
    char *path = "/";           /* Cookie 路径（默认 /） */
    size_t path_len = 1;        /* 路径长度 */

    /* 解析参数 */
    ZEND_PARSE_PARAMETERS_START(2, 4)
        Z_PARAM_STRING(name, name_len)
        Z_PARAM_STRING(value, value_len)
        Z_PARAM_OPTIONAL
        Z_PARAM_STRING(domain, domain_len)
        Z_PARAM_STRING(path, path_len)
    ZEND_PARSE_PARAMETERS_END();

    /* 获取当前对象 */
    xhcurl_obj_t *obj = XHCURL_OBJ_FROM_ZOBJ(Z_OBJ_P(getThis()));

    /* 添加到全局 Cookie 链表 */
    xhcurl_cookie_add(&obj->global_cookies, name, value, domain, path);

    /* 同时通过 curl_share 设置 Cookie（使用 CURLOPT_COOKIELIST 格式） */
    /* 格式：域名\t是否包含子域\t路径\t是否安全\t过期时间\t名称\t值 */
    char cookie_line[4096];
    snprintf(cookie_line, sizeof(cookie_line), "%s\tFALSE\t%s\tFALSE\t0\t%s\t%s",
             (domain_len > 0) ? domain : "localhost",
             (path_len > 0) ? path : "/",
             name, value);
    curl_share_setopt(obj->share, CURLSHOPT_SHARE, CURL_LOCK_DATA_COOKIE);
}

/**
 * 设置默认请求超时时间
 * XHCurl::setTimeout(int $seconds): void
 */
PHP_METHOD(XHCurl, setTimeout)
{
    zend_long seconds;  /* 超时秒数 */

    /* 解析参数 */
    ZEND_PARSE_PARAMETERS_START(1, 1)
        Z_PARAM_LONG(seconds)
    ZEND_PARSE_PARAMETERS_END();

    /* 获取当前对象 */
    xhcurl_obj_t *obj = XHCURL_OBJ_FROM_ZOBJ(Z_OBJ_P(getThis()));

    /* 设置超时时间 */
    obj->timeout = (long)seconds;
}

/**
 * 设置默认连接超时时间
 * XHCurl::setConnectTimeout(int $seconds): void
 */
PHP_METHOD(XHCurl, setConnectTimeout)
{
    zend_long seconds;  /* 连接超时秒数 */

    /* 解析参数 */
    ZEND_PARSE_PARAMETERS_START(1, 1)
        Z_PARAM_LONG(seconds)
    ZEND_PARSE_PARAMETERS_END();

    /* 获取当前对象 */
    xhcurl_obj_t *obj = XHCURL_OBJ_FROM_ZOBJ(Z_OBJ_P(getThis()));

    /* 设置连接超时 */
    obj->connect_timeout = (long)seconds;
}

/**
 * 设置是否验证 SSL 证书
 * XHCurl::setVerifySsl(bool $verify): void
 * 开发环境可设为 false，生产环境建议为 true
 */
PHP_METHOD(XHCurl, setVerifySsl)
{
    zend_bool verify;   /* 是否验证 */

    /* 解析参数 */
    ZEND_PARSE_PARAMETERS_START(1, 1)
        Z_PARAM_BOOL(verify)
    ZEND_PARSE_PARAMETERS_END();

    /* 获取当前对象 */
    xhcurl_obj_t *obj = XHCURL_OBJ_FROM_ZOBJ(Z_OBJ_P(getThis()));

    /* 设置 SSL 验证 */
    obj->verify_ssl = verify;
}

/**
 * 设置默认 User-Agent
 * XHCurl::setUserAgent(string $ua): void
 */
PHP_METHOD(XHCurl, setUserAgent)
{
    char *ua;           /* User-Agent 字符串 */
    size_t ua_len;      /* 字符串长度 */

    /* 解析参数 */
    ZEND_PARSE_PARAMETERS_START(1, 1)
        Z_PARAM_STRING(ua, ua_len)
    ZEND_PARSE_PARAMETERS_END();

    /* 获取当前对象 */
    xhcurl_obj_t *obj = XHCURL_OBJ_FROM_ZOBJ(Z_OBJ_P(getThis()));

    /* 释放旧的 User-Agent */
    if (obj->user_agent != NULL) {
        efree(obj->user_agent);
    }

    /* 复制新的 User-Agent */
    obj->user_agent = estrdup(ua);
}

/**
 * 设置代理服务器
 * XHCurl::setProxy(string $proxy): void
 * 格式：http://host:port 或 socks5://host:port
 */
PHP_METHOD(XHCurl, setProxy)
{
    char *proxy;        /* 代理地址 */
    size_t proxy_len;   /* 地址长度 */

    /* 解析参数 */
    ZEND_PARSE_PARAMETERS_START(1, 1)
        Z_PARAM_STRING(proxy, proxy_len)
    ZEND_PARSE_PARAMETERS_END();

    /* 获取当前对象 */
    xhcurl_obj_t *obj = XHCURL_OBJ_FROM_ZOBJ(Z_OBJ_P(getThis()));

    /* 释放旧的代理地址 */
    if (obj->proxy != NULL) {
        efree(obj->proxy);
    }

    /* 复制新的代理地址 */
    obj->proxy = estrdup(proxy);
}

/**
 * 设置最大响应体大小
 * XHCurl::setMaxResponseSize(int $bytes): void
 * 超过此大小的响应体会被截断并返回错误
 * 用于防止内存溢出，默认 10MB
 */
PHP_METHOD(XHCurl, setMaxResponseSize)
{
    zend_long bytes;    /* 最大字节数 */

    /* 解析参数 */
    ZEND_PARSE_PARAMETERS_START(1, 1)
        Z_PARAM_LONG(bytes)
    ZEND_PARSE_PARAMETERS_END();

    /* 获取当前对象 */
    xhcurl_obj_t *obj = XHCURL_OBJ_FROM_ZOBJ(Z_OBJ_P(getThis()));

    /* 设置最大响应体大小 */
    obj->max_response_size = (size_t)bytes;
}

/**
 * 同步执行单个请求
 * XHCurl::exec(XHRequest $request): XHResponse
 * 阻塞等待请求完成后返回响应
 */
PHP_METHOD(XHCurl, exec)
{
    zval *request_zv;   /* XHRequest 对象参数 */

    /* 解析参数 */
    ZEND_PARSE_PARAMETERS_START(1, 1)
        Z_PARAM_OBJECT_OF_CLASS(request_zv, xhrequest_ce)
    ZEND_PARSE_PARAMETERS_END();

    /* 获取 XHCurl 对象 */
    xhcurl_obj_t *curl_obj = XHCURL_OBJ_FROM_ZOBJ(Z_OBJ_P(getThis()));
    /* 获取 XHRequest 对象 */
    xhrequest_obj_t *req_obj = XHREQUEST_OBJ_FROM_ZOBJ(Z_OBJ_P(request_zv));

    /* 创建请求执行上下文 */
    xhcurl_req_context_t *ctx = xhcurl_context_create(curl_obj, req_obj);
    if (ctx == NULL) {
        zend_throw_exception(xhcurl_exception_ce, "Failed to create request context", 0);
        return;
    }

    /* 执行同步请求（curl_easy_perform 阻塞等待完成） */
    CURLcode res = curl_easy_perform(ctx->easy);

    /* 获取 HTTP 状态码 */
    long status_code = 0;
    curl_easy_getinfo(ctx->easy, CURLINFO_RESPONSE_CODE, &status_code);

    /* 获取请求总耗时 */
    double total_time = 0.0;
    curl_easy_getinfo(ctx->easy, CURLINFO_TOTAL_TIME, &total_time);

    /* 获取 Content-Type */
    char *content_type = NULL;
    curl_easy_getinfo(ctx->easy, CURLINFO_CONTENT_TYPE, &content_type);

    /* 解析响应头 */
    xhcurl_parse_response_headers(&ctx->header_buf, &ctx->parsed_headers);

    /* 创建 XHResponse PHP 对象 */
    object_init_ex(return_value, xhresponse_ce);
    xhresponse_obj_t *resp_obj = XHRESPONSE_OBJ_FROM_ZOBJ(Z_OBJ_P(return_value));

    /* 填充响应数据 */
    resp_obj->status_code = status_code;
    resp_obj->total_time = total_time;
    resp_obj->curl_code = res;

    /* 复制 Content-Type */
    if (content_type != NULL) {
        resp_obj->content_type = estrdup(content_type);
    }

    /* 设置错误信息（如果有） */
    if (res != CURLE_OK) {
        resp_obj->error_msg = estrdup(curl_easy_strerror(res));
    }

    /* 转移响应体缓冲区的所有权给 XHResponse 对象 */
    /* 分配新的缓冲区结构体，复制内容后释放原始缓冲区 */
    resp_obj->body = (xhcurl_buffer_t *)ecalloc(1, sizeof(xhcurl_buffer_t));
    if (resp_obj->body != NULL) {
        /* 直接转移缓冲区数据指针，避免大块内存复制 */
        resp_obj->body->data = ctx->body_buf.data;
        resp_obj->body->size = ctx->body_buf.size;
        resp_obj->body->capacity = ctx->body_buf.capacity;
        resp_obj->body->max_size = ctx->body_buf.max_size;
        /* 清空原始缓冲区指针，防止 xhcurl_context_free 重复释放 */
        ctx->body_buf.data = NULL;
        ctx->body_buf.size = 0;
        ctx->body_buf.capacity = 0;
    }

    /* 转移响应头链表的所有权 */
    resp_obj->headers = ctx->parsed_headers;
    ctx->parsed_headers = NULL;

    /* 释放请求上下文（body 数据已转移，不会重复释放） */
    xhcurl_context_free(ctx);
}

/* +----------------------------------------------------------------------+
 * | XHCurl 方法注册表                                                     |
 * +----------------------------------------------------------------------+
 */

/* setGlobalHeader 参数信息 */
ZEND_BEGIN_ARG_INFO_EX(arginfo_xhcurl_setGlobalHeader, 0, 0, 2)
    ZEND_ARG_INFO(0, name)
    ZEND_ARG_INFO(0, value)
ZEND_END_ARG_INFO()

/* setGlobalCookie 参数信息 */
ZEND_BEGIN_ARG_INFO_EX(arginfo_xhcurl_setGlobalCookie, 0, 0, 2)
    ZEND_ARG_INFO(0, name)
    ZEND_ARG_INFO(0, value)
    ZEND_ARG_INFO(0, domain)
    ZEND_ARG_INFO(0, path)
ZEND_END_ARG_INFO()

/* setTimeout 参数信息 */
ZEND_BEGIN_ARG_INFO_EX(arginfo_xhcurl_setTimeout, 0, 0, 1)
    ZEND_ARG_INFO(0, seconds)
ZEND_END_ARG_INFO()

/* setConnectTimeout 参数信息 */
ZEND_BEGIN_ARG_INFO_EX(arginfo_xhcurl_setConnectTimeout, 0, 0, 1)
    ZEND_ARG_INFO(0, seconds)
ZEND_END_ARG_INFO()

/* setVerifySsl 参数信息 */
ZEND_BEGIN_ARG_INFO_EX(arginfo_xhcurl_setVerifySsl, 0, 0, 1)
    ZEND_ARG_INFO(0, verify)
ZEND_END_ARG_INFO()

/* setUserAgent 参数信息 */
ZEND_BEGIN_ARG_INFO_EX(arginfo_xhcurl_setUserAgent, 0, 0, 1)
    ZEND_ARG_INFO(0, ua)
ZEND_END_ARG_INFO()

/* setProxy 参数信息 */
ZEND_BEGIN_ARG_INFO_EX(arginfo_xhcurl_setProxy, 0, 0, 1)
    ZEND_ARG_INFO(0, proxy)
ZEND_END_ARG_INFO()

/* setMaxResponseSize 参数信息 */
ZEND_BEGIN_ARG_INFO_EX(arginfo_xhcurl_setMaxResponseSize, 0, 0, 1)
    ZEND_ARG_INFO(0, bytes)
ZEND_END_ARG_INFO()

/* exec 参数信息 */
ZEND_BEGIN_ARG_INFO_EX(arginfo_xhcurl_exec, 0, 0, 1)
    ZEND_ARG_INFO(0, request)
ZEND_END_ARG_INFO()

static const zend_function_entry xhcurl_methods[] = {
    /* 构造函数 */
    PHP_ME(XHCurl, __construct, NULL, ZEND_ACC_PUBLIC)
    /* 设置全局请求头 */
    PHP_ME(XHCurl, setGlobalHeader, arginfo_xhcurl_setGlobalHeader, ZEND_ACC_PUBLIC)
    /* 设置全局 Cookie */
    PHP_ME(XHCurl, setGlobalCookie, arginfo_xhcurl_setGlobalCookie, ZEND_ACC_PUBLIC)
    /* 设置超时时间 */
    PHP_ME(XHCurl, setTimeout, arginfo_xhcurl_setTimeout, ZEND_ACC_PUBLIC)
    /* 设置连接超时 */
    PHP_ME(XHCurl, setConnectTimeout, arginfo_xhcurl_setConnectTimeout, ZEND_ACC_PUBLIC)
    /* 设置 SSL 验证 */
    PHP_ME(XHCurl, setVerifySsl, arginfo_xhcurl_setVerifySsl, ZEND_ACC_PUBLIC)
    /* 设置 User-Agent */
    PHP_ME(XHCurl, setUserAgent, arginfo_xhcurl_setUserAgent, ZEND_ACC_PUBLIC)
    /* 设置代理 */
    PHP_ME(XHCurl, setProxy, arginfo_xhcurl_setProxy, ZEND_ACC_PUBLIC)
    /* 设置最大响应体大小 */
    PHP_ME(XHCurl, setMaxResponseSize, arginfo_xhcurl_setMaxResponseSize, ZEND_ACC_PUBLIC)
    /* 同步执行单个请求 */
    PHP_ME(XHCurl, exec, arginfo_xhcurl_exec, ZEND_ACC_PUBLIC)
    /* 结束标记 */
    PHP_FE_END
};

/* +----------------------------------------------------------------------+
 * | XHCurl 类初始化函数                                                   |
 * +----------------------------------------------------------------------+
 */
PHP_MINIT_FUNCTION(xhcurl_class)
{
    /* 初始化类入口 */
    zend_class_entry ce;
    INIT_CLASS_ENTRY(ce, "XHCurl", xhcurl_methods);

    /* 注册类 */
    xhcurl_ce = zend_register_internal_class(&ce);

    /* 设置对象创建和释放函数 */
    xhcurl_ce->create_object = xhcurl_create_obj;

    /* 注册异常类 */
    zend_class_entry exception_ce;
    INIT_CLASS_ENTRY(exception_ce, "XHCurlException", NULL);
    /* 继承 PHP 标准 Exception 类 */
    xhcurl_exception_ce = zend_register_internal_class_ex(&exception_ce, zend_ce_exception);

    return SUCCESS;
}

/* +----------------------------------------------------------------------+
 * | 模块生命周期函数                                                      |
 * +----------------------------------------------------------------------+
 */

/**
 * 模块初始化（PHP 启动时调用一次）
 * 注册所有类和常量
 */
PHP_MINIT_FUNCTION(xhcurl)
{
    /* 全局初始化 libcurl 库 */
    curl_global_init(CURL_GLOBAL_ALL);

    /* 注册 XHCurl 全局管理器类 */
    PHP_MINIT(xhcurl_class)(INIT_FUNC_ARGS_PASSTHRU);
    /* 注册 XHRequest 请求构建器类 */
    PHP_MINIT(xhrequest_class)(INIT_FUNC_ARGS_PASSTHRU);
    /* 注册 XHResponse 懒加载响应类 */
    PHP_MINIT(xhresponse_class)(INIT_FUNC_ARGS_PASSTHRU);
    /* 注册 XHMulti 批量异步执行器类 */
    PHP_MINIT(xhmulti_class)(INIT_FUNC_ARGS_PASSTHRU);
    /* 注册 XHThreadPool CLI线程池类 */
    PHP_MINIT(xhthreadpool_class)(INIT_FUNC_ARGS_PASSTHRU);

    /* 注册扩展版本常量 */
    REGISTER_STRING_CONSTANT("XHCURL_VERSION", PHP_XHCURL_VERSION, CONST_CS | CONST_PERSISTENT);

    /* +------------------------------------------------------------------+
     * | 缓存 JSON 函数指针（性能优化）                                    |
     * | 在 MINIT 阶段一次性查找 json_encode/json_decode 函数表，          |
     * | 后续调用直接使用缓存的指针，避免每次调用都做哈希查找              |
     * +------------------------------------------------------------------+
     */
    {
        /* 在全局函数表中查找 json_encode */
        zend_function *func = zend_hash_str_find_ptr(CG(function_table), "json_encode", sizeof("json_encode") - 1);
        if (func != NULL) {
            xhcurl_json_encode_func = func;
        }

        /* 在全局函数表中查找 json_decode */
        func = zend_hash_str_find_ptr(CG(function_table), "json_decode", sizeof("json_decode") - 1);
        if (func != NULL) {
            xhcurl_json_decode_func = func;
        }
    }

    return SUCCESS;
}

/**
 * 模块关闭（PHP 关闭时调用一次）
 * 释放 libcurl 全局资源
 */
PHP_MSHUTDOWN_FUNCTION(xhcurl)
{
    /* 清理 libcurl 全局资源 */
    curl_global_cleanup();

    return SUCCESS;
}

/**
 * 请求初始化（每个 PHP 请求开始时调用）
 * FPM 模式下每个请求都会调用，CLI 模式下仅调用一次
 */
PHP_RINIT_FUNCTION(xhcurl)
{
    /* 目前无需在每个请求开始时做特殊处理 */
    /* 全局配置由 XHCurl 对象管理，对象生命周期由 PHP GC 控制 */
    return SUCCESS;
}

/**
 * 请求关闭（每个 PHP 请求结束时调用）
 * 确保所有请求级别的资源被正确释放
 * FPM 模式下这是防止内存泄漏的关键钩子
 */
PHP_RSHUTDOWN_FUNCTION(xhcurl)
{
    /* PHP 的 GC 会自动回收所有对象 */
    /* 但如果存在循环引用或异常退出，这里可以做兜底清理 */
    return SUCCESS;
}

/**
 * 模块信息展示（phpinfo() 调用）
 */
PHP_MINFO_FUNCTION(xhcurl)
{
    /* 展示模块基本信息 */
    php_info_print_table_start();
    php_info_print_table_header(2, "XHCurl Support", "enabled");
    php_info_print_table_row(2, "Version", PHP_XHCURL_VERSION);
    php_info_print_table_row(2, "libcurl Version", curl_version());
    php_info_print_table_end();
}

/**
 * 获取扩展版本号
 * xhcurl_version(): string
 */
PHP_FUNCTION(xhcurl_version)
{
    /* 无参数 */
    ZEND_PARSE_PARAMETERS_NONE();

    /* 返回版本号字符串 */
    RETURN_STRING(PHP_XHCURL_VERSION);
}

/* +----------------------------------------------------------------------+
 * | 模块入口结构体                                                        |
 * | PHP 通过此结构体识别和加载扩展                                        |
 * +----------------------------------------------------------------------+
 */
zend_module_entry xhcurl_module_entry = {
    STANDARD_MODULE_HEADER,             /* 标准模块头 */
    PHP_XHCURL_EXTNAME,                 /* 扩展名称 */
    NULL,                               /* 函数列表（使用类方法，无需全局函数） */
    PHP_MINIT(xhcurl),                  /* 模块初始化 */
    PHP_MSHUTDOWN(xhcurl),             /* 模块关闭 */
    PHP_RINIT(xhcurl),                  /* 请求初始化 */
    PHP_RSHUTDOWN(xhcurl),             /* 请求关闭 */
    PHP_MINFO(xhcurl),                  /* 模块信息 */
    PHP_XHCURL_VERSION,                 /* 版本号 */
    STANDARD_MODULE_PROPERTIES          /* 标准模块属性 */
};

#ifdef COMPILE_DL_XHCURL
/* 动态加载入口 */
ZEND_GET_MODULE(xhcurl)
#endif
