/// DNS-over-HTTPS 查询
/// 
/// 查询 HTTPS 记录以获取 ECH 配置
/// 
/// 基于 ech-workers Go 实现

use tracing::{debug, info, warn};
use crate::error::{Error, Result};
use super::config;

const TYPE_HTTPS: u16 = 65;  // HTTPS RR type
const SVCPARAM_ECH: u16 = 5;  // ECH SvcParam key

/// 查询 ECH 配置
/// 
/// 通过 DoH 查询指定域名的 HTTPS 记录，提取并验证 ECH 配置
/// 
/// # 参数
/// - `domain`: 要查询的域名（如 "cloudflare-ech.com"）
/// - `doh_server`: DoH 服务器地址（如 "dns.alidns.com/dns-query"）
/// 
/// # 返回
/// - 验证通过的 ECHConfigList 原始字节
pub async fn query_ech_config(domain: &str, doh_server: &str) -> Result<Vec<u8>> {
    debug!("Querying ECH config for {} via {}", domain, doh_server);
    
    // 1. 构建 DNS 查询
    let dns_query = build_dns_query(domain, TYPE_HTTPS);
    
    // 2. Base64 编码（URL-safe, no padding）
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    let dns_base64 = URL_SAFE_NO_PAD.encode(&dns_query);
    
    // 3. 构建 DoH URL
    let base_url = if doh_server.starts_with("http") {
        doh_server.to_string()
    } else {
        format!("https://{}", doh_server)
    };
    
    // 处理 URL 末尾可能存在的 ? 或 &
    let separator = if base_url.contains('?') { "&" } else { "?" };
    let doh_url = format!("{}{}dns={}", base_url.trim_end_matches('?').trim_end_matches('&'), separator, dns_base64);
    
    debug!("DoH URL: {}", doh_url);
    
    // 4. 发送 HTTP GET 请求（禁用代理，避免循环依赖）
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .no_proxy()  // 关键：DoH 请求不走代理
        .build()
        .map_err(|e| Error::Dns(e.to_string()))?;
    
    let response = client
        .get(&doh_url)
        .header("Accept", "application/dns-message")
        .send()
        .await
        .map_err(|e| Error::Dns(format!("DoH request failed: {}", e)))?;
    
    if !response.status().is_success() {
        return Err(Error::Dns(format!(
            "DoH server returned error: {}",
            response.status()
        )));
    }
    
    let body = response
        .bytes()
        .await
        .map_err(|e| Error::Dns(format!("Failed to read response: {}", e)))?;
    
    // 5. 解析 DNS 响应
    let ech_config = parse_dns_response(&body)?;
    
    // 6. 验证 ECH 配置格式
    config::validate_ech_config_list(&ech_config)?;
    
    // 7. 检查是否有支持的版本
    if !config::has_supported_version(&ech_config) {
        warn!("ECH config has no supported version (draft-18)");
    }
    
    Ok(ech_config)
}

/// 构建 DNS 查询
fn build_dns_query(domain: &str, qtype: u16) -> Vec<u8> {
    let mut query = Vec::with_capacity(512);
    
    // DNS Header (12 bytes)
    query.extend_from_slice(&[
        0x00, 0x01,  // Transaction ID
        0x01, 0x00,  // Flags: standard query
        0x00, 0x01,  // Questions: 1
        0x00, 0x00,  // Answer RRs: 0
        0x00, 0x00,  // Authority RRs: 0
        0x00, 0x00,  // Additional RRs: 0
    ]);
    
    // Question section
    for label in domain.split('.') {
        query.push(label.len() as u8);
        query.extend_from_slice(label.as_bytes());
    }
    query.push(0x00);  // End of domain name
    
    // QTYPE and QCLASS
    query.push((qtype >> 8) as u8);
    query.push((qtype & 0xFF) as u8);
    query.push(0x00);  // QCLASS: IN
    query.push(0x01);
    
    query
}

/// 解析 DNS 响应
fn parse_dns_response(response: &[u8]) -> Result<Vec<u8>> {
    if response.len() < 12 {
        return Err(Error::Dns("Response too short".into()));
    }
    
    // 检查 Answer count
    let ancount = u16::from_be_bytes([response[6], response[7]]);
    if ancount == 0 {
        return Err(Error::Dns("No answer records".into()));
    }
    
    // 跳过 Question section
    let mut offset = 12;
    while offset < response.len() && response[offset] != 0 {
        offset += response[offset] as usize + 1;
    }
    offset += 5;  // Skip null byte + QTYPE + QCLASS
    
    // 解析 Answer section
    for _ in 0..ancount {
        if offset >= response.len() {
            break;
        }
        
        // Skip name (可能是压缩指针)
        if response[offset] & 0xC0 == 0xC0 {
            offset += 2;
        } else {
            while offset < response.len() && response[offset] != 0 {
                offset += response[offset] as usize + 1;
            }
            offset += 1;
        }
        
        if offset + 10 > response.len() {
            break;
        }
        
        // RR Type
        let rr_type = u16::from_be_bytes([response[offset], response[offset + 1]]);
        offset += 8;  // Skip TYPE + CLASS + TTL
        
        // Data length
        let data_len = u16::from_be_bytes([response[offset], response[offset + 1]]) as usize;
        offset += 2;
        
        if offset + data_len > response.len() {
            break;
        }
        
        let data = &response[offset..offset + data_len];
        offset += data_len;
        
        // 检查是否是 HTTPS 记录
        if rr_type == TYPE_HTTPS {
            if let Some(ech) = parse_https_record(data) {
                info!("Found ECH config: {} bytes", ech.len());
                return Ok(ech);
            }
        }
    }
    
    Err(Error::Dns("No ECH config found".into()))
}

/// 解析 HTTPS 记录
fn parse_https_record(data: &[u8]) -> Option<Vec<u8>> {
    if data.len() < 2 {
        return None;
    }
    
    let mut offset = 2;  // Skip priority
    
    // Skip target name
    if offset < data.len() && data[offset] == 0 {
        offset += 1;
    } else {
        while offset < data.len() && data[offset] != 0 {
            offset += data[offset] as usize + 1;
        }
        offset += 1;
    }
    
    // Parse SvcParams
    while offset + 4 <= data.len() {
        let key = u16::from_be_bytes([data[offset], data[offset + 1]]);
        let length = u16::from_be_bytes([data[offset + 2], data[offset + 3]]) as usize;
        offset += 4;
        
        if offset + length > data.len() {
            break;
        }
        
        let value = &data[offset..offset + length];
        offset += length;
        
        // SvcParam key=5 是 ECH 配置
        if key == SVCPARAM_ECH {
            debug!("Found ECH SvcParam: {} bytes", value.len());
            return Some(value.to_vec());
        }
    }
    
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // 需要网络连接
    async fn test_query_ech_config() {
        let result = query_ech_config(
            "cloudflare-ech.com",
            "dns.alidns.com/dns-query"
        ).await;

        // 应该返回 ECH 配置字节
        assert!(result.is_ok());
    }
}
