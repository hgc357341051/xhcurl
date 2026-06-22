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
#include <ctype.h>  /* tolower（Windows 下 strcasestr 兼容实现需要） */

/* 引入 PHP 异常处理相关头文件（zend_throw_exception / zend_ce_exception） */
#include "zend_exceptions.h"
/* 引入 PHP SAPI 头文件（sapi_module 全局变量） */
#include "SAPI.h"

/* php_raw_url_encode 函数由 PHP 核心导出（在 php-core DLL/so 中），
 * 但其声明头文件 ext/standard/php_url.h 不在扩展的公共 include 路径中，
 * 无法通过 #include 引入。
 *
 * 解决方案：使用 extern 声明该函数，链接时由 PHP 核心提供符号。
 *
 * 函数签名：zend_string *php_raw_url_encode(const char *str, size_t len);
 * - 等价于 PHP 的 rawurlencode() 函数
 * - 返回值需要调用 zend_string_release() 释放
 */
extern zend_string *php_raw_url_encode(const char *str, size_t len);

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
# include <unistd.h>  /* usleep（重试间隔 sleep） */
#endif

/* +----------------------------------------------------------------------+
 * | 跨平台字符串函数兼容层                                                |
 * | Windows 下缺少 memmem 和 strcasestr，提供内联实现                    |
 * +----------------------------------------------------------------------+
 */

#ifdef PHP_WIN32
/**
 * Windows 下 memmem 的兼容实现
 * 在内存块中查找子内存块
 * @param haystack    主内存块
 * @param haystacklen 主内存块长度
 * @param needle      要查找的子内存块
 * @param needlelen   子内存块长度
 * @return 找到的位置指针，未找到返回 NULL
 */
static inline void *xhcurl_memmem(const void *haystack, size_t haystacklen,
                                   const void *needle, size_t needlelen)
{
    /* 子串比主串长，不可能找到 */
    if (needlelen > haystacklen) return NULL;
    /* 空子串匹配起始位置 */
    if (needlelen == 0) return (void *)haystack;

    /* 逐位置检查是否匹配 */
    const char *h = (const char *)haystack;
    const char *n = (const char *)needle;
    /* 可选起始位置数 */
    size_t max_pos = haystacklen - needlelen;
    for (size_t i = 0; i <= max_pos; i++) {
        if (memcmp(h + i, n, needlelen) == 0) {
            return (void *)(h + i);
        }
    }
    return NULL;
}

/**
 * Windows 下 strcasestr 的兼容实现
 * 不区分大小写地查找子串
 * @param haystack 主字符串
 * @param needle   要查找的子串
 * @return 找到的位置指针，未找到返回 NULL
 */
static inline char *xhcurl_strcasestr(const char *haystack, const char *needle)
{
    /* 空子串匹配起始位置 */
    if (*needle == '\0') return (char *)haystack;

    /* 逐起始位置检查 */
    for (const char *h = haystack; *h != '\0'; h++) {
        const char *p = h;
        const char *n = needle;
        /* 不区分大小写逐字符比较 */
        while (*p && *n && tolower((unsigned char)*p) == tolower((unsigned char)*n)) {
            p++;
            n++;
        }
        /* 完整匹配子串 */
        if (*n == '\0') return (char *)h;
    }
    return NULL;
}

/* 用宏将标准函数名重定向到兼容实现 */
#define memmem(h, hl, n, nl) xhcurl_memmem((h), (hl), (n), (nl))
#define strcasestr(h, n)     xhcurl_strcasestr((h), (n))

#endif /* PHP_WIN32 */

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
    xhcurl_header_t    *global_headers_raw; /* 全局请求头列表（原始键值对格式，用于合并） */
    xhcurl_cookie_t    *global_cookies;     /* 全局 Cookie 列表 */
    long                timeout;            /* 默认请求超时时间（秒） */
    long                connect_timeout;    /* 默认连接超时时间（秒） */
    zend_bool           verify_ssl;         /* 是否验证 SSL 证书 */
    char               *user_agent;         /* 默认 User-Agent */
    char               *proxy;              /* 默认代理服务器地址 */
    size_t              max_response_size;  /* 响应体最大允许大小（字节） */
    zend_bool           http2_enabled;      /* 是否启用 HTTP/2（默认启用） */
    long                retry_count;        /* 失败重试次数（默认 0 不重试） */
    long                retry_delay_ms;     /* 重试间隔（毫秒，默认 100ms） */
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
    char               *proxy;              /* 请求级代理（NULL=使用全局，""=禁用代理） */
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
    zval                user_data;          /* 用户自定义数据（add 时传入，可通过 getUserData() 获取） */
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
    zval                user_data;           /* 用户自定义数据（add 时传入，回调时原样返回） */
    long                status_code;         /* HTTP 状态码 */
    CURLcode            curl_code;           /* curl 执行结果码（CURLE_OK 表示成功，工作线程中保存） */
    char               *content_type;        /* Content-Type 值 */
    struct curl_slist  *request_headers;     /* curl_slist 格式的请求头 */
} xhcurl_req_context_t;

/* +----------------------------------------------------------------------+
 * | easy 句柄 → 上下文索引 哈希表节点                                     |
 * | 用于 O(1) 查找 curl easy 句柄对应的请求上下文                         |
 * | 替代原来的 O(n) 线性查找，百万级请求时性能差异显著                    |
 * +----------------------------------------------------------------------+
 */
typedef struct _xhcurl_easy_map_entry {
    CURL                        *easy;           /* curl easy 句柄（键） */
    xhcurl_req_context_t        *context;        /* 请求上下文（值） */
    struct _xhcurl_easy_map_entry *next;         /* 哈希冲突链表下一节点 */
} xhcurl_easy_map_entry_t;

/* +----------------------------------------------------------------------+
 * | easy 句柄 → 上下文 哈希表                                             |
 * | 固定桶数的开链哈希表，适合 curl_multi 场景                            |
 * | 桶数取 2 的幂次，用位运算替代取模加速                                 |
 * +----------------------------------------------------------------------+
 */
#define XHCURL_EASY_MAP_BUCKETS 1024  /* 哈希桶数（2 的幂次） */

typedef struct _xhcurl_easy_map {
    xhcurl_easy_map_entry_t    *buckets[XHCURL_EASY_MAP_BUCKETS]; /* 桶数组 */
    int                         size;            /* 已存储的键值对数量 */
} xhcurl_easy_map_t;

/* +----------------------------------------------------------------------+
 * | XHMulti 批量异步执行器对象（PHP 对象内部数据）                        |
 * | 基于 curl_multi 接口，FPM 和 CLI 模式通用                            |
 * |                                                                      |
 * | 滑动窗口设计：                                                        |
 * |   - add() 时仅将 XHRequest 引用存入 pending 队列                     |
 * |   - execute() 时按 max_concurrent 窗口大小分批创建上下文              |
 * |   - 一个请求完成立即从 pending 取下一个补充，保持满窗并发             |
 * |   - 有 callback 时实时回调 + 释放已完成响应，内存恒定                 |
 * |   - 无 callback 时返回全量数组（兼容旧 API）                          |
 * +----------------------------------------------------------------------+
 */
typedef struct _xhmulti_obj {
    CURLM                      *multi;           /* curl multi 句柄 */
    xhcurl_obj_t               *curl_obj;        /* 关联的 XHCurl 全局管理器引用 */
    zval                        curl_zval;       /* XHCurl PHP 对象引用（防止 GC 回收） */

    /* --- 待执行请求队列（add 时入队，execute 时消费） --- */
    zval                       *pending_requests;/* 待执行请求的 XHRequest zval 数组 */
    zval                       *pending_user_data;/* 待执行请求的用户自定义数据数组（与 pending_requests 一一对应） */
    int                         pending_count;   /* 待执行请求数量 */
    int                         pending_capacity;/* 待执行数组容量 */
    int                         pending_head;    /* 队列头部索引（下一个要执行的） */

    /* --- 活跃请求上下文（execute 期间使用） --- */
    xhcurl_req_context_t      **contexts;        /* 活跃请求上下文指针数组 */
    int                         context_count;   /* 活跃请求上下文数量 */
    int                         context_capacity;/* 上下文数组已分配容量 */

    /* --- 并发控制 --- */
    int                         max_concurrent;  /* 最大并发数（滑动窗口大小） */
    zend_long                   global_timeout;  /* 全局总超时（秒），0 = 不限制，默认 300 */
    zend_bool                   is_executing;    /* 是否正在执行（防止 execute 期间调用 add） */

    /* --- easy → context 哈希表（O(1) 查找） --- */
    xhcurl_easy_map_t           easy_map;        /* easy 句柄到上下文的映射 */

    /* --- 已完成请求结果（无 callback 模式下使用） --- */
    zval                       *results;         /* 已完成的 XHResponse zval 数组 */
    int                         result_count;    /* 已完成结果数量 */
    int                         result_capacity; /* 结果数组容量 */
    int                         completed_count; /* 已完成请求数（含回调模式，用于返回值） */

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
    volatile int                done;            /* 完成标志（0=运行中，1=已完成），使用原子操作写入 */
    int                         error;           /* 错误码（0=无错误），使用原子操作写入 */
#ifdef PHP_WIN32
    HANDLE                      thread_handle;   /* Windows 线程句柄 */
#else
    pthread_t                   thread_id;       /* POSIX 线程 ID */
#endif
} xhcurl_worker_arg_t;

/* +----------------------------------------------------------------------+
 * | XHThreadPool CLI线程池对象（PHP 对象内部数据）                        |
 * | 仅在 CLI 模式下可用，使用多线程并发执行请求                           |
 * |                                                                      |
 * | 延迟创建设计：                                                        |
 * |   - add() 仅将 XHRequest 引用存入待执行队列                          |
 * |   - execute() 时才创建 curl 上下文并分配给工作线程                    |
 * +----------------------------------------------------------------------+
 */
typedef struct _xhthreadpool_obj {
    xhcurl_obj_t               *curl_obj;        /* 关联的 XHCurl 全局管理器引用 */
    zval                        curl_zval;       /* XHCurl PHP 对象引用（防止 GC 回收） */
    int                         worker_count;    /* 工作线程数量 */

    /* --- 待执行请求队列（add 时入队，execute 时消费） --- */
    zval                       *pending_requests;/* 待执行请求的 XHRequest zval 数组 */
    zval                       *pending_user_data;/* 待执行请求的用户自定义数据数组（与 pending_requests 一一对应） */
    int                         pending_count;   /* 待执行请求数量 */
    int                         pending_capacity;/* 待执行数组容量 */
    zend_bool                   is_executing;    /* 是否正在执行（防止 execute 期间调用 add） */

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
 * | JSON 函数指针缓存（性能优化）                                        |
 * | 在 MINIT 时缓存 json_encode/json_decode 的函数指针，                 |
 * | 避免每次调用都通过 zend_hash_find 查找函数表                         |
 * +----------------------------------------------------------------------+
 */

/* json_encode 函数指针（MINIT 时初始化） */
extern zend_function *xhcurl_json_encode_func;
/* json_decode 函数指针（MINIT 时初始化） */
extern zend_function *xhcurl_json_decode_func;

/* +----------------------------------------------------------------------+
 * | 缓冲区操作函数声明（xhcurl_buffer.c）                                |
 * +----------------------------------------------------------------------+
 */

/* 初始化缓冲区，initial_cap 为初始容量，max_sz 为最大允许大小 */
int  xhcurl_buffer_init(xhcurl_buffer_t *buf, size_t initial_cap, size_t max_sz);

/* 向缓冲区追加数据，返回 0 成功，-1 表示超过最大限制 */
int  xhcurl_buffer_write(xhcurl_buffer_t *buf, const char *data, size_t len);

/* 释放缓冲区资源 */
void xhcurl_buffer_free(xhcurl_buffer_t *buf);

/* +----------------------------------------------------------------------+
 * | 头部链表操作函数声明                                                  |
 * +----------------------------------------------------------------------+
 */

/* 向头部链表追加一个头部，name 会被转为小写存储 */
void xhcurl_header_add(xhcurl_header_t **list, const char *name, const char *value);

/* 设置头部值（存在则替换，不存在则添加），用于请求头去重 */
void xhcurl_header_set(xhcurl_header_t **list, const char *name, const char *value);

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
