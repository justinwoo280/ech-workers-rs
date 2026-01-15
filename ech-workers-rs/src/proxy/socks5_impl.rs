/// SOCKS5 协议实现
/// 
/// 关键点：
/// 1. 域名透传（不做本地 DNS 解析）
/// 2. 正确的字节序（Big-Endian）
/// 3. 错误处理

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tracing::{debug, trace};
use std::net::{Ipv4Addr, Ipv6Addr};

use crate::error::{Error, Result};

/// SOCKS5 版本号
const SOCKS5_VERSION: u8 = 0x05;

/// 认证方法
#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum AuthMethod {
    NoAuth = 0x00,
    GSSAPI = 0x01,
    UsernamePassword = 0x02,
    NoAcceptable = 0xFF,
}

/// SOCKS5 命令
#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum Command {
    Connect = 0x01,
    Bind = 0x02,
    UdpAssociate = 0x03,
}

/// 地址类型
#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum AddressType {
    IPv4 = 0x01,
    Domain = 0x03,
    IPv6 = 0x04,
}

/// 目标地址
#[derive(Debug, Clone)]
pub enum TargetAddr {
    /// IPv4 地址
    Ipv4(Ipv4Addr, u16),
    /// 域名（推荐，用于 ECH）
    Domain(String, u16),
    /// IPv6 地址
    Ipv6(Ipv6Addr, u16),
}

impl TargetAddr {
    /// 序列化为 SOCKS5 格式（Big-Endian）
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        
        match self {
            TargetAddr::Ipv4(ip, port) => {
                buf.push(AddressType::IPv4 as u8);
                buf.extend_from_slice(&ip.octets());
                buf.extend_from_slice(&port.to_be_bytes());
            }
            TargetAddr::Domain(domain, port) => {
                buf.push(AddressType::Domain as u8);
                buf.push(domain.len() as u8);
                buf.extend_from_slice(domain.as_bytes());
                buf.extend_from_slice(&port.to_be_bytes());
            }
            TargetAddr::Ipv6(ip, port) => {
                buf.push(AddressType::IPv6 as u8);
                buf.extend_from_slice(&ip.octets());
                buf.extend_from_slice(&port.to_be_bytes());
            }
        }
        
        buf
    }
    
    /// 从字节流读取（Big-Endian）
    pub async fn from_reader<R: AsyncRead + Unpin>(reader: &mut R) -> Result<Self> {
        let atyp = reader.read_u8().await?;
        
        match atyp {
            0x01 => {
                // IPv4
                let mut ip_bytes = [0u8; 4];
                reader.read_exact(&mut ip_bytes).await?;
                let ip = Ipv4Addr::from(ip_bytes);
                let port = reader.read_u16().await?; // Big-Endian
                Ok(TargetAddr::Ipv4(ip, port))
            }
            0x03 => {
                // 域名（推荐）
                let len = reader.read_u8().await?;
                let mut domain_bytes = vec![0u8; len as usize];
                reader.read_exact(&mut domain_bytes).await?;
                let domain = String::from_utf8(domain_bytes)
                    .map_err(|_| Error::Protocol("Invalid domain name".into()))?;
                let port = reader.read_u16().await?; // Big-Endian
                Ok(TargetAddr::Domain(domain, port))
            }
            0x04 => {
                // IPv6
                let mut ip_bytes = [0u8; 16];
                reader.read_exact(&mut ip_bytes).await?;
                let ip = Ipv6Addr::from(ip_bytes);
                let port = reader.read_u16().await?; // Big-Endian
                Ok(TargetAddr::Ipv6(ip, port))
            }
            _ => Err(Error::Protocol(format!("Unknown address type: {}", atyp))),
        }
    }
    
    /// 获取显示字符串
    pub fn display(&self) -> String {
        match self {
            TargetAddr::Ipv4(ip, port) => format!("{}:{}", ip, port),
            TargetAddr::Domain(domain, port) => format!("{}:{}", domain, port),
            TargetAddr::Ipv6(ip, port) => format!("[{}]:{}", ip, port),
        }
    }
}

/// SOCKS5 握手
pub async fn socks5_handshake<S>(stream: &mut S) -> Result<TargetAddr>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    // 1. 读取客户端问候
    let version = stream.read_u8().await?;
    if version != SOCKS5_VERSION {
        return Err(Error::Protocol(format!("Invalid SOCKS version: {}", version)));
    }
    
    let nmethods = stream.read_u8().await?;
    let mut methods = vec![0u8; nmethods as usize];
    stream.read_exact(&mut methods).await?;
    
    trace!("SOCKS5 client methods: {:?}", methods);
    
    // 2. 选择认证方法（无认证）
    stream.write_all(&[SOCKS5_VERSION, AuthMethod::NoAuth as u8]).await?;
    stream.flush().await?;
    
    debug!("SOCKS5 auth: NoAuth");
    
    // 3. 读取客户端请求
    let version = stream.read_u8().await?;
    if version != SOCKS5_VERSION {
        return Err(Error::Protocol(format!("Invalid SOCKS version: {}", version)));
    }
    
    let cmd = stream.read_u8().await?;
    if cmd != Command::Connect as u8 {
        // 只支持 CONNECT
        stream.write_all(&[SOCKS5_VERSION, 0x07, 0x00, 0x01, 0, 0, 0, 0, 0, 0]).await?;
        return Err(Error::Protocol(format!("Unsupported command: {}", cmd)));
    }
    
    let _rsv = stream.read_u8().await?; // Reserved
    
    // 4. 读取目标地址
    let target = TargetAddr::from_reader(stream).await?;
    
    debug!("SOCKS5 target: {}", target.display());
    
    // 5. 发送成功响应
    // VER REP RSV ATYP BND.ADDR BND.PORT
    stream.write_all(&[
        SOCKS5_VERSION,
        0x00, // Success
        0x00, // Reserved
        0x01, // IPv4
        0, 0, 0, 0, // 0.0.0.0
        0, 0, // Port 0
    ]).await?;
    stream.flush().await?;
    
    Ok(target)
}

/// 发送目标地址到远程服务器
pub async fn send_target<W>(writer: &mut W, target: &TargetAddr) -> Result<()>
where
    W: AsyncWrite + Unpin,
{
    let bytes = target.to_bytes();
    trace!("Sending target address: {} bytes: {:02x?}", bytes.len(), bytes);
    writer.write_all(&bytes).await?;
    writer.flush().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::BufReader;

    #[tokio::test]
    async fn test_target_addr_domain() {
        let target = TargetAddr::Domain("example.com".to_string(), 443);
        let bytes = target.to_bytes();
        
        // 验证格式
        assert_eq!(bytes[0], 0x03); // Domain type
        assert_eq!(bytes[1], 11); // Length
        assert_eq!(&bytes[2..13], b"example.com");
        assert_eq!(u16::from_be_bytes([bytes[13], bytes[14]]), 443);
        
        // 验证往返
        let mut reader = BufReader::new(&bytes[..]);
        let parsed = TargetAddr::from_reader(&mut reader).await.unwrap();
        
        match parsed {
            TargetAddr::Domain(domain, port) => {
                assert_eq!(domain, "example.com");
                assert_eq!(port, 443);
            }
            _ => panic!("Expected domain"),
        }
    }

    #[tokio::test]
    async fn test_target_addr_ipv4() {
        let target = TargetAddr::Ipv4(Ipv4Addr::new(1, 2, 3, 4), 8080);
        let bytes = target.to_bytes();
        
        assert_eq!(bytes[0], 0x01); // IPv4 type
        assert_eq!(&bytes[1..5], &[1, 2, 3, 4]);
        assert_eq!(u16::from_be_bytes([bytes[5], bytes[6]]), 8080);
    }

    #[test]
    fn test_endianness() {
        let port: u16 = 443;
        let be_bytes = port.to_be_bytes();
        assert_eq!(be_bytes, [0x01, 0xBB]); // 443 = 0x01BB in big-endian
    }
}
