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

/* XHThreadPool 自定义对象操作函数表（用于注册 free_obj，确保对象销毁时释放资源） */
static zend_object_handlers xhthreadpool_object_handlers;

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
        /* 创建失败，设置错误码（使用原子操作确保主线程可见） */
#ifdef PHP_WIN32
        InterlockedExchange((volatile LONG *)&worker->error, -1);
        InterlockedExchange((volatile LONG *)&worker->done, 1);
#else
        __atomic_store_n(&worker->error, -1, __ATOMIC_RELEASE);
        __atomic_store_n(&worker->done, 1, __ATOMIC_RELEASE);
#endif
        return 0;
    }

    /* 将所有分配给此线程的请求添加到 multi 句柄 */
    for (int i = 0; i < worker->context_count; i++) {
        xhcurl_req_context_t *ctx = worker->contexts[i];
        if (ctx != NULL && ctx->easy != NULL) {
            /* 添加 easy 句柄到 multi */
            CURLMcode mc = curl_multi_add_handle(multi, ctx->easy);
            if (mc != CURLM_OK) {
                /* +------------------------------------------------------+
                 * | 添加失败：直接标记该请求的 curl_code                   |
                 * | curl_multi_add_handle 失败时，该请求不会进入事件循环， |
                 * | curl_multi_info_read 也不会报告其完成。               |
                 * | 如果不在此处标记，后续的 CURLE_PARTIAL_FILE 标记      |
                 * | 会给出模糊的错误信息。直接设置具体的错误码更精确。     |
                 * +------------------------------------------------------+
                 */
                ctx->curl_code = CURLE_FAILED_INIT;
            }
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

                    /* +----------------------------------------------------------+
                     * | 保存 curl 执行结果码到上下文                              |
                     * | 修复：原实现未保存 curl 错误码，导致主线程无法判断请求    |
                     * | 失败的具体原因（如超时、DNS 失败、连接重置等），           |
                     * | 只能返回通用的 "Request failed" 错误信息。                |
                     * | 保存 curl_code 后，主线程可生成精确的错误描述。            |
                     * +----------------------------------------------------------+
                     */
                    ctx->curl_code = msg->data.result;

                    /* 注意：不在线程中获取/设置 Content-Type */
                    /* 原因：工作线程中不能使用 PHP 内存管理器（emalloc/efree）， */
                    /* 而 strdup 分配的内存后续在 xhcurl_context_free 中用 efree 释放 */
                    /* 会导致 malloc/free 与 efree 不匹配，引发堆损坏或崩溃 */
                    /* Content-Type 的获取移到主线程的 execute 阶段 5 中进行 */

                    /* 注意：不在线程中解析响应头 */
                    /* xhcurl_parse_response_headers 使用 ecalloc/efree， */
                    /* 在工作线程中调用会导致 zend_mm_heap corrupted */
                    /* 响应头解析移到主线程的 execute 阶段 5 中进行 */

                    break;
                }
            }
        }
    }

    /* +--------------------------------------------------------------+
     * | 标记未被 curl_multi_info_read 报告的请求为失败               |
     * | 当 curl_multi_perform 返回错误或 curl_multi_wait 失败时，    |
     * | 事件循环提前退出，部分请求可能已接收数据但未完成。            |
     * | 这些请求不会被 curl_multi_info_read 报告（CURLMSG_DONE），   |
     * | 其 curl_code 仍为初始值 CURLE_OK，主线程无法判断其状态。     |
     * | 修复：对未被标记为完成的请求，设置 curl_code 为               |
     * | CURLE_PARTIAL_FILE（表示数据传输不完整），主线程可据此        |
     * | 生成准确的错误信息。                                         |
     * +--------------------------------------------------------------+
     */
    for (int i = 0; i < worker->context_count; i++) {
        xhcurl_req_context_t *ctx = worker->contexts[i];
        if (ctx != NULL && ctx->curl_code == CURLE_OK && ctx->status_code == 0) {
            /* 未被 curl_multi_info_read 报告的请求，标记为数据不完整 */
            ctx->curl_code = CURLE_PARTIAL_FILE;
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

    /* 标记工作线程完成（使用原子操作确保主线程可见） */
#ifdef PHP_WIN32
    InterlockedExchange((volatile LONG *)&worker->done, 1);
#else
    __atomic_store_n(&worker->done, 1, __ATOMIC_RELEASE);
#endif

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

    /* 释放待执行请求队列中的用户自定义数据 */
    if (obj->pending_user_data != NULL) {
        for (int i = 0; i < obj->pending_count; i++) {
            if (!Z_ISUNDEF(obj->pending_user_data[i])) {
                zval_ptr_dtor(&obj->pending_user_data[i]);
                ZVAL_UNDEF(&obj->pending_user_data[i]);
            }
        }
        efree(obj->pending_user_data);
        obj->pending_user_data = NULL;
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
    obj->pending_user_data = NULL;      /* 用户数据队列在首次 add 时分配 */
    obj->pending_count = 0;             /* 初始待执行数为 0 */
    obj->pending_capacity = 0;          /* 初始容量为 0 */
    obj->is_executing = 0;              /* 初始未在执行 */
    ZVAL_UNDEF(&obj->curl_zval);        /* 初始化 XHCurl 引用 */

    /* 设置自定义对象操作函数表（包含 free_obj，确保对象销毁时调用 xhthreadpool_free_obj） */
    obj->std.handlers = &xhthreadpool_object_handlers;

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
    zval *request_zv;       /* XHRequest 对象参数 */
    zval *user_data_zv = NULL; /* 用户自定义数据（可选） */

    /* 解析参数：XHRequest 必填，mixed $userData 可选 */
    ZEND_PARSE_PARAMETERS_START(1, 2)
        Z_PARAM_OBJECT_OF_CLASS(request_zv, xhrequest_ce)
        Z_PARAM_OPTIONAL
        Z_PARAM_ZVAL(user_data_zv)
    ZEND_PARSE_PARAMETERS_END();

    /* 获取 XHThreadPool 对象 */
    xhthreadpool_obj_t *obj = XHTHREADPOOL_OBJ_FROM_ZOBJ(Z_OBJ_P(getThis()));

    /* 防止在 execute 期间调用 add()，避免并发修改 pending 队列 */
    if (obj->is_executing) {
        zend_throw_exception(xhcurl_exception_ce,
            "Cannot add requests while execute() is running", 0);
        return;
    }

    /* 检查是否需要扩容待执行队列 */
    if (obj->pending_count >= obj->pending_capacity) {
        /* +--------------------------------------------------------------+
         * | 计算新容量（初始 64，之后翻倍增长）                          |
         * | 整数溢出检查：pending_capacity * 2 可能超过 INT_MAX，        |
         * | 导致 new_capacity 变为负数或 0，erealloc 分配失败或崩溃。    |
         * | 检查方式：如果 capacity 已超过 INT_MAX / 2，不再翻倍，       |
         * | 直接使用 pending_count + 1 作为新容量。                      |
         * +--------------------------------------------------------------+
         */
        int new_capacity;
        if (obj->pending_capacity == 0) {
            new_capacity = 64;
        } else if (obj->pending_capacity > INT_MAX / 2) {
            /* 容量已接近 INT_MAX，无法安全翻倍 */
            new_capacity = obj->pending_count + 1;
            /* 再次检查是否溢出 */
            if (new_capacity <= obj->pending_count) {
                zend_throw_exception(xhcurl_exception_ce, "Too many pending requests", 0);
                return;
            }
        } else {
            new_capacity = obj->pending_capacity * 2;
        }

        /* 重新分配请求队列数组 */
        zval *new_requests = (zval *)erealloc(
            obj->pending_requests, new_capacity * sizeof(zval));
        if (new_requests == NULL) {
            zend_throw_exception(xhcurl_exception_ce, "Failed to allocate pending queue", 0);
            return;
        }
        obj->pending_requests = new_requests;

        /* 重新分配用户数据数组（与请求队列一一对应） */
        zval *new_user_data = (zval *)erealloc(
            obj->pending_user_data, new_capacity * sizeof(zval));
        if (new_user_data == NULL) {
            /* +----------------------------------------------------------+
             * | 回滚：pending_requests 已扩容但 pending_user_data 失败   |
             * | 由于 PHP 的 erealloc 在内存不足时会调用                   |
             * | zend_error_noreturn 终止请求，此分支实际不会执行。        |
             * | 但为了防御性编程，仍需保证数据一致性。                     |
             * | pending_requests 已扩容但 pending_user_data 未扩容，     |
             * | 两者容量不一致，无法安全回滚。                             |
             * | 实际处理：由于 erealloc 不会返回 NULL，此处不会执行。     |
             * +----------------------------------------------------------+
             */
            zend_throw_exception(xhcurl_exception_ce, "Failed to allocate user data queue", 0);
            return;
        }
        obj->pending_user_data = new_user_data;

        /* 初始化新分配的空间 */
        for (int i = obj->pending_count; i < new_capacity; i++) {
            ZVAL_UNDEF(&obj->pending_requests[i]);
            ZVAL_UNDEF(&obj->pending_user_data[i]);
        }
        obj->pending_capacity = new_capacity;
    }

    /* 将 XHRequest 引用存入待执行队列（仅增加引用计数，不创建 curl 上下文） */
    ZVAL_COPY(&obj->pending_requests[obj->pending_count], request_zv);

    /* 将用户自定义数据存入对应位置（可选参数，未传则设为 UNDEF） */
    if (user_data_zv != NULL) {
        ZVAL_COPY(&obj->pending_user_data[obj->pending_count], user_data_zv);
    } else {
        ZVAL_UNDEF(&obj->pending_user_data[obj->pending_count]);
    }

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

    /* 设置执行标志（防止 execute 期间调用 add） */
    obj->is_executing = 1;

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

        /* 将用户自定义数据拷贝到上下文（线程安全：主线程写入，工作线程只读） */
        if (!Z_ISUNDEF(obj->pending_user_data[i])) {
            ZVAL_COPY(&ctx->user_data, &obj->pending_user_data[i]);
        }

        /* 线程安全：清除 PHP 回调引用，防止工作线程中 curl 回调触发 PHP 函数调用 */
        /* 工作线程中不能调用 call_user_function，否则会导致崩溃或数据损坏 */
        if (!Z_ISUNDEF(ctx->chunk_callback)) {
            zval_ptr_dtor(&ctx->chunk_callback);
            ZVAL_UNDEF(&ctx->chunk_callback);
        }
        if (!Z_ISUNDEF(ctx->header_callback)) {
            zval_ptr_dtor(&ctx->header_callback);
            ZVAL_UNDEF(&ctx->header_callback);
        }

        /* +--------------------------------------------------------------+
         * | 线程安全：取消 curl_share 绑定                               |
         * | curl_share 在多线程中共享时需要注册锁回调，否则会导致数据竞争 |
         * | 全局 Cookie 已在 xhcurl_context_create 中通过               |
         * | CURLOPT_COOKIELIST 直接设置到 easy 句柄，无需 share 共享    |
         * | 不取消 share 绑定会导致多线程并发访问 share 内部数据时崩溃   |
         * +--------------------------------------------------------------+
         */
        curl_easy_setopt(ctx->easy, CURLOPT_SHARE, NULL);

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
            /* 清理用户自定义数据 */
            if (!Z_ISUNDEF(obj->pending_user_data[i])) {
                zval_ptr_dtor(&obj->pending_user_data[i]);
                ZVAL_UNDEF(&obj->pending_user_data[i]);
            }
        }
        obj->pending_count = 0;
        /* 清除执行标志（所有上下文创建失败） */
        obj->is_executing = 0;
        return;
    }

    /* +--------------------------------------------------------------+
     * | 创建压缩后的活跃上下文数组（用于分配给工作线程）              |
     * | 保留原始 contexts 数组（含 NULL 间隙）用于与 pending_requests |
     * | 一一对应，以便在阶段 4 为失败的请求创建错误 XHResponse。      |
     * | 压缩后的 active_contexts[0..context_count-1] 全部为有效指针。 |
     * +--------------------------------------------------------------+
     */
    xhcurl_req_context_t **active_contexts = (xhcurl_req_context_t **)ecalloc(
        context_count, sizeof(xhcurl_req_context_t *));
    if (active_contexts == NULL) {
        /* 压缩数组分配失败，释放所有上下文 */
        for (int i = 0; i < obj->pending_count; i++) {
            if (contexts[i] != NULL) {
                xhcurl_context_free(contexts[i]);
            }
        }
        efree(contexts);
        zend_throw_exception(xhcurl_exception_ce, "Failed to allocate active context array", 0);
        obj->is_executing = 0;
        return;
    }
    {
        int write_idx = 0; /* 压缩后的写入位置 */
        for (int i = 0; i < obj->pending_count; i++) {
            if (contexts[i] != NULL) {
                /* 将非 NULL 上下文复制到活跃数组 */
                active_contexts[write_idx] = contexts[i];
                write_idx++;
            }
        }
        /* context_count 应与 write_idx 一致（双重校验） */
        context_count = write_idx;
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
        efree(active_contexts);
        /* +--------------------------------------------------------------+
         * | 修复：清理 pending 队列中的 zval 引用                       |
         * | 原实现缺少此清理，导致 pending_requests 和 pending_user_data |
         * | 中的 zval 引用计数未减少，造成内存泄漏。                    |
         * | 每个引用都需要调用 zval_ptr_dtor 释放。                     |
         * +--------------------------------------------------------------+
         */
        for (int i = 0; i < obj->pending_count; i++) {
            if (!Z_ISUNDEF(obj->pending_requests[i])) {
                zval_ptr_dtor(&obj->pending_requests[i]);
                ZVAL_UNDEF(&obj->pending_requests[i]);
            }
            if (!Z_ISUNDEF(obj->pending_user_data[i])) {
                zval_ptr_dtor(&obj->pending_user_data[i]);
                ZVAL_UNDEF(&obj->pending_user_data[i]);
            }
        }
        obj->pending_count = 0;
        /* 重置 is_executing 标志 */
        obj->is_executing = 0;
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
        workers[i].contexts = &active_contexts[ctx_offset];
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
    /* 记录成功创建的线程数（用于等待和清理） */
    int valid_thread_count = 0;
    for (int i = 0; i < actual_workers; i++) {
        thread_handles[i] = (HANDLE)_beginthreadex(
            NULL,                   /* 默认安全属性 */
            0,                      /* 默认栈大小 */
            xhcurl_worker_thread,   /* 线程函数 */
            &workers[i],            /* 线程参数 */
            0,                      /* 立即运行 */
            NULL                    /* 不需要线程 ID */
        );
        if (thread_handles[i] != NULL) {
            /* 线程创建成功，计入有效线程数 */
            valid_thread_count++;
        } else {
            /* 线程创建失败，标记该工作线程为错误状态（使用原子操作） */
            InterlockedExchange((volatile LONG *)&workers[i].error, -1);
            InterlockedExchange((volatile LONG *)&workers[i].done, 1);
        }
    }

    /* +--------------------------------------------------------------+
     * | 等待所有工作线程完成                                          |
     * | 修复：不能直接将含 NULL 句柄的数组传给 WaitForMultipleObjects，|
     * | 因为 NULL 句柄会导致 ERROR_INVALID_PARAMETER 错误。           |
     * | 改为逐个等待有效句柄，跳过创建失败的线程。                    |
     * +--------------------------------------------------------------+
     */
    for (int i = 0; i < actual_workers; i++) {
        if (thread_handles[i] != NULL) {
            /* 等待单个线程完成，超时时间 30 分钟（防止永久阻塞） */
            DWORD wait_result = WaitForSingleObject(thread_handles[i], 30 * 60 * 1000);
            if (wait_result == WAIT_TIMEOUT) {
                /* 线程超时未完成，标记错误（但不终止其他线程） */
                workers[i].error = -2;
            }
            /* 关闭线程句柄（无论是否超时，都释放内核对象） */
            CloseHandle(thread_handles[i]);
        }
    }

    /* Windows：检查线程创建失败的情况，标记受影响的请求 */
    /* 必须在 efree(thread_handles) 之前，因为需要检查句柄是否为 NULL */
    {
        int ctx_offset = 0; /* 活跃上下文数组偏移量 */
        for (int i = 0; i < actual_workers; i++) {
            int count = base_count + ((i < remainder) ? 1 : 0);
            if (thread_handles[i] == NULL) {
                /* 线程创建失败，标记该线程负责的所有上下文 */
                for (int j = 0; j < count; j++) {
                    if (active_contexts[ctx_offset + j] != NULL) {
                        active_contexts[ctx_offset + j]->curl_code = CURLE_FAILED_INIT;
                    }
                }
            }
            ctx_offset += count;
        }
    }

    efree(thread_handles);
#else
    /* Unix/Linux 平台：使用 pthread */
    pthread_t *thread_ids = (pthread_t *)ecalloc(actual_workers, sizeof(pthread_t));
    /* 记录每个线程是否成功创建（用于决定是否需要 pthread_join） */
    /* ecalloc 在内存不足时会终止请求（zend_error_noreturn），不会返回 NULL */
    int *thread_created = (int *)ecalloc(actual_workers, sizeof(int));
    /* ecalloc 已将内存初始化为 0，thread_created[i] 默认为 0（未创建） */
    for (int i = 0; i < actual_workers; i++) {
        int ret = pthread_create(&thread_ids[i], NULL, xhcurl_worker_thread, &workers[i]);
        if (ret == 0) {
            /* 线程创建成功，标记需要 join */
            thread_created[i] = 1;
        } else {
            /* 线程创建失败（使用原子操作设置错误状态） */
            __atomic_store_n(&workers[i].error, -1, __ATOMIC_RELEASE);
            __atomic_store_n(&workers[i].done, 1, __ATOMIC_RELEASE);
        }
    }

    /* +--------------------------------------------------------------+
     * | 等待所有工作线程完成                                          |
     * | 修复：原实现使用 !is_done || has_error == 0 条件判断，       |
     * | 当线程正常完成（done=1, error=0）时条件为 true 会 join，     |
     * | 但线程出错完成（done=1, error=-1）时条件为 false 不 join，   |
     * | 导致线程资源泄漏（pthread 未回收线程栈等资源）。              |
     * | 正确做法：对所有成功创建的线程始终调用 pthread_join，         |
     * | 无论其完成状态如何。pthread_create 失败的线程不需要 join。    |
     * +--------------------------------------------------------------+
     */
    for (int i = 0; i < actual_workers; i++) {
        /* 仅对成功创建的线程调用 pthread_join（回收线程资源） */
        if (thread_created[i]) {
            pthread_join(thread_ids[i], NULL);
        }
    }
    efree(thread_ids);
    efree(thread_created);

    /* Unix：检查线程创建失败的情况，标记受影响的请求 */
    {
        int ctx_offset = 0; /* 活跃上下文数组偏移量 */
        for (int i = 0; i < actual_workers; i++) {
            int count = base_count + ((i < remainder) ? 1 : 0);
            if (!thread_created[i]) {
                /* 线程创建失败，标记该线程负责的所有上下文 */
                for (int j = 0; j < count; j++) {
                    if (active_contexts[ctx_offset + j] != NULL) {
                        active_contexts[ctx_offset + j]->curl_code = CURLE_FAILED_INIT;
                    }
                }
            }
            ctx_offset += count;
        }
    }
#endif

    /* --- 阶段 4：为每个请求创建 XHResponse PHP 对象 --- */
    array_init_size(return_value, obj->pending_count);

    for (int i = 0; i < obj->pending_count; i++) {
        xhcurl_req_context_t *ctx = contexts[i];
        if (ctx == NULL) {
            /* +----------------------------------------------------------+
             * | 上下文创建失败：返回带错误信息的 XHResponse 而非 null    |
             * | 修复：原实现返回 null，导致调用方需要额外判空，           |
             * | 与成功请求的 XHResponse 返回类型不一致。                 |
             * | 新实现：创建包含错误信息的 XHResponse，保持 API 一致性。  |
             * +----------------------------------------------------------+
             */
            zval response_zv;
            object_init_ex(&response_zv, xhresponse_ce);
            /* 检查对象创建是否成功（防止内存不足时访问无效指针） */
            if (Z_TYPE(response_zv) != IS_OBJECT) {
                add_next_index_null(return_value);
                continue;
            }
            xhresponse_obj_t *resp_obj = XHRESPONSE_OBJ_FROM_ZOBJ(Z_OBJ_P(&response_zv));
            /* 设置状态码为 0 表示请求未执行 */
            resp_obj->status_code = 0;
            /* 设置错误信息 */
            resp_obj->error_msg = estrdup("Failed to create request context");
            /* 转移用户自定义数据（即使请求失败也保留用户数据） */
            if (!Z_ISUNDEF(obj->pending_user_data[i])) {
                ZVAL_COPY(&resp_obj->user_data, &obj->pending_user_data[i]);
            }
            add_next_index_zval(return_value, &response_zv);
            continue;
        }

        /* 创建 XHResponse PHP 对象 */
        zval response_zv;
        object_init_ex(&response_zv, xhresponse_ce);
        /* +----------------------------------------------------------+
         * | 检查 object_init_ex 是否成功                              |
         * | object_init_ex 可能因内存不足等原因失败，此时              |
         * | response_zv 不是 IS_OBJECT 类型，Z_OBJ_P 会返回无效指针， |
         * | 后续代码访问 resp_obj 会导致崩溃。                        |
         * | 失败时跳过此请求的响应创建，避免访问无效内存。             |
         * +----------------------------------------------------------+
         */
        if (Z_TYPE(response_zv) != IS_OBJECT) {
            /* 对象创建失败（内存不足等），跳过此请求 */
            add_next_index_null(return_value);
            continue;
        }
        xhresponse_obj_t *resp_obj = XHRESPONSE_OBJ_FROM_ZOBJ(Z_OBJ_P(&response_zv));

        /* 填充响应数据 */
        resp_obj->status_code = ctx->status_code;

        /* 获取请求总耗时 */
        curl_easy_getinfo(ctx->easy, CURLINFO_TOTAL_TIME, &resp_obj->total_time);

        /* +----------------------------------------------------------+
         * | 在主线程中获取 Content-Type                                |
         * | 工作线程中不能使用 estrdup（PHP 内存管理器非线程安全），    |
         * | 因此 Content-Type 的获取和设置必须在主线程中完成，         |
         * | 使用 estrdup 确保与 xhcurl_context_free 中的 efree 匹配   |
         * +----------------------------------------------------------+
         */
        {
            char *ct = NULL;
            curl_easy_getinfo(ctx->easy, CURLINFO_CONTENT_TYPE, &ct);
            if (ct != NULL) {
                /* 使用 estrdup 分配（PHP 内存管理器），与 efree 匹配 */
                resp_obj->content_type = estrdup(ct);
            }
        }

        /* 设置错误信息 */
        /* +--------------------------------------------------------------+
         * | 修复：使用工作线程中保存的 curl_code 生成精确错误信息        |
         * | 原实现只检查 status_code == 0 && body_buf.size == 0，        |
         * | 无法区分超时、DNS 失败、连接重置等不同错误类型。             |
         * | 新实现：curl_code != CURLE_OK 时使用 curl_easy_strerror      |
         * | 生成具体错误描述，帮助用户快速定位问题。                     |
         * +--------------------------------------------------------------+
         */
        if (ctx->curl_code != CURLE_OK) {
            /* curl 执行失败（网络错误、超时、DNS 失败等），使用 curl 提供的错误描述 */
            resp_obj->error_msg = estrdup(curl_easy_strerror(ctx->curl_code));
            /* 同时保存 curl 错误码到响应对象 */
            resp_obj->curl_code = ctx->curl_code;
        } else if (ctx->status_code == 0 && ctx->body_buf.size == 0) {
            /* curl 执行成功但无响应数据（可能是服务器关闭连接等） */
            resp_obj->error_msg = estrdup("Request failed: no response received");
        }

        /* 在主线程中解析响应头（工作线程中不能使用 ecalloc/efree） */
        xhcurl_parse_response_headers(&ctx->header_buf, &ctx->parsed_headers);

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

        /* 将用户自定义数据从上下文转移到响应对象（可通过 getUserData() 获取） */
        if (!Z_ISUNDEF(ctx->user_data)) {
            ZVAL_COPY(&resp_obj->user_data, &ctx->user_data);
        }

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
    /* 释放活跃上下文数组（仅是指针数组，上下文已在上面释放） */
    efree(active_contexts);

    /* 释放工作线程参数 */
    efree(workers);

    /* 清理 pending 队列 */
    for (int i = 0; i < obj->pending_count; i++) {
        if (!Z_ISUNDEF(obj->pending_requests[i])) {
            zval_ptr_dtor(&obj->pending_requests[i]);
            ZVAL_UNDEF(&obj->pending_requests[i]);
        }
        /* 清理用户自定义数据 */
        if (!Z_ISUNDEF(obj->pending_user_data[i])) {
            zval_ptr_dtor(&obj->pending_user_data[i]);
            ZVAL_UNDEF(&obj->pending_user_data[i]);
        }
    }
    obj->pending_count = 0;
    /* 清除执行标志（允许再次 add） */
    obj->is_executing = 0;
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
    ZEND_ARG_INFO(0, userData)   /* 用户自定义数据（可选，可通过 getUserData() 获取） */
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

    /* 设置对象创建函数 */
    xhthreadpool_ce->create_object = xhthreadpool_create_obj;

    /* +--------------------------------------------------------------+
     * | 初始化自定义对象操作函数表                                     |
     * | 关键：设置 free_obj 回调，确保 PHP 对象销毁时释放 C 侧资源   |
     * | 不设置 free_obj 会导致 pending 队列/curl_zval 等资源泄漏     |
     * +--------------------------------------------------------------+
     */
    memcpy(&xhthreadpool_object_handlers, zend_get_std_object_handlers(), sizeof(zend_object_handlers));
    /* 设置 free_obj 回调：PHP GC 回收对象时自动调用 xhthreadpool_free_obj */
    xhthreadpool_object_handlers.free_obj = xhthreadpool_free_obj;
    /* 设置 std 字段在结构体中的偏移量 */
    xhthreadpool_object_handlers.offset = XtOffsetOf(xhthreadpool_obj_t, std);

    return SUCCESS;
}
