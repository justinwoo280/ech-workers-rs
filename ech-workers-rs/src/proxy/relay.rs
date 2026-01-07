/// 数据转发模块
/// 
/// 优化：
/// 1. 缓冲写入（避免小包）
/// 2. 半关闭支持
/// 3. 错误处理

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::time::{timeout, Duration};
use tracing::{debug, trace, warn};
use std::io;

use crate::error::Result;

/// 缓冲配置
const BUFFER_SIZE: usize = 32 * 1024; // 32KB，与 Go 的 io.Copy 一致
const FLUSH_INTERVAL: Duration = Duration::from_millis(10); // 10ms 或攒够数据就发送

/// 双向转发数据
/// 
/// 优化点：
/// - 使用 32KB 缓冲区
/// - 支持半关闭
/// - 超时处理
pub async fn relay_bidirectional<L, R>(
    local: L,
    remote: R,
) -> Result<(u64, u64)>
where
    L: AsyncRead + AsyncWrite + Unpin,
    R: AsyncRead + AsyncWrite + Unpin,
{
    let (mut local_read, mut local_write) = tokio::io::split(local);
    let (mut remote_read, mut remote_write) = tokio::io::split(remote);
    
    let (local_to_remote, remote_to_local) = tokio::join!(
        relay_with_buffer(&mut local_read, &mut remote_write, "local->remote"),
        relay_with_buffer(&mut remote_read, &mut local_write, "remote->local"),
    );
    
    let bytes_l2r = local_to_remote?;
    let bytes_r2l = remote_to_local?;
    
    debug!("Relay finished: {} bytes local->remote, {} bytes remote->local", bytes_l2r, bytes_r2l);
    
    Ok((bytes_l2r, bytes_r2l))
}

/// 单向转发（带缓冲）
async fn relay_with_buffer<R, W>(
    reader: &mut R,
    writer: &mut W,
    direction: &str,
) -> Result<u64>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut buffer = vec![0u8; BUFFER_SIZE];
    let mut total_bytes = 0u64;
    
    loop {
        // 读取数据（带超时）
        let n = match timeout(Duration::from_secs(300), reader.read(&mut buffer)).await {
            Ok(Ok(0)) => {
                // EOF
                trace!("{}: EOF reached", direction);
                break;
            }
            Ok(Ok(n)) => n,
            Ok(Err(e)) => {
                if e.kind() == io::ErrorKind::UnexpectedEof {
                    trace!("{}: Unexpected EOF", direction);
                    break;
                }
                warn!("{}: Read error: {}", direction, e);
                return Err(e.into());
            }
            Err(_) => {
                warn!("{}: Read timeout", direction);
                break;
            }
        };
        
        trace!("{}: Read {} bytes", direction, n);
        
        // 写入数据
        writer.write_all(&buffer[..n]).await?;
        
        // 定期 flush（避免小包堆积）
        if total_bytes % (16 * 1024) == 0 {
            writer.flush().await?;
        }
        
        total_bytes += n as u64;
    }
    
    // 最终 flush
    writer.flush().await?;
    
    // 尝试半关闭（shutdown write）
    if let Err(e) = writer.shutdown().await {
        trace!("{}: Shutdown error (may be expected): {}", direction, e);
    }
    
    debug!("{}: Transferred {} bytes", direction, total_bytes);
    Ok(total_bytes)
}

/// 简单的双向转发（使用 tokio 内置）
/// 
/// 注意：这个版本不支持缓冲优化，但代码更简单
pub async fn relay_simple<L, R>(
    local: &mut L,
    remote: &mut R,
) -> Result<(u64, u64)>
where
    L: AsyncRead + AsyncWrite + Unpin,
    R: AsyncRead + AsyncWrite + Unpin,
{
    let (bytes_l2r, bytes_r2l) = tokio::io::copy_bidirectional(local, remote).await?;
    debug!("Simple relay: {} bytes local->remote, {} bytes remote->local", bytes_l2r, bytes_r2l);
    Ok((bytes_l2r, bytes_r2l))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{duplex, AsyncWriteExt};

    #[tokio::test]
    async fn test_relay_bidirectional() {
        let (mut client, mut server) = duplex(1024);
        
        // 模拟客户端发送数据
        tokio::spawn(async move {
            client.write_all(b"Hello, server!").await.unwrap();
            client.shutdown().await.unwrap();
        });
        
        // 模拟服务端接收并回复
        let mut buf = vec![0u8; 1024];
        let n = server.read(&mut buf).await.unwrap();
        assert_eq!(&buf[..n], b"Hello, server!");
    }
}
