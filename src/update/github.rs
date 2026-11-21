//! Источник релизов — GitHub Releases (ADR-0067).
//!
//! Только HTTPS и хосты из allowlist; обязательный `User-Agent`; черновики и
//! теги, не являющиеся semver, отбрасываются.

use reqwest::Client;
use semver::Version;
use serde::Deserialize;

use super::{Channel, Release, ReleaseAsset, UpdateError};

const API: &str = "https://api.github.com";
const ACCEPT: &str = "application/vnd.github+json";
const GITHUB_API_VERSION: &str = "2022-11-28";
const REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);
/// Хосты, с которых разрешено читать релизы и ассеты (ADR-0067 §7).
const ALLOWED_HOSTS: [&str; 3] = [
    "api.github.com",
    "github.com",
    "objects.githubusercontent.com",
];

/// Источник списка релизов. Трейт позволяет подставить фейк в тестах.
pub trait UpdateSource: Send + Sync {
    async fn releases(&self) -> Result<Vec<Release>, UpdateError>;
}

/// Источник GitHub Releases.
pub struct GithubSource {
    client: Client,
    repo: String,
}

impl GithubSource {
    pub fn new(repo: impl Into<String>) -> Result<Self, UpdateError> {
        let client = Client::builder()
            .user_agent(user_agent())
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(|error| UpdateError::Network(error.to_string()))?;
        Ok(Self {
            client,
            repo: repo.into(),
        })
    }

    pub fn orbitfm() -> Result<Self, UpdateError> {
        Self::new("ModernByte/orbitfm")
    }
}

/// `User-Agent` обязателен по правилам GitHub API.
pub fn user_agent() -> String {
    format!("OrbitFM/1.0 (ModernByte; {}; Rust)", std::env::consts::OS)
}

impl UpdateSource for GithubSource {
    async fn releases(&self) -> Result<Vec<Release>, UpdateError> {
        let url = format!("{API}/repos/{}/releases?per_page=30", self.repo);
        let response = self
            .client
            .get(&url)
            .header("Accept", ACCEPT)
            .header("X-GitHub-Api-Version", GITHUB_API_VERSION)
            .send()
            .await
            .map_err(|error| UpdateError::Network(error.to_string()))?;
        if !response.status().is_success() {
            return Err(UpdateError::Http(format!("HTTP {}", response.status())));
        }
        let body = response
            .text()
            .await
            .map_err(|error| UpdateError::Network(error.to_string()))?;
        parse_releases(&body)
    }
}

#[derive(Deserialize)]
struct GhRelease {
    tag_name: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    published_at: Option<String>,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    html_url: Option<String>,
    #[serde(default)]
    assets: Vec<GhAsset>,
}

#[derive(Deserialize)]
struct GhAsset {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    size: u64,
}

/// Разбор ответа GitHub. Черновики, непарсящиеся теги и ассеты с недопустимым
/// хостом отбрасываются либо дают ошибку.
pub fn parse_releases(body: &str) -> Result<Vec<Release>, UpdateError> {
    let raw: Vec<GhRelease> =
        serde_json::from_str(body).map_err(|error| UpdateError::Parse(error.to_string()))?;
    let mut releases = Vec::with_capacity(raw.len());
    for item in raw {
        if item.draft {
            continue;
        }
        let Some(version) = parse_tag(&item.tag_name) else {
            continue;
        };
        let mut assets = Vec::with_capacity(item.assets.len());
        for asset in item.assets {
            validate_host(&asset.browser_download_url)?;
            assets.push(ReleaseAsset {
                name: asset.name,
                url: asset.browser_download_url,
                size: asset.size,
            });
        }
        releases.push(Release {
            tag: item.tag_name,
            version,
            channel: if item.prerelease {
                Channel::Dev
            } else {
                Channel::Stable
            },
            name: item.name,
            notes: item.body,
            published_at: item.published_at,
            html_url: item.html_url,
            assets,
        });
    }
    Ok(releases)
}

fn parse_tag(tag: &str) -> Option<Version> {
    tag.trim_start_matches('v').parse().ok()
}

/// Хост ассета обязан быть HTTPS и входить в allowlist.
pub fn validate_host(url: &str) -> Result<(), UpdateError> {
    let parsed = reqwest::Url::parse(url).map_err(|_| UpdateError::Host(url.to_owned()))?;
    if parsed.scheme() != "https" {
        return Err(UpdateError::Host(url.to_owned()));
    }
    let host = parsed.host_str().unwrap_or_default();
    if ALLOWED_HOSTS.contains(&host) {
        Ok(())
    } else {
        Err(UpdateError::Host(host.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = r#"[
      {"tag_name":"v0.23.0-rc.3","name":"0.23.0-rc.3","body":"notes rc","prerelease":true,
       "draft":false,"published_at":"2026-11-19T08:00:00Z",
       "html_url":"https://github.com/ModernByte/orbitfm/releases/tag/v0.23.0-rc.3",
       "assets":[{"name":"OrbitFM-0.23.0-rc.3-x86_64.AppImage",
                  "browser_download_url":"https://github.com/ModernByte/orbitfm/releases/download/v0.23.0-rc.3/x.AppImage",
                  "size":100}]},
      {"tag_name":"v0.22.0","name":"0.22.0","body":"stable notes","prerelease":false,
       "draft":false,"published_at":"2026-11-01T08:00:00Z",
       "html_url":"https://github.com/ModernByte/orbitfm/releases/tag/v0.22.0",
       "assets":[{"name":"OrbitFM-0.22.0-x86_64.AppImage",
                  "browser_download_url":"https://github.com/ModernByte/orbitfm/releases/download/v0.22.0/y.AppImage",
                  "size":200},
                 {"name":"SHA256SUMS",
                  "browser_download_url":"https://github.com/ModernByte/orbitfm/releases/download/v0.22.0/SHA256SUMS",
                  "size":50}]},
      {"tag_name":"draft-tag","prerelease":false,"draft":true,"assets":[]},
      {"tag_name":"nightly","prerelease":true,"draft":false,"assets":[]}
    ]"#;

    #[test]
    fn parses_and_filters_releases() {
        let releases = parse_releases(FIXTURE).expect("parse");
        // Черновик и не-semver тег отброшены.
        assert_eq!(releases.len(), 2);
        let stable = releases
            .iter()
            .find(|release| release.channel == Channel::Stable)
            .expect("stable");
        assert_eq!(stable.version.to_string(), "0.22.0");
        assert_eq!(stable.notes.as_deref(), Some("stable notes"));
        assert!(stable.checksums_url().is_some());
        assert!(stable.published_at.is_some());
    }

    #[test]
    fn rejects_foreign_asset_host() {
        let body = r#"[{"tag_name":"v1.0.0","prerelease":false,"draft":false,
          "assets":[{"name":"x.AppImage","browser_download_url":"https://evil.example/x","size":1}]}]"#;
        assert!(matches!(parse_releases(body), Err(UpdateError::Host(_))));
        let http = r#"[{"tag_name":"v1.0.0","assets":[{"name":"x","browser_download_url":"http://github.com/x","size":1}]}]"#;
        assert!(matches!(parse_releases(http), Err(UpdateError::Host(_))));
    }

    #[test]
    fn user_agent_carries_brand() {
        let ua = user_agent();
        assert!(ua.starts_with("OrbitFM/1.0 (ModernByte;"));
    }
}
