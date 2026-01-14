//! IP 包构造器
//! 
//! 构造 IPv4 TCP/UDP 包用于写回 TUN 设备

use std::net::Ipv4Addr;

/// 构造 IPv4 + TCP 包
pub fn build_tcp_packet(
    src_ip: Ipv4Addr,
    dst_ip: Ipv4Addr,
    src_port: u16,
    dst_port: u16,
    seq: u32,
    ack: u32,
    flags: TcpFlags,
    window: u16,
    payload: &[u8],
) -> Vec<u8> {
    let tcp_len = 20 + payload.len(); // TCP header (20) + payload
    let total_len = 20 + tcp_len;     // IP header (20) + TCP
    
    let mut packet = Vec::with_capacity(total_len);
    
    // ========== IPv4 Header (20 bytes) ==========
    packet.push(0x45);                           // Version (4) + IHL (5)
    packet.push(0x00);                           // DSCP + ECN
    packet.extend(&(total_len as u16).to_be_bytes()); // Total Length
    packet.extend(&[0x00, 0x00]);                // Identification
    packet.extend(&[0x40, 0x00]);                // Flags (DF) + Fragment Offset
    packet.push(64);                             // TTL
    packet.push(6);                              // Protocol (TCP = 6)
    packet.extend(&[0x00, 0x00]);                // Header Checksum (placeholder)
    packet.extend(&src_ip.octets());             // Source IP
    packet.extend(&dst_ip.octets());             // Destination IP
    
    // Calculate IP checksum
    let ip_checksum = calculate_checksum(&packet[0..20]);
    packet[10] = (ip_checksum >> 8) as u8;
    packet[11] = (ip_checksum & 0xff) as u8;
    
    // ========== TCP Header (20 bytes) ==========
    let tcp_start = packet.len();
    packet.extend(&src_port.to_be_bytes());      // Source Port
    packet.extend(&dst_port.to_be_bytes());      // Destination Port
    packet.extend(&seq.to_be_bytes());           // Sequence Number
    packet.extend(&ack.to_be_bytes());           // Acknowledgment Number
    packet.push(0x50);                           // Data Offset (5) + Reserved
    packet.push(flags.to_byte());                // Flags
    packet.extend(&window.to_be_bytes());        // Window Size
    packet.extend(&[0x00, 0x00]);                // Checksum (placeholder)
    packet.extend(&[0x00, 0x00]);                // Urgent Pointer
    
    // Add payload
    packet.extend(payload);
    
    // Calculate TCP checksum (with pseudo-header)
    let tcp_checksum = calculate_tcp_checksum(
        src_ip,
        dst_ip,
        &packet[tcp_start..],
    );
    packet[tcp_start + 16] = (tcp_checksum >> 8) as u8;
    packet[tcp_start + 17] = (tcp_checksum & 0xff) as u8;
    
    packet
}

/// 构造 IPv4 + UDP 包
pub fn build_udp_packet(
    src_ip: Ipv4Addr,
    dst_ip: Ipv4Addr,
    src_port: u16,
    dst_port: u16,
    payload: &[u8],
) -> Vec<u8> {
    let udp_len = 8 + payload.len();  // UDP header (8) + payload
    let total_len = 20 + udp_len;     // IP header (20) + UDP
    
    let mut packet = Vec::with_capacity(total_len);
    
    // ========== IPv4 Header (20 bytes) ==========
    packet.push(0x45);                           // Version (4) + IHL (5)
    packet.push(0x00);                           // DSCP + ECN
    packet.extend(&(total_len as u16).to_be_bytes()); // Total Length
    packet.extend(&[0x00, 0x00]);                // Identification
    packet.extend(&[0x40, 0x00]);                // Flags (DF) + Fragment Offset
    packet.push(64);                             // TTL
    packet.push(17);                             // Protocol (UDP = 17)
    packet.extend(&[0x00, 0x00]);                // Header Checksum (placeholder)
    packet.extend(&src_ip.octets());             // Source IP
    packet.extend(&dst_ip.octets());             // Destination IP
    
    // Calculate IP checksum
    let ip_checksum = calculate_checksum(&packet[0..20]);
    packet[10] = (ip_checksum >> 8) as u8;
    packet[11] = (ip_checksum & 0xff) as u8;
    
    // ========== UDP Header (8 bytes) ==========
    let udp_start = packet.len();
    packet.extend(&src_port.to_be_bytes());      // Source Port
    packet.extend(&dst_port.to_be_bytes());      // Destination Port
    packet.extend(&(udp_len as u16).to_be_bytes()); // Length
    packet.extend(&[0x00, 0x00]);                // Checksum (placeholder)
    
    // Add payload
    packet.extend(payload);
    
    // Calculate UDP checksum (with pseudo-header)
    let udp_checksum = calculate_udp_checksum(
        src_ip,
        dst_ip,
        &packet[udp_start..],
    );
    packet[udp_start + 6] = (udp_checksum >> 8) as u8;
    packet[udp_start + 7] = (udp_checksum & 0xff) as u8;
    
    packet
}

/// TCP 标志位
#[derive(Debug, Clone, Copy, Default)]
pub struct TcpFlags {
    pub fin: bool,
    pub syn: bool,
    pub rst: bool,
    pub psh: bool,
    pub ack: bool,
    pub urg: bool,
}

impl TcpFlags {
    pub fn new() -> Self {
        Self::default()
    }
    
    pub fn syn() -> Self {
        Self { syn: true, ..Default::default() }
    }
    
    pub fn syn_ack() -> Self {
        Self { syn: true, ack: true, ..Default::default() }
    }
    
    pub fn ack() -> Self {
        Self { ack: true, ..Default::default() }
    }
    
    pub fn psh_ack() -> Self {
        Self { psh: true, ack: true, ..Default::default() }
    }
    
    pub fn fin_ack() -> Self {
        Self { fin: true, ack: true, ..Default::default() }
    }
    
    pub fn rst() -> Self {
        Self { rst: true, ..Default::default() }
    }
    
    fn to_byte(&self) -> u8 {
        let mut flags = 0u8;
        if self.fin { flags |= 0x01; }
        if self.syn { flags |= 0x02; }
        if self.rst { flags |= 0x04; }
        if self.psh { flags |= 0x08; }
        if self.ack { flags |= 0x10; }
        if self.urg { flags |= 0x20; }
        flags
    }
}

/// 计算 IP 校验和
fn calculate_checksum(data: &[u8]) -> u16 {
    let mut sum: u32 = 0;
    
    // 按 16-bit 字累加
    let mut i = 0;
    while i < data.len() {
        let word = if i + 1 < data.len() {
            ((data[i] as u32) << 8) | (data[i + 1] as u32)
        } else {
            (data[i] as u32) << 8
        };
        sum += word;
        i += 2;
    }
    
    // 折叠进位
    while sum >> 16 != 0 {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    
    // 取反
    !sum as u16
}

/// 计算 TCP 校验和（包含伪头部）
fn calculate_tcp_checksum(src_ip: Ipv4Addr, dst_ip: Ipv4Addr, tcp_segment: &[u8]) -> u16 {
    let mut sum: u32 = 0;
    
    // 伪头部
    let src = src_ip.octets();
    let dst = dst_ip.octets();
    sum += ((src[0] as u32) << 8) | (src[1] as u32);
    sum += ((src[2] as u32) << 8) | (src[3] as u32);
    sum += ((dst[0] as u32) << 8) | (dst[1] as u32);
    sum += ((dst[2] as u32) << 8) | (dst[3] as u32);
    sum += 6u32;  // Protocol (TCP)
    sum += tcp_segment.len() as u32;
    
    // TCP 段
    let mut i = 0;
    while i < tcp_segment.len() {
        let word = if i + 1 < tcp_segment.len() {
            ((tcp_segment[i] as u32) << 8) | (tcp_segment[i + 1] as u32)
        } else {
            (tcp_segment[i] as u32) << 8
        };
        sum += word;
        i += 2;
    }
    
    // 折叠进位
    while sum >> 16 != 0 {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    
    !sum as u16
}

/// 计算 UDP 校验和（包含伪头部）
fn calculate_udp_checksum(src_ip: Ipv4Addr, dst_ip: Ipv4Addr, udp_segment: &[u8]) -> u16 {
    let mut sum: u32 = 0;
    
    // 伪头部
    let src = src_ip.octets();
    let dst = dst_ip.octets();
    sum += ((src[0] as u32) << 8) | (src[1] as u32);
    sum += ((src[2] as u32) << 8) | (src[3] as u32);
    sum += ((dst[0] as u32) << 8) | (dst[1] as u32);
    sum += ((dst[2] as u32) << 8) | (dst[3] as u32);
    sum += 17u32; // Protocol (UDP)
    sum += udp_segment.len() as u32;
    
    // UDP 段
    let mut i = 0;
    while i < udp_segment.len() {
        let word = if i + 1 < udp_segment.len() {
            ((udp_segment[i] as u32) << 8) | (udp_segment[i + 1] as u32)
        } else {
            (udp_segment[i] as u32) << 8
        };
        sum += word;
        i += 2;
    }
    
    // 折叠进位
    while sum >> 16 != 0 {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    
    let checksum = !sum as u16;
    // UDP 校验和为 0 时应设置为 0xFFFF
    if checksum == 0 { 0xFFFF } else { checksum }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_tcp_packet_build() {
        let packet = build_tcp_packet(
            Ipv4Addr::new(10, 0, 0, 1),
            Ipv4Addr::new(93, 184, 216, 34),
            12345,
            80,
            1000,
            0,
            TcpFlags::syn(),
            65535,
            &[],
        );
        
        // 验证基本结构
        assert_eq!(packet[0] & 0xF0, 0x40); // IPv4
        assert_eq!(packet[9], 6);           // TCP protocol
        assert_eq!(packet.len(), 40);       // IP(20) + TCP(20)
    }
    
    #[test]
    fn test_udp_packet_build() {
        let packet = build_udp_packet(
            Ipv4Addr::new(10, 0, 0, 1),
            Ipv4Addr::new(8, 8, 8, 8),
            54321,
            53,
            b"DNS query",
        );
        
        // 验证基本结构
        assert_eq!(packet[0] & 0xF0, 0x40); // IPv4
        assert_eq!(packet[9], 17);          // UDP protocol
        assert_eq!(packet.len(), 20 + 8 + 9); // IP(20) + UDP(8) + payload(9)
    }
}
