/// 连接建立流程
/// 
/// DoH → ECH → Zig TLS → WebSocket → Yamux

use tracing::{info, debug};
use crate::error::{Error, Result};
use crate::ech;
use crate::tls::{TlsTunnel, TunnelConfig};
use crate::utils::parse_server_addr;

/// 建立 ECH + TLS 连接
/// 
/// ⚠️ 严格模式：如果启用 ECH，则必须成功，否则失败
/// 不会回退到普通 TLS
pub async fn establish_ech_tls(
    server_addr: &str,
    doh_server: &str,
    use_ech: bool,
) -> Result<TlsTunnel> {
    let (host, port, _path) = parse_server_addr(server_addr)?;
    
    info!("Establishing ECH + TLS connection to {}:{}", host, port);
    
    let config = if use_ech {
        // ECH 模式：必须查询到配置
        debug!("Querying ECH config for {} via {}", host, doh_server);
        let ech_config = ech::query_ech_config(&host, doh_server).await
            .map_err(|e| {
                Error::Dns(format!("ECH query failed (no fallback): {}", e))
            })?;
        
        info!("✅ Got ECH config: {} bytes", ech_config.len());
        
        // enforce_ech = true: 强制验证 ECH
        TunnelConfig::new(&host, port).with_ech(ech_config, true)
    } else {
        // 非 ECH 模式：普通 TLS（仅用于测试）
        info!("⚠️  ECH disabled, using plain TLS");
        TunnelConfig::new(&host, port)
    };
    
    // 建立 TLS 连接
    let tunnel = TlsTunnel::connect(config)?;
    
    // 严格验证 ECH 状态
    if use_ech {
        let info = tunnel.info()?;
        if info.used_ech {
            info!("✅ ECH successfully negotiated");
        } else {
            // ECH 未被接受 = 连接失败
            return Err(Error::Dns(
                "ECH not accepted by server (possible downgrade attack or misconfiguration)".into()
            ));
        }
    }
    
    Ok(tunnel)
}
