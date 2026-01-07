/// 地址解析工具

use crate::error::{Error, Result};

/// 解析服务器地址
/// 
/// 从 "host:port" 或 "host:port/path" 格式解析
/// 
/// # 返回
/// 
/// (host, port, path)
pub fn parse_server_addr(addr: &str) -> Result<(String, u16, String)> {
    // 分离路径
    let (addr_part, path) = if let Some(pos) = addr.find('/') {
        let (a, p) = addr.split_at(pos);
        (a, p.to_string())
    } else {
        (addr, "/".to_string())
    };

    // 分离主机和端口
    let parts: Vec<&str> = addr_part.split(':').collect();
    if parts.len() != 2 {
        return Err(Error::Config(format!("Invalid server address: {}", addr)));
    }

    let host = parts[0].to_string();
    let port = parts[1].parse::<u16>()
        .map_err(|_| Error::Config(format!("Invalid port: {}", parts[1])))?;

    Ok((host, port, path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_server_addr() {
        let (host, port, path) = parse_server_addr("example.com:443").unwrap();
        assert_eq!(host, "example.com");
        assert_eq!(port, 443);
        assert_eq!(path, "/");

        let (host, port, path) = parse_server_addr("example.com:443/ws").unwrap();
        assert_eq!(host, "example.com");
        assert_eq!(port, 443);
        assert_eq!(path, "/ws");
    }
}
