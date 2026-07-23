//! 客户端 IP 解析（限流用）。信任边界：仅当直连 peer 为受信代理时才读转发头。
//!
//! 拓扑：TLS 终结（Caddy/LB）→ web → gateway → 上游。web/gateway 均在私网；
//! 公网 peer 发来的 `X-Forwarded-For` / `X-Real-IP` **一律忽略**，防伪造绕过限流。
//!
//! 受信 peer（loopback / RFC1918 / link-local / ULA）时：
//! 1. 优先 `X-Real-IP`（边缘代理写入）
//! 2. 否则取 `X-Forwarded-For` **最右**段（代理追加模式下最近一跳客户端）
//! 3. 再否则回退 peer 本身

use std::net::{IpAddr, Ipv6Addr};

/// 解析用于限流的客户端 IP 字符串。
pub fn resolve_client_ip(
    peer: Option<IpAddr>,
    x_real_ip: Option<&str>,
    x_forwarded_for: Option<&str>,
) -> String {
    let Some(peer_ip) = peer else {
        return "unknown".into();
    };
    if !is_trusted_proxy(peer_ip) {
        return peer_ip.to_string();
    }
    if let Some(ip) = parse_ip_token(x_real_ip) {
        return ip.to_string();
    }
    if let Some(ip) = rightmost_xff(x_forwarded_for) {
        return ip.to_string();
    }
    peer_ip.to_string()
}

/// loopback / 私网 / link-local / IPv6 ULA 视为容器/反代侧 peer。
pub fn is_trusted_proxy(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback() || v4.is_private() || v4.is_link_local() || v4.is_unspecified()
        }
        IpAddr::V6(v6) => {
            v6.is_loopback()
                || v6.is_unspecified()
                || is_unique_local(v6)
                || v6.to_ipv4_mapped().is_some_and(|v4| {
                    v4.is_loopback() || v4.is_private() || v4.is_link_local() || v4.is_unspecified()
                })
        }
    }
}

fn is_unique_local(v6: Ipv6Addr) -> bool {
    // fc00::/7
    (v6.octets()[0] & 0xfe) == 0xfc
}

fn parse_ip_token(raw: Option<&str>) -> Option<IpAddr> {
    let s = raw?.trim();
    if s.is_empty() {
        return None;
    }
    // 兼容 `[::1]:1234` / `1.2.3.4:5678`（偶发带端口）
    if let Ok(ip) = s.parse::<IpAddr>() {
        return Some(ip);
    }
    if let Some((host, _)) = s.rsplit_once(':') {
        if host.chars().filter(|c| *c == ':').count() == 0 {
            // IPv4:port
            return host.parse().ok();
        }
    }
    None
}

fn rightmost_xff(xff: Option<&str>) -> Option<IpAddr> {
    let s = xff?.trim();
    if s.is_empty() {
        return None;
    }
    s.split(',')
        .rev()
        .find_map(|part| parse_ip_token(Some(part)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    #[test]
    fn public_peer_ignores_xff() {
        let peer: IpAddr = "203.0.113.10".parse().unwrap();
        let ip = resolve_client_ip(Some(peer), Some("1.1.1.1"), Some("8.8.8.8, 9.9.9.9"));
        assert_eq!(ip, "203.0.113.10");
    }

    #[test]
    fn private_peer_prefers_x_real_ip() {
        let peer: IpAddr = "10.0.0.2".parse().unwrap();
        let ip = resolve_client_ip(Some(peer), Some("198.51.100.7"), Some("8.8.8.8"));
        assert_eq!(ip, "198.51.100.7");
    }

    #[test]
    fn private_peer_uses_rightmost_xff() {
        let peer: IpAddr = "172.18.0.5".parse().unwrap();
        let ip = resolve_client_ip(Some(peer), None, Some("1.1.1.1, 198.51.100.9"));
        assert_eq!(ip, "198.51.100.9");
    }

    #[test]
    fn missing_peer_is_unknown() {
        assert_eq!(resolve_client_ip(None, Some("1.1.1.1"), None), "unknown");
    }

    #[test]
    fn loopback_is_trusted() {
        assert!(is_trusted_proxy(IpAddr::V4(Ipv4Addr::LOCALHOST)));
    }
}
