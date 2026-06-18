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
        for (int i = 0; i < obj->context_count; i++) {
            if (obj->contexts[i] != NULL && obj->contexts[i]->easy != NULL) {
                curl_multi_remove_handle(obj->multi, obj->contexts[i]->easy);
            }
        }
        curl_multi_cleanup(obj->multi);
        obj->multi = NULL;
    }

    /* 释放所有请求上下文 */
    if (obj->contexts != NULL) {
        for (int i = 0; i < obj->context_count; i++) {
            if (obj->contexts[i] != NULL) {
                xhcurl_context_free(obj->contexts[i]);
                obj->contexts[i] = NULL;
            }
        }
        efree(obj->contexts);
        obj->contexts = NULL;
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
    obj->multi = NULL;              /* curl multi 句柄在构造函数中创建 */
    obj->curl_obj = NULL;           /* XHCurl 引用在构造函数中设置 */
    obj->contexts = NULL;           /* 上下文数组在首次 add 时分配 */
    obj->context_count = 0;         /* 初始请求数为 0 */
    obj->context_capacity = 0;      /* 初始容量为 0 */
    obj->request_count = 0;         /* 兼容字段 */
    ZVAL_UNDEF(&obj->curl_zval);    /* 初始化 XHCurl 引用 */

    /* 设置对象释放函数 */
    obj->std.handlers = zend_get_std_object_handlers();

    return &obj->std;
}

/* +----------------------------------------------------------------------+
 * | 内部辅助函数                                                          |
 * +----------------------------------------------------------------------+
 */

/**
 * 在上下文数组中根据 CURL easy 句柄查找对应的上下文
 * @param obj  XHMulti 对象
 * @param easy CURL easy 句柄
 * @return 请求上下文指针，未找到返回 NULL
 */
static xhcurl_req_context_t *xhmulti_find_context(xhmulti_obj_t *obj, CURL *easy)
{
    /* 线性遍历上下文数组查找匹配的 easy 句柄 */
    for (int i = 0; i < obj->context_count; i++) {
        if (obj->contexts[i] != NULL && obj->contexts[i]->easy == easy) {
            return obj->contexts[i];
        }
    }
    return NULL;
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

    /* 检查是否需要扩容上下文数组 */
    if (obj->context_count >= obj->context_capacity) {
        /* 计算新容量（初始 16，之后翻倍增长） */
        int new_capacity = (obj->context_capacity == 0) ? 16 : obj->context_capacity * 2;
        /* 重新分配上下文数组 */
        obj->contexts = (xhcurl_req_context_t **)erealloc(
            obj->contexts, new_capacity * sizeof(xhcurl_req_context_t *));
        if (obj->contexts == NULL) {
            /* 扩容失败，回滚：从 multi 移除 easy 句柄并释放上下文 */
            curl_multi_remove_handle(obj->multi, ctx->easy);
            xhcurl_context_free(ctx);
            zend_throw_exception(xhcurl_exception_ce, "Failed to allocate context array", 0);
            return;
        }
        /* 初始化新分配的空间为 NULL */
        for (int i = obj->context_capacity; i < new_capacity; i++) {
            obj->contexts[i] = NULL;
        }
        obj->context_capacity = new_capacity;
    }

    /* 将上下文添加到数组（保持添加顺序） */
    obj->contexts[obj->context_count] = ctx;
    obj->context_count++;
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
    if (obj->context_count == 0) {
        /* 无请求，返回空数组 */
        array_init(return_value);
        return;
    }

    /* 初始化返回数组（按添加顺序） */
    array_init_size(return_value, obj->context_count);

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

    /* 处理所有已完成的请求：更新状态码、Content-Type、解析响应头 */
    CURLMsg *msg = NULL;
    int msgs_left = 0;
    while ((msg = curl_multi_info_read(obj->multi, &msgs_left)) != NULL) {
        if (msg->msg == CURLMSG_DONE) {
            /* 请求完成，在数组中查找对应的上下文 */
            xhcurl_req_context_t *ctx = xhmulti_find_context(obj, msg->easy_handle);
            if (ctx != NULL) {
                /* 获取 HTTP 状态码 */
                ctx->status_code = 0;
                curl_easy_getinfo(ctx->easy, CURLINFO_RESPONSE_CODE, &ctx->status_code);

                /* 获取 Content-Type */
                char *ct = NULL;
                curl_easy_getinfo(ctx->easy, CURLINFO_CONTENT_TYPE, &ct);
                if (ct != NULL && ctx->content_type == NULL) {
                    ctx->content_type = estrdup(ct);
                }

                /* 解析响应头 */
                xhcurl_parse_response_headers(&ctx->header_buf, &ctx->parsed_headers);
            }
        }
    }

    /* 按添加顺序为每个请求创建 XHResponse 对象 */
    for (int i = 0; i < obj->context_count; i++) {
        xhcurl_req_context_t *ctx = obj->contexts[i];
        if (ctx == NULL) {
            /* 上下文为空，添加一个空响应 */
            zval null_zv;
            ZVAL_NULL(&null_zv);
            add_next_index_zval(return_value, &null_zv);
            continue;
        }

        /* 创建 XHResponse PHP 对象 */
        zval response_zv;
        object_init_ex(&response_zv, xhresponse_ce);
        xhresponse_obj_t *resp_obj = XHRESPONSE_OBJ_FROM_ZOBJ(Z_OBJ_P(&response_zv));

        /* 填充响应数据 */
        resp_obj->status_code = ctx->status_code;
        resp_obj->curl_code = CURLE_OK;

        /* 获取请求总耗时 */
        curl_easy_getinfo(ctx->easy, CURLINFO_TOTAL_TIME, &resp_obj->total_time);

        /* 复制 Content-Type */
        if (ctx->content_type != NULL) {
            resp_obj->content_type = estrdup(ctx->content_type);
        }

        /* 设置错误信息：如果状态码为 0 且无数据，则视为请求失败 */
        if (ctx->status_code == 0 && ctx->body_buf.size == 0) {
            resp_obj->error_msg = estrdup("Request failed");
        }

        /* 转移响应体缓冲区所有权（避免大块内存复制） */
        resp_obj->body = (xhcurl_buffer_t *)ecalloc(1, sizeof(xhcurl_buffer_t));
        if (resp_obj->body != NULL) {
            resp_obj->body->data = ctx->body_buf.data;
            resp_obj->body->size = ctx->body_buf.size;
            resp_obj->body->capacity = ctx->body_buf.capacity;
            resp_obj->body->max_size = ctx->body_buf.max_size;
            /* 清空原始缓冲区指针，防止 xhcurl_context_free 重复释放 */
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

    /* 清理：从 multi 句柄移除所有 easy 句柄 */
    for (int i = 0; i < obj->context_count; i++) {
        if (obj->contexts[i] != NULL && obj->contexts[i]->easy != NULL) {
            curl_multi_remove_handle(obj->multi, obj->contexts[i]->easy);
        }
    }

    /* 释放所有请求上下文 */
    for (int i = 0; i < obj->context_count; i++) {
        if (obj->contexts[i] != NULL) {
            xhcurl_context_free(obj->contexts[i]);
            obj->contexts[i] = NULL;
        }
    }

    /* 重置请求数量和容量（保留已分配的数组供下次使用） */
    obj->context_count = 0;
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
    RETURN_LONG(obj->context_count);
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
