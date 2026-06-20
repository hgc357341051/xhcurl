/* +----------------------------------------------------------------------+
 * | XHCurl 扩展 - XHThreadPool CLI线程池类实现                           |
 * | 仅在 CLI 模式下可用，使用多线程并发执行请求                           |
 * | 核心设计：                                                            |
 * |   - add() 仅入队 XHRequest 引用，延迟创建 curl 上下文                |
 * |   - execute() 时才创建上下文并分配给工作线程                          |
 * |   - 每个工作线程独立运行 curl_multi 事件循环                          |
 * |   - 工作线程中不调用任何 PHP 函数（线程安全限制）                     |
 * |   - 请求结果在工作线程中缓冲，完成后由主线程创建 XHResponse 对象      |
 * |   - 流式回调（onChunk/onHeader）在线程池模式下不可用                  |
 * +----------------------------------------------------------------------+
 */

#include "xhcurl_priv.h"

/* 类入口指针 */
zend_class_entry *xhthreadpool_ce;

/* +----------------------------------------------------------------------+
 * | 工作线程函数                                                          |
 * | 每个工作线程独立运行 curl_multi 事件循环                              |
 * | 不调用任何 PHP 函数，仅操作 C 数据结构                                |
 * +----------------------------------------------------------------------+
 */

/**
 * 工作线程入口函数
 * @param arg 工作线程参数指针（xhcurl_worker_arg_t）
 * @return 线程返回值
 */
#ifdef PHP_WIN32
static unsigned __stdcall xhcurl_worker_thread(void *arg)
#else
static void *xhcurl_worker_thread(void *arg)
#endif
{
    /* 获取工作线程参数 */
    xhcurl_worker_arg_t *worker = (xhcurl_worker_arg_t *)arg;

    /* 创建当前线程的 curl multi 句柄 */
    CURLM *multi = curl_multi_init();
    if (multi == NULL) {
        /* 创建失败，设置错误码 */
        worker->error = -1;
        worker->done = 1;
        return 0;
    }

    /* 将所有分配给此线程的请求添加到 multi 句柄 */
    for (int i = 0; i < worker->context_count; i++) {
        xhcurl_req_context_t *ctx = worker->contexts[i];
        if (ctx != NULL && ctx->easy != NULL) {
            /* 注意：线程池模式下，不设置 PHP 回调（线程安全限制） */
            /* 清除可能存在的 PHP 回调引用 */
            /* 回调已在主线程的 xhcurl_context_create 中设置， */
            /* 但在线程池中不会被调用，因为 curl 回调中会检查回调是否有效 */
            /* 这里我们确保不触发 PHP 回调 */

            /* 添加 easy 句柄到 multi */
            curl_multi_add_handle(multi, ctx->easy);
        }
    }

    /* 运行 curl_multi 事件循环 */
    int still_running = 0;
    do {
        /* 执行一次 multi 操作 */
        CURLMcode mc = curl_multi_perform(multi, &still_running);
        if (mc != CURLM_OK) {
            break;
        }

        /* 等待活动（最多 100ms），避免 CPU 空转 */
        if (still_running > 0) {
            mc = curl_multi_wait(multi, NULL, 0, 100, NULL);
            if (mc != CURLM_OK) {
                break;
            }
        }
    } while (still_running > 0);

    /* 处理所有已完成的请求 */
    CURLMsg *msg = NULL;
    int msgs_left = 0;
    while ((msg = curl_multi_info_read(multi, &msgs_left)) != NULL) {
        if (msg->msg == CURLMSG_DONE) {
            /* 在分配给此线程的上下文中查找匹配的 easy 句柄 */
            for (int i = 0; i < worker->context_count; i++) {
                xhcurl_req_context_t *ctx = worker->contexts[i];
                if (ctx != NULL && ctx->easy == msg->easy_handle) {
                    /* 获取 HTTP 状态码 */
                    curl_easy_getinfo(ctx->easy, CURLINFO_RESPONSE_CODE, &ctx->status_code);

                    /* 获取 Content-Type */
                    char *ct = NULL;
                    curl_easy_getinfo(ctx->easy, CURLINFO_CONTENT_TYPE, &ct);
                    if (ct != NULL && ctx->content_type == NULL) {
                        /* 使用 strdup 而非 estrdup（线程中不能使用 PHP 内存管理器） */
                        ctx->content_type = strdup(ct);
                    }

                    /* 解析响应头 */
                    xhcurl_parse_response_headers(&ctx->header_buf, &ctx->parsed_headers);

                    break;
                }
            }
        }
    }

    /* 清理：从 multi 句柄移除所有 easy 句柄 */
    for (int i = 0; i < worker->context_count; i++) {
        xhcurl_req_context_t *ctx = worker->contexts[i];
        if (ctx != NULL && ctx->easy != NULL) {
            curl_multi_remove_handle(multi, ctx->easy);
        }
    }

    /* 释放 multi 句柄 */
    curl_multi_cleanup(multi);

    /* 标记工作线程完成 */
    worker->done = 1;

    return 0;
}

/* +----------------------------------------------------------------------+
 * | 对象生命周期函数                                                      |
 * +----------------------------------------------------------------------+
 */

/**
 * 释放 XHThreadPool 对象资源
 * @param object zend_object 指针
 */
static void xhthreadpool_free_obj(zend_object *object)
{
    /* 从 zend_object 获取 XHThreadPool 对象 */
    xhthreadpool_obj_t *obj = XHTHREADPOOL_OBJ_FROM_ZOBJ(object);

    /* 释放待执行请求队列中的 XHRequest 引用 */
    if (obj->pending_requests != NULL) {
        for (int i = 0; i < obj->pending_count; i++) {
            if (!Z_ISUNDEF(obj->pending_requests[i])) {
                zval_ptr_dtor(&obj->pending_requests[i]);
                ZVAL_UNDEF(&obj->pending_requests[i]);
            }
        }
        efree(obj->pending_requests);
        obj->pending_requests = NULL;
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
 * 创建 XHThreadPool 对象
 * @param class_type 类入口指针
 * @return zend_object 指针
 */
static zend_object *xhthreadpool_create_obj(zend_class_entry *class_type)
{
    /* 分配对象内存 */
    xhthreadpool_obj_t *obj = (xhthreadpool_obj_t *)zend_object_alloc(
        sizeof(xhthreadpool_obj_t), class_type);

    /* 初始化 PHP 标准对象 */
    zend_object_std_init(&obj->std, class_type);
    /* 初始化对象属性 */
    object_properties_init(&obj->std, class_type);

    /* 初始化自定义字段 */
    obj->curl_obj = NULL;               /* XHCurl 引用在构造函数中设置 */
    obj->worker_count = XHCURL_DEFAULT_THREAD_POOL_SIZE; /* 默认 4 个工作线程 */
    obj->pending_requests = NULL;       /* 待执行队列在首次 add 时分配 */
    obj->pending_count = 0;             /* 初始待执行数为 0 */
    obj->pending_capacity = 0;          /* 初始容量为 0 */
    ZVAL_UNDEF(&obj->curl_zval);        /* 初始化 XHCurl 引用 */

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
 * XHThreadPool::__construct(XHCurl $curl, int $workers = 4)
 * 仅在 CLI 模式下可用，FPM 模式下会抛出异常
 */
PHP_METHOD(XHThreadPool, __construct)
{
    zval *curl_zv;                  /* XHCurl 对象参数 */
    zend_long workers = XHCURL_DEFAULT_THREAD_POOL_SIZE; /* 工作线程数（默认 4） */

    /* 解析参数 */
    ZEND_PARSE_PARAMETERS_START(1, 2)
        Z_PARAM_OBJECT_OF_CLASS(curl_zv, xhcurl_ce)
        Z_PARAM_OPTIONAL
        Z_PARAM_LONG(workers)
    ZEND_PARSE_PARAMETERS_END();

    /* 检查是否为 CLI 模式（线程池仅在 CLI 下可用） */
    if (!xhcurl_is_cli_mode()) {
        /* FPM 模式下不允许使用线程池，避免线程安全问题 */
        zend_throw_exception(xhcurl_exception_ce,
            "XHThreadPool is only available in CLI mode. Use XHMulti for FPM.", 0);
        return;
    }

    /* 验证工作线程数 */
    if (workers <= 0 || workers > 64) {
        /* 线程数必须在 1-64 之间 */
        zend_throw_exception(xhcurl_exception_ce,
            "Worker count must be between 1 and 64", 0);
        return;
    }

    /* 获取 XHThreadPool 对象 */
    xhthreadpool_obj_t *obj = XHTHREADPOOL_OBJ_FROM_ZOBJ(Z_OBJ_P(getThis()));

    /* 保存 XHCurl PHP 对象引用 */
    ZVAL_COPY(&obj->curl_zval, curl_zv);
    /* 保存 XHCurl 内部对象指针 */
    obj->curl_obj = XHCURL_OBJ_FROM_ZOBJ(Z_OBJ_P(curl_zv));
    /* 设置工作线程数 */
    obj->worker_count = (int)workers;
}

/**
 * 添加请求到线程池
 * XHThreadPool::add(XHRequest $request): void
 *
 * 延迟创建模式：add() 仅将 XHRequest 引用存入待执行队列，
 * 不创建 curl 上下文。上下文在 execute() 时按需创建。
 * 注意：线程池模式下 onChunk/onHeader 回调无效
 */
PHP_METHOD(XHThreadPool, add)
{
    zval *request_zv;   /* XHRequest 对象参数 */

    /* 解析参数 */
    ZEND_PARSE_PARAMETERS_START(1, 1)
        Z_PARAM_OBJECT_OF_CLASS(request_zv, xhrequest_ce)
    ZEND_PARSE_PARAMETERS_END();

    /* 获取 XHThreadPool 对象 */
    xhthreadpool_obj_t *obj = XHTHREADPOOL_OBJ_FROM_ZOBJ(Z_OBJ_P(getThis()));

    /* 检查是否需要扩容待执行队列 */
    if (obj->pending_count >= obj->pending_capacity) {
        /* 计算新容量（初始 64，之后翻倍增长） */
        int new_capacity = (obj->pending_capacity == 0) ? 64 : obj->pending_capacity * 2;
        /* 重新分配队列数组 */
        obj->pending_requests = (zval *)erealloc(
            obj->pending_requests, new_capacity * sizeof(zval));
        if (obj->pending_requests == NULL) {
            zend_throw_exception(xhcurl_exception_ce, "Failed to allocate pending queue", 0);
            return;
        }
        /* 初始化新分配的空间 */
        for (int i = obj->pending_count; i < new_capacity; i++) {
            ZVAL_UNDEF(&obj->pending_requests[i]);
        }
        obj->pending_capacity = new_capacity;
    }

    /* 将 XHRequest 引用存入待执行队列（仅增加引用计数，不创建 curl 上下文） */
    ZVAL_COPY(&obj->pending_requests[obj->pending_count], request_zv);
    obj->pending_count++;
}

/**
 * 执行所有已添加的请求（多线程并发）
 * XHThreadPool::execute(): array
 *
 * 执行流程：
 *   1. 从 pending 队列创建所有请求上下文（此时才创建 curl 句柄）
 *   2. 将上下文均匀分配给各工作线程
 *   3. 启动所有工作线程并发执行
 *   4. 等待所有线程完成
 *   5. 为每个请求创建 XHResponse 对象
 *   6. 释放所有上下文资源
 */
PHP_METHOD(XHThreadPool, execute)
{
    /* 无参数 */
    ZEND_PARSE_PARAMETERS_NONE();

    /* 获取 XHThreadPool 对象 */
    xhthreadpool_obj_t *obj = XHTHREADPOOL_OBJ_FROM_ZOBJ(Z_OBJ_P(getThis()));

    /* 检查是否有请求需要执行 */
    if (obj->pending_count == 0) {
        array_init(return_value);
        return;
    }

    /* --- 阶段 1：从 pending 队列创建所有请求上下文 --- */
    /* 此时才创建 curl easy 句柄，分配网络资源 */
    xhcurl_req_context_t **contexts = (xhcurl_req_context_t **)ecalloc(
        obj->pending_count, sizeof(xhcurl_req_context_t *));
    if (contexts == NULL) {
        zend_throw_exception(xhcurl_exception_ce, "Failed to allocate context array", 0);
        return;
    }

    int context_count = 0; /* 成功创建的上下文数量 */
    for (int i = 0; i < obj->pending_count; i++) {
        /* 获取 XHRequest 内部对象 */
        xhrequest_obj_t *req_obj = XHREQUEST_OBJ_FROM_ZOBJ(Z_OBJ_P(&obj->pending_requests[i]));

        /* 创建请求执行上下文 */
        xhcurl_req_context_t *ctx = xhcurl_context_create(obj->curl_obj, req_obj);
        if (ctx == NULL) {
            /* 创建失败，跳过此请求 */
            contexts[i] = NULL;
            continue;
        }

        /* 保存 XHRequest PHP 对象引用 */
        ZVAL_COPY(&ctx->request_zval, &obj->pending_requests[i]);

        contexts[i] = ctx;
        context_count++;
    }

    /* 检查是否有成功创建的上下文 */
    if (context_count == 0) {
        /* 所有上下文创建失败 */
        efree(contexts);
        array_init(return_value);
        /* 清理 pending 队列 */
        for (int i = 0; i < obj->pending_count; i++) {
            if (!Z_ISUNDEF(obj->pending_requests[i])) {
                zval_ptr_dtor(&obj->pending_requests[i]);
                ZVAL_UNDEF(&obj->pending_requests[i]);
            }
        }
        obj->pending_count = 0;
        return;
    }

    /* --- 阶段 2：将请求均匀分配给各工作线程 --- */
    int actual_workers = (context_count < obj->worker_count) ?
                          context_count : obj->worker_count;

    /* 分配工作线程参数数组 */
    xhcurl_worker_arg_t *workers = (xhcurl_worker_arg_t *)ecalloc(
        actual_workers, sizeof(xhcurl_worker_arg_t));
    if (workers == NULL) {
        /* 分配失败，释放所有上下文 */
        for (int i = 0; i < obj->pending_count; i++) {
            if (contexts[i] != NULL) {
                xhcurl_context_free(contexts[i]);
            }
        }
        efree(contexts);
        zend_throw_exception(xhcurl_exception_ce, "Failed to allocate worker args", 0);
        return;
    }

    /* 将请求均匀分配给各工作线程 */
    int base_count = context_count / actual_workers;   /* 每个线程的基础请求数 */
    int remainder = context_count % actual_workers;     /* 余数（前 remainder 个线程多处理一个） */
    int ctx_offset = 0;                                 /* 请求上下文数组偏移量 */

    for (int i = 0; i < actual_workers; i++) {
        /* 计算当前线程负责的请求数量 */
        int count = base_count + ((i < remainder) ? 1 : 0);

        /* 初始化工作线程参数 */
        workers[i].worker_id = i;
        workers[i].contexts = &contexts[ctx_offset];
        workers[i].context_count = count;
        workers[i].done = 0;
        workers[i].error = 0;

        /* 更新偏移量 */
        ctx_offset += count;
    }

    /* --- 阶段 3：创建并启动工作线程 --- */
#ifdef PHP_WIN32
    /* Windows 平台：使用 _beginthreadex */
    HANDLE *thread_handles = (HANDLE *)ecalloc(actual_workers, sizeof(HANDLE));
    for (int i = 0; i < actual_workers; i++) {
        thread_handles[i] = (HANDLE)_beginthreadex(
            NULL,                   /* 默认安全属性 */
            0,                      /* 默认栈大小 */
            xhcurl_worker_thread,   /* 线程函数 */
            &workers[i],            /* 线程参数 */
            0,                      /* 立即运行 */
            NULL                    /* 不需要线程 ID */
        );
        if (thread_handles[i] == NULL) {
            /* 线程创建失败 */
            workers[i].error = -1;
            workers[i].done = 1;
        }
    }

    /* 等待所有工作线程完成 */
    WaitForMultipleObjects(actual_workers, thread_handles, TRUE, INFINITE);

    /* 关闭线程句柄 */
    for (int i = 0; i < actual_workers; i++) {
        if (thread_handles[i] != NULL) {
            CloseHandle(thread_handles[i]);
        }
    }
    efree(thread_handles);
#else
    /* Unix/Linux 平台：使用 pthread */
    pthread_t *thread_ids = (pthread_t *)ecalloc(actual_workers, sizeof(pthread_t));
    for (int i = 0; i < actual_workers; i++) {
        int ret = pthread_create(&thread_ids[i], NULL, xhcurl_worker_thread, &workers[i]);
        if (ret != 0) {
            /* 线程创建失败 */
            workers[i].error = -1;
            workers[i].done = 1;
        }
    }

    /* 等待所有工作线程完成 */
    for (int i = 0; i < actual_workers; i++) {
        if (!workers[i].done || workers[i].error == 0) {
            pthread_join(thread_ids[i], NULL);
        }
    }
    efree(thread_ids);
#endif

    /* --- 阶段 4：为每个请求创建 XHResponse PHP 对象 --- */
    array_init_size(return_value, obj->pending_count);

    for (int i = 0; i < obj->pending_count; i++) {
        xhcurl_req_context_t *ctx = contexts[i];
        if (ctx == NULL) {
            /* 上下文为空（创建失败），添加一个空响应 */
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

        /* 获取请求总耗时 */
        curl_easy_getinfo(ctx->easy, CURLINFO_TOTAL_TIME, &resp_obj->total_time);

        /* 复制 Content-Type */
        if (ctx->content_type != NULL) {
            /* 线程中使用 strdup 分配，这里需要转为 estrdup */
            resp_obj->content_type = estrdup(ctx->content_type);
            /* 释放线程中分配的字符串 */
            free(ctx->content_type);
            ctx->content_type = NULL;
        }

        /* 设置错误信息 */
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
            /* 清空原始缓冲区指针 */
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

    /* --- 阶段 5：释放所有请求上下文 --- */
    for (int i = 0; i < obj->pending_count; i++) {
        if (contexts[i] != NULL) {
            xhcurl_context_free(contexts[i]);
        }
    }
    efree(contexts);

    /* 释放工作线程参数 */
    efree(workers);

    /* 清理 pending 队列 */
    for (int i = 0; i < obj->pending_count; i++) {
        if (!Z_ISUNDEF(obj->pending_requests[i])) {
            zval_ptr_dtor(&obj->pending_requests[i]);
            ZVAL_UNDEF(&obj->pending_requests[i]);
        }
    }
    obj->pending_count = 0;
}

/**
 * 获取已添加的请求数量
 * XHThreadPool::count(): int
 */
PHP_METHOD(XHThreadPool, count)
{
    /* 无参数 */
    ZEND_PARSE_PARAMETERS_NONE();

    /* 获取 XHThreadPool 对象 */
    xhthreadpool_obj_t *obj = XHTHREADPOOL_OBJ_FROM_ZOBJ(Z_OBJ_P(getThis()));

    /* 返回待执行请求数量 */
    RETURN_LONG(obj->pending_count);
}

/* +----------------------------------------------------------------------+
 * | 方法注册表                                                            |
 * +----------------------------------------------------------------------+
 */

/* 构造函数参数信息 */
ZEND_BEGIN_ARG_INFO_EX(arginfo_xhthreadpool_construct, 0, 0, 1)
    ZEND_ARG_INFO(0, curl)       /* XHCurl 对象（必填） */
    ZEND_ARG_INFO(0, workers)    /* 工作线程数（可选） */
ZEND_END_ARG_INFO()

/* add 参数信息 */
ZEND_BEGIN_ARG_INFO_EX(arginfo_xhthreadpool_add, 0, 0, 1)
    ZEND_ARG_INFO(0, request)    /* XHRequest 对象（必填） */
ZEND_END_ARG_INFO()

/* execute 参数信息（无参数） */
ZEND_BEGIN_ARG_INFO_EX(arginfo_xhthreadpool_execute, 0, 0, 0)
ZEND_END_ARG_INFO()

/* count 参数信息（无参数） */
ZEND_BEGIN_ARG_INFO_EX(arginfo_xhthreadpool_count, 0, 0, 0)
ZEND_END_ARG_INFO()

static const zend_function_entry xhthreadpool_methods[] = {
    /* 构造函数 */
    PHP_ME(XHThreadPool, __construct, arginfo_xhthreadpool_construct, ZEND_ACC_PUBLIC)
    /* 添加请求 */
    PHP_ME(XHThreadPool, add, arginfo_xhthreadpool_add, ZEND_ACC_PUBLIC)
    /* 执行所有请求（多线程） */
    PHP_ME(XHThreadPool, execute, arginfo_xhthreadpool_execute, ZEND_ACC_PUBLIC)
    /* 获取请求数量 */
    PHP_ME(XHThreadPool, count, arginfo_xhthreadpool_count, ZEND_ACC_PUBLIC)
    /* 结束标记 */
    PHP_FE_END
};

/* +----------------------------------------------------------------------+
 * | 类初始化函数                                                          |
 * +----------------------------------------------------------------------+
 */
PHP_MINIT_FUNCTION(xhthreadpool_class)
{
    /* 初始化类入口 */
    zend_class_entry ce;
    INIT_CLASS_ENTRY(ce, "XHThreadPool", xhthreadpool_methods);

    /* 注册类 */
    xhthreadpool_ce = zend_register_internal_class(&ce);

    /* 设置对象创建和释放函数 */
    xhthreadpool_ce->create_object = xhthreadpool_create_obj;

    return SUCCESS;
}
