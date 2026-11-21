//! Безопасная установка обновления AppImage (ADR-0067 §6–§7).
//!
//! Только Linux AppImage и только после сверки SHA-256 из `SHA256SUMS` того же
//! релиза. Замена атомарная (`tmp` → `rename`), root не нужен, shell не
//! используется. MSI/DMG не устанавливаются автоматически (ADR-0052).

use std::path::{Path, PathBuf};

use reqwest::Client;
use sha2::{Digest, Sha256};

use super::github::user_agent;
use super::{Release, UpdateError};

/// Идёт ли процесс из AppImage: рантайм ставит переменную `APPIMAGE`.
pub fn is_appimage() -> bool {
    std::env::var_os("APPIMAGE").is_some_and(|value| !value.is_empty())
}

/// Куда установлен AppImage: `$XDG_BIN_HOME/orbitfm` или `~/.local/bin/orbitfm`.
pub fn installed_path() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("XDG_BIN_HOME").filter(|value| !value.is_empty()) {
        return Some(PathBuf::from(dir).join("orbitfm"));
    }
    std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/bin/orbitfm"))
}

/// Клиент загрузки: `User-Agent` обязателен, таймаут — на большой файл.
pub fn download_client() -> Result<Client, UpdateError> {
    Client::builder()
        .user_agent(user_agent())
        .timeout(std::time::Duration::from_secs(600))
        .build()
        .map_err(|error| UpdateError::Network(error.to_string()))
}

/// Найти SHA-256 конкретного файла в `SHA256SUMS`
/// (формат `<hex>  <name>`; звёздочка бинарного режима снимается).
pub fn checksum_for(sums: &str, file_name: &str) -> Option<String> {
    for line in sums.lines() {
        let mut parts = line.split_whitespace();
        let (Some(hash), Some(name)) = (parts.next(), parts.next()) else {
            continue;
        };
        if name.trim_start_matches('*') == file_name {
            return Some(hash.to_ascii_lowercase());
        }
    }
    None
}

/// Скачать и атомарно установить AppImage поверх `target`.
pub async fn install(release: &Release, target: &Path, client: &Client) -> Result<(), UpdateError> {
    let asset = release.platform_asset().ok_or(UpdateError::Unsupported)?;
    if !asset.name.ends_with(".AppImage") {
        return Err(UpdateError::Unsupported);
    }
    let sums_url = release.checksums_url().ok_or(UpdateError::NoChecksum)?;
    let sums = client
        .get(sums_url)
        .send()
        .await
        .map_err(|error| UpdateError::Network(error.to_string()))?
        .error_for_status()
        .map_err(|error| UpdateError::Http(error.to_string()))?
        .text()
        .await
        .map_err(|error| UpdateError::Network(error.to_string()))?;
    let expected = checksum_for(&sums, &asset.name).ok_or(UpdateError::NoChecksum)?;

    let bytes = client
        .get(&asset.url)
        .send()
        .await
        .map_err(|error| UpdateError::Network(error.to_string()))?
        .error_for_status()
        .map_err(|error| UpdateError::Http(error.to_string()))?
        .bytes()
        .await
        .map_err(|error| UpdateError::Network(error.to_string()))?;

    let actual: String = Sha256::digest(&bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    if actual != expected {
        return Err(UpdateError::ChecksumMismatch);
    }

    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(|error| UpdateError::Io(error.to_string()))?;
    }
    let temp = target.with_extension("new");
    std::fs::write(&temp, &bytes).map_err(|error| UpdateError::Io(error.to_string()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&temp, std::fs::Permissions::from_mode(0o755))
            .map_err(|error| UpdateError::Io(error.to_string()))?;
    }
    std::fs::rename(&temp, target).map_err(|error| UpdateError::Io(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::update::{Channel, ReleaseAsset};
    use semver::Version;

    #[test]
    fn checksum_parsing_handles_plain_and_binary() {
        let sums = "aabbcc  OrbitFM-1.0.0-x86_64.AppImage\nDDEEFF *other.AppImage\n";
        assert_eq!(
            checksum_for(sums, "OrbitFM-1.0.0-x86_64.AppImage"),
            Some("aabbcc".to_owned())
        );
        assert_eq!(
            checksum_for(sums, "other.AppImage"),
            Some("ddeeff".to_owned())
        );
        assert_eq!(checksum_for(sums, "missing"), None);
    }

    fn release_with(name: &str, has_sums: bool) -> Release {
        let mut assets = vec![ReleaseAsset {
            name: name.to_owned(),
            url: "https://github.com/x".to_owned(),
            size: 1,
        }];
        if has_sums {
            assets.push(ReleaseAsset {
                name: "SHA256SUMS".to_owned(),
                url: "https://github.com/sums".to_owned(),
                size: 1,
            });
        }
        Release {
            tag: "v1.0.0".to_owned(),
            version: Version::parse("1.0.0").unwrap(),
            channel: Channel::Stable,
            name: None,
            notes: None,
            published_at: None,
            html_url: None,
            assets,
        }
    }

    #[test]
    fn missing_checksums_is_refused_before_download() {
        let release = release_with("OrbitFM-1.0.0-x86_64.AppImage", false);
        assert_eq!(release.checksums_url(), None);
    }
}
