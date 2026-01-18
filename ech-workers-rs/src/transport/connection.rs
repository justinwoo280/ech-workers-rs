/// 连接建立流程
/// 
/// DoH → ECH → Zig TLS → WebSocket → Yamux

use tracing::{info, debug, warn, error};
use crate::error::{Error, Result};
use crate::ech;
use crate::tls::{TlsTunnel, TunnelConfig};
use crate::utils::parse_server_addr;

/// 建立 ECH + TLS 连接
/// 
/// ⚠️ 严格模式：如果启用 ECH，则必须成功，否则失败
/// 不会回退到普通 TLS
/// 
/// # 参数
/// - `server_addr`: 服务器地址（host:port）
/// - `server_ip`: 可选的连接 IP/主机名（用于绕过 DNS）
/// - `doh_server`: DoH 服务器地址
/// - `use_ech`: 是否启用 ECH
pub async fn establish_ech_tls(
    server_addr: &str,
    server_ip: Option<&str>,
    doh_server: &str,
    use_ech: bool,
) -> Result<TlsTunnel> {
    let (host, port, _path) = parse_server_addr(server_addr)?;
    
    if let Some(ip) = server_ip {
        info!("Establishing ECH + TLS connection via {} (SNI: {})", ip, host);
    } else {
        info!("Establishing ECH + TLS connection to {}:{}", host, port);
    }
    
    let mut config = if use_ech {
        // ECH 模式：必须查询到配置
        info!("📡 [1/4] Querying ECH config for {} via DoH ({})", host, doh_server);
        let ech_config = ech::query_ech_config(&host, doh_server).await
            .map_err(|e| {
                error!("❌ ECH query failed: {}", e);
                Error::Dns(format!("ECH query failed (no fallback): {}", e))
            })?;
        
        info!("✅ [1/4] Got ECH config: {} bytes", ech_config.len());
        
        // enforce_ech = true: 强制验证 ECH
        TunnelConfig::new(&host, port).with_ech(ech_config, true)
    } else {
        // 非 ECH 模式：普通 TLS（仅用于测试）
        warn!("⚠️  ECH disabled, using plain TLS (not recommended)");
        TunnelConfig::new(&host, port)
    };
    
    // 如果指定了 server_ip，设置 connect_host
    if let Some(ip) = server_ip {
        info!("🔀 Using server_ip: {} (TCP target, SNI remains: {})", ip, host);
        config = config.with_connect_host(ip);
    }
    
    // 建立 TLS 连接
    info!("🔐 [2/4] Establishing TLS connection...");
    let tunnel = TlsTunnel::connect(config).map_err(|e| {
        error!("❌ TLS connection failed: {:?}", e);
        e
    })?;
    info!("✅ [2/4] TLS connection established");
    
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
