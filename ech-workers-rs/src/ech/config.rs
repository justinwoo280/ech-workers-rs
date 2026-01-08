/// ECH 配置解析

use crate::error::{Error, Result};

/// 解析 ECH 配置
/// 
/// 从 DNS HTTPS 记录中提取的原始字节解析为 ECH 配置
pub fn parse_ech_config(bytes: &[u8]) -> Result<Vec<u8>> {
    // TODO: 实现 ECH 配置解析
    // ECH 配置格式参考：
    // https://datatracker.ietf.org/doc/draft-ietf-tls-esni/

    if bytes.is_empty() {
        return Err(Error::Ech("Empty ECH config".to_string()));
    }

    // 目前直接返回原始字节
    // 实际应该解析 ECHConfigList 结构
    Ok(bytes.to_vec())
}
