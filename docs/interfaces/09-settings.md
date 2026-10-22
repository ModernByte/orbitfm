# Интерфейс: Настройки оформления

```rust
/// Режим темы интерфейса.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    #[default]
    System,
    Dark,
    Light,
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
    pub const ALL: [Self; 6];
    /// RGB-компоненты пресета.
    pub fn rgb(self) -> [u8; 3];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub theme: ThemeMode,
    pub accent: AccentColor,
}

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

pub struct SettingsStore { /* path: Option<PathBuf>, settings: Settings */ }

impl SettingsStore {
    pub fn load(path: PathBuf) -> Result<Self, SettingsError>;
    pub fn load_default() -> Result<Self, SettingsError>;
    pub fn ephemeral() -> Self;
    pub fn path(&self) -> Option<&Path>;
    pub fn settings(&self) -> Settings;
    pub fn set_theme(&mut self, theme: ThemeMode) -> Result<(), SettingsError>;
    pub fn set_accent(&mut self, accent: AccentColor) -> Result<(), SettingsError>;
}

/// Стандартный путь по XDG Base Directory Specification.
pub fn default_path() -> Result<PathBuf, SettingsError>;
```

## 1. Назначение

Контракт хранения пользовательских предпочтений оформления. Модуль не зависит от
egui, GUI и доменных модулей: Application Core переводит настройки в
`egui::ThemePreference` и цвета палитры.

## 2. Семантика

| Метод | Поведение |
|-------|-----------|
| `load` | Читает JSON; `NotFound` → значения по умолчанию; повреждённый JSON → `Parse` |
| `load_default` | `load(default_path()?)` |
| `ephemeral` | Хранилище в памяти; `save` — no-op, файл не создаётся |
| `settings` | Возвращает копию `Settings` для применения в GUI |
| `set_theme` / `set_accent` | Меняет значение и сохраняет; повторная установка того же значения — успех без записи |

## 3. Расположение и формат

1. Если `XDG_CONFIG_HOME` задан и абсолютен —
   `$XDG_CONFIG_HOME/orbitfm/settings.json`.
2. Иначе — `$HOME/.config/orbitfm/settings.json`.
3. Относительный `XDG_CONFIG_HOME` игнорируется; неабсолютный или отсутствующий
   `HOME` даёт `SettingsError::Path`.

```json
{
  "theme": "system",
  "accent": "blue"
}
```

Допустимые значения: `theme` — `system`/`dark`/`light`, `accent` — `blue`/`green`/
`amber`/`violet`/`rose`/`cyan`. Отсутствующие поля заменяются значениями по
умолчанию; неизвестное значение enum — ошибка разбора.

## 4. Запись и отказоустойчивость

- Запись синхронная, только при изменении настройки; файл мал.
- Атомарная запись: `settings.json.tmp` + `rename`; каталог создаётся при
  необходимости.
- При ошибке загрузки Application Core использует `ephemeral()` и показывает
  предупреждение: повреждённый файл не перезаписывается.
- Ошибки записи возвращаются вызывающему; выбранное значение остаётся в памяти до
  завершения процесса, GUI показывает ошибку.

## 5. Палитра и акценты

`AccentColor::rgb()` задаёт цвет маркеров и выделения. Для светлой темы
Application Core затемняет акцент для текста (ссылки, подпись страны), сохраняя
яркий цвет маркеров на глобусе. Контраст текста на выделении выбирается по
яркости акцента.

## 6. Тесты

- `default_path_prefers_absolute_xdg_config_home`
- `default_path_falls_back_to_home_for_relative_xdg`
- `missing_file_uses_defaults`
- `roundtrip_persists_theme_and_accent`
- `missing_fields_use_defaults`
- `corrupt_file_returns_parse_error`
- `ephemeral_store_keeps_changes_without_file`

## 7. Связанные документы

- ADR-0014: Темы и кастомизация интерфейса
- `docs/modules/10-settings.md`
