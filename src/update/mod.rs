//! Проверка и установка обновлений (ADR-0067).
//!
//! Два канала: `stable` (публичные релизы `vX.Y.Z` без суффикса) и `dev`
//! (предрелизы `vX.Y.Z-rc.N`, помеченные `prerelease`). Источник — GitHub
//! Releases репозитория `ModernByte/orbitfm`; сравнение версий — `semver`.

pub mod cli;
pub mod github;
pub mod scheduler;

use semver::Version;
use serde::{Deserialize, Serialize};

/// Канал релизов.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Channel {
    /// Публичный канал: финальные `vX.Y.Z`.
    #[default]
    Stable,
    /// Канал разработки: предрелизы `vX.Y.Z-rc.N`.
    Dev,
}

impl Channel {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "stable" => Some(Self::Stable),
            "dev" => Some(Self::Dev),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::Dev => "dev",
        }
    }
}

/// Ассет релиза.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseAsset {
    pub name: String,
    pub url: String,
    pub size: u64,
}

/// Один релиз GitHub.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub tag: String,
    pub version: Version,
    pub channel: Channel,
    pub name: Option<String>,
    pub notes: Option<String>,
    pub published_at: Option<String>,
    pub html_url: Option<String>,
    pub assets: Vec<ReleaseAsset>,
}

impl Release {
    /// Ассет платформы (Linux — AppImage, Windows — MSI, macOS — DMG).
    pub fn platform_asset(&self) -> Option<&ReleaseAsset> {
        let matches = |asset: &ReleaseAsset| {
            if cfg!(target_os = "linux") {
                asset.name.ends_with(".AppImage")
            } else if cfg!(target_os = "windows") {
                asset.name.ends_with(".msi")
            } else if cfg!(target_os = "macos") {
                asset.name.ends_with(".dmg") && asset.name.contains(std::env::consts::ARCH)
            } else {
                false
            }
        };
        // На macOS имя ассета содержит `arm64`/`x86_64`, а не `aarch64`/`x86_64`.
        self.assets.iter().find(|asset| matches(asset)).or_else(|| {
            self.assets
                .iter()
                .find(|asset| asset.name.ends_with(".dmg"))
        })
    }

    /// URL ассета контрольных сумм, если он есть в релизе.
    pub fn checksums_url(&self) -> Option<&str> {
        self.assets
            .iter()
            .find(|asset| asset.name == "SHA256SUMS")
            .map(|asset| asset.url.as_str())
    }
}

/// Состояние проверки обновлений (кэш последнего результата).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateState {
    pub channel: Channel,
    pub current: Version,
    pub latest: Option<Release>,
    pub checked_at: Option<String>,
}

impl UpdateState {
    pub fn new(channel: Channel, current: Version) -> Self {
        Self {
            channel,
            current,
            latest: None,
            checked_at: None,
        }
    }

    /// Доступно ли обновление в текущем канале.
    pub fn available(&self) -> bool {
        self.latest
            .as_ref()
            .is_some_and(|release| release.version > self.current)
    }
}

/// Компактный срез для аддитивного поля `update` в `status --json` (схема 1).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct StatusUpdate {
    pub channel: &'static str,
    pub current: String,
    pub latest: Option<String>,
    pub available: bool,
}

impl UpdateState {
    /// Срез для `status` (`docs/interfaces/19-update.md` §3).
    pub fn status(&self) -> StatusUpdate {
        StatusUpdate {
            channel: self.channel.as_str(),
            current: self.current.to_string(),
            latest: self
                .latest
                .as_ref()
                .map(|release| release.version.to_string()),
            available: self.available(),
        }
    }
}

/// Одна сетевая проверка: получить релизы канала и собрать состояние.
pub async fn check_once(channel: Channel, current: Version) -> Result<UpdateState, UpdateError> {
    use github::UpdateSource as _;

    let source = github::GithubSource::orbitfm()?;
    let releases = source.releases().await?;
    let mut state = UpdateState::new(channel, current);
    state.latest = pick_release(&releases, channel);
    state.checked_at = Some(format_utc_now());
    Ok(state)
}

/// Выбрать максимальный релиз канала (сравнение по semver).
pub fn pick_release(releases: &[Release], channel: Channel) -> Option<Release> {
    releases
        .iter()
        .filter(|release| release.channel == channel)
        .max_by(|left, right| left.version.cmp(&right.version))
        .cloned()
}

/// Ошибки подсистемы обновлений.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum UpdateError {
    #[error("сеть обновлений: {0}")]
    Network(String),
    #[error("ответ GitHub: {0}")]
    Http(String),
    #[error("некорректный JSON релизов: {0}")]
    Parse(String),
    #[error("недопустимый хост: {0}")]
    Host(String),
}

/// Текущее время UTC в формате RFC 3339 (`ГГГГ-ММ-ДДTЧЧ:ММ:ССZ`).
pub fn format_utc_now() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or_default(|duration| duration.as_secs()) as i64;
    format_utc(secs)
}

fn format_utc(secs: i64) -> String {
    let (year, month, day) = civil_from_days(secs.div_euclid(86_400));
    let seconds = secs.rem_euclid(86_400);
    let (hour, minute, second) = (seconds / 3600, (seconds % 3600) / 60, seconds % 60);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

/// `civil_from_days` без внешних зависимостей.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    let year = if month <= 2 { year + 1 } else { year };
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(tag: &str, prerelease: bool) -> Release {
        Release {
            tag: tag.to_owned(),
            version: tag.trim_start_matches('v').parse().expect("semver"),
            channel: if prerelease {
                Channel::Dev
            } else {
                Channel::Stable
            },
            name: None,
            notes: None,
            published_at: None,
            html_url: None,
            assets: Vec::new(),
        }
    }

    #[test]
    fn channel_parsing() {
        assert_eq!(Channel::parse("stable"), Some(Channel::Stable));
        assert_eq!(Channel::parse("dev"), Some(Channel::Dev));
        assert_eq!(Channel::parse("nope"), None);
        assert_eq!(Channel::default(), Channel::Stable);
    }

    #[test]
    fn picks_highest_version_in_channel() {
        let releases = vec![
            release("v0.23.0-rc.1", true),
            release("v0.23.0-rc.3", true),
            release("v0.22.0", false),
        ];
        assert_eq!(
            pick_release(&releases, Channel::Dev).map(|r| r.tag),
            Some("v0.23.0-rc.3".to_owned())
        );
        assert_eq!(
            pick_release(&releases, Channel::Stable).map(|r| r.tag),
            Some("v0.22.0".to_owned())
        );
    }

    #[test]
    fn final_release_outranks_prerelease() {
        assert!(release("v0.24.0", false).version > release("v0.24.0-rc.5", true).version);
        assert!(release("v0.24.0-rc.2", true).version > release("v0.24.0-rc.1", true).version);
    }

    #[test]
    fn utc_format_is_rfc3339() {
        assert_eq!(format_utc(0), "1970-01-01T00:00:00Z");
        // 2026-11-21T00:00:00Z — день из плана.
        assert_eq!(format_utc(1_795_219_200), "2026-11-21T00:00:00Z");
    }

    #[test]
    fn status_slice_reports_channel_and_availability() {
        let mut state = UpdateState::new(Channel::Dev, "0.23.0-rc.1".parse().unwrap());
        assert_eq!(state.status().latest, None);
        state.latest = Some(release("v0.23.0-rc.3", true));
        let value = serde_json::to_value(state.status()).expect("json");
        assert_eq!(value["channel"], "dev");
        assert_eq!(value["available"], true);
        assert_eq!(value["latest"], "0.23.0-rc.3");
        assert_eq!(value["current"], "0.23.0-rc.1");
    }

    #[test]
    fn availability_compares_against_current() {
        let mut state = UpdateState::new(Channel::Stable, "0.23.0".parse().unwrap());
        assert!(!state.available());
        state.latest = Some(release("v0.24.0", false));
        assert!(state.available());
        state.latest = Some(release("v0.23.0", false));
        assert!(!state.available());
    }
}
