/* +----------------------------------------------------------------------+
 * | XHCurl 扩展 - 公共头文件                                             |
 * | 类似 curl 的 PHP 扩展，支持单次/批量/多线程同步异步请求               |
 * | 本文件声明模块对外可见的函数和宏                                      |
 * +----------------------------------------------------------------------+
 */

#ifndef PHP_XHCURL_H
#define PHP_XHCURL_H

/* 引入 PHP 扩展开发核心头文件 */
#include "php.h"
/* 引入 PHP 扩展标准宏定义 */
#include "php_ini.h"
/* 引入 PHP 扩展信息展示函数声明 */
#include "ext/standard/info.h"

/* +----------------------------------------------------------------------+
 * | 模块版本和名称定义                                                    |
 * +----------------------------------------------------------------------+
 */

/* 扩展版本号 */
#define PHP_XHCURL_VERSION "1.0.0"

/* 扩展名称 */
#define PHP_XHCURL_EXTNAME "xhcurl"

/* 默认最大响应体大小（10MB），防止内存溢出 */
#define XHCURL_DEFAULT_MAX_RESPONSE_SIZE (10 * 1024 * 1024)

/* 默认请求超时时间（秒） */
#define XHCURL_DEFAULT_TIMEOUT 30

/* 默认连接超时时间（秒） */
#define XHCURL_DEFAULT_CONNECT_TIMEOUT 10

/* 默认最大重定向次数 */
#define XHCURL_DEFAULT_MAX_REDIRECTS 5

/* 默认缓冲区初始容量（4KB） */
#define XHCURL_BUFFER_INIT_CAPACITY 4096

/* 默认线程池工作线程数 */
#define XHCURL_DEFAULT_THREAD_POOL_SIZE 4

/* 默认最大并发数（滑动窗口大小） */
#define XHCURL_DEFAULT_MAX_CONCURRENT 100

/* +----------------------------------------------------------------------+
 * | 模块全局声明（extern，供其他源文件引用）                              |
 * +----------------------------------------------------------------------+
 */

/* 模块入口结构体（在 xhcurl.c 中定义） */
extern zend_module_entry xhcurl_module_entry;

/* 模块入口宏（PHP 扩展标准宏） */
#define phpext_xhcurl_ptr &xhcurl_module_entry

/* +----------------------------------------------------------------------+
 * | 各类的 PHP 类入口声明（在各自的 .c 文件中定义）                       |
 * +----------------------------------------------------------------------+
 */

/* XHCurl 全局管理器类入口 */
extern PHP_MINIT_FUNCTION(xhcurl_class);
/* XHRequest 请求构建器类入口 */
extern PHP_MINIT_FUNCTION(xhrequest_class);
/* XHResponse 懒加载响应类入口 */
extern PHP_MINIT_FUNCTION(xhresponse_class);
/* XHMulti 批量异步执行器类入口 */
extern PHP_MINIT_FUNCTION(xhmulti_class);
/* XHThreadPool CLI线程池类入口 */
extern PHP_MINIT_FUNCTION(xhthreadpool_class);

/* +----------------------------------------------------------------------+
 * | 模块生命周期函数声明                                                  |
 * +----------------------------------------------------------------------+
 */

/* 模块初始化（PHP 启动时调用） */
PHP_MINIT_FUNCTION(xhcurl);
/* 模块关闭（PHP 关闭时调用） */
PHP_MSHUTDOWN_FUNCTION(xhcurl);
/* 请求初始化（每个 PHP 请求开始时调用） */
PHP_RINIT_FUNCTION(xhcurl);
/* 请求关闭（每个 PHP 请求结束时调用） */
PHP_RSHUTDOWN_FUNCTION(xhcurl);
/* 模块信息展示（phpinfo() 调用） */
PHP_MINFO_FUNCTION(xhcurl);

/* +----------------------------------------------------------------------+
 * | PHP 函数声明                                                          |
 * +----------------------------------------------------------------------+
 */

/* 获取扩展版本号 */
PHP_FUNCTION(xhcurl_version);

#endif /* PHP_XHCURL_H */
