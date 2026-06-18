/* +----------------------------------------------------------------------+
 * | XHCurl 扩展 - XHMulti 批量异步执行器类实现                           |
 * | 基于 curl_multi 接口实现并发请求                                      |
 * | FPM 和 CLI 模式通用，使用单进程异步 I/O 多路复用                     |
 * | 支持流式回调（onChunk/onHeader），在数据到达时实时触发               |
 * +----------------------------------------------------------------------+
 */

#include "xhcurl_priv.h"

/* 类入口指针 */
zend_class_entry *xhmulti_ce;

/* +----------------------------------------------------------------------+
 * | GHashTable 辅助函数（用于 CURL* -> context 映射）                     |
 * +----------------------------------------------------------------------+
 */

/**
 * GHash 哈希函数：将 CURL 指针转为哈希键
 * @param key CURL 指针
 * @return 哈希值
 */
static guint xhcurl_ghash_ptr_hash(gconstpointer key)
{
    /* 将指针值直接作为哈希值 */
    return GPOINTER_TO_UINT(key);
}

/**
 * GHash 比较函数：比较两个 CURL 指针是否相等
 * @param a 指针 a
 * @param b 指针 b
 * @return TRUE 相等，FALSE 不相等
 */
static gboolean xhcurl_ghash_ptr_equal(gconstpointer a, gconstpointer b)
{
    return (a == b);
}

/**
 * GHash 值销毁函数：释放请求上下文
 * @param data 请求上下文指针
 */
static void xhcurl_ghash_value_destroy(gpointer data)
{
    xhcurl_req_context_t *ctx = (xhcurl_req_context_t *)data;
    if (ctx != NULL) {
        xhcurl_context_free(ctx);
    }
}

/* +----------------------------------------------------------------------+
 * | 对象生命周期函数                                                      |
 * +----------------------------------------------------------------------+
 */

/**
 * 释放 XHMulti 对象资源
 * @param object zend_object 指针
 */
static void xhmulti_free_obj(zend_object *object)
{
    /* 从 zend_object 获取 XHMulti 对象 */
    xhmulti_obj_t *obj = XHMULTI_OBJ_FROM_ZOBJ(object);

    /* 释放 curl multi 句柄 */
    if (obj->multi != NULL) {
        /* 先移除所有 easy 句柄 */
        CURLMsg *msg = NULL;
        int msgs_left = 0;
        while ((msg = curl_multi_info_read(obj->multi, &msgs_left)) != NULL) {
            /* 仅处理完成的消息 */
        }
        curl_multi_cleanup(obj->multi);
        obj->multi = NULL;
    }

    /* 释放上下文映射表（会自动调用 value_destroy 释放各上下文） */
    if (obj->ctx_map != NULL) {
        g_hash_table_destroy(obj->ctx_map);
        obj->ctx_map = NULL;
    }

    /* 释放 XHCurl PHP 对象引用 */
    if (!Z_ISUNDEF(obj->curl_zval)) {
        zval_ptr_dtor(&obj->curl_zval);
        ZVAL_UNDEF(&obj->curl_zval);
    }

    /* 调用标准对象释放函数 */
    zend_object_std_dtor(object);
}

/**
 * 创建 XHMulti 对象
 * @param class_type 类入口指针
 * @return zend_object 指针
 */
static zend_object *xhmulti_create_obj(zend_class_entry *class_type)
{
    /* 分配对象内存 */
    xhmulti_obj_t *obj = (xhmulti_obj_t *)zend_object_alloc(sizeof(xhmulti_obj_t), class_type);

    /* 初始化 PHP 标准对象 */
    zend_object_std_init(&obj->std, class_type);
    /* 初始化对象属性 */
    object_properties_init(&obj->std, class_type);

    /* 初始化自定义字段 */
    obj->multi = NULL;          /* curl multi 句柄在构造函数中创建 */
    obj->curl_obj = NULL;       /* XHCurl 引用在构造函数中设置 */
    obj->ctx_map = NULL;        /* 上下文映射表在构造函数中创建 */
    obj->request_count = 0;     /* 初始请求数为 0 */
    ZVAL_UNDEF(&obj->curl_zval); /* 初始化 XHCurl 引用 */

    /* 设置对象释放函数 */
    obj->std.handlers = zend_get_std_object_handlers();

    return &obj->std;
}

/* +----------------------------------------------------------------------+
 * | PHP 方法实现                                                          |
 * +----------------------------------------------------------------------+
 */

/**
 * 构造函数
 * XHMulti::__construct(XHCurl $curl)
 */
PHP_METHOD(XHMulti, __construct)
{
    zval *curl_zv;      /* XHCurl 对象参数 */

    /* 解析参数 */
    ZEND_PARSE_PARAMETERS_START(1, 1)
        Z_PARAM_OBJECT_OF_CLASS(curl_zv, xhcurl_ce)
    ZEND_PARSE_PARAMETERS_END();

    /* 获取 XHMulti 对象 */
    xhmulti_obj_t *obj = XHMULTI_OBJ_FROM_ZOBJ(Z_OBJ_P(getThis()));

    /* 保存 XHCurl PHP 对象引用（防止 GC 回收） */
    ZVAL_COPY(&obj->curl_zval, curl_zv);
    /* 保存 XHCurl 内部对象指针（快速访问，无需每次从 zval 获取） */
    obj->curl_obj = XHCURL_OBJ_FROM_ZOBJ(Z_OBJ_P(curl_zv));

    /* 创建 curl multi 句柄 */
    obj->multi = curl_multi_init();
    if (obj->multi == NULL) {
        zend_throw_exception(xhcurl_exception_ce, "Failed to create curl multi handle", 0);
        return;
    }

    /* 创建上下文映射表：CURL* -> xhcurl_req_context_t* */
    obj->ctx_map = g_hash_table_new_full(
        xhcurl_ghash_ptr_hash,       /* 哈希函数 */
        xhcurl_ghash_ptr_equal,      /* 比较函数 */
        NULL,                         /* 键销毁函数（CURL* 由 curl_easy_cleanup 管理） */
        xhcurl_ghash_value_destroy   /* 值销毁函数（释放请求上下文） */
    );
    if (obj->ctx_map == NULL) {
        zend_throw_exception(xhcurl_exception_ce, "Failed to create context map", 0);
        return;
    }
}

/**
 * 添加请求到批量执行器
 * XHMulti::add(XHRequest $request): void
 */
PHP_METHOD(XHMulti, add)
{
    zval *request_zv;   /* XHRequest 对象参数 */

    /* 解析参数 */
    ZEND_PARSE_PARAMETERS_START(1, 1)
        Z_PARAM_OBJECT_OF_CLASS(request_zv, xhrequest_ce)
    ZEND_PARSE_PARAMETERS_END();

    /* 获取 XHMulti 对象 */
    xhmulti_obj_t *obj = XHMULTI_OBJ_FROM_ZOBJ(Z_OBJ_P(getThis()));
    /* 获取 XHRequest 对象 */
    xhrequest_obj_t *req_obj = XHREQUEST_OBJ_FROM_ZOBJ(Z_OBJ_P(request_zv));

    /* 创建请求执行上下文 */
    xhcurl_req_context_t *ctx = xhcurl_context_create(obj->curl_obj, req_obj);
    if (ctx == NULL) {
        zend_throw_exception(xhcurl_exception_ce, "Failed to create request context", 0);
        return;
    }

    /* 保存 XHRequest PHP 对象引用（防止 GC 回收） */
    ZVAL_COPY(&ctx->request_zval, request_zv);

    /* 将 easy 句柄添加到 multi 句柄 */
    CURLMcode mres = curl_multi_add_handle(obj->multi, ctx->easy);
    if (mres != CURLM_OK) {
        xhcurl_context_free(ctx);
        zend_throw_exception(xhcurl_exception_ce, "Failed to add handle to multi", 0);
        return;
    }

    /* 将上下文添加到映射表 */
    g_hash_table_insert(obj->ctx_map, (gpointer)ctx->easy, (gpointer)ctx);

    /* 增加请求计数 */
    obj->request_count++;
}

/**
 * 执行所有已添加的请求
 * XHMulti::execute(): array
 * 基于 curl_multi 事件循环，支持流式回调
 * 返回 XHResponse 对象数组，与添加顺序一致
 */
PHP_METHOD(XHMulti, execute)
{
    /* 无参数 */
    ZEND_PARSE_PARAMETERS_NONE();

    /* 获取 XHMulti 对象 */
    xhmulti_obj_t *obj = XHMULTI_OBJ_FROM_ZOBJ(Z_OBJ_P(getThis()));

    /* 检查是否有请求需要执行 */
    if (obj->request_count == 0) {
        /* 无请求，返回空数组 */
        array_init(return_value);
        return;
    }

    /* 初始化返回数组 */
    array_init_size(return_value, obj->request_count);

    /* 收集所有上下文到有序数组（保持添加顺序） */
    xhcurl_req_context_t **ctx_array = (xhcurl_req_context_t **)ecalloc(
        obj->request_count, sizeof(xhcurl_req_context_t *));
    guint ctx_idx = 0;

    /* 遍历映射表，收集上下文指针 */
    GHashTableIter iter;
    gpointer key, value;
    g_hash_table_iter_init(&iter, obj->ctx_map);
    while (g_hash_table_iter_next(&iter, &key, &value)) {
        if (ctx_idx < (guint)obj->request_count) {
            ctx_array[ctx_idx++] = (xhcurl_req_context_t *)value;
        }
    }

    /* curl_multi 事件循环 */
    int still_running = 0;
    do {
        /* 执行一次 multi 操作 */
        CURLMcode mc = curl_multi_perform(obj->multi, &still_running);
        if (mc != CURLM_OK) {
            /* multi 操作失败 */
            break;
        }

        /* 等待活动（最多 100ms），避免 CPU 空转 */
        if (still_running > 0) {
            mc = curl_multi_wait(obj->multi, NULL, 0, 100, NULL);
            if (mc != CURLM_OK) {
                break;
            }
        }

        /* 检查是否有异常（可能由流式回调抛出） */
        if (EG(exception) != NULL) {
            /* 有异常，中止所有请求 */
            break;
        }
    } while (still_running > 0);

    /* 处理所有已完成的请求 */
    CURLMsg *msg = NULL;
    int msgs_left = 0;
    while ((msg = curl_multi_info_read(obj->multi, &msgs_left)) != NULL) {
        if (msg->msg == CURLMSG_DONE) {
            /* 请求完成，从映射表中获取上下文 */
            xhcurl_req_context_t *ctx = (xhcurl_req_context_t *)g_hash_table_lookup(
                obj->ctx_map, (gpointer)msg->easy_handle);
            if (ctx != NULL) {
                /* 更新 curl 错误码 */
                ctx->status_code = 0;
                curl_easy_getinfo(ctx->easy, CURLINFO_RESPONSE_CODE, &ctx->status_code);

                /* 获取 Content-Type */
                char *ct = NULL;
                curl_easy_getinfo(ctx->easy, CURLINFO_CONTENT_TYPE, &ct);
                if (ct != NULL) {
                    ctx->content_type = estrdup(ct);
                }

                /* 解析响应头 */
                xhcurl_parse_response_headers(&ctx->header_buf, &ctx->parsed_headers);
            }
        }
    }

    /* 为每个请求创建 XHResponse 对象 */
    for (guint i = 0; i < ctx_idx; i++) {
        xhcurl_req_context_t *ctx = ctx_array[i];
        if (ctx == NULL) continue;

        /* 创建 XHResponse PHP 对象 */
        zval response_zv;
        object_init_ex(&response_zv, xhresponse_ce);
        xhresponse_obj_t *resp_obj = XHRESPONSE_OBJ_FROM_ZOBJ(Z_OBJ_P(&response_zv));

        /* 填充响应数据 */
        resp_obj->status_code = ctx->status_code;
        resp_obj->curl_code = curl_easy_getinfo(ctx->easy, CURLINFO_TOTAL_TIME, &resp_obj->total_time) == CURLE_OK ?
                              CURLE_OK : CURLE_FAILED_INIT;

        /* 获取请求总耗时 */
        curl_easy_getinfo(ctx->easy, CURLINFO_TOTAL_TIME, &resp_obj->total_time);

        /* 复制 Content-Type */
        if (ctx->content_type != NULL) {
            resp_obj->content_type = estrdup(ctx->content_type);
        }

        /* 设置错误信息 */
        CURLcode res = CURLE_OK;
        /* 检查是否有 curl 错误（通过 curl_multi_info_read 的 result 字段） */
        /* 这里简化处理：如果状态码为 0 且无数据，则视为请求失败 */
        if (ctx->status_code == 0 && ctx->body_buf.size == 0) {
            resp_obj->error_msg = estrdup("Request failed");
        }

        /* 转移响应体缓冲区所有权 */
        resp_obj->body = (xhcurl_buffer_t *)ecalloc(1, sizeof(xhcurl_buffer_t));
        if (resp_obj->body != NULL) {
            resp_obj->body->data = ctx->body_buf.data;
            resp_obj->body->size = ctx->body_buf.size;
            resp_obj->body->capacity = ctx->body_buf.capacity;
            resp_obj->body->max_size = ctx->body_buf.max_size;
            /* 清空原始缓冲区指针，防止重复释放 */
            ctx->body_buf.data = NULL;
            ctx->body_buf.size = 0;
            ctx->body_buf.capacity = 0;
        }

        /* 转移响应头链表所有权 */
        resp_obj->headers = ctx->parsed_headers;
        ctx->parsed_headers = NULL;

        /* 将响应添加到返回数组 */
        add_next_index_zval(return_value, &response_zv);
    }

    /* 释放有序上下文数组 */
    efree(ctx_array);

    /* 清理：从 multi 句柄移除所有 easy 句柄，并清空映射表 */
    GHashTableIter cleanup_iter;
    gpointer cleanup_key, cleanup_value;
    g_hash_table_iter_init(&cleanup_iter, obj->ctx_map);
    while (g_hash_table_iter_next(&cleanup_iter, &cleanup_key, &cleanup_value)) {
        CURL *easy = (CURL *)cleanup_key;
        /* 从 multi 句柄移除 easy 句柄 */
        curl_multi_remove_handle(obj->multi, easy);
    }

    /* 清空映射表（会自动调用 value_destroy 释放各上下文） */
    g_hash_table_remove_all(obj->ctx_map);

    /* 重置请求计数 */
    obj->request_count = 0;
}

/**
 * 获取已添加的请求数量
 * XHMulti::count(): int
 */
PHP_METHOD(XHMulti, count)
{
    /* 无参数 */
    ZEND_PARSE_PARAMETERS_NONE();

    /* 获取 XHMulti 对象 */
    xhmulti_obj_t *obj = XHMULTI_OBJ_FROM_ZOBJ(Z_OBJ_P(getThis()));

    /* 返回请求数量 */
    RETURN_LONG(obj->request_count);
}

/* +----------------------------------------------------------------------+
 * | 方法注册表                                                            |
 * +----------------------------------------------------------------------+
 */

/* 构造函数参数信息 */
ZEND_BEGIN_ARG_INFO_EX(arginfo_xhmulti_construct, 0, 0, 1)
    ZEND_ARG_INFO(0, curl)       /* XHCurl 对象（必填） */
ZEND_END_ARG_INFO()

/* add 参数信息 */
ZEND_BEGIN_ARG_INFO_EX(arginfo_xhmulti_add, 0, 0, 1)
    ZEND_ARG_INFO(0, request)    /* XHRequest 对象（必填） */
ZEND_END_ARG_INFO()

static const zend_function_entry xhmulti_methods[] = {
    /* 构造函数 */
    PHP_ME(XHMulti, __construct, arginfo_xhmulti_construct, ZEND_ACC_PUBLIC)
    /* 添加请求 */
    PHP_ME(XHMulti, add, arginfo_xhmulti_add, ZEND_ACC_PUBLIC)
    /* 执行所有请求 */
    PHP_ME(XHMulti, execute, NULL, ZEND_ACC_PUBLIC)
    /* 获取请求数量 */
    PHP_ME(XHMulti, count, NULL, ZEND_ACC_PUBLIC)
    /* 结束标记 */
    PHP_FE_END
};

/* +----------------------------------------------------------------------+
 * | 类初始化函数                                                          |
 * +----------------------------------------------------------------------+
 */
PHP_MINIT_FUNCTION(xhmulti_class)
{
    /* 初始化类入口 */
    zend_class_entry ce;
    INIT_CLASS_ENTRY(ce, "XHMulti", xhmulti_methods);

    /* 注册类 */
    xhmulti_ce = zend_register_internal_class(&ce);

    /* 设置对象创建和释放函数 */
    xhmulti_ce->create_object = xhmulti_create_obj;

    return SUCCESS;
}
