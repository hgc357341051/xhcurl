/* +----------------------------------------------------------------------+
 * | XHCurl 扩展 - 缓冲区管理与辅助函数实现                               |
 * | 包含：缓冲区操作、头部链表操作、Cookie链表操作、                      |
 * |       curl 回调函数、请求上下文管理、辅助工具函数                     |
 * +----------------------------------------------------------------------+
 */

#include "xhcurl_priv.h"

/* +----------------------------------------------------------------------+
 * | 缓冲区操作函数实现                                                    |
 * | 使用 malloc/free 管理内存，不计入 PHP memory_limit                   |
 * | 通过 max_size 限制防止内存溢出                                        |
 * +----------------------------------------------------------------------+
 */

/**
 * 初始化缓冲区
 * @param buf         缓冲区指针
 * @param initial_cap 初始分配容量（字节）
 * @param max_sz      最大允许大小（字节），0 表示无限制
 * @return 0 成功，-1 分配内存失败
 */
int xhcurl_buffer_init(xhcurl_buffer_t *buf, size_t initial_cap, size_t max_sz)
{
    /* 参数有效性检查 */
    if (buf == NULL) {
        return -1;
    }

    /* 分配初始容量内存，使用 malloc 而非 emalloc 避免计入 PHP memory_limit */
    buf->data = (char *)malloc(initial_cap);
    if (buf->data == NULL) {
        /* 内存分配失败 */
        return -1;
    }

    /* 初始化缓冲区各字段 */
    buf->size = 0;              /* 当前已写入数据量为 0 */
    buf->capacity = initial_cap;/* 已分配容量 */
    buf->max_size = max_sz;     /* 最大允许大小 */

    return 0;
}

/**
 * 向缓冲区追加写入数据
 * @param buf  缓冲区指针
 * @param data 待写入的数据指针
 * @param len  待写入的数据长度
 * @return 0 成功，-1 超过最大限制或内存分配失败
 */
int xhcurl_buffer_write(xhcurl_buffer_t *buf, const char *data, size_t len)
{
    /* 参数有效性检查 */
    if (buf == NULL || data == NULL || len == 0) {
        return 0;  /* 空写入视为成功 */
    }

    /* 检查写入后是否超过最大允许大小 */
    if (buf->max_size > 0 && (buf->size + len) > buf->max_size) {
        /* 超过最大限制，拒绝写入，防止内存溢出 */
        return -1;
    }

    /* 检查当前容量是否足够容纳新数据 */
    if ((buf->size + len) > buf->capacity) {
        /* 容量不足，需要扩容 */
        /* 计算新容量：至少为当前需要的 2 倍，采用指数增长策略减少频繁扩容 */
        size_t new_capacity = buf->capacity;
        while (new_capacity < (buf->size + len)) {
            new_capacity *= 2;
        }

        /* 检查新容量是否超过最大限制 */
        if (buf->max_size > 0 && new_capacity > buf->max_size) {
            new_capacity = buf->max_size;
        }

        /* 重新分配内存 */
        char *new_data = (char *)realloc(buf->data, new_capacity);
        if (new_data == NULL) {
            /* 内存分配失败 */
            return -1;
        }

        /* 更新缓冲区指针和容量 */
        buf->data = new_data;
        buf->capacity = new_capacity;
    }

    /* 将新数据追加到缓冲区末尾 */
    memcpy(buf->data + buf->size, data, len);
    /* 更新已写入数据大小 */
    buf->size += len;

    return 0;
}

/**
 * 从缓冲区读取指定范围的数据
 * @param buf      缓冲区指针
 * @param offset   读取起始偏移量（字节）
 * @param length   读取长度（字节）
 * @param out_data 输出数据指针（需调用方 free 释放）
 * @param out_len  输出数据实际长度
 * @return 0 成功，-1 参数错误或内存分配失败
 */
int xhcurl_buffer_read(xhcurl_buffer_t *buf, size_t offset, size_t length,
                        char **out_data, size_t *out_len)
{
    /* 参数有效性检查 */
    if (buf == NULL || out_data == NULL || out_len == NULL) {
        return -1;
    }

    /* 检查偏移量是否超出缓冲区范围 */
    if (offset >= buf->size) {
        /* 偏移量超出范围，返回空数据 */
        *out_data = NULL;
        *out_len = 0;
        return 0;
    }

    /* 计算实际可读取的长度（防止越界） */
    size_t available = buf->size - offset;
    size_t read_len = (length < available) ? length : available;

    /* 分配输出缓冲区，使用 emalloc 因为返回给 PHP 使用 */
    char *result = (char *)emalloc(read_len + 1);
    if (result == NULL) {
        return -1;
    }

    /* 复制数据到输出缓冲区 */
    memcpy(result, buf->data + offset, read_len);
    /* 添加 null 终止符（便于作为 PHP 字符串使用） */
    result[read_len] = '\0';

    /* 设置输出参数 */
    *out_data = result;
    *out_len = read_len;

    return 0;
}

/**
 * 释放缓冲区资源
 * @param buf 缓冲区指针
 */
void xhcurl_buffer_free(xhcurl_buffer_t *buf)
{
    if (buf == NULL) {
        return;
    }

    /* 释放数据内存（使用 free 对应 malloc/realloc） */
    if (buf->data != NULL) {
        free(buf->data);
        buf->data = NULL;
    }

    /* 重置缓冲区状态 */
    buf->size = 0;
    buf->capacity = 0;
    buf->max_size = 0;
}

/* +----------------------------------------------------------------------+
 * | 头部链表操作函数实现                                                  |
 * +----------------------------------------------------------------------+
 */

/**
 * 向头部链表追加一个头部
 * @param list  头部链表指针的指针（可能修改头指针）
 * @param name  头部名称（会被复制并转为小写存储）
 * @param value 头部值（会被复制存储）
 */
void xhcurl_header_add(xhcurl_header_t **list, const char *name, const char *value)
{
    /* 参数有效性检查 */
    if (list == NULL || name == NULL || value == NULL) {
        return;
    }

    /* 分配新节点内存 */
    xhcurl_header_t *node = (xhcurl_header_t *)ecalloc(1, sizeof(xhcurl_header_t));
    if (node == NULL) {
        return;
    }

    /* 复制头部名称并转为小写（便于不区分大小写查找） */
    node->name = estrdup(name);
    xhcurl_str_tolower(node->name, strlen(node->name));

    /* 复制头部值 */
    node->value = estrdup(value);

    /* 将新节点插入链表头部（O(1) 操作） */
    node->next = *list;
    *list = node;
}

/**
 * 在头部链表中查找指定名称的头部值
 * @param list 头部链表指针
 * @param name 要查找的头部名称（不区分大小写）
 * @return 头部值字符串指针，未找到返回 NULL
 */
const char *xhcurl_header_find(xhcurl_header_t *list, const char *name)
{
    /* 参数有效性检查 */
    if (name == NULL) {
        return NULL;
    }

    /* 创建小写化的查找键 */
    char *key = estrdup(name);
    xhcurl_str_tolower(key, strlen(key));

    /* 遍历链表查找匹配的头部 */
    xhcurl_header_t *current = list;
    while (current != NULL) {
        if (strcmp(current->name, key) == 0) {
            /* 找到匹配的头部 */
            efree(key);
            return current->value;
        }
        current = current->next;
    }

    /* 未找到 */
    efree(key);
    return NULL;
}

/**
 * 释放整个头部链表
 * @param list 头部链表指针
 */
void xhcurl_header_list_free(xhcurl_header_t *list)
{
    xhcurl_header_t *current = list;
    while (current != NULL) {
        /* 保存下一个节点指针 */
        xhcurl_header_t *next = current->next;
        /* 释放头部名称 */
        if (current->name != NULL) efree(current->name);
        /* 释放头部值 */
        if (current->value != NULL) efree(current->value);
        /* 释放节点本身 */
        efree(current);
        /* 移动到下一个节点 */
        current = next;
    }
}

/**
 * 解析 HTTP 响应头原始文本，填充到头部链表
 * @param header_buf 原始头部数据缓冲区
 * @param headers    输出头部链表指针的指针
 */
void xhcurl_parse_response_headers(xhcurl_buffer_t *header_buf, xhcurl_header_t **headers)
{
    /* 参数有效性检查 */
    if (header_buf == NULL || headers == NULL || header_buf->data == NULL) {
        return;
    }

    /* 释放已有的头部链表 */
    if (*headers != NULL) {
        xhcurl_header_list_free(*headers);
        *headers = NULL;
    }

    /* 逐行解析头部数据 */
    char *data = header_buf->data;
    size_t remaining = header_buf->size;
    char *line_start = data;

    while (remaining > 0) {
        /* 查找行结束标记 \r\n */
        char *line_end = memmem(line_start, remaining, "\r\n", 2);
        if (line_end == NULL) {
            /* 没有找到 \r\n，剩余部分作为最后一行 */
            line_end = line_start + remaining;
        }

        /* 计算当前行长度 */
        size_t line_len = line_end - line_start;

        /* 跳过空行和 HTTP 状态行（HTTP/1.x ...） */
        if (line_len > 0 && strncmp(line_start, "HTTP/", 5) != 0) {
            /* 查找冒号分隔符 */
            char *colon = memchr(line_start, ':', line_len);
            if (colon != NULL) {
                /* 计算名称长度 */
                size_t name_len = colon - line_start;
                /* 跳过冒号后的空白字符 */
                char *val_start = colon + 1;
                while (val_start < line_end && (*val_start == ' ' || *val_start == '\t')) {
                    val_start++;
                }
                /* 计算值长度 */
                size_t val_len = line_end - val_start;

                /* 复制名称和值（添加 null 终止符） */
                char *name = (char *)ecalloc(1, name_len + 1);
                memcpy(name, line_start, name_len);
                name[name_len] = '\0';

                char *value = (char *)ecalloc(1, val_len + 1);
                memcpy(value, val_start, val_len);
                value[val_len] = '\0';

                /* 添加到头部链表 */
                xhcurl_header_add(headers, name, value);

                /* 释放临时字符串（xhcurl_header_add 内部已复制） */
                efree(name);
                efree(value);
            }
        }

        /* 移动到下一行 */
        if (line_end == line_start + remaining) {
            /* 已到末尾 */
            break;
        }
        /* 跳过 \r\n */
        remaining -= (line_end - line_start) + 2;
        line_start = line_end + 2;
    }
}

/* +----------------------------------------------------------------------+
 * | Cookie 链表操作函数实现                                               |
 * +----------------------------------------------------------------------+
 */

/**
 * 向 Cookie 链表追加一个 Cookie
 * @param list   Cookie 链表指针的指针
 * @param name   Cookie 名称
 * @param value  Cookie 值
 * @param domain Cookie 域名
 * @param path   Cookie 路径
 */
void xhcurl_cookie_add(xhcurl_cookie_t **list, const char *name, const char *value,
                        const char *domain, const char *path)
{
    /* 参数有效性检查 */
    if (list == NULL || name == NULL || value == NULL) {
        return;
    }

    /* 分配新节点内存 */
    xhcurl_cookie_t *node = (xhcurl_cookie_t *)ecalloc(1, sizeof(xhcurl_cookie_t));
    if (node == NULL) {
        return;
    }

    /* 复制各字段 */
    node->name = estrdup(name);
    node->value = estrdup(value);
    node->domain = (domain != NULL) ? estrdup(domain) : estrdup("");
    node->path = (path != NULL) ? estrdup(path) : estrdup("/");

    /* 将新节点插入链表头部 */
    node->next = *list;
    *list = node;
}

/**
 * 释放整个 Cookie 链表
 * @param list Cookie 链表指针
 */
void xhcurl_cookie_list_free(xhcurl_cookie_t *list)
{
    xhcurl_cookie_t *current = list;
    while (current != NULL) {
        /* 保存下一个节点指针 */
        xhcurl_cookie_t *next = current->next;
        /* 释放各字段 */
        if (current->name != NULL) efree(current->name);
        if (current->value != NULL) efree(current->value);
        if (current->domain != NULL) efree(current->domain);
        if (current->path != NULL) efree(current->path);
        /* 释放节点本身 */
        efree(current);
        /* 移动到下一个节点 */
        current = next;
    }
}

/* +----------------------------------------------------------------------+
 * | 请求上下文操作函数实现                                                |
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
        /* 构建 Cookie 头部字符串 */
        smart_str cookie_str = {0};
        xhcurl_cookie_t *cookie = req_obj->cookies;
        while (cookie != NULL) {
            if (cookie_str.s != NULL) {
                smart_str_appendl(&cookie_str, "; ", 2);
            }
            smart_str_appendl(&cookie_str, cookie->name, strlen(cookie->name));
            smart_str_appendc(&cookie_str, '=');
            smart_str_appendl(&cookie_str, cookie->value, strlen(cookie->value));
            cookie = cookie->next;
        }
        smart_str_0(&cookie_str);
        if (cookie_str.s != NULL) {
            curl_easy_setopt(ctx->easy, CURLOPT_COOKIE, ZSTR_VAL(cookie_str.s));
            smart_str_free(&cookie_str);
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

/* +----------------------------------------------------------------------+
 * | 辅助工具函数实现                                                      |
 * +----------------------------------------------------------------------+
 */

/**
 * 检查当前是否为 CLI 模式
 * 线程池功能仅在 CLI 模式下可用，FPM 模式下禁用
 * @return 1 为 CLI 模式，0 为其他模式
 */
zend_bool xhcurl_is_cli_mode(void)
{
    /* 通过 sapi_module.name 判断当前运行模式 */
    return (sapi_module.name != NULL && strcmp(sapi_module.name, "cli") == 0);
}

/**
 * 将字符串转为小写（原地修改）
 * 用于头部名称规范化，实现不区分大小写的头部查找
 * @param str 字符串指针
 * @param len 字符串长度
 */
void xhcurl_str_tolower(char *str, size_t len)
{
    if (str == NULL) return;
    for (size_t i = 0; i < len; i++) {
        /* 将大写字母转为小写 */
        if (str[i] >= 'A' && str[i] <= 'Z') {
            str[i] = str[i] - 'A' + 'a';
        }
    }
}

/**
 * 判断 Content-Type 是否为 JSON 类型
 * @param content_type Content-Type 头部值
 * @return 1 为 JSON 类型，0 为非 JSON 类型
 */
zend_bool xhcurl_is_json_content_type(const char *content_type)
{
    if (content_type == NULL) {
        return 0;
    }
    /* 检查是否包含 "json" 子串（不区分大小写） */
    return (strcasestr(content_type, "json") != NULL) ? 1 : 0;
}
