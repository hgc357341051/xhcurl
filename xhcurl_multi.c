/* +----------------------------------------------------------------------+
 * | XHCurl 扩展 - XHMulti 批量异步执行器类实现                           |
 * | 基于 curl_multi 接口实现并发请求                                      |
 * | FPM 和 CLI 模式通用，使用单进程异步 I/O 多路复用                     |
 * | 支持流式回调（onChunk/onHeader），在数据到达时实时触发               |
 * |                                                                      |
 * | 滑动窗口设计（v2.0 重构）：                                          |
 * |   - add() 仅入队 XHRequest 引用，不创建 curl 句柄                    |
 * |   - execute() 按滑动窗口调度，始终保持 maxConcurrent 个并发          |
 * |   - 一个请求完成 → 立即从队列取下一个补充 → 内存恒定                 |
 * |   - 可选 callback 参数：有回调时实时通知 + 释放响应，无回调时全量返回 |
 * +----------------------------------------------------------------------+
 */

#include "xhcurl_priv.h"

/* 类入口指针 */
zend_class_entry *xhmulti_ce;

/* +----------------------------------------------------------------------+
 * | 哈希表操作函数（easy 句柄 → 上下文映射）                              |
 * | O(1) 查找替代原来的 O(n) 线性遍历                                    |
 * +----------------------------------------------------------------------+
 */

/**
 * 初始化哈希表
 * @param map 哈希表指针
 */
static void xhcurl_easy_map_init(xhcurl_easy_map_t *map)
{
    /* 将所有桶指针初始化为 NULL */
    memset(map->buckets, 0, sizeof(map->buckets));
    /* 初始大小为 0 */
    map->size = 0;
}

/**
 * 向哈希表插入键值对
 * @param map     哈希表指针
 * @param easy    curl easy 句柄（键）
 * @param context 请求上下文（值）
 */
static void xhcurl_easy_map_insert(xhcurl_easy_map_t *map, CURL *easy, xhcurl_req_context_t *context)
{
    /* 计算哈希值：将指针转为 size_t 后取模（位运算优化） */
    size_t key = (size_t)easy;
    /* XHCURL_EASY_MAP_BUCKETS 是 1024 = 2^10，用 & 替代 % */
    size_t bucket_idx = key & (XHCURL_EASY_MAP_BUCKETS - 1);

    /* 分配新的哈希节点 */
    xhcurl_easy_map_entry_t *entry = (xhcurl_easy_map_entry_t *)ecalloc(1, sizeof(xhcurl_easy_map_entry_t));
    entry->easy = easy;       /* 设置键 */
    entry->context = context; /* 设置值 */
    /* 头插法：新节点插入到桶链表头部 */
    entry->next = map->buckets[bucket_idx];
    map->buckets[bucket_idx] = entry;
    /* 更新大小 */
    map->size++;
}

/**
 * 从哈希表查找 easy 句柄对应的上下文
 * @param map  哈希表指针
 * @param easy curl easy 句柄（键）
 * @return 请求上下文指针，未找到返回 NULL
 */
static xhcurl_req_context_t *xhcurl_easy_map_find(xhcurl_easy_map_t *map, CURL *easy)
{
    /* 计算桶索引 */
    size_t key = (size_t)easy;
    size_t bucket_idx = key & (XHCURL_EASY_MAP_BUCKETS - 1);

    /* 遍历桶链表查找 */
    xhcurl_easy_map_entry_t *entry = map->buckets[bucket_idx];
    while (entry != NULL) {
        if (entry->easy == easy) {
            /* 找到匹配的键 */
            return entry->context;
        }
        entry = entry->next;
    }
    /* 未找到 */
    return NULL;
}

/**
 * 从哈希表移除指定 easy 句柄的条目
 * @param map  哈希表指针
 * @param easy curl easy 句柄（键）
 * @return 被移除的上下文指针，未找到返回 NULL
 */
static xhcurl_req_context_t *xhcurl_easy_map_remove(xhcurl_easy_map_t *map, CURL *easy)
{
    /* 计算桶索引 */
    size_t key = (size_t)easy;
    size_t bucket_idx = key & (XHCURL_EASY_MAP_BUCKETS - 1);

    /* 遍历桶链表查找并移除 */
    xhcurl_easy_map_entry_t **pp = &map->buckets[bucket_idx]; /* 指向指针的指针 */
    while (*pp != NULL) {
        if ((*pp)->easy == easy) {
            /* 找到匹配的键，从链表中摘除 */
            xhcurl_easy_map_entry_t *target = *pp;
            xhcurl_req_context_t *context = target->context;
            *pp = target->next; /* 前驱节点指向后继 */
            efree(target);      /* 释放哈希节点 */
            map->size--;        /* 更新大小 */
            return context;
        }
        pp = &(*pp)->next;
    }
    /* 未找到 */
    return NULL;
}

/**
 * 释放哈希表所有资源
 * @param map 哈希表指针
 */
static void xhcurl_easy_map_free(xhcurl_easy_map_t *map)
{
    /* 遍历所有桶 */
    for (int i = 0; i < XHCURL_EASY_MAP_BUCKETS; i++) {
        /* 释放桶链表中所有节点 */
        xhcurl_easy_map_entry_t *entry = map->buckets[i];
        while (entry != NULL) {
            xhcurl_easy_map_entry_t *next = entry->next;
            efree(entry); /* 释放节点（不释放 context，由调用方管理） */
            entry = next;
        }
        map->buckets[i] = NULL;
    }
    map->size = 0;
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
        /* 先移除所有活跃的 easy 句柄 */
        for (int i = 0; i < obj->context_count; i++) {
            if (obj->contexts[i] != NULL && obj->contexts[i]->easy != NULL) {
                curl_multi_remove_handle(obj->multi, obj->contexts[i]->easy);
            }
        }
        curl_multi_cleanup(obj->multi);
        obj->multi = NULL;
    }

    /* 释放所有活跃请求上下文 */
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

    /* 释放已完成结果数组 */
    if (obj->results != NULL) {
        for (int i = 0; i < obj->result_count; i++) {
            if (!Z_ISUNDEF(obj->results[i])) {
                zval_ptr_dtor(&obj->results[i]);
                ZVAL_UNDEF(&obj->results[i]);
            }
        }
        efree(obj->results);
        obj->results = NULL;
    }

    /* 释放哈希表 */
    xhcurl_easy_map_free(&obj->easy_map);

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
    obj->multi = NULL;                  /* curl multi 句柄在构造函数中创建 */
    obj->curl_obj = NULL;               /* XHCurl 引用在构造函数中设置 */
    obj->pending_requests = NULL;       /* 待执行队列在首次 add 时分配 */
    obj->pending_count = 0;             /* 初始待执行数为 0 */
    obj->pending_capacity = 0;          /* 初始容量为 0 */
    obj->pending_head = 0;              /* 队列头部索引为 0 */
    obj->contexts = NULL;               /* 活跃上下文数组在 execute 时分配 */
    obj->context_count = 0;             /* 初始活跃数为 0 */
    obj->context_capacity = 0;          /* 初始容量为 0 */
    obj->max_concurrent = XHCURL_DEFAULT_MAX_CONCURRENT; /* 默认最大并发 100 */
    obj->results = NULL;                /* 结果数组在 execute 时分配 */
    obj->result_count = 0;              /* 初始结果数为 0 */
    obj->result_capacity = 0;           /* 初始容量为 0 */
    ZVAL_UNDEF(&obj->curl_zval);        /* 初始化 XHCurl 引用 */

    /* 初始化哈希表 */
    xhcurl_easy_map_init(&obj->easy_map);

    /* 设置对象释放函数 */
    obj->std.handlers = zend_get_std_object_handlers();

    return &obj->std;
}

/* +----------------------------------------------------------------------+
 * | 内部辅助函数                                                          |
 * +----------------------------------------------------------------------+
 */

/**
 * 从待执行队列中取出下一个请求并创建上下文，添加到 multi 句柄
 * @param obj XHMulti 对象
 * @return 新创建的上下文指针，无待执行请求时返回 NULL
 */
static xhcurl_req_context_t *xhmulti_dispatch_next(xhmulti_obj_t *obj)
{
    /* 检查是否还有待执行的请求 */
    if (obj->pending_head >= obj->pending_count) {
        return NULL; /* 队列已空 */
    }

    /* 从队列中取出下一个 XHRequest 的 zval */
    zval *request_zv = &obj->pending_requests[obj->pending_head];
    if (Z_TYPE_P(request_zv) == IS_UNDEF) {
        return NULL; /* 无效引用 */
    }

    /* 获取 XHRequest 内部对象 */
    xhrequest_obj_t *req_obj = XHREQUEST_OBJ_FROM_ZOBJ(Z_OBJ_P(request_zv));

    /* 创建请求执行上下文（此时才创建 curl easy 句柄） */
    xhcurl_req_context_t *ctx = xhcurl_context_create(obj->curl_obj, req_obj);
    if (ctx == NULL) {
        return NULL; /* 创建失败 */
    }

    /* 保存 XHRequest PHP 对象引用（防止 GC 回收） */
    ZVAL_COPY(&ctx->request_zval, request_zv);

    /* 将 easy 句柄添加到 multi 句柄 */
    CURLMcode mres = curl_multi_add_handle(obj->multi, ctx->easy);
    if (mres != CURLM_OK) {
        /* 添加失败，释放上下文 */
        xhcurl_context_free(ctx);
        return NULL;
    }

    /* 插入哈希表（O(1) 查找） */
    xhcurl_easy_map_insert(&obj->easy_map, ctx->easy, ctx);

    /* 检查是否需要扩容活跃上下文数组 */
    if (obj->context_count >= obj->context_capacity) {
        /* 计算新容量（初始 16，之后翻倍增长） */
        int new_capacity = (obj->context_capacity == 0) ? 16 : obj->context_capacity * 2;
        /* 重新分配数组 */
        obj->contexts = (xhcurl_req_context_t **)erealloc(
            obj->contexts, new_capacity * sizeof(xhcurl_req_context_t *));
        if (obj->contexts == NULL) {
            /* 扩容失败，回滚 */
            curl_multi_remove_handle(obj->multi, ctx->easy);
            xhcurl_easy_map_remove(&obj->easy_map, ctx->easy);
            xhcurl_context_free(ctx);
            return NULL;
        }
        /* 初始化新分配的空间为 NULL */
        for (int i = obj->context_capacity; i < new_capacity; i++) {
            obj->contexts[i] = NULL;
        }
        obj->context_capacity = new_capacity;
    }

    /* 将上下文添加到活跃数组 */
    obj->contexts[obj->context_count] = ctx;
    obj->context_count++;

    /* 移动队列头部指针（消费一个待执行请求） */
    obj->pending_head++;

    return ctx;
}

/**
 * 处理已完成的请求：提取结果、移除 easy 句柄、释放上下文
 * @param obj      XHMulti 对象
 * @param ctx      请求上下文
 * @param fci      回调函数调用信息（fci->size == 0 表示无回调）
 * @param fcc      回调函数调用缓存
 * @param has_callback 是否有回调函数
 * @param response_zv 输出的 XHResponse zval（调用方需释放）
 * @return 0 成功，-1 失败
 */
static int xhmulti_process_completed(xhmulti_obj_t *obj, xhcurl_req_context_t *ctx,
                                      zend_fcall_info *fci, zend_fcall_info_cache *fcc,
                                      zend_bool has_callback, zval *response_zv)
{
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

    /* 创建 XHResponse PHP 对象 */
    object_init_ex(response_zv, xhresponse_ce);
    xhresponse_obj_t *resp_obj = XHRESPONSE_OBJ_FROM_ZOBJ(Z_OBJ_P(response_zv));

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

    /* 如果有回调函数，立即调用 */
    if (has_callback) {
        /* 调用 PHP 回调：callback(XHResponse $response, int $completed, int $total) */
        zval args[3];
        ZVAL_COPY(&args[0], response_zv);                      /* 响应对象 */
        ZVAL_LONG(&args[1], obj->result_count + 1);            /* 已完成数 */
        ZVAL_LONG(&args[2], obj->pending_count);               /* 总请求数 */

        zval retval;
        /* 设置 fci 参数 */
        fci->retval = &retval;          /* 返回值指针 */
        fci->params = args;             /* 参数数组 */
        fci->param_count = 3;           /* 参数数量 */

        /* 使用 zend_call_function 调用回调（比 call_user_function 更高效） */
        int call_result = zend_call_function(fci, fcc);

        /* 释放参数和返回值 */
        zval_ptr_dtor(&args[0]);
        if (call_result == SUCCESS) {
            zval_ptr_dtor(&retval);
        }

        /* 检查回调是否抛出异常 */
        if (EG(exception) != NULL) {
            return -1;
        }
    }

    return 0;
}

/**
 * 清理一个已完成的请求上下文
 * 从 multi 移除 easy 句柄、从哈希表移除、释放上下文
 * @param obj XHMulti 对象
 * @param ctx 请求上下文
 */
static void xhmulti_cleanup_context(xhmulti_obj_t *obj, xhcurl_req_context_t *ctx)
{
    /* 从 multi 句柄移除 easy 句柄 */
    curl_multi_remove_handle(obj->multi, ctx->easy);
    /* 从哈希表移除 */
    xhcurl_easy_map_remove(&obj->easy_map, ctx->easy);
    /* 释放上下文资源 */
    xhcurl_context_free(ctx);
}

/* +----------------------------------------------------------------------+
 * | PHP 方法实现                                                          |
 * +----------------------------------------------------------------------+
 */

/**
 * 构造函数
 * XHMulti::__construct(XHCurl $curl, int $maxConcurrent = 100)
 * maxConcurrent 控制滑动窗口大小，即同时活跃的最大请求数
 */
PHP_METHOD(XHMulti, __construct)
{
    zval *curl_zv;      /* XHCurl 对象参数 */
    zend_long max_concurrent = XHCURL_DEFAULT_MAX_CONCURRENT; /* 最大并发数（默认 100） */

    /* 解析参数：XHCurl 对象 + 可选的最大并发数 */
    ZEND_PARSE_PARAMETERS_START(1, 2)
        Z_PARAM_OBJECT_OF_CLASS(curl_zv, xhcurl_ce)
        Z_PARAM_OPTIONAL
        Z_PARAM_LONG(max_concurrent)
    ZEND_PARSE_PARAMETERS_END();

    /* 验证最大并发数 */
    if (max_concurrent <= 0) {
        /* 并发数必须大于 0 */
        zend_throw_exception(xhcurl_exception_ce,
            "maxConcurrent must be greater than 0", 0);
        return;
    }

    /* 限制最大并发数不超过 10000，防止系统资源耗尽 */
    if (max_concurrent > 10000) {
        max_concurrent = 10000;
    }

    /* 获取 XHMulti 对象 */
    xhmulti_obj_t *obj = XHMULTI_OBJ_FROM_ZOBJ(Z_OBJ_P(getThis()));

    /* 保存 XHCurl PHP 对象引用（防止 GC 回收） */
    ZVAL_COPY(&obj->curl_zval, curl_zv);
    /* 保存 XHCurl 内部对象指针（快速访问，无需每次从 zval 获取） */
    obj->curl_obj = XHCURL_OBJ_FROM_ZOBJ(Z_OBJ_P(curl_zv));
    /* 设置最大并发数 */
    obj->max_concurrent = (int)max_concurrent;

    /* 创建 curl multi 句柄 */
    obj->multi = curl_multi_init();
    if (obj->multi == NULL) {
        zend_throw_exception(xhcurl_exception_ce, "Failed to create curl multi handle", 0);
        return;
    }

    /* 设置 multi 句柄的最大连接数（与 max_concurrent 匹配） */
    curl_multi_setopt(obj->multi, CURLMOPT_MAXCONNECTS, (long)max_concurrent);
    /* 设置最大总连接数 */
    curl_multi_setopt(obj->multi, CURLMOPT_MAX_TOTAL_CONNECTIONS, (long)max_concurrent);
}

/**
 * 添加请求到批量执行器
 * XHMulti::add(XHRequest $request): void
 *
 * 滑动窗口模式：add() 仅将 XHRequest 引用存入待执行队列，
 * 不创建 curl 句柄，不占用网络资源。
 * 真正的上下文创建和 curl 句柄分配在 execute() 时按需进行。
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

    /* 将 XHRequest 引用存入待执行队列（仅增加引用计数，不创建 curl 句柄） */
    ZVAL_COPY(&obj->pending_requests[obj->pending_count], request_zv);
    obj->pending_count++;
}

/**
 * 执行所有已添加的请求（滑动窗口模式）
 * XHMulti::execute(?callable $callback = null): array
 *
 * 滑动窗口调度算法：
 *   1. 初始填充窗口：从 pending 队列取 min(maxConcurrent, pendingCount) 个请求
 *   2. 事件循环：curl_multi_perform + curl_multi_wait
 *   3. 请求完成时：
 *      - 提取结果（状态码、响应头、响应体）
 *      - 有 callback → 立即回调通知 + 释放 XHResponse（内存恒定）
 *      - 无 callback → 存入结果数组（兼容旧 API，但需注意内存）
 *      - 从 pending 队列取下一个请求补充窗口
 *   4. 循环直到所有请求完成
 *
 * @param callback 可选回调函数 callback(XHResponse $response, int $completed, int $total)
 *                 有回调时内存恒定（每次回调后释放响应），无回调时返回全量数组
 */
PHP_METHOD(XHMulti, execute)
{
    /* 回调函数调用信息（PHP 8.4+ 要求 Z_PARAM_FUNC_OR_NULL 传 fci + fcc 两个参数） */
    zend_fcall_info callback_fci;           /* 函数调用信息（含参数计数、返回值等） */
    zend_fcall_info_cache callback_fcc;     /* 函数调用缓存（加速后续调用） */
    /* 标记回调是否已设置（fci.size > 0 表示有效） */
    zend_bool has_callback = 0;

    /* 初始化 fci（清零，避免野指针） */
    memset(&callback_fci, 0, sizeof(callback_fci));
    memset(&callback_fcc, 0, sizeof(callback_fcc));

    /* 解析参数：可选的回调函数 */
    /* Z_PARAM_FUNC_OR_NULL 在 PHP 8.4+ 需要 fci 和 fcc 两个参数 */
    ZEND_PARSE_PARAMETERS_START(0, 1)
        Z_PARAM_OPTIONAL
        Z_PARAM_FUNC_OR_NULL(callback_fci, callback_fcc)
    ZEND_PARSE_PARAMETERS_END();

    /* 检查回调是否有效（fci.size > 0 表示用户传入了回调函数） */
    has_callback = (callback_fci.size > 0);

    /* 获取 XHMulti 对象 */
    xhmulti_obj_t *obj = XHMULTI_OBJ_FROM_ZOBJ(Z_OBJ_P(getThis()));

    /* 检查是否有请求需要执行 */
    if (obj->pending_count == 0) {
        /* 无请求，返回空数组 */
        array_init(return_value);
        return;
    }

    /* 重置队列头部索引（从头开始消费） */
    obj->pending_head = 0;

    /* 初始化结果数组（无 callback 模式下使用） */
    if (obj->results == NULL || obj->result_capacity < obj->pending_count) {
        /* 分配足够大的结果数组 */
        if (obj->results != NULL) {
            /* 释放旧的结果数组 */
            for (int i = 0; i < obj->result_count; i++) {
                if (!Z_ISUNDEF(obj->results[i])) {
                    zval_ptr_dtor(&obj->results[i]);
                }
            }
            efree(obj->results);
        }
        obj->results = (zval *)ecalloc(obj->pending_count, sizeof(zval));
        obj->result_capacity = obj->pending_count;
    }
    obj->result_count = 0;

    /* 重置活跃上下文计数 */
    obj->context_count = 0;

    /* --- 阶段 1：初始填充滑动窗口 --- */
    /* 取 min(maxConcurrent, pendingCount) 个请求开始执行 */
    int initial_batch = (obj->max_concurrent < obj->pending_count) ?
                         obj->max_concurrent : obj->pending_count;
    for (int i = 0; i < initial_batch; i++) {
        if (xhmulti_dispatch_next(obj) == NULL) {
            /* 调度失败（可能是内存不足），跳过 */
            break;
        }
    }

    /* --- 阶段 2：滑动窗口事件循环 --- */
    int still_running = 0;
    do {
        /* 执行一次 multi 操作 */
        CURLMcode mc = curl_multi_perform(obj->multi, &still_running);
        if (mc != CURLM_OK) {
            /* multi 操作失败 */
            break;
        }

        /* 检查已完成的请求 */
        CURLMsg *msg = NULL;
        int msgs_left = 0;
        while ((msg = curl_multi_info_read(obj->multi, &msgs_left)) != NULL) {
            if (msg->msg != CURLMSG_DONE) {
                continue; /* 跳过非完成消息 */
            }

            /* 从哈希表 O(1) 查找对应的上下文 */
            xhcurl_req_context_t *ctx = xhcurl_easy_map_find(&obj->easy_map, msg->easy_handle);
            if (ctx == NULL) {
                continue; /* 未找到上下文，跳过 */
            }

            /* 处理已完成的请求：提取结果 */
            zval response_zv;
            ZVAL_UNDEF(&response_zv);
            int process_result = xhmulti_process_completed(obj, ctx,
                &callback_fci, &callback_fcc, has_callback, &response_zv);

            if (process_result == 0 && !Z_ISUNDEF(response_zv)) {
                if (has_callback) {
                    /* 有回调：回调已在 process_completed 中调用，释放响应对象 */
                    zval_ptr_dtor(&response_zv);
                } else {
                    /* 无回调：将响应存入结果数组 */
                    if (obj->result_count < obj->result_capacity) {
                        ZVAL_COPY_VALUE(&obj->results[obj->result_count], &response_zv);
                    } else {
                        /* 结果数组已满，需要扩容 */
                        int new_cap = obj->result_capacity * 2;
                        obj->results = (zval *)erealloc(obj->results, new_cap * sizeof(zval));
                        ZVAL_COPY_VALUE(&obj->results[obj->result_count], &response_zv);
                        obj->result_capacity = new_cap;
                    }
                    obj->result_count++;
                }
            } else if (!Z_ISUNDEF(response_zv)) {
                /* 处理失败但有响应对象，释放 */
                zval_ptr_dtor(&response_zv);
            }

            /* 清理已完成的上下文 */
            xhmulti_cleanup_context(obj, ctx);

            /* 滑动窗口：从 pending 队列取下一个请求补充窗口 */
            if (obj->pending_head < obj->pending_count) {
                xhmulti_dispatch_next(obj);
            }
        }

        /* 等待活动（最多 100ms），避免 CPU 空转 */
        if (still_running > 0) {
            mc = curl_multi_wait(obj->multi, NULL, 0, 100, NULL);
            if (mc != CURLM_OK) {
                break;
            }
        }

        /* 检查是否有异常（可能由回调抛出） */
        if (EG(exception) != NULL) {
            /* 有异常，中止所有请求 */
            break;
        }
    } while (still_running > 0 || obj->pending_head < obj->pending_count);

    /* --- 阶段 3：清理剩余活跃上下文（异常或中断时） --- */
    for (int i = 0; i < obj->context_count; i++) {
        if (obj->contexts[i] != NULL) {
            /* 从 multi 移除 easy 句柄 */
            curl_multi_remove_handle(obj->multi, obj->contexts[i]->easy);
            /* 释放上下文 */
            xhcurl_context_free(obj->contexts[i]);
            obj->contexts[i] = NULL;
        }
    }
    obj->context_count = 0;

    /* 清空哈希表 */
    xhcurl_easy_map_free(&obj->easy_map);
    xhcurl_easy_map_init(&obj->easy_map);

    /* --- 阶段 4：构建返回值 --- */
    if (has_callback) {
        /* 有回调模式：返回已完成数和总数 ['completed' => N, 'total' => M] */
        array_init(return_value);
        add_assoc_long(return_value, "completed", obj->result_count);
        add_assoc_long(return_value, "total", obj->pending_count);
    } else {
        /* 无回调模式：返回 XHResponse 数组（兼容旧 API） */
        array_init_size(return_value, obj->result_count);
        for (int i = 0; i < obj->result_count; i++) {
            if (!Z_ISUNDEF(obj->results[i])) {
                add_next_index_zval(return_value, &obj->results[i]);
                ZVAL_UNDEF(&obj->results[i]); /* 转移所有权，不释放 */
            }
        }
    }

    /* 重置状态（保留已分配的内存供下次使用） */
    obj->result_count = 0;

    /* 释放待执行队列中的 XHRequest 引用 */
    for (int i = 0; i < obj->pending_count; i++) {
        if (!Z_ISUNDEF(obj->pending_requests[i])) {
            zval_ptr_dtor(&obj->pending_requests[i]);
            ZVAL_UNDEF(&obj->pending_requests[i]);
        }
    }
    obj->pending_count = 0;
    obj->pending_head = 0;
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

    /* 返回待执行请求数量 */
    RETURN_LONG(obj->pending_count);
}

/* +----------------------------------------------------------------------+
 * | 方法注册表                                                            |
 * +----------------------------------------------------------------------+
 */

/* 构造函数参数信息 */
ZEND_BEGIN_ARG_INFO_EX(arginfo_xhmulti_construct, 0, 0, 1)
    ZEND_ARG_INFO(0, curl)            /* XHCurl 对象（必填） */
    ZEND_ARG_INFO(0, maxConcurrent)   /* 最大并发数（可选，默认 100） */
ZEND_END_ARG_INFO()

/* add 参数信息 */
ZEND_BEGIN_ARG_INFO_EX(arginfo_xhmulti_add, 0, 0, 1)
    ZEND_ARG_INFO(0, request)         /* XHRequest 对象（必填） */
ZEND_END_ARG_INFO()

/* execute 参数信息 */
ZEND_BEGIN_ARG_INFO_EX(arginfo_xhmulti_execute, 0, 0, 0)
    ZEND_ARG_INFO(0, callback)        /* 可选回调函数 */
ZEND_END_ARG_INFO()

/* count 参数信息（无参数） */
ZEND_BEGIN_ARG_INFO_EX(arginfo_xhmulti_count, 0, 0, 0)
ZEND_END_ARG_INFO()

static const zend_function_entry xhmulti_methods[] = {
    /* 构造函数 */
    PHP_ME(XHMulti, __construct, arginfo_xhmulti_construct, ZEND_ACC_PUBLIC)
    /* 添加请求 */
    PHP_ME(XHMulti, add, arginfo_xhmulti_add, ZEND_ACC_PUBLIC)
    /* 执行所有请求（滑动窗口模式） */
    PHP_ME(XHMulti, execute, arginfo_xhmulti_execute, ZEND_ACC_PUBLIC)
    /* 获取请求数量 */
    PHP_ME(XHMulti, count, arginfo_xhmulti_count, ZEND_ACC_PUBLIC)
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
