/// TLS 连接建立
/// 
/// ⚠️ 注意：这个模块用于 WebSocket 层的 TLS（使用 rustls）
/// 真正的 ECH 支持在 src/tls/tunnel.rs（使用 Zig TLS）

use std::sync::Arc;
use tokio::net::TcpStream;
use tokio_rustls::{TlsConnector, rustls};
use tokio_rustls::client::TlsStream;
use rustls::{ClientConfig, RootCertStore};
use rustls::pki_types::ServerName;
use tracing::{info, debug};

use crate::error::{Error, Result};

/// 创建简单的 TLS 配置（用于 WebSocket）
pub fn create_tls_config() -> Result<Arc<ClientConfig>> {
    let mut root_store = RootCertStore::empty();
    root_store.extend(
        webpki_roots::TLS_SERVER_ROOTS
            .iter()
            .cloned()
    );

    let config = ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth();

    Ok(Arc::new(config))
}

/// 建立 TLS 连接（用于 WebSocket 层）
pub async fn establish_tls(
    tcp: TcpStream,
    server_name: String,
) -> Result<TlsStream<TcpStream>> {
    debug!("Establishing TLS connection to {}", server_name);
    
    let config = create_tls_config()?;
    let connector = TlsConnector::from(config);
    
    let domain = ServerName::try_from(server_name.as_str())
        .map_err(|e| Error::Tls(format!("Invalid server name: {}", e)))?
        .to_owned();

    let tls_stream = connector.connect(domain, tcp).await
        .map_err(|e| Error::Tls(format!("TLS handshake failed: {}", e)))?;

    info!("✅ TLS connection established");
    Ok(tls_stream)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore]  // 需要网络连接
    async fn test_tls_connection() {
        let tcp = TcpStream::connect("www.google.com:443").await.unwrap();
        let _tls = establish_tls(tcp, "www.google.com".to_string()).await.unwrap();
    }
}
