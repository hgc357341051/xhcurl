dnl -*- autoconf -*-
dnl +----------------------------------------------------------------------+
dnl | XHCurl 扩展 - 构建配置 (Unix/Linux/macOS)                           |
dnl | 类似 curl 的 PHP 扩展，支持单次/批量/多线程同步异步请求               |
dnl +----------------------------------------------------------------------+

dnl 检查用户是否启用 xhcurl 扩展
PHP_ARG_ENABLE([xhcurl],
  [whether to enable xhcurl support],
  [AS_HELP_STRING([--enable-xhcurl],
    [Enable xhcurl support])],
  [no])

dnl 如果用户启用了 xhcurl，则进行依赖检查和编译配置
if test "$PHP_XHCURL" != "no"; then

  dnl 检查系统中是否安装了 pkg-config 工具
  AC_PATH_PROG(PKG_CONFIG, pkg-config, no)

  dnl 通过 pkg-config 获取 libcurl 的编译和链接参数
  if test -x "$PKG_CONFIG" && $PKG_CONFIG --exists libcurl; then
    CURL_CFLAGS=`$PKG_CONFIG --cflags libcurl`
    CURL_LIBS=`$PKG_CONFIG --libs libcurl`
  else
    dnl 如果 pkg-config 不可用，尝试手动查找 libcurl
    AC_MSG_CHECKING([for libcurl])
    if test -f /usr/include/curl/curl.h; then
      CURL_CFLAGS=""
      CURL_LIBS="-lcurl"
      AC_MSG_RESULT([found in /usr])
    elif test -f /usr/local/include/curl/curl.h; then
      CURL_CFLAGS="-I/usr/local/include"
      CURL_LIBS="-L/usr/local/lib -lcurl"
      AC_MSG_RESULT([found in /usr/local])
    else
      AC_MSG_ERROR([libcurl not found. Please install libcurl development files.])
    fi
  fi

  dnl 将 libcurl 的编译参数添加到扩展的 CFLAGS 中
  PHP_EVAL_INCLINE($CURL_CFLAGS)

  dnl 将 libcurl 的链接参数添加到扩展的共享库链接标志中
  PHP_EVAL_LIBLINE($CURL_LIBS, XHCURL_SHARED_LIBADD)

  dnl 检查是否支持 pthread（线程池功能依赖）
  AC_CHECK_HEADERS([pthread.h], [
    dnl 找到 pthread.h，添加线程库链接
    PHP_EVAL_LIBLINE([-lpthread], XHCURL_SHARED_LIBADD)
    AC_DEFINE([HAVE_PTHREAD], [1], [Define to 1 if you have pthread support])
  ], [
    AC_MSG_WARN([pthread not found, XHThreadPool will be disabled])
  ])

  dnl 替换共享库链接标志变量
  PHP_SUBST(XHCURL_SHARED_LIBADD)

  dnl 声明扩展的源文件列表
  dnl 模块划分（遵循单一职责原则）：
  dnl   xhcurl.c           - 模块入口 + XHCurl 全局管理器类
  dnl   xhcurl_buffer.c    - 响应缓冲区管理（init/write/read/free）
  dnl   xhcurl_header.c    - HTTP 头部链表操作（add/find/free/parse）
  dnl   xhcurl_cookie.c    - Cookie 链表操作（add/free）
  dnl   xhcurl_context.c   - 请求执行上下文 + curl 回调函数
  dnl   xhcurl_utils.c     - 辅助工具函数（SAPI 判断、字符串处理）
  dnl   xhcurl_response.c  - XHResponse 懒加载响应类
  dnl   xhcurl_request.c   - XHRequest 请求构建器类
  dnl   xhcurl_multi.c     - XHMulti 批量异步执行器类
  dnl   xhcurl_threadpool.c - XHThreadPool CLI 线程池类
  PHP_NEW_EXTENSION(xhcurl,
    xhcurl.c \
    xhcurl_buffer.c \
    xhcurl_header.c \
    xhcurl_cookie.c \
    xhcurl_context.c \
    xhcurl_utils.c \
    xhcurl_response.c \
    xhcurl_request.c \
    xhcurl_multi.c \
    xhcurl_threadpool.c,
    $ext_shared,, -DZEND_ENABLE_STATIC_TSRMLS_CACHE=1)

  dnl 定义扩展可用宏
  AC_DEFINE(HAVE_XHCURL, 1, [Have xhcurl extension])
fi
