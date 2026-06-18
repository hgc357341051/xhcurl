/* +----------------------------------------------------------------------+
 * | XHCurl 扩展 - 请求执行上下文管理实现                                 |
 * | 职责：创建/释放请求上下文、curl 回调函数实现                          |
 * | 从 xhcurl_buffer.c 拆分而来，遵循单一职责原则                        |
 * +----------------------------------------------------------------------+
 */

#include "xhcurl_priv.h"

/* +----------------------------------------------------------------------+
 * | 请求上下文操作函数实现                                                |
 * | 上下文关联 curl easy 句柄与请求数据，在 curl_multi 执行期间使用      |
 * +----------------------------------------------------------------------+
 */

/**
 * 创建请求执行上下文
 * @param curl_obj XHCurl 全局管理器对象
 * @param req_obj  XHRequest 请求对象
 * @return 请求上下文指针，失败返回 NULL
 */
xhcurl_req_context_t *xhcurl_context_create(xhcurl_obj_t *curl_obj, xhrequest_obj_t *req_obj)
{
    /* 参数有效性检查 */
    if (curl_obj == NULL || req_obj == NULL) {
        return NULL;
    }

    /* 分配上下文内存，使用 ecalloc 确保初始化为零 */
    xhcurl_req_context_t *ctx = (xhcurl_req_context_t *)ecalloc(1, sizeof(xhcurl_req_context_t));
    if (ctx == NULL) {
        return NULL;
    }

    /* 创建 curl easy 句柄 */
    ctx->easy = curl_easy_init();
    if (ctx->easy == NULL) {
        efree(ctx);
        return NULL;
    }

    /* 初始化响应体缓冲区（使用 malloc，不计入 PHP memory_limit） */
    if (xhcurl_buffer_init(&ctx->body_buf, XHCURL_BUFFER_INIT_CAPACITY,
                            curl_obj->max_response_size) != 0) {
        curl_easy_cleanup(ctx->easy);
        efree(ctx);
        return NULL;
    }

    /* 初始化响应头缓冲区 */
    if (xhcurl_buffer_init(&ctx->header_buf, XHCURL_BUFFER_INIT_CAPACITY, 0) != 0) {
        xhcurl_buffer_free(&ctx->body_buf);
        curl_easy_cleanup(ctx->easy);
        efree(ctx);
        return NULL;
    }

    /* 设置请求 URL */
    curl_easy_setopt(ctx->easy, CURLOPT_URL, req_obj->url);

    /* 设置 HTTP 方法 */
    if (strcmp(req_obj->method, "POST") == 0) {
        curl_easy_setopt(ctx->easy, CURLOPT_POST, 1L);
    } else if (strcmp(req_obj->method, "PUT") == 0) {
        curl_easy_setopt(ctx->easy, CURLOPT_CUSTOMREQUEST, "PUT");
    } else if (strcmp(req_obj->method, "DELETE") == 0) {
        curl_easy_setopt(ctx->easy, CURLOPT_CUSTOMREQUEST, "DELETE");
    } else if (strcmp(req_obj->method, "PATCH") == 0) {
        curl_easy_setopt(ctx->easy, CURLOPT_CUSTOMREQUEST, "PATCH");
    } else if (strcmp(req_obj->method, "HEAD") == 0) {
        curl_easy_setopt(ctx->easy, CURLOPT_NOBODY, 1L);
    } else if (strcmp(req_obj->method, "OPTIONS") == 0) {
        curl_easy_setopt(ctx->easy, CURLOPT_CUSTOMREQUEST, "OPTIONS");
    }
    /* GET 方法是默认值，无需额外设置 */

    /* 设置请求体（POST/PUT/PATCH 等方法） */
    if (req_obj->body != NULL && req_obj->body_len > 0) {
        curl_easy_setopt(ctx->easy, CURLOPT_POSTFIELDS, req_obj->body);
        curl_easy_setopt(ctx->easy, CURLOPT_POSTFIELDSIZE, (long)req_obj->body_len);
    }

    /* 设置超时时间（请求级优先，否则使用全局默认值） */
    long timeout = (req_obj->timeout > 0) ? req_obj->timeout : curl_obj->timeout;
    curl_easy_setopt(ctx->easy, CURLOPT_TIMEOUT, timeout);

    /* 设置连接超时时间 */
    long connect_timeout = (req_obj->connect_timeout > 0) ? req_obj->connect_timeout : curl_obj->connect_timeout;
    curl_easy_setopt(ctx->easy, CURLOPT_CONNECTTIMEOUT, connect_timeout);

    /* 设置 SSL 验证 */
    curl_easy_setopt(ctx->easy, CURLOPT_SSL_VERIFYPEER, (long)curl_obj->verify_ssl);
    curl_easy_setopt(ctx->easy, CURLOPT_SSL_VERIFYHOST, (long)curl_obj->verify_ssl);

    /* 设置 User-Agent */
    if (curl_obj->user_agent != NULL) {
        curl_easy_setopt(ctx->easy, CURLOPT_USERAGENT, curl_obj->user_agent);
    }

    /* 设置代理 */
    if (curl_obj->proxy != NULL) {
        curl_easy_setopt(ctx->easy, CURLOPT_PROXY, curl_obj->proxy);
    }

    /* 设置重定向跟随 */
    if (req_obj->follow_redirects) {
        curl_easy_setopt(ctx->easy, CURLOPT_FOLLOWLOCATION, 1L);
        curl_easy_setopt(ctx->easy, CURLOPT_MAXREDIRS, req_obj->max_redirects);
    }

    /* 构建请求头列表：先添加全局头部，再添加请求级头部（请求级可覆盖全局） */
    struct curl_slist *headers = NULL;

    /* 添加全局头部 */
    xhcurl_header_t *global_h = curl_obj->global_headers_raw;
    while (global_h != NULL) {
        /* 构建 "Name: Value" 格式的头部字符串 */
        char header_line[4096];
        snprintf(header_line, sizeof(header_line), "%s: %s", global_h->name, global_h->value);
        headers = curl_slist_append(headers, header_line);
        global_h = global_h->next;
    }

    /* 添加请求级头部 */
    xhcurl_header_t *req_h = req_obj->headers;
    while (req_h != NULL) {
        char header_line[4096];
        snprintf(header_line, sizeof(header_line), "%s: %s", req_h->name, req_h->value);
        headers = curl_slist_append(headers, header_line);
        req_h = req_h->next;
    }

    /* 设置请求头到 curl 句柄 */
    if (headers != NULL) {
        curl_easy_setopt(ctx->easy, CURLOPT_HTTPHEADER, headers);
        ctx->request_headers = headers;
    }

    /* 设置 Cookie：全局 Cookie + 请求级 Cookie */
    /* 使用 curl_share 共享全局 Cookie */
    if (curl_obj->share != NULL) {
        curl_easy_setopt(ctx->easy, CURLOPT_SHARE, curl_obj->share);
    }

    /* 设置请求级 Cookie（通过 Cookie 头部） */
    if (req_obj->cookies != NULL) {
        /* 使用 zend_string 构建 Cookie 头部字符串（替代 smart_str） */
        zend_string *cookie_str = NULL;
        xhcurl_cookie_t *cookie = req_obj->cookies;
        while (cookie != NULL) {
            /* 计算当前 Cookie 片段长度：name=value */
            size_t seg_len = strlen(cookie->name) + 1 + strlen(cookie->value);
            if (cookie_str == NULL) {
                /* 第一个 Cookie，直接创建 */
                cookie_str = zend_string_alloc(seg_len, 0);
                snprintf(ZSTR_VAL(cookie_str), seg_len + 1, "%s=%s", cookie->name, cookie->value);
            } else {
                /* 后续 Cookie，追加 "; name=value" */
                size_t old_len = ZSTR_LEN(cookie_str);
                size_t new_len = old_len + 2 + seg_len; /* 2 = "; " */
                cookie_str = zend_string_extend(cookie_str, new_len, 0);
                /* 在末尾追加 "; name=value" */
                snprintf(ZSTR_VAL(cookie_str) + old_len, 3 + seg_len, "; %s=%s", cookie->name, cookie->value);
            }
            cookie = cookie->next;
        }
        /* 设置 curl Cookie 选项 */
        if (cookie_str != NULL) {
            curl_easy_setopt(ctx->easy, CURLOPT_COOKIE, ZSTR_VAL(cookie_str));
            zend_string_release(cookie_str);
        }
    }

    /* 设置写数据回调（响应体） */
    curl_easy_setopt(ctx->easy, CURLOPT_WRITEFUNCTION, xhcurl_write_callback);
    curl_easy_setopt(ctx->easy, CURLOPT_WRITEDATA, ctx);

    /* 设置头部回调（响应头） */
    curl_easy_setopt(ctx->easy, CURLOPT_HEADERFUNCTION, xhcurl_header_callback);
    curl_easy_setopt(ctx->easy, CURLOPT_HEADERDATA, ctx);

    /* 保存 PHP 回调引用 */
    if (!Z_ISUNDEF(req_obj->chunk_callback)) {
        ZVAL_COPY(&ctx->chunk_callback, &req_obj->chunk_callback);
    } else {
        ZVAL_UNDEF(&ctx->chunk_callback);
    }

    if (!Z_ISUNDEF(req_obj->header_callback)) {
        ZVAL_COPY(&ctx->header_callback, &req_obj->header_callback);
    } else {
        ZVAL_UNDEF(&ctx->header_callback);
    }

    /* 初始化其他字段 */
    ctx->parsed_headers = NULL;
    ctx->status_code = 0;
    ctx->content_type = NULL;

    return ctx;
}

/**
 * 释放请求执行上下文及其所有资源
 * @param ctx 请求上下文指针
 */
void xhcurl_context_free(xhcurl_req_context_t *ctx)
{
    if (ctx == NULL) {
        return;
    }

    /* 释放 curl easy 句柄 */
    if (ctx->easy != NULL) {
        curl_easy_cleanup(ctx->easy);
        ctx->easy = NULL;
    }

    /* 释放响应体缓冲区 */
    xhcurl_buffer_free(&ctx->body_buf);

    /* 释放响应头缓冲区 */
    xhcurl_buffer_free(&ctx->header_buf);

    /* 释放解析后的头部链表 */
    xhcurl_header_list_free(ctx->parsed_headers);

    /* 释放 curl_slist 格式的请求头 */
    if (ctx->request_headers != NULL) {
        curl_slist_free_all(ctx->request_headers);
        ctx->request_headers = NULL;
    }

    /* 释放 Content-Type 字符串 */
    if (ctx->content_type != NULL) {
        efree(ctx->content_type);
        ctx->content_type = NULL;
    }

    /* 释放 PHP 回调引用 */
    if (!Z_ISUNDEF(ctx->chunk_callback)) {
        zval_ptr_dtor(&ctx->chunk_callback);
        ZVAL_UNDEF(&ctx->chunk_callback);
    }

    if (!Z_ISUNDEF(ctx->header_callback)) {
        zval_ptr_dtor(&ctx->header_callback);
        ZVAL_UNDEF(&ctx->header_callback);
    }

    /* 释放 XHRequest PHP 对象引用 */
    if (!Z_ISUNDEF(ctx->request_zval)) {
        zval_ptr_dtor(&ctx->request_zval);
        ZVAL_UNDEF(&ctx->request_zval);
    }

    /* 释放上下文本身 */
    efree(ctx);
}

/* +----------------------------------------------------------------------+
 * | curl 回调函数实现                                                     |
 * | 由 libcurl 在数据到达时调用，将数据写入上下文缓冲区                   |
 * +----------------------------------------------------------------------+
 */

/**
 * curl 写数据回调（响应体）
 * 当 curl 接收到响应体数据时调用
 * @param contents 数据指针
 * @param size     每个数据元素的大小（始终为 1）
 * @param nmemb    数据元素个数
 * @param userp    用户数据指针（xhcurl_req_context_t）
 * @return 实际处理的数据大小，与传入不一致则 curl 中止传输
 */
size_t xhcurl_write_callback(void *contents, size_t size, size_t nmemb, void *userp)
{
    /* 获取请求上下文 */
    xhcurl_req_context_t *ctx = (xhcurl_req_context_t *)userp;
    /* 计算总数据大小 */
    size_t total_size = size * nmemb;

    /* 将数据写入响应体缓冲区 */
    if (xhcurl_buffer_write(&ctx->body_buf, (const char *)contents, total_size) != 0) {
        /* 缓冲区写入失败（超过最大限制），中止传输 */
        return 0;
    }

    /* 如果注册了流式数据回调，则调用 PHP 回调函数 */
    if (!Z_ISUNDEF(ctx->chunk_callback)) {
        /* 创建 PHP 字符串参数 */
        zval args[1];
        ZVAL_STRINGL(&args[0], (const char *)contents, total_size);

        /* 调用 PHP 回调函数 */
        zval retval;
        int call_result = call_user_function(CG(function_table), NULL,
                                              &ctx->chunk_callback, &retval, 1, args);

        /* 释放参数和返回值 */
        zval_ptr_dtor(&args[0]);
        if (call_result == SUCCESS) {
            zval_ptr_dtor(&retval);
        }

        /* 如果回调抛出异常，中止传输 */
        if (EG(exception) != NULL) {
            return 0;
        }
    }

    return total_size;
}

/**
 * curl 写数据回调（响应头）
 * 当 curl 接收到响应头数据时调用
 * @param contents 数据指针
 * @param size     每个数据元素的大小（始终为 1）
 * @param nmemb    数据元素个数
 * @param userp    用户数据指针（xhcurl_req_context_t）
 * @return 实际处理的数据大小
 */
size_t xhcurl_header_callback(void *contents, size_t size, size_t nmemb, void *userp)
{
    /* 获取请求上下文 */
    xhcurl_req_context_t *ctx = (xhcurl_req_context_t *)userp;
    /* 计算总数据大小 */
    size_t total_size = size * nmemb;

    /* 将头部原始数据写入缓冲区（后续统一解析） */
    xhcurl_buffer_write(&ctx->header_buf, (const char *)contents, total_size);

    /* 如果注册了头部回调，则调用 PHP 回调函数 */
    if (!Z_ISUNDEF(ctx->header_callback)) {
        /* 创建 PHP 字符串参数 */
        zval args[1];
        ZVAL_STRINGL(&args[0], (const char *)contents, total_size);

        /* 调用 PHP 回调函数 */
        zval retval;
        int call_result = call_user_function(CG(function_table), NULL,
                                              &ctx->header_callback, &retval, 1, args);

        /* 释放参数和返回值 */
        zval_ptr_dtor(&args[0]);
        if (call_result == SUCCESS) {
            zval_ptr_dtor(&retval);
        }

        /* 如果回调抛出异常，中止传输 */
        if (EG(exception) != NULL) {
            return 0;
        }
    }

    return total_size;
}
