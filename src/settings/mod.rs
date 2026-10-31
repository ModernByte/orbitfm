//! Пользовательские настройки оформления с локальной персистентностью.

use std::env;
use std::ffi::OsString;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub mod language;

pub use language::Language;

const CONFIG_DIR: &str = "orbitfm";
const FILE_NAME: &str = "settings.json";
const TMP_FILE_NAME: &str = "settings.json.tmp";

/// Режим темы интерфейса.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    #[default]
    System,
    Dark,
    Light,
}

impl ThemeMode {
    pub const ALL: [Self; 3] = [Self::System, Self::Dark, Self::Light];

    /// Имя темы в IPC, JSON-статусе и установщике интеграций (ADR-0037).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Dark => "dark",
            Self::Light => "light",
        }
    }
}

/// Акцентный цвет интерфейса и маркеров.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AccentColor {
    #[default]
    Blue,
    Green,
    Amber,
    Violet,
    Rose,
    Cyan,
}

impl AccentColor {
    pub const ALL: [Self; 6] = [
        Self::Blue,
        Self::Green,
        Self::Amber,
        Self::Violet,
        Self::Rose,
        Self::Cyan,
    ];

    pub fn rgb(self) -> [u8; 3] {
        match self {
            Self::Blue => [108, 176, 255],
            Self::Green => [104, 211, 145],
            Self::Amber => [255, 190, 92],
            Self::Violet => [178, 148, 255],
            Self::Rose => [255, 138, 165],
            Self::Cyan => [92, 214, 224],
        }
    }
}

/// Реактивность аудио-движения глобуса (ADR-0028).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MotionReactivity {
    Off,
    Low,
    #[default]
    Normal,
    High,
}

impl MotionReactivity {
    pub const ALL: [Self; 4] = [Self::Off, Self::Low, Self::Normal, Self::High];

    pub fn is_enabled(self) -> bool {
        self != Self::Off
    }

    /// Максимальное сжатие по вертикали, доли единицы.
    pub fn squash_limit(self) -> f32 {
        match self {
            Self::Off => 0.0,
            Self::Low => 0.025,
            Self::Normal => 0.050,
            Self::High => 0.090,
        }
    }

    /// Максимальная амплитуда ряби, доли радиуса.
    pub fn ripple_limit(self) -> f32 {
        match self {
            Self::Off => 0.0,
            Self::Low => 0.0050,
            Self::Normal => 0.0100,
            Self::High => 0.0180,
        }
    }

    /// Амплитуда idle-дыхания, доли единицы.
    pub fn idle_amplitude(self) -> f32 {
        match self {
            Self::Off => 0.0,
            Self::Low | Self::Normal => 0.0070,
            Self::High => 0.0100,
        }
    }

    /// Ограничение сверху для режима окна: Overlay не выше Low.
    pub fn capped(self, cap: Self) -> Self {
        if self.rank() <= cap.rank() { self } else { cap }
    }

    fn rank(self) -> u8 {
        match self {
            Self::Off => 0,
            Self::Low => 1,
            Self::Normal => 2,
            Self::High => 3,
        }
    }
}

/// Характер аудио-движения глобуса (ADR-0028).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MotionStyle {
    #[default]
    Auto,
    SoftPulse,
    GrooveSway,
    BassDrop,
    AmbientDrift,
    SparkleTwitch,
    BeatMarch,
}

impl MotionStyle {
    pub const ALL: [Self; 7] = [
        Self::Auto,
        Self::SoftPulse,
        Self::GrooveSway,
        Self::BassDrop,
        Self::AmbientDrift,
        Self::SparkleTwitch,
        Self::BeatMarch,
    ];

    /// Номер стиля для `CameraUniform.motion2.y`; Auto — 0.
    pub fn id(self) -> f32 {
        match self {
            Self::Auto => 0.0,
            Self::SoftPulse => 1.0,
            Self::GrooveSway => 2.0,
            Self::BassDrop => 3.0,
            Self::AmbientDrift => 4.0,
            Self::SparkleTwitch => 5.0,
            Self::BeatMarch => 6.0,
        }
    }

    /// Стилю нужны полосы Bass/Mid/High; без них он деградирует.
    pub fn requires_bands(self) -> bool {
        matches!(self, Self::BassDrop | Self::SparkleTwitch | Self::BeatMarch)
    }
}

/// Настройки оформления и поведения.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub theme: ThemeMode,
    pub accent: AccentColor,
    /// Язык интерфейса; `System` разрешается при запуске (ADR-0030).
    #[serde(deserialize_with = "language::deserialize_language")]
    pub language: Language,
    /// Число столбцов визуализатора: 16, 24 или 32 (ADR-0019).
    pub viz_bars: u8,
    /// Громкость 0.0..=1.0; сохраняется на отпускании слайдера (ADR-0031).
    pub volume: f64,
    /// Видимость визуализатора и аудиокольца (ADR-0031).
    pub visualizer_enabled: bool,
    /// Видимость карточки управления глобусом; по умолчанию включена.
    pub help_visible: bool,
    /// Реактивность аудио-движения глобуса (ADR-0028).
    pub motion: MotionReactivity,
    /// Характер движения; Auto выбирает по сигналу.
    pub motion_style: MotionStyle,
    /// Перевороты камеры на сильном onset; по умолчанию выключены.
    pub allow_flips: bool,
    /// Reduce motion: геометрия и качка неподвижны, эфир продолжается.
    pub reduce_motion: bool,
    /// Opt-in загрузки обложек по HTTP (ADR-0025, п. 11); по умолчанию только
    /// HTTPS, а http-ссылка беззвучно повторяется как https.
    pub artwork_allow_http: bool,
}

/// Доступные варианты числа столбцов визуализатора.
pub const VIZ_BAR_CHOICES: [u8; 3] = [16, 24, 32];
/// Значение по умолчанию (ТЗ: 24 столбца).
pub const DEFAULT_VIZ_BARS: u8 = 24;
/// Стартовая громкость: mpv по умолчанию звучит громче, чем нужно ночью.
pub const DEFAULT_VOLUME: f64 = 0.7;

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: ThemeMode::System,
            accent: AccentColor::Blue,
            language: Language::System,
            viz_bars: DEFAULT_VIZ_BARS,
            volume: DEFAULT_VOLUME,
            visualizer_enabled: true,
            help_visible: true,
            motion: MotionReactivity::Normal,
            motion_style: MotionStyle::Auto,
            allow_flips: false,
            reduce_motion: false,
            artwork_allow_http: false,
        }
    }
}

/// Ошибки хранилища настроек.
#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    #[error("не удалось определить путь настроек: {0}")]
    Path(String),
    #[error("не удалось прочитать настройки: {0}")]
    Read(String),
    #[error("не удалось разобрать настройки: {0}")]
    Parse(String),
    #[error("не удалось сохранить настройки: {0}")]
    Write(String),
}

/// Настройки оформления с сохранением в файл.
///
/// Хранилище с `path = None` работает только в памяти: сохранения
/// становятся no-op, что позволяет пережить повреждённый файл без его потери.
#[derive(Debug)]
pub struct SettingsStore {
    path: Option<PathBuf>,
    settings: Settings,
}

impl SettingsStore {
    /// Загружает настройки из файла. Отсутствующий файл — значения по умолчанию.
    pub fn load(path: PathBuf) -> Result<Self, SettingsError> {
        let mut settings = match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice::<Settings>(&bytes)
                .map_err(|err| SettingsError::Parse(format!("{}: {err}", path.display())))?,
            Err(err) if err.kind() == ErrorKind::NotFound => Settings::default(),
            Err(err) => return Err(SettingsError::Read(format!("{}: {err}", path.display()))),
        };
        settings.viz_bars = nearest_viz_bars(settings.viz_bars);
        settings.volume = sanitized_volume(settings.volume);
        Ok(Self {
            path: Some(path),
            settings,
        })
    }

    /// Загружает настройки из стандартного каталога конфигурации XDG.
    pub fn load_default() -> Result<Self, SettingsError> {
        Self::load(default_path()?)
    }

    /// Хранилище без файла: изменения живут до завершения процесса.
    pub fn ephemeral() -> Self {
        Self {
            path: None,
            settings: Settings::default(),
        }
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn settings(&self) -> Settings {
        self.settings
    }

    pub fn set_theme(&mut self, theme: ThemeMode) -> Result<(), SettingsError> {
        if self.settings.theme == theme {
            return Ok(());
        }
        self.settings.theme = theme;
        self.save()
    }

    pub fn set_accent(&mut self, accent: AccentColor) -> Result<(), SettingsError> {
        if self.settings.accent == accent {
            return Ok(());
        }
        self.settings.accent = accent;
        self.save()
    }

    pub fn set_language(&mut self, language: Language) -> Result<(), SettingsError> {
        if self.settings.language == language {
            return Ok(());
        }
        self.settings.language = language;
        self.save()
    }

    pub fn set_viz_bars(&mut self, bars: u8) -> Result<(), SettingsError> {
        let bars = nearest_viz_bars(bars);
        if self.settings.viz_bars == bars {
            return Ok(());
        }
        self.settings.viz_bars = bars;
        self.save()
    }

    pub fn set_volume(&mut self, volume: f64) -> Result<(), SettingsError> {
        let volume = sanitized_volume(volume);
        if (self.settings.volume - volume).abs() < f64::EPSILON {
            return Ok(());
        }
        self.settings.volume = volume;
        self.save()
    }

    pub fn set_visualizer_enabled(&mut self, enabled: bool) -> Result<(), SettingsError> {
        if self.settings.visualizer_enabled == enabled {
            return Ok(());
        }
        self.settings.visualizer_enabled = enabled;
        self.save()
    }

    /// Запоминает выбранную видимость карточки управления (ADR-0031, ADR-0036).
    pub fn set_help_visible(&mut self, visible: bool) -> Result<(), SettingsError> {
        if self.settings.help_visible == visible {
            return Ok(());
        }
        self.settings.help_visible = visible;
        self.save()
    }

    pub fn set_motion(&mut self, motion: MotionReactivity) -> Result<(), SettingsError> {
        if self.settings.motion == motion {
            return Ok(());
        }
        self.settings.motion = motion;
        self.save()
    }

    pub fn set_reduce_motion(&mut self, reduce: bool) -> Result<(), SettingsError> {
        if self.settings.reduce_motion == reduce {
            return Ok(());
        }
        self.settings.reduce_motion = reduce;
        self.save()
    }

    pub fn set_allow_flips(&mut self, allow: bool) -> Result<(), SettingsError> {
        if self.settings.allow_flips == allow {
            return Ok(());
        }
        self.settings.allow_flips = allow;
        self.save()
    }

    /// Opt-in HTTP-обложек (ADR-0025, п. 11): по умолчанию выключен.
    pub fn set_artwork_allow_http(&mut self, allow: bool) -> Result<(), SettingsError> {
        if self.settings.artwork_allow_http == allow {
            return Ok(());
        }
        self.settings.artwork_allow_http = allow;
        self.save()
    }

    pub fn set_motion_style(&mut self, style: MotionStyle) -> Result<(), SettingsError> {
        if self.settings.motion_style == style {
            return Ok(());
        }
        self.settings.motion_style = style;
        self.save()
    }

    fn save(&self) -> Result<(), SettingsError> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        let directory = path
            .parent()
            .ok_or_else(|| SettingsError::Write(format!("нет каталога для {}", path.display())))?;
        fs::create_dir_all(directory)
            .map_err(|err| SettingsError::Write(format!("{}: {err}", directory.display())))?;
        let json = serde_json::to_vec_pretty(&self.settings)
            .map_err(|err| SettingsError::Write(err.to_string()))?;
        let tmp = path.with_file_name(TMP_FILE_NAME);
        fs::write(&tmp, json)
            .map_err(|err| SettingsError::Write(format!("{}: {err}", tmp.display())))?;
        fs::rename(&tmp, path).map_err(|err| {
            let _ = fs::remove_file(&tmp);
            SettingsError::Write(format!("{}: {err}", path.display()))
        })
    }
}

pub(crate) fn sanitized_volume(volume: f64) -> f64 {
    if volume.is_finite() {
        volume.clamp(0.0, 1.0)
    } else {
        DEFAULT_VOLUME
    }
}

/// Ближайший допустимый вариант числа столбцов: файл настроек правят руками.
fn nearest_viz_bars(bars: u8) -> u8 {
    VIZ_BAR_CHOICES
        .iter()
        .copied()
        .min_by_key(|choice| (i16::from(*choice) - i16::from(bars)).abs())
        .unwrap_or(DEFAULT_VIZ_BARS)
}

/// Стандартный путь настроек по XDG Base Directory Specification.
pub fn default_path() -> Result<PathBuf, SettingsError> {
    default_path_from(env::var_os("XDG_CONFIG_HOME"), env::var_os("HOME"))
}

fn default_path_from(
    xdg_config_home: Option<OsString>,
    home: Option<OsString>,
) -> Result<PathBuf, SettingsError> {
    if let Some(xdg) = xdg_config_home {
        let path = PathBuf::from(xdg);
        if path.is_absolute() {
            return Ok(path.join(CONFIG_DIR).join(FILE_NAME));
        }
    }
    let home = home.ok_or_else(|| SettingsError::Path("HOME не задан".to_owned()))?;
    let home = PathBuf::from(home);
    if !home.is_absolute() {
        return Err(SettingsError::Path(format!(
            "HOME должен быть абсолютным путём: {}",
            home.display()
        )));
    }
    Ok(home.join(".config").join(CONFIG_DIR).join(FILE_NAME))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_path_prefers_absolute_xdg_config_home() {
        let path = default_path_from(Some("/tmp/xdg".into()), Some("/home/user".into())).unwrap();
        assert_eq!(path, PathBuf::from("/tmp/xdg/orbitfm/settings.json"));
    }

    #[test]
    fn default_path_falls_back_to_home_for_relative_xdg() {
        let path = default_path_from(Some("relative".into()), Some("/home/user".into())).unwrap();
        assert_eq!(
            path,
            PathBuf::from("/home/user/.config/orbitfm/settings.json")
        );
        assert!(default_path_from(None, None).is_err());
    }

    #[test]
    fn missing_file_uses_defaults() {
        let directory = tempfile::tempdir().unwrap();
        let store = SettingsStore::load(directory.path().join("settings.json")).unwrap();
        assert_eq!(store.settings(), Settings::default());
    }

    #[test]
    fn roundtrip_persists_theme_and_accent() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("nested/settings.json");
        {
            let mut store = SettingsStore::load(path.clone()).unwrap();
            store.set_theme(ThemeMode::Light).unwrap();
            store.set_accent(AccentColor::Amber).unwrap();
        }

        let store = SettingsStore::load(path).unwrap();
        assert_eq!(store.settings().theme, ThemeMode::Light);
        assert_eq!(store.settings().accent, AccentColor::Amber);
    }

    #[test]
    fn missing_fields_use_defaults() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        fs::write(&path, b"{\"theme\":\"dark\"}").unwrap();

        let store = SettingsStore::load(path).unwrap();
        assert_eq!(store.settings().theme, ThemeMode::Dark);
        assert_eq!(store.settings().accent, AccentColor::Blue);
        assert_eq!(store.settings().language, Language::System);
        assert_eq!(store.settings().viz_bars, DEFAULT_VIZ_BARS);
        assert!((store.settings().volume - DEFAULT_VOLUME).abs() < f64::EPSILON);
        assert!(store.settings().visualizer_enabled);
        assert!(
            store.settings().help_visible,
            "подсказка по управлению глобусом включена по умолчанию"
        );
        assert_eq!(store.settings().motion, MotionReactivity::Normal);
        assert_eq!(store.settings().motion_style, MotionStyle::Auto);
        assert!(!store.settings().allow_flips);
        assert!(!store.settings().reduce_motion);
        assert!(
            !store.settings().artwork_allow_http,
            "HTTP-обложки по умолчанию выключены"
        );
    }

    #[test]
    fn roundtrip_persists_language_volume_and_visualizer() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        {
            let mut store = SettingsStore::load(path.clone()).unwrap();
            store.set_language(Language::Japanese).unwrap();
            store.set_volume(0.42).unwrap();
            store.set_visualizer_enabled(false).unwrap();
        }

        let store = SettingsStore::load(path).unwrap();
        assert_eq!(store.settings().language, Language::Japanese);
        assert!((store.settings().volume - 0.42).abs() < f64::EPSILON);
        assert!(!store.settings().visualizer_enabled);
    }

    #[test]
    fn unknown_language_code_falls_back_without_touching_other_fields() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        fs::write(
            &path,
            br#"{"theme":"light","language":"klingon","volume":0.3}"#,
        )
        .unwrap();

        let store = SettingsStore::load(path).unwrap();
        assert_eq!(store.settings().theme, ThemeMode::Light);
        assert_eq!(store.settings().language, Language::System);
        assert!((store.settings().volume - 0.3).abs() < f64::EPSILON);
    }

    #[test]
    fn invalid_volume_is_sanitized_on_load_and_write() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        fs::write(&path, br#"{"volume":7.5}"#).unwrap();
        assert!((SettingsStore::load(path).unwrap().settings().volume - 1.0).abs() < f64::EPSILON);

        let mut store = SettingsStore::ephemeral();
        store.set_volume(f64::NAN).unwrap();
        assert!((store.settings().volume - DEFAULT_VOLUME).abs() < f64::EPSILON);
        store.set_volume(-3.0).unwrap();
        assert!(store.settings().volume.abs() < f64::EPSILON);
    }

    #[test]
    fn roundtrip_persists_motion_preferences() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        {
            let mut store = SettingsStore::load(path.clone()).unwrap();
            store.set_motion(MotionReactivity::High).unwrap();
            store.set_reduce_motion(true).unwrap();
            store.set_motion_style(MotionStyle::GrooveSway).unwrap();
        }

        let store = SettingsStore::load(path).unwrap();
        assert_eq!(store.settings().motion, MotionReactivity::High);
        assert_eq!(store.settings().motion_style, MotionStyle::GrooveSway);
        assert!(!store.settings().allow_flips);
        assert!(store.settings().reduce_motion);
    }

    #[test]
    fn roundtrip_persists_allow_flips() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        {
            let mut store = SettingsStore::load(path.clone()).unwrap();
            store.set_allow_flips(true).unwrap();
        }

        let store = SettingsStore::load(path).unwrap();
        assert!(store.settings().allow_flips);
    }

    #[test]
    fn roundtrip_persists_artwork_http_opt_in() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        {
            let mut store = SettingsStore::load(path.clone()).unwrap();
            store.set_artwork_allow_http(true).unwrap();
        }

        let store = SettingsStore::load(path).unwrap();
        assert!(store.settings().artwork_allow_http);
    }

    #[test]
    fn motion_styles_have_ids_and_band_requirements() {
        assert_eq!(MotionStyle::ALL.len(), 7);
        assert_eq!(MotionStyle::Auto.id(), 0.0);
        assert_eq!(MotionStyle::SoftPulse.id(), 1.0);
        assert_eq!(MotionStyle::BeatMarch.id(), 6.0);
        assert!(MotionStyle::BassDrop.requires_bands());
        assert!(MotionStyle::SparkleTwitch.requires_bands());
        assert!(MotionStyle::BeatMarch.requires_bands());
        assert!(!MotionStyle::SoftPulse.requires_bands());
        assert!(!MotionStyle::Auto.requires_bands());
    }

    #[test]
    fn motion_reactivity_caps_and_limits() {
        assert_eq!(
            MotionReactivity::High.capped(MotionReactivity::Low),
            MotionReactivity::Low
        );
        assert_eq!(
            MotionReactivity::Low.capped(MotionReactivity::Low),
            MotionReactivity::Low
        );
        assert_eq!(
            MotionReactivity::Off.capped(MotionReactivity::High),
            MotionReactivity::Off
        );
        assert!(!MotionReactivity::Off.is_enabled());
        assert!(MotionReactivity::Normal.is_enabled());
        assert_eq!(MotionReactivity::Off.squash_limit(), 0.0);
        assert!(MotionReactivity::High.squash_limit() > MotionReactivity::Normal.squash_limit());
    }

    #[test]
    fn roundtrip_persists_visualizer_and_help() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        {
            let mut store = SettingsStore::load(path.clone()).unwrap();
            store.set_viz_bars(32).unwrap();
            store.set_help_visible(false).unwrap();
        }

        let store = SettingsStore::load(path).unwrap();
        assert_eq!(store.settings().viz_bars, 32);
        assert!(!store.settings().help_visible);
    }

    /// Старый ключ `help_seen` больше не читается: подсказка снова видна,
    /// пока пользователь не скроет её сам (ADR-0036).
    #[test]
    fn legacy_help_seen_key_leaves_the_help_visible() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        fs::write(&path, br#"{"help_seen":true}"#).unwrap();

        let store = SettingsStore::load(path).unwrap();
        assert!(store.settings().help_visible);
    }

    #[test]
    fn out_of_range_bar_count_snaps_to_nearest_choice() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        fs::write(&path, b"{\"viz_bars\":200}").unwrap();
        assert_eq!(SettingsStore::load(path).unwrap().settings().viz_bars, 32);

        let path = directory.path().join("low.json");
        fs::write(&path, b"{\"viz_bars\":19}").unwrap();
        assert_eq!(SettingsStore::load(path).unwrap().settings().viz_bars, 16);

        let mut store = SettingsStore::ephemeral();
        store.set_viz_bars(70).unwrap();
        assert_eq!(store.settings().viz_bars, 32);
    }

    #[test]
    fn corrupt_file_returns_parse_error() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        fs::write(&path, b"{ not json").unwrap();
        assert!(matches!(
            SettingsStore::load(path),
            Err(SettingsError::Parse(_))
        ));
    }

    #[test]
    fn ephemeral_store_keeps_changes_without_file() {
        let mut store = SettingsStore::ephemeral();
        store.set_theme(ThemeMode::Dark).unwrap();
        store.set_accent(AccentColor::Rose).unwrap();
        assert_eq!(store.settings().theme, ThemeMode::Dark);
        assert_eq!(store.settings().accent, AccentColor::Rose);
        assert!(store.path().is_none());
    }
}
