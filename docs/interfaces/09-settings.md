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

impl ThemeMode {
    pub const ALL: [Self; 3];
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
    /// Числовой id для `CameraUniform.motion2.y`: Auto 0 … Beat March 6.
    pub fn id(self) -> f32;
    /// Стиль осмыслен только при доступных Bass/High.
    pub fn requires_bands(self) -> bool;
}

/// Язык интерфейса (ADR-0030); контракт — `docs/interfaces/12-i18n.md`.
/// Здесь используются `ALL`, `code()`, `native_name()` и `resolve()`.
pub use language::Language;

/// Доступные варианты числа столбцов визуализатора.
pub const VIZ_BAR_CHOICES: [u8; 3] = [16, 24, 32];
pub const DEFAULT_VIZ_BARS: u8 = 24;
/// Стартовая громкость: mpv по умолчанию звучит громче, чем нужно ночью.
pub const DEFAULT_VOLUME: f64 = 0.7;

/// `Settings` теряет `Eq` из-за `volume: f64`, остаётся `PartialEq`.
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
    /// Разрешить favicon по HTTP, если HTTPS недоступен (ADR-0025 ред. 3).
    pub artwork_allow_http: bool,
    /// Видимость карточки управления глобусом; по умолчанию включена (ADR-0036).
    pub help_visible: bool,
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
    pub fn set_language(&mut self, language: Language) -> Result<(), SettingsError>;
    pub fn set_viz_bars(&mut self, bars: u8) -> Result<(), SettingsError>;
    pub fn set_volume(&mut self, volume: f64) -> Result<(), SettingsError>;
    pub fn set_visualizer_enabled(&mut self, enabled: bool) -> Result<(), SettingsError>;
    pub fn set_artwork_allow_http(&mut self, allow: bool) -> Result<(), SettingsError>;
    pub fn set_help_visible(&mut self, visible: bool) -> Result<(), SettingsError>;
    pub fn set_motion(&mut self, motion: MotionReactivity) -> Result<(), SettingsError>;
    pub fn set_motion_style(&mut self, style: MotionStyle) -> Result<(), SettingsError>;
    pub fn set_allow_flips(&mut self, allow: bool) -> Result<(), SettingsError>;
    pub fn set_reduce_motion(&mut self, reduce: bool) -> Result<(), SettingsError>;
}

/// Стандартный путь по XDG Base Directory Specification.
pub fn default_path() -> Result<PathBuf, SettingsError>;
```

## 1. Назначение

Контракт хранения пользовательских предпочтений оформления и поведения. Модуль
не зависит от egui, GUI и доменных модулей: Application Core переводит настройки
в `egui::ThemePreference`, цвета палитры, громкость mpv и параметры движения
глобуса.

## 2. Семантика

| Метод | Поведение |
|-------|-----------|
| `load` | Читает JSON; `NotFound` → значения по умолчанию; повреждённый JSON → `Parse`; при загрузке `viz_bars` притягивается к ближайшему допустимому числу, `volume` клампится |
| `load_default` | `load(default_path()?)` |
| `ephemeral` | Хранилище в памяти; `save` — no-op, файл не создаётся |
| `settings` | Возвращает копию `Settings` для применения в GUI |
| `set_theme` / `set_accent` | Меняет значение и сохраняет; повторная установка того же значения — успех без записи |
| `set_language` | Меняет язык (`System` или один из 10 языков каталога); сохраняет только при изменении |
| `set_viz_bars` | Приводит число к ближайшему из `VIZ_BAR_CHOICES` и сохраняет |
| `set_volume` | Клампит в `0.0..=1.0` (non-finite → `DEFAULT_VOLUME`), сравнение по `f64::EPSILON`, сохраняет только при изменении |
| `set_visualizer_enabled` | Включает или выключает визуализатор и аудиокольцо; сохраняет только при изменении |
| `set_artwork_allow_http` | Разрешает или запрещает favicon по HTTP; значение по умолчанию `false`, сохраняет только при изменении. Применение к сетевой политике — `ArtworkFetcher::set_allow_http` (ADR-0025 ред. 3) |
| `set_help_visible` | Включает или выключает карточку управления глобусом; значение по умолчанию `true`, сохраняет только при изменении. Старый ключ `help_seen` игнорируется (ADR-0036) |
| `set_motion` / `set_reduce_motion` | Меняет реактивность движения и Reduce motion; сохраняет только при изменении |
| `set_motion_style` | Меняет характер движения (`Auto` или один из шести стилей); сохраняет только при изменении |
| `set_allow_flips` | Разрешает перевороты камеры; по умолчанию `false`, сохраняет только при изменении |

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
  "language": "system",
  "viz_bars": 24,
  "volume": 0.7,
  "visualizer_enabled": true,
  "artwork_allow_http": false,
  "help_visible": true,
  "motion": "normal",
  "motion_style": "auto",
  "allow_flips": false,
  "reduce_motion": false
}
```

Запись pretty-printed. Допустимые значения: `theme` — `system`/`dark`/`light`,
`accent` — `blue`/`green`/`amber`/`violet`/`rose`/`cyan`, `motion` —
`off`/`low`/`normal`/`high`, `motion_style` — `auto`/`soft-pulse`/`groove-sway`/
`bass-drop`/`ambient-drift`/`sparkle-twitch`/`beat-march`, `language` —
`system`/`en`/`ru`/`fr`/`de`/`es`/`it`/`pt`/`zh`/`ko`/`ja`. `viz_bars` при
загрузке притягивается к ближайшему из 16/24/32, `volume` клампится в
`0.0..=1.0`. Отсутствующие поля заменяются значениями по умолчанию. Неизвестное
значение enum — ошибка разбора, кроме `language`: неизвестный код языка даёт
`System` и не стоит остальных настроек (`language::deserialize_language`).

Пределы движения (доли единицы): `squash_limit` — 2,5/5,0/9,0 % для
Low/Normal/High, `ripple_limit` — 0,50/1,00/1,80 %, `idle_amplitude` —
0,70/0,70/1,00 %; `Off` — нули. При выключенной реактивности глобус
вращается сам в простое (ADR-0023), при `reduce_motion` авто-вращение тоже
выключено, а эффективная реактивность равна `Off`; сами поля настроек при
этом не перезаписываются.

## 4. Запись и отказоустойчивость

- Запись синхронная, только при изменении настройки; файл мал.
- Атомарная запись: `settings.json.tmp` + `rename`; каталог создаётся при
  необходимости; при неудачном `rename` временный файл удаляется.
- При ошибке загрузки Application Core использует `ephemeral()` и показывает
  предупреждение: повреждённый файл не перезаписывается.
- Ошибки записи возвращаются вызывающему; выбранное значение остаётся в памяти до
  завершения процесса, GUI показывает ошибку локализованным тостом (ADR-0030).
- `set_volume` зовётся из commit-пути: `Session::set_volume` (дискретные команды
  IPC, трея, MPRIS) и `Session::commit_volume` (отпускание слайдера, клик по треку,
  клавиатурный шаг и щелчок колеса); drag значения в файл не пишет — контракт
  ADR-0031, `docs/interfaces/10-session.md`.

## 5. Палитра, акценты и подписи

`AccentColor::rgb()` задаёт цвет маркеров и выделения. Для светлой темы
Application Core затемняет акцент для текста (ссылки, подпись страны), сохраняя
яркий цвет маркеров на глобусе. Контраст текста на выделении выбирается по
яркости акцента.

Подписей у самих enum'ов нет: `MotionReactivity::label`, `MotionStyle::label` и
`WindowMode::label` удалены как мёртвый код. Единственное место, где переводятся
значения настроек, — `src/i18n/labels.rs`: `theme_label`, `accent_label`,
`motion_label`, `style_label`, `language_label` (ADR-0030). Модуль `settings`
не знает про каталог.

## 6. Тесты

- `default_path_prefers_absolute_xdg_config_home`
- `default_path_falls_back_to_home_for_relative_xdg`
- `missing_file_uses_defaults`
- `roundtrip_persists_theme_and_accent`
- `missing_fields_use_defaults` (все одиннадцать остальных полей, включая
  `language`, `volume`, `visualizer_enabled`, `artwork_allow_http`)
- `roundtrip_persists_language_volume_and_visualizer`
- `roundtrip_persists_artwork_http_opt_in`
- `unknown_language_code_falls_back_without_touching_other_fields`
- `invalid_volume_is_sanitized_on_load_and_write`
- `roundtrip_persists_motion_preferences` (включая `motion_style`)
- `roundtrip_persists_allow_flips`
- `motion_reactivity_caps_and_limits`
- `motion_styles_have_ids_and_band_requirements`
- `roundtrip_persists_visualizer_and_help`
- `out_of_range_bar_count_snaps_to_nearest_choice`
- `corrupt_file_returns_parse_error`
- `ephemeral_store_keeps_changes_without_file`
- `language`: `codes_round_trip`, `locale_tags_map_to_primary_languages`,
  `system_resolution_uses_lc_all_then_messages_then_lang` (ADR-0030)

## 7. Связанные документы

- ADR-0014: Темы и кастомизация интерфейса
- ADR-0019: cava-визуализатор и настройка числа столбцов
- ADR-0028: Аудио-реактивная деформация глобуса
- ADR-0030: Локализация интерфейса (язык и подписи значений)
- ADR-0031: Панель настроек и персистентность (`language`, `volume`,
  `visualizer_enabled`, commit-on-release)
- `docs/modules/10-settings.md`, `docs/modules/14-ui-settings.md`
