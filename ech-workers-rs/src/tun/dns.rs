//! DNS 处理模块
//! 
//! 通过 DoH (DNS over HTTPS) 处理 DNS 查询

use crate::error::{Error, Result};
use std::net::Ipv4Addr;

/// DNS 查询类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DnsQueryType {
    A,
    AAAA,
    CNAME,
    MX,
    TXT,
    NS,
    SOA,
    PTR,
    Unknown(u16),
}

impl From<u16> for DnsQueryType {
    fn from(value: u16) -> Self {
        match value {
            1 => DnsQueryType::A,
            28 => DnsQueryType::AAAA,
            5 => DnsQueryType::CNAME,
            15 => DnsQueryType::MX,
            16 => DnsQueryType::TXT,
            2 => DnsQueryType::NS,
            6 => DnsQueryType::SOA,
            12 => DnsQueryType::PTR,
            _ => DnsQueryType::Unknown(value),
        }
    }
}

impl DnsQueryType {
    /// 转换为 u16
    pub fn to_u16(&self) -> u16 {
        match self {
            DnsQueryType::A => 1,
            DnsQueryType::AAAA => 28,
            DnsQueryType::CNAME => 5,
            DnsQueryType::MX => 15,
            DnsQueryType::TXT => 16,
            DnsQueryType::NS => 2,
            DnsQueryType::SOA => 6,
            DnsQueryType::PTR => 12,
            DnsQueryType::Unknown(v) => *v,
        }
    }
}

/// 解析的 DNS 查询
#[derive(Debug, Clone)]
pub struct DnsQuery {
    /// 事务 ID
    pub id: u16,
    /// 查询域名
    pub name: String,
    /// 查询类型
    pub qtype: DnsQueryType,
    /// 查询类
    pub qclass: u16,
}

/// DNS 响应
#[derive(Debug, Clone)]
pub struct DnsResponse {
    /// 事务 ID
    pub id: u16,
    /// 响应数据（原始 DNS 响应包）
    pub data: Vec<u8>,
}

/// DNS 处理器
pub struct DnsHandler {
    /// DoH 服务器 URL
    doh_server: String,
    /// HTTP 客户端
    client: reqwest::Client,
}

impl DnsHandler {
    /// 创建新的 DNS 处理器
    pub fn new(doh_server: &str) -> Self {
        Self {
            doh_server: doh_server.to_string(),
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(5))
                .build()
                .unwrap_or_default(),
        }
    }
    
    /// 解析 DNS 查询包
    pub fn parse_query(data: &[u8]) -> Result<DnsQuery> {
        if data.len() < 12 {
            return Err(Error::Protocol("DNS packet too short".into()));
        }
        
        // 事务 ID
        let id = u16::from_be_bytes([data[0], data[1]]);
        
        // 跳过头部，解析问题部分
        let mut pos = 12;
        
        // 解析域名
        let mut name_parts = Vec::new();
        loop {
            if pos >= data.len() {
                return Err(Error::Protocol("DNS name truncated".into()));
            }
            
            let len = data[pos] as usize;
            if len == 0 {
                pos += 1;
                break;
            }
            
            if len > 63 {
                // 压缩指针，暂不支持
                return Err(Error::Protocol("DNS compression not supported".into()));
            }
            
            pos += 1;
            if pos + len > data.len() {
                return Err(Error::Protocol("DNS name truncated".into()));
            }
            
            name_parts.push(String::from_utf8_lossy(&data[pos..pos + len]).to_string());
            pos += len;
        }
        
        let name = name_parts.join(".");
        
        // 解析类型和类
        if pos + 4 > data.len() {
            return Err(Error::Protocol("DNS question truncated".into()));
        }
        
        let qtype = u16::from_be_bytes([data[pos], data[pos + 1]]);
        let qclass = u16::from_be_bytes([data[pos + 2], data[pos + 3]]);
        
        Ok(DnsQuery {
            id,
            name,
            qtype: qtype.into(),
            qclass,
        })
    }
    
    /// 通过 DoH 查询 DNS
    pub async fn query_doh(&self, dns_packet: &[u8]) -> Result<Vec<u8>> {
        // 使用 POST 方法发送 DNS 查询
        let response = self.client
            .post(&self.doh_server)
            .header("Content-Type", "application/dns-message")
            .header("Accept", "application/dns-message")
            .body(dns_packet.to_vec())
            .send()
            .await
            .map_err(|e| Error::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("DoH request failed: {}", e)
            )))?;
        
        if !response.status().is_success() {
            return Err(Error::Protocol(format!(
                "DoH server returned error: {}", response.status()
            )));
        }
        
        let data = response.bytes().await
            .map_err(|e| Error::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to read DoH response: {}", e)
            )))?;
        
        Ok(data.to_vec())
    }
    
    /// 构造简单的 DNS 响应（用于本地拦截）
    pub fn build_response(query: &DnsQuery, ip: Ipv4Addr) -> Vec<u8> {
        let mut response = Vec::with_capacity(64);
        
        // Header
        response.extend(&query.id.to_be_bytes());     // ID
        response.extend(&[0x81, 0x80]);               // Flags: QR=1, RD=1, RA=1
        response.extend(&[0x00, 0x01]);               // QDCOUNT = 1
        response.extend(&[0x00, 0x01]);               // ANCOUNT = 1
        response.extend(&[0x00, 0x00]);               // NSCOUNT = 0
        response.extend(&[0x00, 0x00]);               // ARCOUNT = 0
        
        // Question section (echo back)
        for part in query.name.split('.') {
            response.push(part.len() as u8);
            response.extend(part.as_bytes());
        }
        response.push(0); // End of name
        response.extend(&query.qtype.to_u16().to_be_bytes());
        response.extend(&query.qclass.to_be_bytes());
        
        // Answer section
        response.extend(&[0xc0, 0x0c]);               // Name pointer to question
        response.extend(&[0x00, 0x01]);               // Type A
        response.extend(&[0x00, 0x01]);               // Class IN
        response.extend(&[0x00, 0x00, 0x00, 0x3c]);   // TTL = 60
        response.extend(&[0x00, 0x04]);               // RDLENGTH = 4
        response.extend(&ip.octets());                // RDATA (IP address)
        
        response
    }
    
    /// 构造 NXDOMAIN 响应
    pub fn build_nxdomain(query: &DnsQuery) -> Vec<u8> {
        let mut response = Vec::with_capacity(32);
        
        // Header
        response.extend(&query.id.to_be_bytes());     // ID
        response.extend(&[0x81, 0x83]);               // Flags: QR=1, RD=1, RA=1, RCODE=3 (NXDOMAIN)
        response.extend(&[0x00, 0x01]);               // QDCOUNT = 1
        response.extend(&[0x00, 0x00]);               // ANCOUNT = 0
        response.extend(&[0x00, 0x00]);               // NSCOUNT = 0
        response.extend(&[0x00, 0x00]);               // ARCOUNT = 0
        
        // Question section (echo back)
        for part in query.name.split('.') {
            response.push(part.len() as u8);
            response.extend(part.as_bytes());
        }
        response.push(0); // End of name
        response.extend(&query.qtype.to_u16().to_be_bytes());
        response.extend(&query.qclass.to_be_bytes());
        
        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parse_dns_query() {
        // 简单的 DNS 查询包 (example.com A)
        let packet = [
            0x12, 0x34,             // ID
            0x01, 0x00,             // Flags
            0x00, 0x01,             // QDCOUNT
            0x00, 0x00,             // ANCOUNT
            0x00, 0x00,             // NSCOUNT
            0x00, 0x00,             // ARCOUNT
            0x07, b'e', b'x', b'a', b'm', b'p', b'l', b'e',
            0x03, b'c', b'o', b'm',
            0x00,                   // End of name
            0x00, 0x01,             // Type A
            0x00, 0x01,             // Class IN
        ];
        
        let query = DnsHandler::parse_query(&packet).unwrap();
        assert_eq!(query.id, 0x1234);
        assert_eq!(query.name, "example.com");
        assert_eq!(query.qtype, DnsQueryType::A);
    }
}
