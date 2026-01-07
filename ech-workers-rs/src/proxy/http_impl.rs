/// HTTP CONNECT 协议实现

use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tracing::{debug, trace};

use crate::error::{Error, Result};
use super::socks5_impl::TargetAddr;

/// 解析 HTTP CONNECT 请求
pub async fn parse_connect_request<S>(stream: &mut S) -> Result<TargetAddr>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    
    // 读取请求行
    reader.read_line(&mut line).await?;
    trace!("HTTP request line: {}", line.trim());
    
    // 解析 CONNECT host:port HTTP/1.1
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 2 {
        return Err(Error::Protocol("Invalid HTTP request".into()));
    }
    
    if parts[0] != "CONNECT" {
        return Err(Error::Protocol(format!("Unsupported HTTP method: {}", parts[0])));
    }
    
    let host_port = parts[1];
    
    // 解析 host:port
    let (host, port) = if let Some(pos) = host_port.rfind(':') {
        let host = &host_port[..pos];
        let port = host_port[pos + 1..]
            .parse::<u16>()
            .map_err(|_| Error::Protocol("Invalid port".into()))?;
        (host.to_string(), port)
    } else {
        return Err(Error::Protocol("Missing port in CONNECT request".into()));
    };
    
    // 读取剩余的 headers（直到空行）
    loop {
        line.clear();
        reader.read_line(&mut line).await?;
        if line.trim().is_empty() {
            break;
        }
        trace!("HTTP header: {}", line.trim());
    }
    
    debug!("HTTP CONNECT target: {}:{}", host, port);
    
    // HTTP CONNECT 总是使用域名
    Ok(TargetAddr::Domain(host, port))
}

/// 发送 HTTP 200 响应
pub async fn send_connect_response<W>(writer: &mut W) -> Result<()>
where
    W: AsyncWrite + Unpin,
{
    writer.write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n").await?;
    writer.flush().await?;
    debug!("Sent HTTP 200 Connection Established");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::duplex;

    #[tokio::test]
    async fn test_parse_connect_request() {
        let request = b"CONNECT example.com:443 HTTP/1.1\r\n\
                        Host: example.com:443\r\n\
                        User-Agent: curl/7.68.0\r\n\
                        \r\n";
        
        let (mut client, mut server) = duplex(1024);
        
        // 写入请求
        tokio::spawn(async move {
            use tokio::io::AsyncWriteExt;
            client.write_all(request).await.unwrap();
        });
        
        let target = parse_connect_request(&mut server).await.unwrap();
        
        match target {
            TargetAddr::Domain(host, port) => {
                assert_eq!(host, "example.com");
                assert_eq!(port, 443);
            }
            _ => panic!("Expected domain"),
        }
    }

    #[tokio::test]
    async fn test_parse_connect_with_ipv6() {
        let request = b"CONNECT [2001:db8::1]:8080 HTTP/1.1\r\n\r\n";
        
        let (mut client, mut server) = duplex(1024);
        
        tokio::spawn(async move {
            use tokio::io::AsyncWriteExt;
            client.write_all(request).await.unwrap();
        });
        
        let target = parse_connect_request(&mut server).await.unwrap();
        
        match target {
            TargetAddr::Domain(host, port) => {
                assert_eq!(host, "[2001:db8::1]");
                assert_eq!(port, 8080);
            }
            _ => panic!("Expected domain"),
        }
    }
}
