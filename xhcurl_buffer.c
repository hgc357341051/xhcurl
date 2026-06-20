/* +----------------------------------------------------------------------+
 * | XHCurl 扩展 - 响应缓冲区管理实现                                     |
 * | 职责：缓冲区的初始化、写入、读取、释放                                |
 * | 使用 malloc/free 管理内存，不计入 PHP memory_limit                   |
 * | 通过 max_size 限制防止内存溢出                                        |
 * | 头部链表/Cookie/上下文/回调/辅助函数已拆分到各自模块                  |
 * +----------------------------------------------------------------------+
 */

#include "xhcurl_priv.h"

/* +----------------------------------------------------------------------+
 * | 缓冲区操作函数实现                                                    |
 * | 使用 malloc/free 管理内存，不计入 PHP memory_limit                   |
 * | 通过 max_size 限制防止内存溢出                                        |
 * +----------------------------------------------------------------------+
 */

/**
 * 初始化缓冲区
 * @param buf         缓冲区指针
 * @param initial_cap 初始分配容量（字节）
 * @param max_sz      最大允许大小（字节），0 表示无限制
 * @return 0 成功，-1 分配内存失败
 */
int xhcurl_buffer_init(xhcurl_buffer_t *buf, size_t initial_cap, size_t max_sz)
{
    /* 参数有效性检查 */
    if (buf == NULL) {
        return -1;
    }

    /* 分配初始容量内存，使用 malloc 而非 emalloc 避免计入 PHP memory_limit */
    buf->data = (char *)malloc(initial_cap);
    if (buf->data == NULL) {
        /* 内存分配失败 */
        return -1;
    }

    /* 初始化缓冲区各字段 */
    buf->size = 0;              /* 当前已写入数据量为 0 */
    buf->capacity = initial_cap;/* 已分配容量 */
    buf->max_size = max_sz;     /* 最大允许大小 */

    return 0;
}

/**
 * 向缓冲区追加写入数据
 * @param buf  缓冲区指针
 * @param data 待写入的数据指针
 * @param len  待写入的数据长度
 * @return 0 成功，-1 超过最大限制或内存分配失败
 */
int xhcurl_buffer_write(xhcurl_buffer_t *buf, const char *data, size_t len)
{
    /* 参数有效性检查 */
    if (buf == NULL || data == NULL || len == 0) {
        return 0;  /* 空写入视为成功 */
    }

    /* +--------------------------------------------------------------+
     * | 整数溢出检查：buf->size + len 可能超过 SIZE_MAX              |
     * | size_t 是无符号类型，溢出时会回绕到很小的值，                |
     * | 导致后续的容量检查和内存分配出现严重错误。                    |
     * | 检查方式：如果 size + len < size，说明发生了回绕（溢出）。    |
     * +--------------------------------------------------------------+
     */
    if (buf->size + len < buf->size) {
        /* 整数溢出，无法容纳这么多数据 */
        return -1;
    }

    /* +--------------------------------------------------------------+
     * | 检查写入后是否超过最大允许大小                                |
     * | 修复：原实现整块拒绝超过 max_size 的写入，导致已接收的有效   |
     * | 数据被丢弃。新实现先写入 max_size 以内的部分数据，再返回错误，|
     * | 确保不丢失已到达的有效数据。                                  |
     * +--------------------------------------------------------------+
     */
    if (buf->max_size > 0 && (buf->size + len) > buf->max_size) {
        /* 计算还能写入多少字节（不超过 max_size） */
        size_t remaining = buf->max_size - buf->size;
        if (remaining > 0) {
            /* 还有剩余空间，先写入部分数据 */
            /* 检查当前容量是否足够 */
            if (buf->capacity < buf->max_size) {
                /* 扩容到 max_size（不再翻倍，因为已到上限） */
                char *new_data = (char *)realloc(buf->data, buf->max_size);
                if (new_data != NULL) {
                    buf->data = new_data;
                    buf->capacity = buf->max_size;
                }
            }
            /* 写入部分数据（不超过剩余容量） */
            if (buf->size + remaining <= buf->capacity) {
                memcpy(buf->data + buf->size, data, remaining);
                buf->size += remaining;
            }
        }
        /* 超过最大限制，返回 -1 通知调用方（curl 回调会中止传输） */
        return -1;
    }

    /* 检查当前容量是否足够容纳新数据 */
    if ((buf->size + len) > buf->capacity) {
        /* 容量不足，需要扩容 */
        /* 计算新容量：至少为当前需要的 2 倍，采用指数增长策略减少频繁扩容 */
        size_t new_capacity = buf->capacity;
        /* 防止整数溢出：new_capacity * 2 可能超过 SIZE_MAX */
        size_t needed = buf->size + len;
        /* 如果当前容量为 0，从合理的初始值开始 */
        if (new_capacity == 0) {
            new_capacity = XHCURL_BUFFER_INIT_CAPACITY;
        }
        while (new_capacity < needed) {
            /* 检查翻倍是否会溢出 */
            if (new_capacity > SIZE_MAX / 2) {
                /* 无法再翻倍，直接使用所需大小 */
                new_capacity = needed;
                break;
            }
            new_capacity *= 2;
        }

        /* 检查新容量是否超过最大限制 */
        if (buf->max_size > 0 && new_capacity > buf->max_size) {
            new_capacity = buf->max_size;
        }

        /* 重新分配内存 */
        char *new_data = (char *)realloc(buf->data, new_capacity);
        if (new_data == NULL) {
            /* 内存分配失败 */
            return -1;
        }

        /* 更新缓冲区指针和容量 */
        buf->data = new_data;
        buf->capacity = new_capacity;
    }

    /* 将新数据追加到缓冲区末尾 */
    memcpy(buf->data + buf->size, data, len);
    /* 更新已写入数据大小 */
    buf->size += len;

    return 0;
}

/**
 * 释放缓冲区资源
 * @param buf 缓冲区指针
 */
void xhcurl_buffer_free(xhcurl_buffer_t *buf)
{
    if (buf == NULL) {
        return;
    }

    /* 释放数据内存（使用 free 对应 malloc/realloc） */
    if (buf->data != NULL) {
        free(buf->data);
        buf->data = NULL;
    }

    /* 重置缓冲区状态 */
    buf->size = 0;
    buf->capacity = 0;
    buf->max_size = 0;
}
