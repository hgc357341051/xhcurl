/* +----------------------------------------------------------------------+
 * | XHCurl 扩展 - 辅助工具函数实现                                       |
 * | 职责：SAPI 模式判断、字符串处理、Content-Type 识别                   |
 * | 从 xhcurl_buffer.c 拆分而来，遵循单一职责原则                        |
 * +----------------------------------------------------------------------+
 */

#include "xhcurl_priv.h"

/* +----------------------------------------------------------------------+
 * | 辅助工具函数实现                                                      |
 * +----------------------------------------------------------------------+
 */

/**
 * 检查当前是否为 CLI 模式
 * 线程池功能仅在 CLI 模式下可用，FPM 模式下禁用
 * @return 1 为 CLI 模式，0 为其他模式
 */
zend_bool xhcurl_is_cli_mode(void)
{
    /* 通过 sapi_module.name 判断当前运行模式 */
    return (sapi_module.name != NULL && strcmp(sapi_module.name, "cli") == 0);
}

/**
 * 将字符串转为小写（原地修改）
 * 用于头部名称规范化，实现不区分大小写的头部查找
 * @param str 字符串指针
 * @param len 字符串长度
 */
void xhcurl_str_tolower(char *str, size_t len)
{
    if (str == NULL) return;
    /* 逐字符转换大写字母为小写 */
    for (size_t i = 0; i < len; i++) {
        if (str[i] >= 'A' && str[i] <= 'Z') {
            str[i] = str[i] - 'A' + 'a';
        }
    }
}

/**
 * 判断 Content-Type 是否为 JSON 类型
 * 检查 Content-Type 字符串是否包含 "json" 子串（不区分大小写）
 * @param content_type Content-Type 头部值
 * @return 1 为 JSON 类型，0 为非 JSON 类型
 */
zend_bool xhcurl_is_json_content_type(const char *content_type)
{
    if (content_type == NULL) {
        return 0;
    }
    /* 使用 strcasestr 进行不区分大小写的子串查找 */
    return (strcasestr(content_type, "json") != NULL) ? 1 : 0;
}
