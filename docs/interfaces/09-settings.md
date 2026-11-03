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

/// Профиль сетевого поведения (ADR-0048).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NetworkProfile {
    #[default]
    Unlimited,
    Metered,
    OfflineFirst,
}

impl NetworkProfile {
    pub const ALL: [Self; 3];
    /// Токен для IPC и `status --json`.
    pub fn as_str(self) -> &'static str;
    /// Разрешены ли фоновые сетевые загрузки (мир, favicon).
    pub fn allows_background_network(self) -> bool;
    /// Потолок битрейта discovery; `None` — без ограничения.
    pub fn bitrate_cap(self) -> Option<u32>;
    /// Размер мировой выборки; `0` — фоновая загрузка не нужна.
    pub fn world_sample_limit(self) -> u32;
}

/// Потолок битрейта discovery в профиле `Metered`.
pub const METERED_BITRATE_CAP: u32 = 128;

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
    /// Разрешить favicon по HTTP, если HTTPS недоступен (ADR-0025 ред. 4).
    pub artwork_allow_http: bool,
    /// Видимость карточки управления глобусом; по умолчанию включена (ADR-0036).
    pub help_visible: bool,
    /// Мастер первого запуска завершён или пропущен (ADR-0041).
    pub onboarding_done: bool,
    pub motion: MotionReactivity,
    pub motion_style: MotionStyle,
    pub allow_flips: bool,
    pub reduce_motion: bool,
    /// `next` после последней станции возвращается к первой (ADR-0046).
    pub queue_wrap: bool,
    /// Профиль сети: фон, discovery и favicon (ADR-0048).
    pub network_profile: NetworkProfile,
    /// Вести ли локальную статистику прослушивания (ADR-0049). По умолчанию
    /// `true`: счётчики живут только на устройстве и никуда не отправляются.
    pub stats_enabled: bool,
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
    /// Был ли файл настроек на месте при загрузке (признак первого запуска).
    pub fn existed_at_load(&self) -> bool;
    pub fn settings(&self) -> Settings;
    pub fn set_theme(&mut self, theme: ThemeMode) -> Result<(), SettingsError>;
    pub fn set_accent(&mut self, accent: AccentColor) -> Result<(), SettingsError>;
    pub fn set_language(&mut self, language: Language) -> Result<(), SettingsError>;
    pub fn set_viz_bars(&mut self, bars: u8) -> Result<(), SettingsError>;
    pub fn set_volume(&mut self, volume: f64) -> Result<(), SettingsError>;
    pub fn set_visualizer_enabled(&mut self, enabled: bool) -> Result<(), SettingsError>;
    pub fn set_artwork_allow_http(&mut self, allow: bool) -> Result<(), SettingsError>;
    pub fn set_help_visible(&mut self, visible: bool) -> Result<(), SettingsError>;
    pub fn set_onboarding_done(&mut self) -> Result<(), SettingsError>;
    pub fn set_motion(&mut self, motion: MotionReactivity) -> Result<(), SettingsError>;
    pub fn set_motion_style(&mut self, style: MotionStyle) -> Result<(), SettingsError>;
    pub fn set_allow_flips(&mut self, allow: bool) -> Result<(), SettingsError>;
    pub fn set_reduce_motion(&mut self, reduce: bool) -> Result<(), SettingsError>;
    pub fn set_queue_wrap(&mut self, wrap: bool) -> Result<(), SettingsError>;
    pub fn set_network_profile(&mut self, profile: NetworkProfile)
        -> Result<(), SettingsError>;
    pub fn set_stats_enabled(&mut self, enabled: bool) -> Result<(), SettingsError>;
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
| `existed_at_load` | `true`, если файл был прочитан при `load`; вместе с `onboarding_done == false` это признак первого запуска (ADR-0041) |
| `settings` | Возвращает копию `Settings` для применения в GUI |
| `set_theme` / `set_accent` | Меняет значение и сохраняет; повторная установка того же значения — успех без записи |
| `set_language` | Меняет язык (`System` или один из 16 языков каталога); сохраняет только при изменении |
| `set_viz_bars` | Приводит число к ближайшему из `VIZ_BAR_CHOICES` и сохраняет |
| `set_volume` | Клампит в `0.0..=1.0` (non-finite → `DEFAULT_VOLUME`), сравнение по `f64::EPSILON`, сохраняет только при изменении |
| `set_visualizer_enabled` | Включает или выключает визуализатор и аудиокольцо; сохраняет только при изменении |
| `set_artwork_allow_http` | Разрешает или запрещает favicon по HTTP; значение по умолчанию `false`, сохраняет только при изменении. Применение к сетевой политике — `ArtworkFetcher::set_allow_http` (ADR-0025 ред. 4) |
| `set_help_visible` | Включает или выключает карточку управления глобусом; значение по умолчанию `true`, сохраняет только при изменении. Старый ключ `help_seen` игнорируется (ADR-0036) |
| `set_onboarding_done` | Ставит `onboarding_done = true` и сохраняет; повторный вызов — успех без записи. Вызывается кнопками «Начать слушать» и «Пропустить» мастера (ADR-0041) |
| `set_motion` / `set_reduce_motion` | Меняет реактивность движения и Reduce motion; сохраняет только при изменении |
| `set_motion_style` | Меняет характер движения (`Auto` или один из шести стилей); сохраняет только при изменении |
| `set_allow_flips` | Разрешает перевороты камеры; по умолчанию `false`, сохраняет только при изменении |
| `set_queue_wrap` | Включает кольцевой переход очереди; по умолчанию `true`, сохраняет только при изменении (ADR-0046) |
| `set_network_profile` | Меняет профиль сети (`unlimited`/`metered`/`offline-first`); по умолчанию `unlimited`, сохраняет только при изменении (ADR-0048) |
| `set_stats_enabled` | Включает или выключает учёт локальной статистики прослушивания; по умолчанию `true`, сохраняет только при изменении (ADR-0049). При `false` накопление и запись `stats.json` останавливаются, текущий накопитель сбрасывается, а существующий файл остаётся нетронутым — удаление счётчиков делает только `stats clear` (интерфейс 17) |

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
  "onboarding_done": true,
  "motion": "normal",
  "motion_style": "auto",
  "allow_flips": false,
  "reduce_motion": false,
  "queue_wrap": true,
  "network_profile": "unlimited",
  "stats_enabled": true
}
```

Запись pretty-printed. Допустимые значения: `theme` — `system`/`dark`/`light`,
`accent` — `blue`/`green`/`amber`/`violet`/`rose`/`cyan`, `motion` —
`off`/`low`/`normal`/`high`, `motion_style` — `auto`/`soft-pulse`/`groove-sway`/
`bass-drop`/`ambient-drift`/`sparkle-twitch`/`beat-march`, `language` —
`system`/`en`/`ru`/`uk`/`pl`/`fr`/`de`/`es`/`it`/`pt`/`tr`/`ar`/`hi`/`id`/`zh`/`ko`/`ja`,
`network_profile` — `unlimited`/`metered`/`offline-first`. `viz_bars` при
загрузке притягивается к ближайшему из 16/24/32, `volume` клампится в
`0.0..=1.0`. Отсутствующие поля заменяются значениями по умолчанию (включая
`queue_wrap: true`, `network_profile: "unlimited"` и `stats_enabled: true` в
старых файлах): `#[serde(default)]` на структуре уже есть, поэтому
`settings.json` до ADR-0049 читается без миграции и даёт включённый учёт.
Неизвестное
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
`motion_label`, `style_label`, `language_label`, `network_profile_label` и
`network_profile_hint` (ADR-0030, ADR-0048). Модуль `settings` не знает про
каталог.

## 6. Тесты

- `default_path_prefers_absolute_xdg_config_home`
- `default_path_falls_back_to_home_for_relative_xdg`
- `missing_file_uses_defaults`
- `roundtrip_persists_theme_and_accent`
- `missing_fields_use_defaults` (все остальные поля, включая `language`,
  `volume`, `visualizer_enabled`, `artwork_allow_http`, `onboarding_done`)
- `roundtrip_persists_language_volume_and_visualizer`
- `roundtrip_persists_artwork_http_opt_in`
- `existing_file_disables_the_first_run_wizard` (`existed_at_load`, сохранение
  `onboarding_done`)
- `unknown_language_code_falls_back_without_touching_other_fields`
- `invalid_volume_is_sanitized_on_load_and_write`
- `roundtrip_persists_motion_preferences` (включая `motion_style`)
- `roundtrip_persists_allow_flips`
- `motion_reactivity_caps_and_limits`
- `motion_styles_have_ids_and_band_requirements`
- `roundtrip_persists_visualizer_and_help`
- `roundtrip_persists_network_profile`, `network_profile_maps_tokens_and_limits`
- `roundtrip_persists_stats_enabled` — `stats_enabled` по умолчанию `true`,
  старый файл без поля читается как `true` и не перезаписывается без изменения
  (ADR-0049)
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
- ADR-0041: Встроенные шрифты и мастер первого запуска (`onboarding_done`,
  `existed_at_load`)
- ADR-0048: Профиль сети (`network_profile`)
- ADR-0049: Локальная статистика прослушивания (`stats_enabled`)
- `docs/modules/10-settings.md`, `docs/modules/14-ui-settings.md`
- `docs/interfaces/17-stats.md` — файл `stats.json` схемы 1 и команды `stats`
