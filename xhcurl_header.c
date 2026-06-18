/* +----------------------------------------------------------------------+
 * | XHCurl 扩展 - HTTP 头部链表操作实现                                  |
 * | 职责：头部节点的增删查改、链表释放、响应头解析                       |
 * | 从 xhcurl_buffer.c 拆分而来，遵循单一职责原则                        |
 * +----------------------------------------------------------------------+
 */

#include "xhcurl_priv.h"

/* +----------------------------------------------------------------------+
 * | 头部链表操作函数实现                                                  |
 * | 使用 PHP 内存管理函数（emalloc/efree），生命周期与请求一致           |
 * | 头部名称存储为小写，便于不区分大小写查找                             |
 * +----------------------------------------------------------------------+
 */

/**
 * 向头部链表追加一个头部
 * @param list  头部链表指针的指针（可能修改头指针）
 * @param name  头部名称（会被复制并转为小写存储）
 * @param value 头部值（会被复制存储）
 */
void xhcurl_header_add(xhcurl_header_t **list, const char *name, const char *value)
{
    /* 参数有效性检查 */
    if (list == NULL || name == NULL || value == NULL) {
        return;
    }

    /* 分配新节点内存，使用 ecalloc 确保零初始化 */
    xhcurl_header_t *node = (xhcurl_header_t *)ecalloc(1, sizeof(xhcurl_header_t));
    if (node == NULL) {
        return;
    }

    /* 复制头部名称并转为小写（便于不区分大小写查找） */
    node->name = estrdup(name);
    xhcurl_str_tolower(node->name, strlen(node->name));

    /* 复制头部值 */
    node->value = estrdup(value);

    /* 将新节点插入链表头部（O(1) 操作） */
    node->next = *list;
    *list = node;
}

/**
 * 在头部链表中查找指定名称的头部值
 * @param list 头部链表指针
 * @param name 要查找的头部名称（不区分大小写）
 * @return 头部值字符串指针，未找到返回 NULL
 */
const char *xhcurl_header_find(xhcurl_header_t *list, const char *name)
{
    /* 参数有效性检查 */
    if (name == NULL) {
        return NULL;
    }

    /* 创建小写化的查找键 */
    char *key = estrdup(name);
    xhcurl_str_tolower(key, strlen(key));

    /* 遍历链表查找匹配的头部 */
    xhcurl_header_t *current = list;
    while (current != NULL) {
        if (strcmp(current->name, key) == 0) {
            /* 找到匹配的头部 */
            efree(key);
            return current->value;
        }
        current = current->next;
    }

    /* 未找到 */
    efree(key);
    return NULL;
}

/**
 * 释放整个头部链表
 * @param list 头部链表指针
 */
void xhcurl_header_list_free(xhcurl_header_t *list)
{
    xhcurl_header_t *current = list;
    while (current != NULL) {
        /* 保存下一个节点指针 */
        xhcurl_header_t *next = current->next;
        /* 释放头部名称 */
        if (current->name != NULL) efree(current->name);
        /* 释放头部值 */
        if (current->value != NULL) efree(current->value);
        /* 释放节点本身 */
        efree(current);
        /* 移动到下一个节点 */
        current = next;
    }
}

/**
 * 解析 HTTP 响应头原始文本，填充到头部链表
 * @param header_buf 原始头部数据缓冲区
 * @param headers    输出头部链表指针的指针
 */
void xhcurl_parse_response_headers(xhcurl_buffer_t *header_buf, xhcurl_header_t **headers)
{
    /* 参数有效性检查 */
    if (header_buf == NULL || headers == NULL || header_buf->data == NULL) {
        return;
    }

    /* 释放已有的头部链表 */
    if (*headers != NULL) {
        xhcurl_header_list_free(*headers);
        *headers = NULL;
    }

    /* 逐行解析头部数据 */
    char *data = header_buf->data;
    size_t remaining = header_buf->size;
    char *line_start = data;

    while (remaining > 0) {
        /* 查找行结束标记 \r\n */
        char *line_end = memmem(line_start, remaining, "\r\n", 2);
        if (line_end == NULL) {
            /* 没有找到 \r\n，剩余部分作为最后一行 */
            line_end = line_start + remaining;
        }

        /* 计算当前行长度 */
        size_t line_len = line_end - line_start;

        /* 跳过空行和 HTTP 状态行（HTTP/1.x ...） */
        if (line_len > 0 && strncmp(line_start, "HTTP/", 5) != 0) {
            /* 查找冒号分隔符 */
            char *colon = memchr(line_start, ':', line_len);
            if (colon != NULL) {
                /* 计算名称长度 */
                size_t name_len = colon - line_start;
                /* 跳过冒号后的空白字符 */
                char *val_start = colon + 1;
                while (val_start < line_end && (*val_start == ' ' || *val_start == '\t')) {
                    val_start++;
                }
                /* 计算值长度 */
                size_t val_len = line_end - val_start;

                /* 复制名称和值（添加 null 终止符） */
                char *name = (char *)ecalloc(1, name_len + 1);
                memcpy(name, line_start, name_len);
                name[name_len] = '\0';

                char *value = (char *)ecalloc(1, val_len + 1);
                memcpy(value, val_start, val_len);
                value[val_len] = '\0';

                /* 添加到头部链表 */
                xhcurl_header_add(headers, name, value);

                /* 释放临时字符串（xhcurl_header_add 内部已复制） */
                efree(name);
                efree(value);
            }
        }

        /* 移动到下一行 */
        if (line_end == line_start + remaining) {
            /* 已到末尾 */
            break;
        }
        /* 跳过 \r\n */
        remaining -= (line_end - line_start) + 2;
        line_start = line_end + 2;
    }
}
