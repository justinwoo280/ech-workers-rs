/// ECH 配置解析
/// 
/// 参考规范: https://datatracker.ietf.org/doc/draft-ietf-tls-esni/
/// 
/// ECHConfigList 结构:
/// ```text
/// struct {
///     ECHConfig echConfigs<1..2^16-1>;
/// } ECHConfigList;
/// 
/// struct {
///     uint16 version;
///     uint16 length;
///     opaque contents<1..2^16-1>;
/// } ECHConfig;
/// ```

use crate::error::{Error, Result};
use tracing::debug;

/// ECH 版本号 (draft-ietf-tls-esni-18)
const ECH_VERSION_DRAFT18: u16 = 0xfe0d;

/// 解析后的 ECH 配置
#[derive(Debug, Clone)]
pub struct EchConfig {
    /// 版本号
    pub version: u16,
    /// 原始配置数据 (用于传递给 BoringSSL)
    pub raw: Vec<u8>,
}

/// 解析 ECHConfigList
/// 
/// 从 DNS HTTPS 记录中提取的原始字节解析为 ECH 配置列表
/// 
/// # 参数
/// - `bytes`: ECHConfigList 的原始字节
/// 
/// # 返回
/// - 解析后的 ECH 配置列表
pub fn parse_ech_config_list(bytes: &[u8]) -> Result<Vec<EchConfig>> {
    if bytes.len() < 2 {
        return Err(Error::Ech("ECHConfigList too short".to_string()));
    }
    
    // 读取总长度 (2 bytes, big-endian)
    let total_len = u16::from_be_bytes([bytes[0], bytes[1]]) as usize;
    
    if bytes.len() < 2 + total_len {
        return Err(Error::Ech(format!(
            "ECHConfigList truncated: expected {} bytes, got {}",
            2 + total_len,
            bytes.len()
        )));
    }
    
    let mut configs = Vec::new();
    let mut offset = 2; // Skip length field
    let end = 2 + total_len;
    
    while offset < end {
        if offset + 4 > end {
            return Err(Error::Ech("ECHConfig header truncated".to_string()));
        }
        
        // 读取版本号 (2 bytes)
        let version = u16::from_be_bytes([bytes[offset], bytes[offset + 1]]);
        offset += 2;
        
        // 读取配置长度 (2 bytes)
        let config_len = u16::from_be_bytes([bytes[offset], bytes[offset + 1]]) as usize;
        offset += 2;
        
        if offset + config_len > end {
            return Err(Error::Ech(format!(
                "ECHConfig data truncated: expected {} bytes",
                config_len
            )));
        }
        
        // 提取完整的 ECHConfig (包含 version + length + contents)
        let config_start = offset - 4;
        let config_end = offset + config_len;
        let raw = bytes[config_start..config_end].to_vec();
        
        debug!(
            "Parsed ECHConfig: version=0x{:04x}, len={}, supported={}",
            version,
            config_len,
            version == ECH_VERSION_DRAFT18
        );
        
        configs.push(EchConfig { version, raw });
        
        offset += config_len;
    }
    
    if configs.is_empty() {
        return Err(Error::Ech("No ECH configs found".to_string()));
    }
    
    debug!("Parsed {} ECH configs", configs.len());
    Ok(configs)
}

/// 解析 ECH 配置 (兼容旧接口)
/// 
/// 返回第一个支持的 ECH 配置的原始字节
pub fn parse_ech_config(bytes: &[u8]) -> Result<Vec<u8>> {
    if bytes.is_empty() {
        return Err(Error::Ech("Empty ECH config".to_string()));
    }
    
    let configs = parse_ech_config_list(bytes)?;
    
    // 优先选择 draft-18 版本
    for config in &configs {
        if config.version == ECH_VERSION_DRAFT18 {
            debug!("Selected ECHConfig with version 0x{:04x}", config.version);
            return Ok(config.raw.clone());
        }
    }
    
    // 没有 draft-18，返回第一个配置
    debug!(
        "No draft-18 ECHConfig found, using first config with version 0x{:04x}",
        configs[0].version
    );
    Ok(configs[0].raw.clone())
}

/// 验证 ECH 配置是否有效
pub fn validate_ech_config(bytes: &[u8]) -> Result<bool> {
    if bytes.len() < 4 {
        return Ok(false);
    }
    
    let version = u16::from_be_bytes([bytes[0], bytes[1]]);
    let length = u16::from_be_bytes([bytes[2], bytes[3]]) as usize;
    
    // 检查长度是否匹配
    if bytes.len() != 4 + length {
        return Ok(false);
    }
    
    // 检查版本号
    Ok(version == ECH_VERSION_DRAFT18)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parse_empty() {
        assert!(parse_ech_config(&[]).is_err());
    }
    
    #[test]
    fn test_parse_too_short() {
        assert!(parse_ech_config_list(&[0x00]).is_err());
    }
    
    #[test]
    fn test_parse_valid_config() {
        // 构造一个有效的 ECHConfigList
        // Length: 8 bytes
        // ECHConfig: version=0xfe0d, length=4, contents=[0x01, 0x02, 0x03, 0x04]
        let config_list = vec![
            0x00, 0x08,  // ECHConfigList length = 8
            0xfe, 0x0d,  // version = 0xfe0d (draft-18)
            0x00, 0x04,  // config length = 4
            0x01, 0x02, 0x03, 0x04,  // contents
        ];
        
        let result = parse_ech_config_list(&config_list).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].version, ECH_VERSION_DRAFT18);
        assert_eq!(result[0].raw.len(), 8); // version + length + contents
    }
    
    #[test]
    fn test_validate_config() {
        let valid = vec![
            0xfe, 0x0d,  // version
            0x00, 0x02,  // length
            0x01, 0x02,  // contents
        ];
        assert!(validate_ech_config(&valid).unwrap());
        
        let invalid_version = vec![
            0x00, 0x01,  // wrong version
            0x00, 0x02,
            0x01, 0x02,
        ];
        assert!(!validate_ech_config(&invalid_version).unwrap());
    }
}
