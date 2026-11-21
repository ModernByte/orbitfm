//! Конфиг TUI: `$XDG_CONFIG_HOME/orbitfm/tui.toml` (fallback `~/.config`).
//!
//! Волна 1 читает `[ui]` (тема и глифы). Атомарная запись с debounce — FND-5;
//! сейчас файл только читается и никогда не перезаписывается. Схема `1`:
//! неизвестные поля игнорируются, битый файл — defaults и предупреждение, а
//! не паника (`docs/modules/19-tui.md` §6.8).

use std::path::{Path, PathBuf};

use serde::Deserialize;

/// Поддерживаемая версия схемы конфига.
pub const SCHEMA: u32 = 1;

/// Полный конфиг TUI.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct TuiConfig {
    pub schema: u32,
    pub ui: UiConfig,
}

impl Default for TuiConfig {
    fn default() -> Self {
        Self {
            schema: SCHEMA,
            ui: UiConfig::default(),
        }
    }
}

/// Настройки отображения `[ui]`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct UiConfig {
    pub layout: String,
    pub motion: String,
    pub glyphs: String,
    pub border: String,
    pub theme: String,
    pub frame_fps: u8,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            layout: "desk".to_owned(),
            motion: "full".to_owned(),
            glyphs: "auto".to_owned(),
            border: "rounded".to_owned(),
            theme: "orbit-dark".to_owned(),
            frame_fps: 15,
        }
    }
}

/// Откуда взялся конфиг — для честной строки статуса.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigSource {
    /// Файла нет или он недоступен — defaults.
    Defaults,
    /// Прочитан из файла.
    File(PathBuf),
    /// Файл есть, но невалиден: defaults плюс предупреждение.
    Warning(String),
}

/// Результат загрузки: значения и их происхождение.
#[derive(Debug, Clone)]
pub struct Loaded {
    pub config: TuiConfig,
    pub source: ConfigSource,
}

impl Loaded {
    /// Короткая заметка для строки статуса (None — defaults без файла).
    pub fn note(&self) -> Option<String> {
        match &self.source {
            ConfigSource::Defaults => None,
            ConfigSource::File(path) => Some(format!("config: {}", path.display())),
            ConfigSource::Warning(reason) => Some(format!("config: defaults ({reason})")),
        }
    }
}

/// Путь к конфигу: `XDG_CONFIG_HOME/orbitfm/tui.toml` или `~/.config/...`.
pub fn config_path() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("XDG_CONFIG_HOME").filter(|value| !value.is_empty()) {
        return Some(PathBuf::from(dir).join("orbitfm/tui.toml"));
    }
    std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config/orbitfm/tui.toml"))
}

/// Прочитать конфиг по стандартному пути.
pub fn load() -> Loaded {
    let Some(path) = config_path() else {
        return defaults();
    };
    load_from(&path)
}

/// Прочитать конфиг из конкретного файла (тестируемо).
pub fn load_from(path: &Path) -> Loaded {
    let Ok(text) = std::fs::read_to_string(path) else {
        return defaults();
    };
    match toml::from_str::<TuiConfig>(&text) {
        Ok(config) if config.schema == SCHEMA => Loaded {
            config,
            source: ConfigSource::File(path.to_path_buf()),
        },
        Ok(config) => Loaded {
            config: TuiConfig {
                schema: config.schema,
                ..TuiConfig::default()
            },
            source: ConfigSource::Warning(format!("schema {} != {SCHEMA}", config.schema)),
        },
        Err(error) => Loaded {
            config: TuiConfig::default(),
            source: ConfigSource::Warning(error.to_string()),
        },
    }
}

fn defaults() -> Loaded {
    Loaded {
        config: TuiConfig::default(),
        source: ConfigSource::Defaults,
    }
}

#[cfg(test)]
mod tests {
    use std::assert_matches;

    use super::*;

    #[test]
    fn defaults_have_expected_values() {
        let config = TuiConfig::default();
        assert_eq!(config.schema, SCHEMA);
        assert_eq!(config.ui.theme, "orbit-dark");
        assert_eq!(config.ui.glyphs, "auto");
        assert_eq!(config.ui.frame_fps, 15);
    }

    #[test]
    fn parses_ui_block() {
        let text = "schema = 1\n[ui]\ntheme = \"amber\"\nglyphs = \"ascii\"\n";
        let config: TuiConfig = toml::from_str(text).expect("valid toml");
        assert_eq!(config.ui.theme, "amber");
        assert_eq!(config.ui.glyphs, "ascii");
        // Пропущенные поля берут defaults.
        assert_eq!(config.ui.layout, "desk");
    }

    #[test]
    fn wrong_schema_falls_back_with_warning() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("tui.toml");
        std::fs::write(&path, "schema = 2\n[ui]\ntheme = \"amber\"\n").expect("write");
        let loaded = load_from(&path);
        assert_matches!(loaded.source, ConfigSource::Warning(_));
        assert_eq!(loaded.config.ui.theme, "orbit-dark");
    }

    #[test]
    fn missing_file_is_defaults() {
        let loaded = load_from(Path::new("/nonexistent/orbitfm/tui.toml"));
        assert_matches!(loaded.source, ConfigSource::Defaults);
        assert!(loaded.note().is_none());
    }

    #[test]
    fn broken_file_is_defaults_with_note() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("tui.toml");
        std::fs::write(&path, "this is not toml = = =\n").expect("write");
        let loaded = load_from(&path);
        assert_matches!(loaded.source, ConfigSource::Warning(_));
        assert!(loaded.note().is_some());
    }
}
