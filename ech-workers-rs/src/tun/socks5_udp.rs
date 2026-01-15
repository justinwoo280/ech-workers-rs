//! SOCKS5 UDP ASSOCIATE 实现
//! 
//! 标准 SOCKS5 UDP 代理协议，用于 TUN 模式的 UDP 流量转发
//! 
//! 协议流程：
//! 1. 建立 TCP 控制连接
//! 2. 发送 UDP ASSOCIATE 请求
//! 3. 服务端返回 UDP relay 地址
//! 4. 通过 UDP relay 地址发送/接收 UDP 数据
//! 
//! UDP 数据帧格式 (RFC 1928):
//! +----+------+------+----------+----------+----------+
//! |RSV | FRAG | ATYP | DST.ADDR | DST.PORT |   DATA   |
//! +----+------+------+----------+----------+----------+
//! | 2  |  1   |  1   | Variable |    2     | Variable |
//! +----+------+------+----------+----------+----------+

use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::error::{Error, Result};

/// SOCKS5 版本
const SOCKS5_VERSION: u8 = 0x05;

/// SOCKS5 命令
const CMD_UDP_ASSOCIATE: u8 = 0x03;

/// 地址类型
const ATYP_IPV4: u8 = 0x01;
const ATYP_DOMAIN: u8 = 0x03;
const ATYP_IPV6: u8 = 0x04;

/// SOCKS5 UDP 数据帧
#[derive(Debug, Clone)]
pub struct Socks5UdpFrame {
    /// 分片号 (0 = 不分片)
    pub frag: u8,
    /// 目标地址类型
    pub atyp: u8,
    /// 目标地址
    pub dst_addr: Vec<u8>,
    /// 目标端口
    pub dst_port: u16,
    /// 数据
    pub data: Vec<u8>,
}

impl Socks5UdpFrame {
    /// 创建 IPv4 目标的 UDP 帧
    pub fn new_ipv4(dst_ip: Ipv4Addr, dst_port: u16, data: Vec<u8>) -> Self {
        Self {
            frag: 0,
            atyp: ATYP_IPV4,
            dst_addr: dst_ip.octets().to_vec(),
            dst_port,
            data,
        }
    }
    
    /// 创建域名目标的 UDP 帧
    pub fn new_domain(domain: &str, dst_port: u16, data: Vec<u8>) -> Self {
        let mut dst_addr = Vec::with_capacity(1 + domain.len());
        dst_addr.push(domain.len() as u8);
        dst_addr.extend_from_slice(domain.as_bytes());
        
        Self {
            frag: 0,
            atyp: ATYP_DOMAIN,
            dst_addr,
            dst_port,
            data,
        }
    }
    
    /// 序列化为字节
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(10 + self.dst_addr.len() + self.data.len());
        
        // RSV (2 bytes)
        buf.extend_from_slice(&[0x00, 0x00]);
        // FRAG
        buf.push(self.frag);
        // ATYP
        buf.push(self.atyp);
        // DST.ADDR
        buf.extend_from_slice(&self.dst_addr);
        // DST.PORT
        buf.extend_from_slice(&self.dst_port.to_be_bytes());
        // DATA
        buf.extend_from_slice(&self.data);
        
        buf
    }
    
    /// 从字节解析
    pub fn from_bytes(buf: &[u8]) -> Result<Self> {
        if buf.len() < 10 {
            return Err(Error::Protocol("UDP frame too short".into()));
        }
        
        // RSV (2 bytes) - 跳过
        let frag = buf[2];
        let atyp = buf[3];
        
        let (dst_addr, addr_end) = match atyp {
            ATYP_IPV4 => {
                if buf.len() < 10 {
                    return Err(Error::Protocol("UDP frame too short for IPv4".into()));
                }
                (buf[4..8].to_vec(), 8)
            }
            ATYP_DOMAIN => {
                let len = buf[4] as usize;
                if buf.len() < 7 + len {
                    return Err(Error::Protocol("UDP frame too short for domain".into()));
                }
                (buf[4..5 + len].to_vec(), 5 + len)
            }
            ATYP_IPV6 => {
                if buf.len() < 22 {
                    return Err(Error::Protocol("UDP frame too short for IPv6".into()));
                }
                (buf[4..20].to_vec(), 20)
            }
            _ => return Err(Error::Protocol(format!("Unknown address type: {}", atyp))),
        };
        
        if buf.len() < addr_end + 2 {
            return Err(Error::Protocol("UDP frame missing port".into()));
        }
        
        let dst_port = u16::from_be_bytes([buf[addr_end], buf[addr_end + 1]]);
        let data = buf[addr_end + 2..].to_vec();
        
        Ok(Self {
            frag,
            atyp,
            dst_addr,
            dst_port,
            data,
        })
    }
    
    /// 获取源地址（用于响应）
    pub fn get_source_addr(&self) -> Option<(Ipv4Addr, u16)> {
        if self.atyp == ATYP_IPV4 && self.dst_addr.len() == 4 {
            let ip = Ipv4Addr::new(
                self.dst_addr[0],
                self.dst_addr[1],
                self.dst_addr[2],
                self.dst_addr[3],
            );
            Some((ip, self.dst_port))
        } else {
            None
        }
    }
}

/// SOCKS5 UDP 会话
pub struct Socks5UdpSession {
    /// UDP socket (用于与 SOCKS5 服务器通信)
    socket: UdpSocket,
    /// SOCKS5 服务器的 UDP relay 地址
    relay_addr: SocketAddr,
    /// 本地绑定地址
    local_addr: SocketAddr,
    /// TCP 控制连接（保持打开以维持 UDP 会话）
    _tcp_control: tokio::net::TcpStream,
}

impl Socks5UdpSession {
    /// 建立 SOCKS5 UDP ASSOCIATE 会话
    pub async fn connect(socks5_addr: &str) -> Result<Self> {
        // 解析 SOCKS5 服务器地址
        let socks5_addr: SocketAddr = socks5_addr.parse()
            .map_err(|_| Error::Protocol("Invalid SOCKS5 address".into()))?;
        
        // 1. 建立 TCP 控制连接
        let mut tcp = tokio::net::TcpStream::connect(socks5_addr).await
            .map_err(|e| Error::Io(e))?;
        
        // 2. SOCKS5 握手
        // 发送: VER NMETHODS METHODS
        tcp.write_all(&[SOCKS5_VERSION, 0x01, 0x00]).await?;
        
        // 读取: VER METHOD
        let mut resp = [0u8; 2];
        tcp.read_exact(&mut resp).await?;
        
        if resp[0] != SOCKS5_VERSION || resp[1] != 0x00 {
            return Err(Error::Protocol("SOCKS5 auth failed".into()));
        }
        
        // 3. 发送 UDP ASSOCIATE 请求
        // VER CMD RSV ATYP DST.ADDR DST.PORT
        // 对于 UDP ASSOCIATE，DST.ADDR 和 DST.PORT 是客户端期望发送 UDP 的地址
        // 通常设为 0.0.0.0:0 表示任意地址
        tcp.write_all(&[
            SOCKS5_VERSION,
            CMD_UDP_ASSOCIATE,
            0x00,           // RSV
            ATYP_IPV4,      // ATYP
            0, 0, 0, 0,     // DST.ADDR (0.0.0.0)
            0, 0,           // DST.PORT (0)
        ]).await?;
        
        // 4. 读取响应
        // VER REP RSV ATYP BND.ADDR BND.PORT
        let mut resp = [0u8; 10];
        tcp.read_exact(&mut resp).await?;
        
        if resp[0] != SOCKS5_VERSION {
            return Err(Error::Protocol("Invalid SOCKS5 response".into()));
        }
        
        if resp[1] != 0x00 {
            return Err(Error::Protocol(format!("SOCKS5 UDP ASSOCIATE failed: {}", resp[1])));
        }
        
        // 解析 BND.ADDR 和 BND.PORT (UDP relay 地址)
        let relay_addr = match resp[3] {
            ATYP_IPV4 => {
                let ip = Ipv4Addr::new(resp[4], resp[5], resp[6], resp[7]);
                let port = u16::from_be_bytes([resp[8], resp[9]]);
                
                // 如果服务器返回 0.0.0.0，使用 SOCKS5 服务器的 IP
                let ip = if ip.is_unspecified() {
                    match socks5_addr {
                        SocketAddr::V4(addr) => *addr.ip(),
                        _ => ip,
                    }
                } else {
                    ip
                };
                
                SocketAddr::V4(SocketAddrV4::new(ip, port))
            }
            _ => {
                return Err(Error::Protocol("Only IPv4 UDP relay supported".into()));
            }
        };
        
        tracing::debug!("SOCKS5 UDP relay address: {}", relay_addr);
        
        // 5. 创建 UDP socket
        let socket = UdpSocket::bind("0.0.0.0:0").await
            .map_err(|e| Error::Io(e))?;
        
        let local_addr = socket.local_addr()
            .map_err(|e| Error::Io(e))?;
        
        tracing::debug!("Local UDP socket bound to: {}", local_addr);
        
        // TCP 连接需要保持打开，否则 UDP 会话会被服务器关闭
        // 将 TCP 连接存储在结构体中，当 Socks5UdpSession drop 时自动关闭
        Ok(Self {
            socket,
            relay_addr,
            local_addr,
            _tcp_control: tcp,
        })
    }
    
    /// 发送 UDP 数据
    pub async fn send(&self, frame: &Socks5UdpFrame) -> Result<()> {
        let data = frame.to_bytes();
        self.socket.send_to(&data, self.relay_addr).await
            .map_err(|e| Error::Io(e))?;
        Ok(())
    }
    
    /// 接收 UDP 数据
    pub async fn recv(&self, buf: &mut [u8]) -> Result<(Socks5UdpFrame, SocketAddr)> {
        let (n, addr) = self.socket.recv_from(buf).await
            .map_err(|e| Error::Io(e))?;
        
        let frame = Socks5UdpFrame::from_bytes(&buf[..n])?;
        Ok((frame, addr))
    }
    
    /// 获取本地地址
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }
    
    /// 获取 relay 地址
    pub fn relay_addr(&self) -> SocketAddr {
        self.relay_addr
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_udp_frame_ipv4() {
        let frame = Socks5UdpFrame::new_ipv4(
            Ipv4Addr::new(8, 8, 8, 8),
            53,
            vec![0x01, 0x02, 0x03],
        );
        
        let bytes = frame.to_bytes();
        
        // RSV (2) + FRAG (1) + ATYP (1) + ADDR (4) + PORT (2) + DATA (3) = 13
        assert_eq!(bytes.len(), 13);
        assert_eq!(bytes[0..2], [0x00, 0x00]); // RSV
        assert_eq!(bytes[2], 0x00);             // FRAG
        assert_eq!(bytes[3], ATYP_IPV4);        // ATYP
        assert_eq!(bytes[4..8], [8, 8, 8, 8]);  // ADDR
        assert_eq!(bytes[8..10], [0x00, 0x35]); // PORT (53)
        assert_eq!(bytes[10..], [0x01, 0x02, 0x03]); // DATA
        
        // 解析回来
        let parsed = Socks5UdpFrame::from_bytes(&bytes).unwrap();
        assert_eq!(parsed.atyp, ATYP_IPV4);
        assert_eq!(parsed.dst_port, 53);
        assert_eq!(parsed.data, vec![0x01, 0x02, 0x03]);
    }
    
    #[test]
    fn test_udp_frame_domain() {
        let frame = Socks5UdpFrame::new_domain(
            "example.com",
            443,
            vec![0xAA, 0xBB],
        );
        
        let bytes = frame.to_bytes();
        let parsed = Socks5UdpFrame::from_bytes(&bytes).unwrap();
        
        assert_eq!(parsed.atyp, ATYP_DOMAIN);
        assert_eq!(parsed.dst_port, 443);
        assert_eq!(parsed.data, vec![0xAA, 0xBB]);
    }
}
