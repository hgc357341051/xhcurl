/* +----------------------------------------------------------------------+
 * | XHCurl 扩展 - XHResponse 懒加载响应类实现                            |
 * | 核心设计：不一次性返回所有数据，支持按需分段读取                      |
 * | 响应体存储在 C 侧缓冲区，PHP 端按需读取，避免内存溢出                |
 * +----------------------------------------------------------------------+
 */

#include "xhcurl_priv.h"

/* 类入口指针 */
zend_class_entry *xhresponse_ce;

/* +----------------------------------------------------------------------+
 * | 对象生命周期函数                                                      |
 * +----------------------------------------------------------------------+
 */

/**
 * 释放 XHResponse 对象资源
 * 在 PHP 对象被垃圾回收时调用
 * @param object zend_object 指针
 */
static void xhresponse_free_obj(zend_object *object)
{
    /* 从 zend_object 获取 XHResponse 对象 */
    xhresponse_obj_t *obj = XHRESPONSE_OBJ_FROM_ZOBJ(object);

    /* 释放错误信息字符串 */
    if (obj->error_msg != NULL) {
        efree(obj->error_msg);
        obj->error_msg = NULL;
    }

    /* 释放响应头部链表 */
    if (obj->headers != NULL) {
        xhcurl_header_list_free(obj->headers);
        obj->headers = NULL;
    }

    /* 释放响应体缓冲区 */
    if (obj->body != NULL) {
        xhcurl_buffer_free(obj->body);
        /* 释放缓冲区结构体本身（缓冲区在创建时是 malloc 分配的） */
        efree(obj->body);
        obj->body = NULL;
    }

    /* 释放 Content-Type 字符串 */
    if (obj->content_type != NULL) {
        efree(obj->content_type);
        obj->content_type = NULL;
    }

    /* 释放用户自定义数据引用 */
    if (!Z_ISUNDEF(obj->user_data)) {
        zval_ptr_dtor(&obj->user_data);
        ZVAL_UNDEF(&obj->user_data);
    }

    /* 调用标准对象释放函数 */
    zend_object_std_dtor(object);
}

/**
 * 创建 XHResponse 对象
 * @param class_type 类入口指针
 * @return zend_object 指针
 */
static zend_object *xhresponse_create_obj(zend_class_entry *class_type)
{
    /* 分配对象内存（包含自定义结构和 zend_object） */
    xhresponse_obj_t *obj = (xhresponse_obj_t *)zend_object_alloc(sizeof(xhresponse_obj_t), class_type);

    /* 初始化 PHP 标准对象 */
    zend_object_std_init(&obj->std, class_type);
    /* 初始化对象属性 */
    object_properties_init(&obj->std, class_type);

    /* 初始化自定义字段为默认值 */
    obj->status_code = 0;       /* HTTP 状态码默认 0 */
    obj->error_msg = NULL;      /* 无错误信息 */
    obj->curl_code = CURLE_OK;  /* curl 错误码默认 OK */
    obj->headers = NULL;        /* 无头部 */
    obj->body = NULL;           /* 无响应体（后续由执行器设置） */
    obj->content_type = NULL;   /* 无 Content-Type */
    obj->total_time = 0.0;      /* 耗时为 0 */

    /* 设置对象释放函数 */
    obj->std.handlers = zend_get_std_object_handlers();

    return &obj->std;
}

/* +----------------------------------------------------------------------+
 * | PHP 方法实现                                                          |
 * +----------------------------------------------------------------------+
 */

/**
 * 获取 HTTP 状态码
 * XHResponse::getStatusCode(): int
 */
PHP_METHOD(XHResponse, getStatusCode)
{
    /* 无参数 */
    ZEND_PARSE_PARAMETERS_NONE();

    /* 获取当前对象 */
    xhresponse_obj_t *obj = XHRESPONSE_OBJ_FROM_ZOBJ(Z_OBJ_P(getThis()));

    /* 返回 HTTP 状态码 */
    RETURN_LONG(obj->status_code);
}

/**
 * 获取指定名称的响应头值
 * XHResponse::getHeader(string $name): ?string
 * 不返回所有头部，避免大数据量一次性加载
 */
PHP_METHOD(XHResponse, getHeader)
{
    char *name;         /* 头部名称参数 */
    size_t name_len;    /* 头部名称长度 */

    /* 解析参数：头部名称 */
    ZEND_PARSE_PARAMETERS_START(1, 1)
        Z_PARAM_STRING(name, name_len)
    ZEND_PARSE_PARAMETERS_END();

    /* 获取当前对象 */
    xhresponse_obj_t *obj = XHRESPONSE_OBJ_FROM_ZOBJ(Z_OBJ_P(getThis()));

    /* 在头部链表中查找 */
    const char *value = xhcurl_header_find(obj->headers, name);
    if (value != NULL) {
        /* 找到，返回头部值 */
        RETURN_STRING(value);
    } else {
        /* 未找到，返回 null */
        RETURN_NULL();
    }
}

/**
 * 获取所有响应头
 * XHResponse::getHeaders(): array
 * 注意：数据量大时慎用，建议使用 getHeader() 按需获取
 */
PHP_METHOD(XHResponse, getHeaders)
{
    /* 无参数 */
    ZEND_PARSE_PARAMETERS_NONE();

    /* 获取当前对象 */
    xhresponse_obj_t *obj = XHRESPONSE_OBJ_FROM_ZOBJ(Z_OBJ_P(getThis()));

    /* 初始化返回数组 */
    array_init(return_value);

    /* 遍历头部链表，逐个添加到数组 */
    xhcurl_header_t *current = obj->headers;
    while (current != NULL) {
        /* 添加键值对到数组 */
        add_assoc_string(return_value, current->name, current->value);
        current = current->next;
    }
}

/**
 * 检查指定名称的响应头是否存在
 * XHResponse::hasHeader(string $name): bool
 */
PHP_METHOD(XHResponse, hasHeader)
{
    char *name;         /* 头部名称参数 */
    size_t name_len;    /* 头部名称长度 */

    /* 解析参数 */
    ZEND_PARSE_PARAMETERS_START(1, 1)
        Z_PARAM_STRING(name, name_len)
    ZEND_PARSE_PARAMETERS_END();

    /* 获取当前对象 */
    xhresponse_obj_t *obj = XHRESPONSE_OBJ_FROM_ZOBJ(Z_OBJ_P(getThis()));

    /* 在头部链表中查找 */
    const char *value = xhcurl_header_find(obj->headers, name);

    /* 返回是否存在 */
    RETURN_BOOL(value != NULL);
}

/**
 * 分段读取响应体
 * XHResponse::getBodyChunk(int $offset, int $length): string
 * 核心方法：避免一次性加载整个响应体到 PHP 内存
 */
PHP_METHOD(XHResponse, getBodyChunk)
{
    zend_long offset;   /* 读取起始偏移量 */
    zend_long length;   /* 读取长度 */

    /* 解析参数：偏移量和长度 */
    ZEND_PARSE_PARAMETERS_START(2, 2)
        Z_PARAM_LONG(offset)
        Z_PARAM_LONG(length)
    ZEND_PARSE_PARAMETERS_END();

    /* 参数有效性检查：偏移量和长度不能为负 */
    if (offset < 0 || length <= 0) {
        /* 返回空字符串 */
        RETURN_EMPTY_STRING();
    }

    /* 获取当前对象 */
    xhresponse_obj_t *obj = XHRESPONSE_OBJ_FROM_ZOBJ(Z_OBJ_P(getThis()));

    /* 检查响应体缓冲区是否存在 */
    if (obj->body == NULL || obj->body->data == NULL) {
        RETURN_EMPTY_STRING();
    }

    /* 从缓冲区读取指定范围的数据 */
    char *out_data = NULL;
    size_t out_len = 0;
    int result = xhcurl_buffer_read(obj->body, (size_t)offset, (size_t)length,
                                     &out_data, &out_len);

    if (result != 0 || out_data == NULL || out_len == 0) {
        /* 读取失败或无数据，返回空字符串 */
        RETURN_EMPTY_STRING();
    }

    /* 返回读取的数据（PHP 会自动复制字符串，之后释放 out_data） */
    RETVAL_STRINGL(out_data, out_len);

    /* 释放临时输出缓冲区（使用 efree 对应 emalloc） */
    efree(out_data);
}

/**
 * 获取响应体总长度
 * XHResponse::getBodyLength(): int
 * 用于预先了解响应体大小，合理规划分段读取策略
 */
PHP_METHOD(XHResponse, getBodyLength)
{
    /* 无参数 */
    ZEND_PARSE_PARAMETERS_NONE();

    /* 获取当前对象 */
    xhresponse_obj_t *obj = XHRESPONSE_OBJ_FROM_ZOBJ(Z_OBJ_P(getThis()));

    /* 检查响应体缓冲区是否存在 */
    if (obj->body == NULL) {
        RETURN_LONG(0);
    }

    /* 返回响应体总大小 */
    RETURN_LONG((zend_long)obj->body->size);
}

/**
 * 获取 Content-Type 头部值
 * XHResponse::getContentType(): ?string
 */
PHP_METHOD(XHResponse, getContentType)
{
    /* 无参数 */
    ZEND_PARSE_PARAMETERS_NONE();

    /* 获取当前对象 */
    xhresponse_obj_t *obj = XHRESPONSE_OBJ_FROM_ZOBJ(Z_OBJ_P(getThis()));

    if (obj->content_type != NULL) {
        RETURN_STRING(obj->content_type);
    } else {
        RETURN_NULL();
    }
}

/**
 * 判断响应是否为 JSON 类型
 * XHResponse::isJson(): bool
 * 通过检查 Content-Type 头部判断
 */
PHP_METHOD(XHResponse, isJson)
{
    /* 无参数 */
    ZEND_PARSE_PARAMETERS_NONE();

    /* 获取当前对象 */
    xhresponse_obj_t *obj = XHRESPONSE_OBJ_FROM_ZOBJ(Z_OBJ_P(getThis()));

    /* 检查 Content-Type 是否包含 "json" */
    RETURN_BOOL(xhcurl_is_json_content_type(obj->content_type));
}

/**
 * 将响应体解析为 JSON 数组
 * XHResponse::toJsonArray(): ?array
 * 仅在调用时解析，不自动解析，避免不必要的内存开销
 * 如果响应体很大，建议使用 getBodyChunk 分段读取后自行解析
 */
PHP_METHOD(XHResponse, toJsonArray)
{
    /* 无参数 */
    ZEND_PARSE_PARAMETERS_NONE();

    /* 获取当前对象 */
    xhresponse_obj_t *obj = XHRESPONSE_OBJ_FROM_ZOBJ(Z_OBJ_P(getThis()));

    /* 检查响应体是否存在 */
    if (obj->body == NULL || obj->body->data == NULL || obj->body->size == 0) {
        RETURN_NULL();
    }

    /* 检查是否为 JSON 类型 */
    if (!xhcurl_is_json_content_type(obj->content_type)) {
        /* 非 JSON 类型，返回 null */
        RETURN_NULL();
    }

    /* 将响应体数据作为 PHP 字符串（需要添加 null 终止符） */
    char *json_str = (char *)ecalloc(1, obj->body->size + 1);
    memcpy(json_str, obj->body->data, obj->body->size);
    json_str[obj->body->size] = '\0';

    /* 调用 PHP 内置函数 json_decode 进行解析（避免直接依赖 php_json.h） */
    zval json_ret;       /* json_decode 返回值 */
    zval json_args[4];   /* json_decode 参数：json, assoc, depth, flags */
    ZVAL_UNDEF(&json_ret);
    /* 参数1：JSON 字符串 */
    ZVAL_STRING(&json_args[0], json_str);
    /* 参数2：assoc=true，返回数组而非对象 */
    ZVAL_TRUE(&json_args[1]);
    /* 参数3：递归深度，使用默认值 512 */
    ZVAL_LONG(&json_args[2], 512);
    /* 参数4：flags=0 */
    ZVAL_LONG(&json_args[3], 0);

    /* 释放临时 JSON 字符串 */
    efree(json_str);

    /* 调用 json_decode 函数（优先使用 MINIT 阶段缓存的函数指针） */
    int call_result = FAILURE;
    if (xhcurl_json_decode_func != NULL) {
        /* 使用缓存的函数指针直接调用，避免函数表哈希查找 */
        zend_call_known_function(xhcurl_json_decode_func, NULL, NULL,
                                  &json_ret, 4, json_args, NULL);
        /* zend_call_known_function 无返回值，通过异常判断是否成功 */
        call_result = (EG(exception) == NULL) ? SUCCESS : FAILURE;
    } else {
        /* 回退方案：通过函数名查找调用 */
        /* call_user_function 要求第3个参数为 zval*，不能直接传 zend_string* */
        zval func_name_zv;
        ZVAL_STRING(&func_name_zv, "json_decode");
        call_result = call_user_function(EG(function_table), NULL,
                                          &func_name_zv, &json_ret,
                                          4, json_args);
        zval_ptr_dtor(&func_name_zv);
    }
    /* 释放参数 */
    zval_ptr_dtor(&json_args[0]);

    /* 检查调用是否成功 */
    if (call_result != SUCCESS || EG(exception) != NULL) {
        /* JSON 解析失败，清除异常并返回 null */
        if (EG(exception) != NULL) {
            zend_clear_exception();
        }
        if (Z_TYPE(json_ret) != IS_UNDEF) {
            zval_ptr_dtor(&json_ret);
        }
        RETURN_NULL();
    }

    /* 返回解析后的数组 */
    RETURN_ZVAL(&json_ret, 0, 0);
}

/**
 * 获取错误信息
 * XHResponse::getError(): ?string
 * 请求失败时返回错误描述，成功时返回 null
 */
PHP_METHOD(XHResponse, getError)
{
    /* 无参数 */
    ZEND_PARSE_PARAMETERS_NONE();

    /* 获取当前对象 */
    xhresponse_obj_t *obj = XHRESPONSE_OBJ_FROM_ZOBJ(Z_OBJ_P(getThis()));

    if (obj->error_msg != NULL) {
        RETURN_STRING(obj->error_msg);
    } else {
        RETURN_NULL();
    }
}

/**
 * 获取请求总耗时
 * XHResponse::getTotalTime(): float
 * 返回从请求开始到完成的总时间（秒）
 */
PHP_METHOD(XHResponse, getTotalTime)
{
    /* 无参数 */
    ZEND_PARSE_PARAMETERS_NONE();

    /* 获取当前对象 */
    xhresponse_obj_t *obj = XHRESPONSE_OBJ_FROM_ZOBJ(Z_OBJ_P(getThis()));

    /* 返回耗时（秒，浮点数） */
    RETURN_DOUBLE(obj->total_time);
}

/**
 * 获取用户自定义数据
 * XHResponse::getUserData(): mixed
 *
 * 返回 add() 时传入的用户自定义数据，未设置时返回 null
 * 在 XHMulti 回调模式和无回调模式下均可使用
 *
 * @return mixed 用户自定义数据，未设置时返回 null
 */
PHP_METHOD(XHResponse, getUserData)
{
    /* 无参数 */
    ZEND_PARSE_PARAMETERS_NONE();

    /* 获取当前对象 */
    xhresponse_obj_t *obj = XHRESPONSE_OBJ_FROM_ZOBJ(Z_OBJ_P(getThis()));

    /* 返回用户自定义数据（未设置则返回 null） */
    if (!Z_ISUNDEF(obj->user_data)) {
        RETURN_COPY_VALUE(&obj->user_data);
    } else {
        RETURN_NULL();
    }
}

/* +----------------------------------------------------------------------+
 * | 参数信息定义（arginfo）                                                |
 * | PHP 8+ 要求所有方法必须声明参数信息，否则产生 Missing arginfo 警告     |
 * +----------------------------------------------------------------------+
 */

/* getStatusCode 参数信息（无参数） */
ZEND_BEGIN_ARG_INFO_EX(arginfo_xhresponse_getStatusCode, 0, 0, 0)
ZEND_END_ARG_INFO()

/* getHeader 参数信息 */
ZEND_BEGIN_ARG_INFO_EX(arginfo_xhresponse_getHeader, 0, 0, 1)
    ZEND_ARG_INFO(0, name)        /* 头部名称（必填） */
ZEND_END_ARG_INFO()

/* getHeaders 参数信息（无参数） */
ZEND_BEGIN_ARG_INFO_EX(arginfo_xhresponse_getHeaders, 0, 0, 0)
ZEND_END_ARG_INFO()

/* hasHeader 参数信息 */
ZEND_BEGIN_ARG_INFO_EX(arginfo_xhresponse_hasHeader, 0, 0, 1)
    ZEND_ARG_INFO(0, name)        /* 头部名称（必填） */
ZEND_END_ARG_INFO()

/* getBodyChunk 参数信息 */
ZEND_BEGIN_ARG_INFO_EX(arginfo_xhresponse_getBodyChunk, 0, 0, 1)
    ZEND_ARG_INFO(0, offset)      /* 偏移量（必填） */
    ZEND_ARG_INFO(0, length)      /* 读取长度（可选） */
ZEND_END_ARG_INFO()

/* getBodyLength 参数信息（无参数） */
ZEND_BEGIN_ARG_INFO_EX(arginfo_xhresponse_getBodyLength, 0, 0, 0)
ZEND_END_ARG_INFO()

/* getContentType 参数信息（无参数） */
ZEND_BEGIN_ARG_INFO_EX(arginfo_xhresponse_getContentType, 0, 0, 0)
ZEND_END_ARG_INFO()

/* isJson 参数信息（无参数） */
ZEND_BEGIN_ARG_INFO_EX(arginfo_xhresponse_isJson, 0, 0, 0)
ZEND_END_ARG_INFO()

/* toJsonArray 参数信息（无参数） */
ZEND_BEGIN_ARG_INFO_EX(arginfo_xhresponse_toJsonArray, 0, 0, 0)
ZEND_END_ARG_INFO()

/* getError 参数信息（无参数） */
ZEND_BEGIN_ARG_INFO_EX(arginfo_xhresponse_getError, 0, 0, 0)
ZEND_END_ARG_INFO()

/* getTotalTime 参数信息（无参数） */
ZEND_BEGIN_ARG_INFO_EX(arginfo_xhresponse_getTotalTime, 0, 0, 0)
ZEND_END_ARG_INFO()

/* getUserData 参数信息（无参数） */
ZEND_BEGIN_ARG_INFO_EX(arginfo_xhresponse_getUserData, 0, 0, 0)
ZEND_END_ARG_INFO()

/* +----------------------------------------------------------------------+
 * | 方法注册表                                                            |
 * +----------------------------------------------------------------------+
 */
static const zend_function_entry xhresponse_methods[] = {
    /* 获取 HTTP 状态码 */
    PHP_ME(XHResponse, getStatusCode, arginfo_xhresponse_getStatusCode, ZEND_ACC_PUBLIC)
    /* 获取指定响应头 */
    PHP_ME(XHResponse, getHeader, arginfo_xhresponse_getHeader, ZEND_ACC_PUBLIC)
    /* 获取所有响应头（慎用） */
    PHP_ME(XHResponse, getHeaders, arginfo_xhresponse_getHeaders, ZEND_ACC_PUBLIC)
    /* 检查响应头是否存在 */
    PHP_ME(XHResponse, hasHeader, arginfo_xhresponse_hasHeader, ZEND_ACC_PUBLIC)
    /* 分段读取响应体 */
    PHP_ME(XHResponse, getBodyChunk, arginfo_xhresponse_getBodyChunk, ZEND_ACC_PUBLIC)
    /* 获取响应体总长度 */
    PHP_ME(XHResponse, getBodyLength, arginfo_xhresponse_getBodyLength, ZEND_ACC_PUBLIC)
    /* 获取 Content-Type */
    PHP_ME(XHResponse, getContentType, arginfo_xhresponse_getContentType, ZEND_ACC_PUBLIC)
    /* 判断是否为 JSON */
    PHP_ME(XHResponse, isJson, arginfo_xhresponse_isJson, ZEND_ACC_PUBLIC)
    /* 解析 JSON 为数组 */
    PHP_ME(XHResponse, toJsonArray, arginfo_xhresponse_toJsonArray, ZEND_ACC_PUBLIC)
    /* 获取错误信息 */
    PHP_ME(XHResponse, getError, arginfo_xhresponse_getError, ZEND_ACC_PUBLIC)
    /* 获取请求耗时 */
    PHP_ME(XHResponse, getTotalTime, arginfo_xhresponse_getTotalTime, ZEND_ACC_PUBLIC)
    /* 获取用户自定义数据 */
    PHP_ME(XHResponse, getUserData, arginfo_xhresponse_getUserData, ZEND_ACC_PUBLIC)
    /* 结束标记 */
    PHP_FE_END
};

/* +----------------------------------------------------------------------+
 * | 类初始化函数                                                          |
 * | 在 MINIT 阶段调用，注册 XHResponse 类                                 |
 * +----------------------------------------------------------------------+
 */
PHP_MINIT_FUNCTION(xhresponse_class)
{
    /* 初始化类入口 */
    zend_class_entry ce;
    INIT_CLASS_ENTRY(ce, "XHResponse", xhresponse_methods);

    /* 注册类（不支持实例化，由执行器内部创建） */
    xhresponse_ce = zend_register_internal_class(&ce);

    /* 设置对象创建和释放函数 */
    xhresponse_ce->create_object = xhresponse_create_obj;

    /* 声明属性：statusCode（只读，通过方法访问） */
    zend_declare_property_null(xhresponse_ce, "statusCode", sizeof("statusCode") - 1, ZEND_ACC_PRIVATE);

    return SUCCESS;
}
