/* +----------------------------------------------------------------------+
 * | XHCurl 扩展 - 私有头文件                                             |
 * | 包含所有内部数据结构、宏定义和内部函数声明                            |
 * | 本文件仅供扩展内部源文件使用，不对外暴露                              |
 * +----------------------------------------------------------------------+
 */

#ifndef XHCURL_PRIV_H
#define XHCURL_PRIV_H

#include "php_xhcurl.h"
#include <curl/curl.h>
#include <string.h>
#include <stdlib.h>

/* +----------------------------------------------------------------------+
 * | 平台兼容性宏定义                                                      |
 * +----------------------------------------------------------------------+
 */

/* Windows 平台线程相关头文件 */
#ifdef PHP_WIN32
# include <windows.h>
# include <process.h>
#else
/* Unix/Linux 平台线程相关头文件 */
# include <pthread.h>
#endif

/* +----------------------------------------------------------------------+
 * | 响应缓冲区数据结构                                                    |
 * | 用于存储 HTTP 响应体，支持分段读取，避免一次性加载到 PHP 内存         |
 * | 使用 malloc/free 管理，不计入 PHP memory_limit，                     |
 * | 通过 max_size 限制防止内存溢出                                        |
 * +----------------------------------------------------------------------+
 */
typedef struct _xhcurl_buffer {
    char   *data;       /* 缓冲区数据指针 */
    size_t  size;       /* 当前已写入的数据大小（字节） */
    size_t  capacity;   /* 缓冲区已分配的总容量（字节） */
    size_t  max_size;   /* 缓冲区允许的最大大小（字节），0 表示无限制 */
} xhcurl_buffer_t;

/* +----------------------------------------------------------------------+
 * | HTTP 头部链表节点                                                     |
 * | 使用链表存储 HTTP 响应头，支持同名多值                                |
 * +----------------------------------------------------------------------+
 */
typedef struct _xhcurl_header {
    char *name;                     /* 头部名称（小写化存储，便于查找） */
    char *value;                    /* 头部值 */
    struct _xhcurl_header *next;    /* 链表下一节点指针 */
} xhcurl_header_t;

/* +----------------------------------------------------------------------+
 * | Cookie 链表节点                                                       |
 * | 用于存储全局或请求级别的 Cookie                                       |
 * +----------------------------------------------------------------------+
 */
typedef struct _xhcurl_cookie {
    char *name;                     /* Cookie 名称 */
    char *value;                    /* Cookie 值 */
    char *domain;                   /* Cookie 域名 */
    char *path;                     /* Cookie 路径 */
    struct _xhcurl_cookie *next;    /* 链表下一节点指针 */
} xhcurl_cookie_t;

/* +----------------------------------------------------------------------+
 * | XHCurl 全局管理器对象（PHP 对象内部数据）                             |
 * | 管理全局配置：全局 headers、cookies、超时、代理等                     |
 * | 使用 curl_share 实现多个请求间共享 DNS/SSL 会话                      |
 * +----------------------------------------------------------------------+
 */
typedef struct _xhcurl_obj {
    CURLSH             *share;              /* curl 共享句柄，用于共享 DNS/SSL/Cookie */
    struct curl_slist  *global_headers;     /* 全局请求头列表（curl_slist 格式） */
    xhcurl_header_t    *global_headers_raw; /* 全局请求头列表（原始键值对格式，用于合并） */
    xhcurl_cookie_t    *global_cookies;     /* 全局 Cookie 列表 */
    long                timeout;            /* 默认请求超时时间（秒） */
    long                connect_timeout;    /* 默认连接超时时间（秒） */
    zend_bool           verify_ssl;         /* 是否验证 SSL 证书 */
    char               *user_agent;         /* 默认 User-Agent */
    char               *proxy;              /* 默认代理服务器地址 */
    size_t              max_response_size;  /* 响应体最大允许大小（字节） */
    zend_object         std;                /* PHP 对象标准头（必须放在最后） */
} xhcurl_obj_t;

/* +----------------------------------------------------------------------+
 * | XHRequest 请求构建器对象（PHP 对象内部数据）                          |
 * | 构建单个 HTTP 请求的配置，支持链式调用                                |
 * +----------------------------------------------------------------------+
 */
typedef struct _xhrequest_obj {
    char               *url;                /* 请求 URL */
    char               *method;             /* HTTP 方法（GET/POST/PUT/DELETE 等） */
    xhcurl_header_t    *headers;            /* 请求级别头部列表 */
    xhcurl_cookie_t    *cookies;            /* 请求级别 Cookie 列表 */
    char               *body;               /* 请求体数据 */
    size_t              body_len;            /* 请求体数据长度 */
    long                timeout;            /* 请求超时（0 表示使用全局默认值） */
    long                connect_timeout;    /* 连接超时（0 表示使用全局默认值） */
    zend_bool           follow_redirects;   /* 是否跟随重定向 */
    long                max_redirects;      /* 最大重定向次数 */
    zval                chunk_callback;     /* 流式数据回调（PHP callable） */
    zval                header_callback;    /* 响应头回调（PHP callable） */
    zend_object         std;                /* PHP 对象标准头（必须放在最后） */
} xhrequest_obj_t;

/* +----------------------------------------------------------------------+
 * | XHResponse 懒加载响应对象（PHP 对象内部数据）                         |
 * | 不一次性返回所有数据，支持按需分段读取响应体                          |
 * +----------------------------------------------------------------------+
 */
typedef struct _xhresponse_obj {
    long                status_code;        /* HTTP 状态码 */
    char               *error_msg;          /* 错误信息（无错误时为 NULL） */
    CURLcode            curl_code;          /* curl 原始错误码 */
    xhcurl_header_t    *headers;            /* 响应头部链表 */
    xhcurl_buffer_t    *body;               /* 响应体缓冲区（C 侧管理，按需读取） */
    char               *content_type;       /* Content-Type 头部值 */
    double              total_time;         /* 请求总耗时（秒） */
    zend_object         std;                /* PHP 对象标准头（必须放在最后） */
} xhresponse_obj_t;

/* +----------------------------------------------------------------------+
 * | 请求执行上下文                                                        |
 * | 在 curl_multi 执行期间，关联 curl easy 句柄与请求数据                 |
 * +----------------------------------------------------------------------+
 */
typedef struct _xhcurl_req_context {
    CURL               *easy;               /* curl easy 句柄 */
    xhcurl_buffer_t     body_buf;            /* 响应体缓冲区（内嵌，非指针） */
    xhcurl_buffer_t     header_buf;          /* 响应头原始数据缓冲区 */
    xhcurl_header_t    *parsed_headers;      /* 解析后的响应头链表 */
    zval                chunk_callback;      /* 流式数据回调引用 */
    zval                header_callback;     /* 响应头回调引用 */
    zval                request_zval;        /* 关联的 XHRequest PHP 对象引用 */
    long                status_code;         /* HTTP 状态码 */
    char               *content_type;        /* Content-Type 值 */
    struct curl_slist  *request_headers;     /* curl_slist 格式的请求头 */
} xhcurl_req_context_t;

/* +----------------------------------------------------------------------+
 * | XHMulti 批量异步执行器对象（PHP 对象内部数据）                        |
 * | 基于 curl_multi 接口，FPM 和 CLI 模式通用                            |
 * +----------------------------------------------------------------------+
 */
typedef struct _xhmulti_obj {
    CURLM                      *multi;           /* curl multi 句柄 */
    xhcurl_obj_t               *curl_obj;        /* 关联的 XHCurl 全局管理器引用 */
    GHashTable                 *ctx_map;         /* 映射：CURL* -> xhcurl_req_context_t* */
    zval                        curl_zval;       /* XHCurl PHP 对象引用（防止 GC 回收） */
    int                         request_count;   /* 已添加的请求数量 */
    zend_object                 std;             /* PHP 对象标准头（必须放在最后） */
} xhmulti_obj_t;

/* +----------------------------------------------------------------------+
 * | 线程池工作线程参数                                                    |
 * | 每个工作线程独立执行一批请求，避免线程间竞争                          |
 * +----------------------------------------------------------------------+
 */
typedef struct _xhcurl_worker_arg {
    int                         worker_id;       /* 工作线程编号 */
    xhcurl_req_context_t      **contexts;        /* 该线程负责的请求上下文数组 */
    int                         context_count;   /* 请求上下文数量 */
    volatile int                done;            /* 完成标志（0=运行中，1=已完成） */
    int                         error;           /* 错误码（0=无错误） */
#ifdef PHP_WIN32
    HANDLE                      thread_handle;   /* Windows 线程句柄 */
#else
    pthread_t                   thread_id;       /* POSIX 线程 ID */
#endif
} xhcurl_worker_arg_t;

/* +----------------------------------------------------------------------+
 * | XHThreadPool CLI线程池对象（PHP 对象内部数据）                        |
 * | 仅在 CLI 模式下可用，使用多线程并发执行请求                           |
 * +----------------------------------------------------------------------+
 */
typedef struct _xhthreadpool_obj {
    xhcurl_obj_t               *curl_obj;        /* 关联的 XHCurl 全局管理器引用 */
    zval                        curl_zval;       /* XHCurl PHP 对象引用（防止 GC 回收） */
    int                         worker_count;    /* 工作线程数量 */
    xhcurl_req_context_t      **contexts;        /* 所有请求上下文数组 */
    int                         context_count;   /* 请求上下文总数 */
    int                         context_capacity;/* 请求上下文数组容量 */
    zend_object                 std;             /* PHP 对象标准头（必须放在最后） */
} xhthreadpool_obj_t;

/* +----------------------------------------------------------------------+
 * | 对象获取宏                                                            |
 * | 从 zend_object 指针获取自定义结构体指针                               |
 * | 利用 offsetof 计算偏移量，std 字段必须在结构体末尾                    |
 * +----------------------------------------------------------------------+
 */

/* 从 zend_object 获取 XHCurl 对象 */
#define XHCURL_OBJ_FROM_ZOBJ(zobj) \
    ((xhcurl_obj_t *)((char *)(zobj) - XtOffsetOf(xhcurl_obj_t, std)))

/* 从 zend_object 获取 XHRequest 对象 */
#define XHREQUEST_OBJ_FROM_ZOBJ(zobj) \
    ((xhrequest_obj_t *)((char *)(zobj) - XtOffsetOf(xhrequest_obj_t, std)))

/* 从 zend_object 获取 XHResponse 对象 */
#define XHRESPONSE_OBJ_FROM_ZOBJ(zobj) \
    ((xhresponse_obj_t *)((char *)(zobj) - XtOffsetOf(xhresponse_obj_t, std)))

/* 从 zend_object 获取 XHMulti 对象 */
#define XHMULTI_OBJ_FROM_ZOBJ(zobj) \
    ((xhmulti_obj_t *)((char *)(zobj) - XtOffsetOf(xhmulti_obj_t, std)))

/* 从 zend_object 获取 XHThreadPool 对象 */
#define XHTHREADPOOL_OBJ_FROM_ZOBJ(zobj) \
    ((xhthreadpool_obj_t *)((char *)(zobj) - XtOffsetOf(xhthreadpool_obj_t, std)))

/* +----------------------------------------------------------------------+
 * | 从 zval 获取对象指针的便捷宏                                          |
 * +----------------------------------------------------------------------+
 */

/* 从 zval 获取 XHCurl 对象 */
#define XHCURL_OBJ_FROM_ZVAL(zv) \
    XHCURL_OBJ_FROM_ZOBJ(Z_OBJ_P(zv))

/* 从 zval 获取 XHRequest 对象 */
#define XHREQUEST_OBJ_FROM_ZVAL(zv) \
    XHREQUEST_OBJ_FROM_ZOBJ(Z_OBJ_P(zv))

/* 从 zval 获取 XHResponse 对象 */
#define XHRESPONSE_OBJ_FROM_ZVAL(zv) \
    XHRESPONSE_OBJ_FROM_ZOBJ(Z_OBJ_P(zv))

/* +----------------------------------------------------------------------+
 * | 全局类入口声明（在各自 .c 文件中定义）                                |
 * +----------------------------------------------------------------------+
 */

/* XHCurl 类入口指针 */
extern zend_class_entry *xhcurl_ce;
/* XHRequest 类入口指针 */
extern zend_class_entry *xhrequest_ce;
/* XHResponse 类入口指针 */
extern zend_class_entry *xhresponse_ce;
/* XHMulti 类入口指针 */
extern zend_class_entry *xhmulti_ce;
/* XHThreadPool 类入口指针 */
extern zend_class_entry *xhthreadpool_ce;
/* XHCurlException 异常类入口指针 */
extern zend_class_entry *xhcurl_exception_ce;

/* +----------------------------------------------------------------------+
 * | 缓冲区操作函数声明（xhcurl_buffer.c）                                |
 * +----------------------------------------------------------------------+
 */

/* 初始化缓冲区，initial_cap 为初始容量，max_sz 为最大允许大小 */
int  xhcurl_buffer_init(xhcurl_buffer_t *buf, size_t initial_cap, size_t max_sz);

/* 向缓冲区追加数据，返回 0 成功，-1 表示超过最大限制 */
int  xhcurl_buffer_write(xhcurl_buffer_t *buf, const char *data, size_t len);

/* 从缓冲区读取指定范围的数据，返回读取的字节数，-1 表示参数错误 */
int  xhcurl_buffer_read(xhcurl_buffer_t *buf, size_t offset, size_t length,
                         char **out_data, size_t *out_len);

/* 释放缓冲区资源 */
void xhcurl_buffer_free(xhcurl_buffer_t *buf);

/* +----------------------------------------------------------------------+
 * | 头部链表操作函数声明                                                  |
 * +----------------------------------------------------------------------+
 */

/* 向头部链表追加一个头部，name 会被转为小写存储 */
void xhcurl_header_add(xhcurl_header_t **list, const char *name, const char *value);

/* 在头部链表中查找指定名称的头部值（名称不区分大小写） */
const char *xhcurl_header_find(xhcurl_header_t *list, const char *name);

/* 释放整个头部链表 */
void xhcurl_header_list_free(xhcurl_header_t *list);

/* 解析 HTTP 响应头原始文本，填充到头部链表 */
void xhcurl_parse_response_headers(xhcurl_buffer_t *header_buf, xhcurl_header_t **headers);

/* +----------------------------------------------------------------------+
 * | Cookie 链表操作函数声明                                               |
 * +----------------------------------------------------------------------+
 */

/* 向 Cookie 链表追加一个 Cookie */
void xhcurl_cookie_add(xhcurl_cookie_t **list, const char *name, const char *value,
                        const char *domain, const char *path);

/* 释放整个 Cookie 链表 */
void xhcurl_cookie_list_free(xhcurl_cookie_t *list);

/* +----------------------------------------------------------------------+
 * | 请求上下文操作函数声明                                                |
 * +----------------------------------------------------------------------+
 */

/* 创建请求执行上下文，关联 curl easy 句柄和请求数据 */
xhcurl_req_context_t *xhcurl_context_create(xhcurl_obj_t *curl_obj, xhrequest_obj_t *req_obj);

/* 释放请求执行上下文及其所有资源 */
void xhcurl_context_free(xhcurl_req_context_t *ctx);

/* +----------------------------------------------------------------------+
 * | curl 回调函数声明                                                     |
 * +----------------------------------------------------------------------+
 */

/* curl 写数据回调（响应体） */
size_t xhcurl_write_callback(void *contents, size_t size, size_t nmemb, void *userp);

/* curl 写数据回调（响应头） */
size_t xhcurl_header_callback(void *contents, size_t size, size_t nmemb, void *userp);

/* +----------------------------------------------------------------------+
 * | 辅助函数声明                                                          |
 * +----------------------------------------------------------------------+
 */

/* 检查当前是否为 CLI 模式（线程池仅在 CLI 下可用） */
zend_bool xhcurl_is_cli_mode(void);

/* 将字符串转为小写（原地修改） */
void xhcurl_str_tolower(char *str, size_t len);

/* 判断 Content-Type 是否为 JSON 类型 */
zend_bool xhcurl_is_json_content_type(const char *content_type);

#endif /* XHCURL_PRIV_H */
