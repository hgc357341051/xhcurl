/* +----------------------------------------------------------------------+
 * | XHCurl 扩展 - Cookie 链表操作实现                                    |
 * | 职责：Cookie 节点的增删、链表释放                                    |
 * | 从 xhcurl_buffer.c 拆分而来，遵循单一职责原则                        |
 * +----------------------------------------------------------------------+
 */

#include "xhcurl_priv.h"

/* +----------------------------------------------------------------------+
 * | Cookie 链表操作函数实现                                               |
 * | 使用 PHP 内存管理函数（emalloc/efree），生命周期与请求一致           |
 * +----------------------------------------------------------------------+
 */

/**
 * 向 Cookie 链表追加一个 Cookie（同名 Cookie 会更新值，避免重复）
 * @param list   Cookie 链表指针的指针
 * @param name   Cookie 名称
 * @param value  Cookie 值
 * @param domain Cookie 域名（可为 NULL，存储为空字符串）
 * @param path   Cookie 路径（可为 NULL，存储为 "/"）
 */
void xhcurl_cookie_add(xhcurl_cookie_t **list, const char *name, const char *value,
                        const char *domain, const char *path)
{
    /* 参数有效性检查 */
    if (list == NULL || name == NULL || value == NULL) {
        return;
    }

    /* 去重检查：遍历链表查找同名 Cookie */
    xhcurl_cookie_t *current = *list;
    while (current != NULL) {
        if (current->name != NULL && strcmp(current->name, name) == 0) {
            /* 找到同名 Cookie，更新值（释放旧值，复制新值） */
            efree(current->value);
            current->value = estrdup(value);
            /* 如果提供了新的 domain，也更新 */
            if (domain != NULL && (current->domain == NULL || strcmp(current->domain, domain) != 0)) {
                efree(current->domain);
                current->domain = estrdup(domain);
            }
            /* 如果提供了新的 path，也更新 */
            if (path != NULL && (current->path == NULL || strcmp(current->path, path) != 0)) {
                efree(current->path);
                current->path = estrdup(path);
            }
            /* 同名 Cookie 已更新，直接返回 */
            return;
        }
        current = current->next;
    }

    /* 未找到同名 Cookie，分配新节点内存，使用 ecalloc 确保零初始化 */
    xhcurl_cookie_t *node = (xhcurl_cookie_t *)ecalloc(1, sizeof(xhcurl_cookie_t));
    if (node == NULL) {
        return;
    }

    /* 复制各字段：name、value 必填，domain/path 可选 */
    node->name = estrdup(name);
    node->value = estrdup(value);
    /* domain 为 NULL 时存储空字符串 */
    node->domain = (domain != NULL) ? estrdup(domain) : estrdup("");
    /* path 为 NULL 时存储默认路径 "/" */
    node->path = (path != NULL) ? estrdup(path) : estrdup("/");

    /* 将新节点插入链表头部（O(1) 操作） */
    node->next = *list;
    *list = node;
}

/**
 * 释放整个 Cookie 链表
 * @param list Cookie 链表指针
 */
void xhcurl_cookie_list_free(xhcurl_cookie_t *list)
{
    xhcurl_cookie_t *current = list;
    while (current != NULL) {
        /* 保存下一个节点指针 */
        xhcurl_cookie_t *next = current->next;
        /* 释放各字段 */
        if (current->name != NULL) efree(current->name);
        if (current->value != NULL) efree(current->value);
        if (current->domain != NULL) efree(current->domain);
        if (current->path != NULL) efree(current->path);
        /* 释放节点本身 */
        efree(current);
        /* 移动到下一个节点 */
        current = next;
    }
}
