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
    pub const ALL: [Self; 4];
    pub fn label(self) -> &'static str;
    pub fn is_enabled(self) -> bool;
    /// Максимальное сжатие по вертикали, доли единицы.
    pub fn squash_limit(self) -> f32;
    /// Максимальная амплитуда ряби, доли радиуса.
    pub fn ripple_limit(self) -> f32;
    /// Амплитуда idle-дыхания, доли единицы.
    pub fn idle_amplitude(self) -> f32;
    /// Ограничение сверху для режима окна: Overlay не выше Low.
    pub fn capped(self, cap: Self) -> Self;
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
    pub const ALL: [Self; 7];
    pub fn label(self) -> &'static str;
    /// Числовой id для `CameraUniform.motion2.y`: Auto 0 … Beat March 6.
    pub fn id(self) -> f32;
    /// Стиль осмыслен только при доступных Bass/High.
    pub fn requires_bands(self) -> bool;
}

/// Доступные варианты числа столбцов визуализатора.
pub const VIZ_BAR_CHOICES: [u8; 3] = [16, 24, 32];
pub const DEFAULT_VIZ_BARS: u8 = 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub theme: ThemeMode,
    pub accent: AccentColor,
    /// Число столбцов визуализатора: 16, 24 или 32 (ADR-0019).
    pub viz_bars: u8,
    /// Карточка управления уже показывалась — дальше она только по `?`.
    pub help_seen: bool,
    pub motion: MotionReactivity,
    pub motion_style: MotionStyle,
    pub allow_flips: bool,
    pub reduce_motion: bool,
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
    pub fn set_viz_bars(&mut self, bars: u8) -> Result<(), SettingsError>;
    pub fn set_help_seen(&mut self) -> Result<(), SettingsError>;
    pub fn set_motion(&mut self, motion: MotionReactivity) -> Result<(), SettingsError>;
    pub fn set_motion_style(&mut self, style: MotionStyle) -> Result<(), SettingsError>;
    pub fn set_reduce_motion(&mut self, reduce: bool) -> Result<(), SettingsError>;
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
| `set_viz_bars` | Приводит число к ближайшему из `VIZ_BAR_CHOICES` и сохраняет |
| `set_help_seen` | Одноразово фиксирует показ карточки управления |
| `set_motion` / `set_reduce_motion` | Меняет реактивность движения и Reduce motion; сохраняет только при изменении |
| `set_motion_style` | Меняет характер движения (`Auto` или один из шести стилей); сохраняет только при изменении |

## 3. Расположение и формат

1. Если `XDG_CONFIG_HOME` задан и абсолютен —
   `$XDG_CONFIG_HOME/orbitfm/settings.json`.
2. Иначе — `$HOME/.config/orbitfm/settings.json`.
3. Относительный `XDG_CONFIG_HOME` игнорируется; неабсолютный или отсутствующий
   `HOME` даёт `SettingsError::Path`.

```json
{
  "theme": "system",
  "accent": "blue",
  "viz_bars": 24,
  "help_seen": false,
  "motion": "normal",
  "motion_style": "auto",
  "allow_flips": false,
  "reduce_motion": false
}
```

Допустимые значения: `theme` — `system`/`dark`/`light`, `accent` — `blue`/`green`/
`amber`/`violet`/`rose`/`cyan`, `motion` — `off`/`low`/`normal`/`high`,
`motion_style` — `auto`/`soft-pulse`/`groove-sway`/`bass-drop`/`ambient-drift`/
`sparkle-twitch`/`beat-march`. `viz_bars` при загрузке притягивается к
ближайшему из 16/24/32. Отсутствующие поля заменяются значениями по
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
- `missing_fields_use_defaults` (включая `motion`, `motion_style`, `allow_flips`,
  `reduce_motion`)
- `roundtrip_persists_motion_preferences` (включая `motion_style`)
- `motion_reactivity_caps_and_limits`
- `motion_styles_have_ids_labels_and_band_requirements`
- `roundtrip_persists_visualizer_and_help`
- `out_of_range_bar_count_snaps_to_nearest_choice`
- `corrupt_file_returns_parse_error`
- `ephemeral_store_keeps_changes_without_file`

## 7. Связанные документы

- ADR-0014: Темы и кастомизация интерфейса
- ADR-0019: cava-визуализатор и настройка числа столбцов
- ADR-0028: Аудио-реактивная деформация глобуса
- `docs/modules/10-settings.md`
