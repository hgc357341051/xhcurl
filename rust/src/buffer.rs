// +----------------------------------------------------------------------+
// | XHCurl 扩展 - 响应缓冲区实现                                         |
// | 使用 Rust 所有权模型管理内存，编译期保证无 double-free                |
// | 使用 Vec<u8> 作为底层存储，自动扩容，无需手动管理容量                |
// | 通过 max_size 限制防止内存溢出（对应 C 版本的 max_response_size）    |
// +----------------------------------------------------------------------+

use crate::error::{XhCurlError, XhCurlResult};

/// 响应缓冲区结构体
/// 存储响应体数据，支持按需分段读取，避免一次性加载到 PHP 内存
///
/// # 线程安全
/// 此结构体本身不是线程安全的（未实现 Sync），
/// 但通过 Arc<Mutex<>> 可以在多线程间共享。
/// 响应体数据在请求完成后转移到 XHResponse，不再被多线程访问。
#[derive(Debug, Clone)]
pub struct ResponseBuffer {
    /// 缓冲区数据（使用 Vec 自动管理内存）
    /// 对应 C 版本的 char *data
    data: Vec<u8>,

    /// 最大允许大小（字节），0 表示无限制
    /// 对应 C 版本的 max_size
    /// 超过此大小会返回错误，防止内存溢出
    max_size: usize,
}

impl ResponseBuffer {
    /// 创建新的响应缓冲区
    ///
    /// # 参数
    /// - `initial_capacity`: 初始容量（预分配内存，减少扩容次数）
    /// - `max_size`: 最大允许大小（0 = 无限制）
    ///
    /// # 返回
    /// 新的缓冲区实例
    ///
    /// # 示例
    /// ```
    /// let buf = ResponseBuffer::new(4096, 10 * 1024 * 1024);
    /// ```
    pub fn new(initial_capacity: usize, max_size: usize) -> Self {
        // 预分配容量，避免频繁扩容
        // Vec::with_capacity 不会初始化元素，仅分配内存
        let data = Vec::with_capacity(initial_capacity);

        Self {
            data,
            max_size,
        }
    }

    /// 向缓冲区追加写入数据
    ///
    /// # 参数
    /// - `chunk`: 待写入的数据切片
    ///
    /// # 返回
    /// - `Ok(())`: 写入成功
    /// - `Err(XhCurlError::Memory)`: 超过最大限制
    ///
    /// # 错误处理
    /// 如果写入后总大小超过 max_size，会先写入部分数据（不超过限制），
    /// 然后返回错误。这确保已到达的有效数据不会丢失。
    pub fn write(&mut self, chunk: &[u8]) -> XhCurlResult<()> {
        // 空写入视为成功（对应 C 版本的 len == 0 检查）
        if chunk.is_empty() {
            return Ok(());
        }

        // 检查写入后是否超过最大限制
        if self.max_size > 0 {
            // 计算写入后的总大小（使用 checked_add 防止整数溢出）
            let new_size = self.data.len().checked_add(chunk.len())
                .ok_or_else(|| XhCurlError::Memory("缓冲区大小溢出".to_string()))?;

            if new_size > self.max_size {
                // 超过限制：先写入部分数据（不超过 max_size 的部分）
                let remaining = self.max_size - self.data.len();
                if remaining > 0 {
                    // 还有剩余空间，写入部分数据
                    self.data.extend_from_slice(&chunk[..remaining]);
                }
                // 返回错误通知调用方（对应 C 版本的 return -1）
                return Err(XhCurlError::Memory(format!(
                    "响应体超过最大限制 {} 字节", self.max_size
                )));
            }
        }

        // 容量检查和扩容由 Vec 自动处理，无需手动管理
        // Vec 在容量不足时会自动按 2 倍策略扩容
        self.data.extend_from_slice(chunk);
        Ok(())
    }

    /// 获取缓冲区数据切片（零拷贝读取）
    ///
    /// # 返回
    /// 数据的只读切片引用，生命周期与缓冲区相同
    pub fn as_slice(&self) -> &[u8] {
        &self.data
    }

    /// 获取缓冲区当前数据大小
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// 检查缓冲区是否为空
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// 获取缓冲区当前容量（已分配但可能未使用的空间）
    pub fn capacity(&self) -> usize {
        self.data.capacity()
    }

    /// 分段读取数据（对应 PHP 端的 getBodyChunk 方法）
    ///
    /// # 参数
    /// - `offset`: 读取起始偏移量
    /// - `length`: 读取长度
    ///
    /// # 返回
    /// 指定范围的数据切片引用
    /// 如果 offset 超出范围，返回空切片
    pub fn chunk(&self, offset: usize, length: usize) -> &[u8] {
        // 检查偏移量是否超出范围
        if offset >= self.data.len() {
            return &[];
        }

        // 计算实际可读取的长度（防止越界）
        let available = self.data.len() - offset;
        let read_len = length.min(available);

        // 返回指定范围的切片（零拷贝）
        &self.data[offset..offset + read_len]
    }

    /// 消耗缓冲区，返回内部数据（转移所有权，避免拷贝）
    ///
    /// # 返回
    /// Vec<u8> 数据，调用方获得所有权
    pub fn into_vec(self) -> Vec<u8> {
        self.data
    }

    /// 清空缓冲区数据（保留容量）
    pub fn clear(&mut self) {
        self.data.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试缓冲区基本写入和读取
    #[test]
    fn test_buffer_write_read() {
        // 创建初始容量 1024，无大小限制的缓冲区
        let mut buf = ResponseBuffer::new(1024, 0);

        // 写入数据
        buf.write(b"hello").unwrap();
        buf.write(b" world").unwrap();

        // 验证数据
        assert_eq!(buf.len(), 11);
        assert_eq!(buf.as_slice(), b"hello world");
    }

    /// 测试缓冲区大小限制
    #[test]
    fn test_buffer_max_size() {
        // 创建最大 10 字节的缓冲区
        let mut buf = ResponseBuffer::new(16, 10);

        // 写入 5 字节（成功）
        buf.write(b"hello").unwrap();
        assert_eq!(buf.len(), 5);

        // 写入 8 字节（超过限制，但先写入 5 字节部分数据）
        let result = buf.write(b" world!!!");
        assert!(result.is_err());
        // 验证部分数据已写入
        assert_eq!(buf.len(), 10);
        assert_eq!(buf.as_slice(), b"hello worl");
    }

    /// 测试分段读取
    #[test]
    fn test_buffer_chunk() {
        let mut buf = ResponseBuffer::new(1024, 0);
        buf.write(b"hello world").unwrap();

        // 从偏移 0 读取 5 字节
        assert_eq!(buf.chunk(0, 5), b"hello");

        // 从偏移 6 读取 5 字节
        assert_eq!(buf.chunk(6, 5), b"world");

        // 偏移超出范围返回空切片
        assert_eq!(buf.chunk(100, 5), b"");

        // 读取长度超过可用范围，返回实际可用长度
        assert_eq!(buf.chunk(6, 100), b"world");
    }

    /// 测试空写入
    #[test]
    fn test_buffer_empty_write() {
        let mut buf = ResponseBuffer::new(1024, 0);
        buf.write(b"").unwrap();
        assert!(buf.is_empty());
    }

    /// 测试转移所有权
    #[test]
    fn test_buffer_into_vec() {
        let mut buf = ResponseBuffer::new(1024, 0);
        buf.write(b"test data").unwrap();

        let data = buf.into_vec();
        assert_eq!(data, b"test data");
    }
}
