/// 类型擦除层 - L6 和 L7 的边界
/// 
/// 这个模块定义了完全类型擦除的流抽象，使得 L7 代理层
/// 无需关心底层是 TLS、WebSocket 还是 Yamux

use tokio::io::{AsyncRead, AsyncWrite};
use std::pin::Pin;

/// 统一的 IO trait - 工程内唯一认可的 IO 抽象
/// 
/// ⚠️ 关键原则：
/// - 只使用 tokio::io traits
/// - yamux stream 需要通过 tokio_util::compat 转换
pub trait Io: AsyncRead + AsyncWrite + Unpin + Send {}

/// 自动为所有满足条件的类型实现 Io
impl<T> Io for T where T: AsyncRead + AsyncWrite + Unpin + Send {}

/// 类型擦除的流
/// 
/// 这是 L7 层唯一可见的类型。它隐藏了所有底层实现细节：
/// - 不知道是 TCP 还是 TLS
/// - 不知道是 WebSocket 还是 Yamux  
/// - 只知道能读能写
/// 
/// # 为什么需要 Pin<Box<...>>？
/// 
/// - `Pin`: 保证内存位置不变（async/await 需要）
/// - `Box`: 堆分配，统一大小（trait object 需要）
/// - `dyn`: 动态分发（类型擦除的核心）
pub type OpaqueStream = Pin<Box<dyn Io + 'static>>;

/// 连接上下文
/// 
/// 携带类型擦除的流和元数据，但不暴露具体类型
pub struct ConnectionContext {
    /// 类型擦除的流（核心）
    pub stream: OpaqueStream,
    
    /// 目标地址（用于日志和统计）
    pub target: String,
    
    /// 是否使用了 TLS（元数据）
    pub is_secure: bool,
    
    /// 是否使用了 ECH（元数据）
    pub is_ech: bool,
    
    /// 是否使用了 Yamux（元数据）
    pub is_yamux: bool,
}

impl std::fmt::Debug for ConnectionContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConnectionContext")
            .field("target", &self.target)
            .field("is_secure", &self.is_secure)
            .field("is_ech", &self.is_ech)
            .field("is_yamux", &self.is_yamux)
            .field("stream", &"<opaque>")
            .finish()
    }
}

impl ConnectionContext {
    /// 从具体类型创建连接上下文
    /// 
    /// 这是类型擦除发生的地方！
    /// 
    /// # 类型约束
    /// 
    /// `S` 必须实现：
    /// - `AsyncRead + AsyncWrite`: 基本 IO 能力
    /// - `Send`: 可以跨线程传递（Tokio 需要）
    /// - `Unpin`: 可以安全移动（简化开发）
    /// - `'static`: 拥有所有权（不是借用）
    /// 
    /// # 示例
    /// 
    /// ```rust,ignore
    /// // 从 TlsStream 创建
    /// let tls_stream: TlsStream<TcpStream> = ...;
    /// let ctx = ConnectionContext::new(
    ///     tls_stream,
    ///     "example.com:443".to_string(),
    ///     true,  // is_secure
    ///     true,  // is_ech
    ///     false, // is_yamux
    /// );
    /// 
    /// // 从 YamuxStream 创建
    /// let yamux_stream: yamux::Stream = ...;
    /// let ctx = ConnectionContext::new(
    ///     yamux_stream,
    ///     "example.com:443".to_string(),
    ///     true,  // is_secure
    ///     true,  // is_ech
    ///     true,  // is_yamux
    /// );
    /// ```
    pub fn new<S>(
        stream: S,
        target: String,
        is_secure: bool,
        is_ech: bool,
        is_yamux: bool,
    ) -> Self
    where
        S: AsyncRead + AsyncWrite + Send + Unpin + 'static,
    {
        Self {
            stream: Box::pin(stream),
            target,
            is_secure,
            is_ech,
            is_yamux,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;

    #[tokio::test]
    async fn test_type_erasure() {
        // 模拟：从具体类型（TcpStream）创建 OpaqueStream
        // 注意：这里只是演示类型擦除，实际使用中会是 TlsStream
        
        // 这个函数接受任何实现了 AsyncRead + AsyncWrite 的类型
        async fn use_opaque_stream(mut stream: OpaqueStream) -> std::io::Result<()> {
            // 这里无法调用 TcpStream 的特定方法
            // 只能使用 AsyncRead/AsyncWrite 的方法
            let mut buf = [0u8; 1024];
            let _n = stream.read(&mut buf).await?;
            stream.write_all(b"test").await?;
            Ok(())
        }

        // 类型擦除发生在这里
        // let tcp = TcpStream::connect("127.0.0.1:8080").await.unwrap();
        // let opaque: OpaqueStream = Box::pin(tcp);
        // use_opaque_stream(opaque).await.unwrap();
    }
}
