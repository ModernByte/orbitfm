//! DNS-discovery серверов Radio Browser.

use std::collections::HashSet;
use std::hash::{BuildHasher, Hasher};
use std::net::SocketAddr;
use std::time::Duration;

use tokio::net::lookup_host;

use super::radio_browser::RadioError;

pub(crate) const API_HOST: &str = "all.api.radio-browser.info";
pub(crate) const API_BASE_URL: &str = "https://all.api.radio-browser.info";

const DISCOVERY_TIMEOUT: Duration = Duration::from_secs(3);
const HTTPS_PORT: u16 = 443;

/// Разрешает `all.api.radio-browser.info` в список уникальных адресов.
///
/// Официальная рекомендация Radio Browser — выполнять DNS-lookup перед первым
/// запросом и распределять нагрузку между полученными серверами.
pub(crate) async fn discover_servers() -> Result<Vec<SocketAddr>, RadioError> {
    let lookup = lookup_host((API_HOST, HTTPS_PORT));
    let resolved = tokio::time::timeout(DISCOVERY_TIMEOUT, lookup)
        .await
        .map_err(|_| RadioError::Network("таймаут DNS-discovery".to_owned()))?
        .map_err(|err| RadioError::Network(format!("DNS-discovery: {err}")))?;

    let mut seen = HashSet::new();
    let mut servers = Vec::new();
    for addr in resolved {
        if seen.insert(addr.ip()) {
            servers.push(SocketAddr::new(addr.ip(), HTTPS_PORT));
        }
    }

    if servers.is_empty() {
        return Err(RadioError::NoServers);
    }

    rotate_randomly(&mut servers);
    Ok(servers)
}

fn rotate_randomly(servers: &mut [SocketAddr]) {
    if servers.len() < 2 {
        return;
    }

    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write_usize(servers.len());
    let start = (hasher.finish() as usize) % servers.len();
    servers.rotate_left(start);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotation_keeps_every_server() {
        let mut servers = vec![
            SocketAddr::from(([192, 0, 2, 1], 443)),
            SocketAddr::from(([192, 0, 2, 2], 443)),
            SocketAddr::from(([192, 0, 2, 3], 443)),
        ];
        let expected = servers.clone();

        rotate_randomly(&mut servers);

        assert_eq!(servers.len(), expected.len());
        for addr in expected {
            assert!(servers.contains(&addr));
        }
    }
}
