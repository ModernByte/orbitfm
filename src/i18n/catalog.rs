//! Схема каталога: каждая языковая константа обязана заполнить все поля.

pub(crate) struct Catalog {
    pub common: CommonText,
    pub hud: HudText,
    pub panel: PanelText,
    pub transport: TransportText,
    pub capsule: CapsuleText,
    pub overlay: OverlayText,
    pub settings: SettingsText,
    /// Строки меню трея и подписи `status --json` (ADR-0020, ADR-0037):
    /// нужны SNI на Linux, строке меню macOS и JSON-снимку на всех платформах.
    pub tray: TrayText,
    pub notices: NoticeText,
    pub labels: LabelText,
}

pub(crate) struct CommonText {
    pub nothing_playing: &'static str,
    pub untitled: &'static str,
    pub add_favorite: &'static str,
    pub remove_favorite: &'static str,
    pub hide_window_hint: &'static str,
    pub show_station_list: &'static str,
    pub hide_station_list: &'static str,
    pub open_focus: &'static str,
    pub mouse_controls: &'static str,
    pub settings: &'static str,
    pub close: &'static str,
    pub search: &'static str,
    pub no_match: &'static str,
    pub light_theme: &'static str,
    pub dark_theme: &'static str,
    pub pause: &'static str,
    pub resume: &'static str,
    pub stop: &'static str,
    pub previous_station: &'static str,
    pub next_station: &'static str,
    pub live: &'static str,
    pub quit: &'static str,
    pub retry: &'static str,
    /// Вкладки и пустые состояния попапа (ADR-0037, ревизия 0.16).
    pub favorites: &'static str,
    pub history: &'static str,
    /// Кнопка импорта потока с открытой страницы (ADR-0040).
    pub play_this_page: &'static str,
    /// Состояния импорта потока в попапе (ADR-0040).
    pub no_audio: &'static str,
    pub importing: &'static str,
    pub stream_rejected: &'static str,
}

pub(crate) struct HudText {
    pub subtitle: &'static str,
    pub search_stations: &'static str,
    pub filter_stations: &'static str,
    pub search_country: &'static str,
    pub country: &'static str,
    pub genre: &'static str,
    pub pick_genre: &'static str,
    pub global_genre: &'static str,
    pub custom_tag_hint: &'static str,
    pub genre_search_hint: &'static str,
}

pub(crate) struct PanelText {
    pub stations: &'static str,
    pub country_prefix: &'static str,
    pub genre_prefix: &'static str,
    pub on_map: &'static str,
    pub searching: &'static str,
    pub no_stations: &'static str,
    pub intro: &'static str,
    pub results: &'static str,
    pub favorites: &'static str,
    pub history: &'static str,
    pub station_count: &'static str,
    pub search_failed: &'static str,
    pub results_of: &'static str,
    pub pick_country_or_genre: &'static str,
    pub quiet_sector: &'static str,
    pub no_filter_match: &'static str,
    pub reset_filters: &'static str,
    pub favorites_empty: &'static str,
    pub history_empty: &'static str,
    pub clear_history: &'static str,
    pub search_in_country: &'static str,
    pub search_here: &'static str,
    /// Подпись выпадающего мультивыбора, когда не выбрано ни одного жанра.
    pub all_genres: &'static str,
    /// Заголовок выпадающего списка жанров.
    pub genres: &'static str,
    /// Подпись кнопки мультивыбора с числом выбранных жанров.
    pub genres_selected: &'static str,
    /// Подсказка крестика на бейдже выбранного жанра.
    pub remove_filter: &'static str,
    /// Подсказка кнопки «показать/скрыть фильтры».
    pub filters: &'static str,
    /// Подсказка кнопки сброса всех фильтров.
    pub clear_filters: &'static str,
    /// Счётчик загруженной страницы: «{loaded} из {total}».
    pub loaded_of: &'static str,
    pub load_more: &'static str,
    pub loading_more: &'static str,
    pub sort_by: &'static str,
    pub compact: &'static str,
}

pub(crate) struct TransportText {
    pub radio_roulette: &'static str,
    pub rolling: &'static str,
    pub random_station: &'static str,
    pub now_playing: &'static str,
    pub now_playing_tags: &'static str,
    pub no_station: &'static str,
    pub volume: &'static str,
    pub kbps: &'static str,
}

pub(crate) struct CapsuleText {
    pub favorites: &'static str,
    pub count_of: &'static str,
    pub no_favorites: &'static str,
    pub favorite_hint: &'static str,
}

pub(crate) struct OverlayText {
    pub right_drag: &'static str,
    pub orbit: &'static str,
    pub left_click: &'static str,
    pub country_station: &'static str,
    pub scroll: &'static str,
    pub zoom: &'static str,
    pub map_off: &'static str,
    pub map_density: &'static str,
    pub map_genre: &'static str,
    pub map_quality: &'static str,
    pub map_kind: &'static str,
    pub map_rating: &'static str,
    /// Источник данных карты: счётчики всех стран, выборка панели или мировой топ.
    pub map_scope_global: &'static str,
    pub map_scope_sample: &'static str,
    pub map_scope_top: &'static str,
    pub low: &'static str,
    pub high: &'static str,
}

pub(crate) struct SettingsText {
    pub title: &'static str,
    pub appearance: &'static str,
    pub language: &'static str,
    pub audio: &'static str,
    pub motion: &'static str,
    pub window: &'static str,
    pub about: &'static str,
    pub theme: &'static str,
    pub accent: &'static str,
    pub visualizer: &'static str,
    pub viz_bars: &'static str,
    pub bars: &'static str,
    /// Opt-in загрузки обложек по HTTP (ADR-0025, п. 11).
    pub allow_http_artwork: &'static str,
    pub allow_http_artwork_hint: &'static str,
    pub system_language: &'static str,
    pub language_hint: &'static str,
    pub volume: &'static str,
    /// Подсказка над слайдером: колесо мыши меняет громкость (ADR-0031 §7).
    pub volume_hint: &'static str,
    pub audio_hint: &'static str,
    pub motion_reactivity: &'static str,
    pub motion_spin_hint: &'static str,
    pub motion_style: &'static str,
    pub reduce_motion: &'static str,
    pub reduce_motion_hint: &'static str,
    pub allow_flips: &'static str,
    pub allow_flips_hint: &'static str,
    pub flips_unavailable: &'static str,
    pub window_mode: &'static str,
    pub window_hint: &'static str,
    pub show_help: &'static str,
    pub version: &'static str,
    pub license: &'static str,
    pub app_id: &'static str,
    pub about_hint: &'static str,
    pub saved_hint: &'static str,
    /// Подзаголовок страницы категории: одна строка под её названием.
    pub appearance_page_hint: &'static str,
    pub language_page_hint: &'static str,
    pub audio_page_hint: &'static str,
    pub motion_page_hint: &'static str,
    pub window_page_hint: &'static str,
    pub about_page_hint: &'static str,
}

pub(crate) struct TrayText {
    pub show_window: &'static str,
    pub hide_window: &'static str,
    // Заголовок и Quit нужны только платформенным бэкендам трея; `status --json`
    // их не публикует (ADR-0037), поэтому без трея они не читаются.
    #[cfg_attr(
        not(any(
            all(target_os = "linux", feature = "tray"),
            all(target_os = "macos", feature = "tray")
        )),
        allow(dead_code)
    )]
    pub now_playing: &'static str,
    #[cfg_attr(
        not(any(
            all(target_os = "linux", feature = "tray"),
            all(target_os = "macos", feature = "tray")
        )),
        allow(dead_code)
    )]
    pub paused: &'static str,
    pub nothing_playing: &'static str,
    pub play_pause: &'static str,
    pub stop: &'static str,
    pub previous: &'static str,
    pub next: &'static str,
    pub radio_roulette: &'static str,
    pub volume_up: &'static str,
    pub volume_down: &'static str,
    pub settings: &'static str,
    /// Заголовки групп обновлённого меню (ADR-0020, ревизия 0.16).
    #[cfg_attr(
        not(any(
            all(target_os = "linux", feature = "tray"),
            all(target_os = "macos", feature = "tray")
        )),
        allow(dead_code)
    )]
    pub playback: &'static str,
    #[cfg_attr(
        not(any(
            all(target_os = "linux", feature = "tray"),
            all(target_os = "macos", feature = "tray")
        )),
        allow(dead_code)
    )]
    pub volume: &'static str,
    #[cfg_attr(
        not(any(
            all(target_os = "linux", feature = "tray"),
            all(target_os = "macos", feature = "tray")
        )),
        allow(dead_code)
    )]
    pub appearance: &'static str,
    /// Подменю «О станции»: избранное и домашняя страница.
    #[cfg_attr(
        not(any(
            all(target_os = "linux", feature = "tray"),
            all(target_os = "macos", feature = "tray")
        )),
        allow(dead_code)
    )]
    pub about_station: &'static str,
    #[cfg_attr(
        not(any(
            all(target_os = "linux", feature = "tray"),
            all(target_os = "macos", feature = "tray")
        )),
        allow(dead_code)
    )]
    pub open_homepage: &'static str,
    #[cfg_attr(
        not(any(
            all(target_os = "linux", feature = "tray"),
            all(target_os = "macos", feature = "tray")
        )),
        allow(dead_code)
    )]
    pub quit: &'static str,
}

pub(crate) struct NoticeText {
    pub history_not_saved: &'static str,
    pub favorites_not_saved: &'static str,
    pub history_cleared: &'static str,
    pub clear_history_failed: &'static str,
    pub playback_failed: &'static str,
    pub station_without_url: &'static str,
    pub search_task_ended: &'static str,
    pub load_more_failed: &'static str,
    pub roulette_started: &'static str,
    pub roulette_failed: &'static str,
    pub roulette_task_ended: &'static str,
    pub theme_not_saved: &'static str,
    pub accent_not_saved: &'static str,
    pub motion_not_saved: &'static str,
    pub setting_not_saved: &'static str,
    pub volume_not_saved: &'static str,
    pub language_not_saved: &'static str,
    pub visualizer_not_saved: &'static str,
    pub fft_unavailable: &'static str,
}

pub(crate) struct LabelText {
    pub theme_system: &'static str,
    pub theme_dark: &'static str,
    pub theme_light: &'static str,
    pub accent_orbit: &'static str,
    pub accent_emerald: &'static str,
    pub accent_amber: &'static str,
    pub accent_violet: &'static str,
    pub accent_rose: &'static str,
    pub accent_ocean: &'static str,
    pub motion_off: &'static str,
    pub motion_low: &'static str,
    pub motion_normal: &'static str,
    pub motion_high: &'static str,
    pub style_auto: &'static str,
    pub style_soft_pulse: &'static str,
    pub style_groove_sway: &'static str,
    pub style_bass_drop: &'static str,
    pub style_ambient_drift: &'static str,
    pub style_sparkle_twitch: &'static str,
    pub style_beat_march: &'static str,
    pub mode_focus: &'static str,
    pub mode_overlay: &'static str,
    pub mode_listening: &'static str,
    pub genre_rock: &'static str,
    pub genre_pop: &'static str,
    pub genre_jazz: &'static str,
    pub genre_electronic: &'static str,
    pub genre_news: &'static str,
    pub genre_metal: &'static str,
    pub genre_classical: &'static str,
    pub genre_synthwave: &'static str,
    pub sort_popularity: &'static str,
    pub sort_bitrate: &'static str,
    pub sort_name: &'static str,
    pub playback_stopped: &'static str,
    pub playback_on_air: &'static str,
    pub playback_paused: &'static str,
    pub playback_buffering: &'static str,
    pub playback_reconnecting: &'static str,
    pub playback_error: &'static str,
}
