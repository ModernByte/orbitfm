//! UI-модели: станции, снимок состояния, библиотека и кадры.
//!
//! Данные приходят только из IPC ядра (`status`/`library`/`events`). Здесь нет
//! ни процессов, ни ratatui — только сериализуемые структуры и их инварианты
//! (`docs/interfaces/16-tui.md` §5).

use serde::Deserialize;

/// Состояние эфира — подмножество `status.state` схемы 1.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayState {
    Playing,
    Paused,
    Stopped,
    Buffering,
    Reconnecting,
    Error,
}

impl PlayState {
    /// Разбор строки `status.state`; неизвестное — `Stopped` (не паника).
    pub fn from_status(value: &str) -> Self {
        match value {
            "playing" => Self::Playing,
            "paused" => Self::Paused,
            "buffering" => Self::Buffering,
            "reconnecting" => Self::Reconnecting,
            "error" => Self::Error,
            _ => Self::Stopped,
        }
    }

    /// Английское слово состояния (служебные подписи TUI v1 — английские).
    pub fn word(self) -> &'static str {
        match self {
            Self::Playing => "playing",
            Self::Paused => "paused",
            Self::Stopped => "stopped",
            Self::Buffering => "buffering",
            Self::Reconnecting => "reconnecting",
            Self::Error => "error",
        }
    }
}

/// Компактная станция: `library`/`search`/`status.station`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Default)]
pub struct Station {
    #[serde(default)]
    pub uuid: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub country: String,
    #[serde(default)]
    pub countrycode: String,
    #[serde(default)]
    pub tags: String,
    #[serde(default)]
    pub bitrate: u32,
    #[serde(default)]
    pub codec: String,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub favorite: bool,
}

impl Station {
    /// Пустая станция (нет данных) — рисуется как «ничего не играет».
    pub fn is_empty(&self) -> bool {
        self.uuid.is_empty() && self.name.is_empty()
    }
}

/// Доступность skip-команд.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Default)]
pub struct Can {
    #[serde(default)]
    pub previous: bool,
    #[serde(default)]
    pub next: bool,
}

/// Активный таймер сна.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct SleepState {
    #[serde(default)]
    pub minutes: u32,
    #[serde(default)]
    pub remaining_secs: u64,
}

/// Снимок `status --json` схемы 1 (только то, что нужно TUI).
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct StatusSnapshot {
    #[serde(default)]
    pub schema: u32,
    #[serde(default)]
    pub app: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub station: Option<Station>,
    #[serde(default)]
    pub volume: f32,
    #[serde(default)]
    pub can: Can,
    #[serde(default)]
    pub sleep: Option<SleepState>,
}

impl StatusSnapshot {
    pub fn play_state(&self) -> PlayState {
        PlayState::from_status(&self.state)
    }

    /// Громкость в процентах, зажатая в `0..=100`.
    pub fn volume_percent(&self) -> u8 {
        (self.volume * 100.0).round().clamp(0.0, 100.0) as u8
    }

    /// Станция, пригодная для отрисовки (пустая = None).
    pub fn current_station(&self) -> Option<&Station> {
        self.station.as_ref().filter(|station| !station.is_empty())
    }

    /// Совместимость: `schema == 1`, `app == orbitfm`.
    pub fn is_compatible(&self) -> bool {
        self.schema == 1 && self.app == "orbitfm"
    }
}

/// Визуализатор (`v` циклично).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VisualizerKind {
    Bars,
    Ring,
    Scope,
    Pulse,
}

impl VisualizerKind {
    pub const ALL: [Self; 4] = [Self::Bars, Self::Ring, Self::Scope, Self::Pulse];

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "bars" => Some(Self::Bars),
            "ring" => Some(Self::Ring),
            "scope" => Some(Self::Scope),
            "pulse" => Some(Self::Pulse),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Bars => "bars",
            Self::Ring => "ring",
            Self::Scope => "scope",
            Self::Pulse => "pulse",
        }
    }

    pub fn next(self) -> Self {
        let index =
            (Self::ALL.iter().position(|kind| *kind == self).unwrap_or(0) + 1) % Self::ALL.len();
        Self::ALL[index]
    }
}

/// Режим движения (`M`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Motion {
    Full,
    Reduced,
    Off,
}

impl Motion {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "full" => Some(Self::Full),
            "reduced" => Some(Self::Reduced),
            "off" => Some(Self::Off),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Reduced => "reduced",
            Self::Off => "off",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Full => Self::Reduced,
            Self::Reduced => Self::Off,
            Self::Off => Self::Full,
        }
    }

    /// Длительность тика цикла.
    pub fn tick_ms(self) -> u64 {
        match self {
            Self::Full => 66,
            Self::Reduced => 250,
            Self::Off => 1000,
        }
    }
}

/// Пресет компоновки (`l`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayoutPreset {
    Desk,
    Focus,
    Minimal,
}

impl LayoutPreset {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "desk" => Some(Self::Desk),
            "focus" => Some(Self::Focus),
            "minimal" => Some(Self::Minimal),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Desk => "desk",
            Self::Focus => "focus",
            Self::Minimal => "minimal",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Desk => Self::Focus,
            Self::Focus => Self::Minimal,
            Self::Minimal => Self::Desk,
        }
    }
}

/// Избранное и история (`orbitfm library`), до 20 записей на список.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Default)]
pub struct LibraryPayload {
    #[serde(default)]
    pub favorites: Vec<Station>,
    #[serde(default)]
    pub history: Vec<Station>,
}

/// Кадр `search` — результат запроса, сопоставляется по `id`.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct SearchFrame {
    #[serde(default)]
    pub id: u64,
    #[serde(default)]
    pub query: String,
    #[serde(default)]
    pub results: Vec<Station>,
    #[serde(default)]
    pub error: Option<String>,
}

/// Нормированный кадр спектра для визуализатора (`events --spectrum`).
///
/// Часть полей читает визуализатор в волне 4; сейчас кадр хранится целиком.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct SpectrumFrame {
    pub seq: u64,
    pub level: f32,
    pub centroid_hz: f32,
    pub spread_hz: f32,
    pub flatness: f32,
    pub flux: f32,
    pub bands: Option<SpectrumBands>,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct SpectrumBands {
    pub bass: f32,
    pub mid: f32,
    pub high: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn play_state_parsing() {
        assert_eq!(PlayState::from_status("playing"), PlayState::Playing);
        assert_eq!(PlayState::from_status("error"), PlayState::Error);
        assert_eq!(PlayState::from_status("nonsense"), PlayState::Stopped);
    }

    #[test]
    fn volume_clamps_and_rounds() {
        let mut snapshot = snapshot("playing");
        snapshot.volume = 0.755;
        assert_eq!(snapshot.volume_percent(), 76);
        snapshot.volume = 2.0;
        assert_eq!(snapshot.volume_percent(), 100);
        snapshot.volume = -1.0;
        assert_eq!(snapshot.volume_percent(), 0);
    }

    #[test]
    fn station_null_is_not_current() {
        let mut snapshot = snapshot("stopped");
        snapshot.station = None;
        assert!(snapshot.current_station().is_none());
        snapshot.station = Some(Station::default());
        assert!(snapshot.current_station().is_none());
    }

    #[test]
    fn setting_enums_roundtrip_and_cycle() {
        assert_eq!(VisualizerKind::parse("ring"), Some(VisualizerKind::Ring));
        assert_eq!(VisualizerKind::Bars.next(), VisualizerKind::Ring);
        assert_eq!(VisualizerKind::Pulse.next(), VisualizerKind::Bars);
        assert_eq!(Motion::parse("reduced"), Some(Motion::Reduced));
        assert_eq!(Motion::Off.next(), Motion::Full);
        assert_eq!(LayoutPreset::Minimal.next(), LayoutPreset::Desk);
    }

    #[test]
    fn compatibility_gate() {
        assert!(snapshot("playing").is_compatible());
        let mut other = snapshot("playing");
        other.schema = 2;
        assert!(!other.is_compatible());
        other.schema = 1;
        other.app = "someone-else".to_owned();
        assert!(!other.is_compatible());
    }

    fn snapshot(state: &str) -> StatusSnapshot {
        StatusSnapshot {
            schema: 1,
            app: "orbitfm".to_owned(),
            version: "0.23.0".to_owned(),
            state: state.to_owned(),
            station: None,
            volume: 0.5,
            can: Can::default(),
            sleep: None,
        }
    }
}
