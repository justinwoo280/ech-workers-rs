/// ECH 配置验证
/// 
/// 参考规范: https://datatracker.ietf.org/doc/draft-ietf-tls-esni/
/// 
/// 注意: BoringSSL 的 SSL_set1_ech_config_list() 接受完整的 ECHConfigList，
/// 无需在 Rust 侧解析，只需验证基本格式正确性。

use crate::error::{Error, Result};
use tracing::debug;

/// ECH 版本号 (draft-ietf-tls-esni-18)
pub const ECH_VERSION_DRAFT18: u16 = 0xfe0d;

/// 验证并返回 ECH 配置
/// 
/// 验证 ECHConfigList 格式正确性，直接返回原始字节供 BoringSSL 使用
/// 
/// # 参数
/// - `bytes`: ECHConfigList 的原始字节 (从 DNS HTTPS SvcParam 获取)
/// 
/// # 返回
/// - 验证通过后的原始字节 (不做修改)
pub fn validate_ech_config_list(bytes: &[u8]) -> Result<&[u8]> {
    if bytes.len() < 4 {
        return Err(Error::Ech("ECHConfigList too short".into()));
    }
    
    // 读取总长度 (2 bytes, big-endian)
    let total_len = u16::from_be_bytes([bytes[0], bytes[1]]) as usize;
    
    // 验证长度
    if bytes.len() < 2 + total_len {
        return Err(Error::Ech(format!(
            "ECHConfigList truncated: declared {} bytes, got {}",
            total_len, bytes.len() - 2
        )));
    }
    
    // 验证至少有一个 ECHConfig
    if total_len < 4 {
        return Err(Error::Ech("ECHConfigList contains no configs".into()));
    }
    
    // 读取第一个 ECHConfig 的版本号
    let version = u16::from_be_bytes([bytes[2], bytes[3]]);
    
    debug!(
        "ECHConfigList: {} bytes, first config version=0x{:04x} (draft-18={})",
        total_len, version, version == ECH_VERSION_DRAFT18
    );
    
    Ok(bytes)
}

/// 快速检查 ECH 配置版本
/// 
/// 检查 ECHConfigList 中是否包含支持的版本
pub fn has_supported_version(bytes: &[u8]) -> bool {
    if bytes.len() < 6 {
        return false;
    }
    
    let total_len = u16::from_be_bytes([bytes[0], bytes[1]]) as usize;
    if bytes.len() < 2 + total_len || total_len < 4 {
        return false;
    }
    
    // 遍历检查是否有 draft-18 版本
    let mut offset = 2;
    let end = 2 + total_len;
    
    while offset + 4 <= end {
        let version = u16::from_be_bytes([bytes[offset], bytes[offset + 1]]);
        if version == ECH_VERSION_DRAFT18 {
            return true;
        }
        
        let config_len = u16::from_be_bytes([bytes[offset + 2], bytes[offset + 3]]) as usize;
        offset += 4 + config_len;
    }
    
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_validate_empty() {
        assert!(validate_ech_config_list(&[]).is_err());
    }
    
    #[test]
    fn test_validate_too_short() {
        assert!(validate_ech_config_list(&[0x00, 0x01]).is_err());
    }
    
    #[test]
    fn test_validate_truncated() {
        // 声明 8 字节但只有 4 字节
        let truncated = vec![0x00, 0x08, 0xfe, 0x0d];
        assert!(validate_ech_config_list(&truncated).is_err());
    }
    
    #[test]
    fn test_validate_valid() {
        let config_list = vec![
            0x00, 0x08,  // length = 8
            0xfe, 0x0d,  // version (draft-18)
            0x00, 0x04,  // config length = 4
            0x01, 0x02, 0x03, 0x04,
        ];
        
        let result = validate_ech_config_list(&config_list);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 10);
    }
    
    #[test]
    fn test_has_supported_version() {
        // draft-18
        let supported = vec![
            0x00, 0x08, 0xfe, 0x0d, 0x00, 0x04, 0x01, 0x02, 0x03, 0x04,
        ];
        assert!(has_supported_version(&supported));
        
        // 旧版本
        let unsupported = vec![
            0x00, 0x08, 0xfe, 0x09, 0x00, 0x04, 0x01, 0x02, 0x03, 0x04,
        ];
        assert!(!has_supported_version(&unsupported));
    }
}
