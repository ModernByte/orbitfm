//! Presentation-слой: egui-цикл поверх Application Core.
//!
//! `OrbitApp` живёт ровно столько, сколько открыто окно (ADR-0018): скрытие —
//! это реальное закрытие root-вьюпорта, а всё долгоживущее состояние принадлежит
//! `Session`. Здесь остаются кадры, стеклянные зоны, текстуры иконок станций и
//! превращение [`UiAction`] в команды ядра.

use std::time::Duration;

use eframe::egui;
use tracing::{debug, warn};

use crate::audio::PlaybackState;
use crate::i18n;
use crate::render::{
    BandTargets, GlobeRenderer, MotionMixer, SpectrumOverlay, SpectrumTarget, VisualizerModel,
};
use crate::services::USER_AGENT;
use crate::session::search::SearchMode;
use crate::session::{Notice, NoticeKey, Session, StationStartError, UiIntent, WindowMode};
use crate::settings::{Language, MotionReactivity, MotionStyle, Settings};
use crate::ui::UiAction;
use crate::ui::animation::{self, FrameMotion, Transitions, ZoneFlags};
use crate::ui::globe_view::{self, CalloutState, GlobeState};
use crate::ui::images::{FaviconCache, MAX_FAVICON_BYTES, decode_favicon};
use crate::ui::layout::{DeskLayout, LayoutDensity};
use crate::ui::orbit_ring::OrbitRing;
use crate::ui::palette::{self, Palette};
use crate::ui::settings_panel::{self, SettingsState};
use crate::ui::toast::Toast;
use crate::ui::transport::{self, TransportState};
use crate::ui::{capsule, hud, overlays, station_panel, widgets};

/// Частота перерисовки при включённом FFT: ~30 кадров в секунду.
const SPECTRUM_REPAINT: Duration = Duration::from_millis(33);
/// Idle-дыхание планеты: около 15 кадров в секунду (ADR-0028).
const MOTION_IDLE_REPAINT: Duration = Duration::from_millis(66);
/// Опрос фоновых favicon-загрузок.
const FAVICON_REPAINT: Duration = Duration::from_millis(120);
/// Таймаут HTTP-запроса иконки станции.
const FAVICON_FETCH_TIMEOUT: Duration = Duration::from_secs(6);
/// Сколько редиректов разрешено при загрузке favicon (ADR-0025).
const MAX_FAVICON_REDIRECTS: usize = 3;
/// MIME-типы, допустимые для favicon (ADR-0025).
const ALLOWED_FAVICON_MIME: &[&str] = &[
    "image/png",
    "image/jpeg",
    "image/webp",
    "application/octet-stream",
];
/// Скругление крупных зон (HUD, панель станции).
const GLASS_RADIUS_MAJOR: f32 = 16.0;
/// Скругление маленьких зон (легенда, карточка управления).
const GLASS_RADIUS_MINOR: f32 = 14.0;
/// Затемнение стола под открытым drawer настроек (ADR-0031).
const SCRIM_ALPHA: u8 = 90;

/// Строка тоста для ключа сообщения ядра: `detail` остаётся технической
/// вставкой и не переводится (ADR-0030).
fn notice_text(
    language: crate::settings::Language,
    key: &NoticeKey,
    detail: Option<&str>,
) -> String {
    let catalog = i18n::catalog(language);
    let notices = &catalog.notices;
    let base = match key {
        NoticeKey::HistoryNotSaved => notices.history_not_saved,
        NoticeKey::FavoritesNotSaved => notices.favorites_not_saved,
        NoticeKey::HistoryCleared => notices.history_cleared,
        NoticeKey::ClearHistoryFailed => notices.clear_history_failed,
        NoticeKey::PlaybackFailed => notices.playback_failed,
        NoticeKey::StationWithoutUrl => notices.station_without_url,
        NoticeKey::SearchTaskEnded => notices.search_task_ended,
        NoticeKey::RouletteStarted { station } => {
            let label = station
                .clone()
                .unwrap_or_else(|| catalog.common.untitled.to_owned());
            return i18n::fill(notices.roulette_started, &[("station", &label)]);
        }
        NoticeKey::RouletteFailed => notices.roulette_failed,
        NoticeKey::RouletteTaskEnded => notices.roulette_task_ended,
        NoticeKey::ThemeNotSaved => notices.theme_not_saved,
        NoticeKey::AccentNotSaved => notices.accent_not_saved,
        NoticeKey::MotionNotSaved => notices.motion_not_saved,
        NoticeKey::SettingNotSaved => notices.setting_not_saved,
        NoticeKey::VolumeNotSaved => notices.volume_not_saved,
        NoticeKey::LanguageNotSaved => notices.language_not_saved,
        NoticeKey::VisualizerNotSaved => notices.visualizer_not_saved,
        NoticeKey::FftUnavailable => notices.fft_unavailable,
    };
    match detail {
        Some(detail) => format!("{base}: {detail}"),
        None => base.to_owned(),
    }
}

pub(crate) struct OrbitApp<'a> {
    session: &'a mut Session,
    globe_ready: bool,
    spectrum_overlay: SpectrumOverlay,
    /// Аудио-деформация планеты на время показа окна (ADR-0028).
    motion: MotionMixer,
    /// Орбитальное аудиокольцо за планетой в Focus (ADR-0029).
    orbit_ring: OrbitRing,
    /// Callout выбранной/наведённой страны с fade-переходами.
    callout: CalloutState,
    visualizer: VisualizerModel,
    favicons: FaviconCache,
    http: reqwest::Client,
    toast: Toast,
    /// Переходы UI этого прогона окна (ADR-0032).
    transitions: Transitions,
    /// Прямоугольник drawer'а с последнего открытого кадра: нужен, чтобы
    /// панель гасла по месту, а не исчезала скачком (ADR-0032).
    drawer_rect: Option<egui::Rect>,
}

impl<'a> OrbitApp<'a> {
    pub(crate) fn new(cc: &eframe::CreationContext<'_>, session: &'a mut Session) -> Self {
        let egui_ctx = cc.egui_ctx.clone();
        palette::configure_style(&egui_ctx, session.settings.settings());
        install_emoji_font(&egui_ctx);
        session.wake.install({
            let ctx = egui_ctx.clone();
            move || ctx.request_repaint()
        });
        let globe_ready = if let Some(render_state) = &cc.wgpu_render_state {
            let renderer = GlobeRenderer::new(
                &render_state.device,
                &render_state.queue,
                render_state.target_format,
            );
            render_state
                .renderer
                .write()
                .callback_resources
                .insert(renderer);
            true
        } else {
            warn!("wgpu render state недоступен: 3D-глобус отключён");
            false
        };
        let visualizer = VisualizerModel::new(usize::from(session.settings.settings().viz_bars));
        Self {
            session,
            globe_ready,
            spectrum_overlay: SpectrumOverlay::default(),
            motion: MotionMixer::default(),
            orbit_ring: OrbitRing::default(),
            callout: CalloutState::default(),
            visualizer,
            favicons: FaviconCache::new(),
            http: reqwest::Client::builder()
                .user_agent(USER_AGENT)
                .timeout(FAVICON_FETCH_TIMEOUT)
                .redirect(reqwest::redirect::Policy::limited(MAX_FAVICON_REDIRECTS))
                .build()
                .expect("конфигурация HTTP-клиента favicon корректна"),
            toast: Toast::default(),
            transitions: Transitions::new(),
            drawer_rect: None,
        }
    }

    /// Сообщения ядра: UI решает, как их показать, и делает это тостом.
    fn drain_notices(&mut self, ctx: &egui::Context) {
        let language = self.session.effective_language();
        for notice in self.session.take_notices() {
            let (text, is_error) = match notice {
                Notice::Info(key) => (notice_text(language, &key, None), false),
                Notice::Error { key, detail } => {
                    (notice_text(language, &key, detail.as_deref()), true)
                }
            };
            if is_error {
                self.toast.error(ctx, text);
            } else {
                self.toast.info(ctx, text);
            }
        }
    }

    /// Фоновые favicon-загрузки: очередь → `reqwest` → декод → текстура.
    fn poll_favicons(&mut self, ctx: &egui::Context) {
        self.favicons.begin_frame();
        self.favicons.poll(ctx);
        let sender = self.favicons.sender();
        let runtime = self.session.runtime.clone();
        for url in self.favicons.take_fetches() {
            let http = self.http.clone();
            let sender = sender.clone();
            runtime.spawn(async move {
                let image = if favicon_url_allowed(&url) {
                    match http.get(&url).send().await {
                        Ok(response)
                            if response.status().is_success()
                                && favicon_content_type_allowed(&response) =>
                        {
                            match read_favicon_bytes(response).await {
                                Some(bytes) => {
                                    // Паника декодера не должна навсегда занимать слот in_flight.
                                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                        decode_favicon(&bytes)
                                    }))
                                    .ok()
                                    .flatten()
                                }
                                None => None,
                            }
                        }
                        _ => None,
                    }
                } else {
                    None
                };
                let _ = sender.send((url, image));
            });
        }
        if self.favicons.is_busy() {
            ctx.request_repaint_after(FAVICON_REPAINT);
        }
    }

    /// Кадр раскладки: снимаем состояние, рисуем слои, исполняем действия.
    fn draw(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let viewport = ui.available_rect_before_wrap();
        let frame = Frame::capture(&ctx, self.session, &mut self.visualizer, viewport);
        let mut actions = Vec::new();

        egui::CentralPanel::default()
            .frame(
                egui::Frame::default()
                    .fill(egui::Color32::TRANSPARENT)
                    .inner_margin(egui::Margin::same(0)),
            )
            .show(ui, |ui| {
                actions = self.draw_layers(ui, &frame);
                // Якорь тостов считается по плотности: в Focus стек встаёт над
                // transport, в Overlay — над избранным (ADR-0032 §4).
                self.toast.draw(
                    &ctx,
                    &frame.palette,
                    frame.layout.toast_offset(viewport),
                    frame.settings.reduce_motion,
                );
                ui.allocate_rect(viewport, egui::Sense::hover());
            });

        self.apply_actions(&ctx, actions);
    }

    /// Глобус — нижний слой, поверх него стеклянные зоны `DeskLayout`. Плотность
    /// зон выводится из `WindowMode` (ADR-0021): Focus рисует весь стол, Overlay —
    /// капсулу «станция / мини-глобус / избранное», Listening — одну строку.
    /// Фон зон — градиент с бликом — рисуется до содержимого, потому что
    /// `egui::Frame` умеет только сплошную заливку и остаётся прозрачным.
    fn draw_layers(&mut self, ui: &mut egui::Ui, frame: &Frame) -> Vec<UiAction> {
        let session = &mut *self.session;
        let favicons = &mut self.favicons;
        let spectrum_overlay = &mut self.spectrum_overlay;
        let motion_mixer = &mut self.motion;
        let orbit_ring = &mut self.orbit_ring;
        let callout = &mut self.callout;
        let visualizer = &self.visualizer;
        let palette = &frame.palette;
        let layout = frame.layout;
        let density = layout.density;

        // Прогрессы переходов считаются один раз на кадр (ADR-0032): зоны
        // читают готовые значения, а каркас стеклянных зон гаснет здесь.
        if layout.settings.is_some() {
            self.drawer_rect = layout.settings;
        }
        let mut ui_motion = self.transitions.frame(
            ui.ctx(),
            ZoneFlags {
                scene: density == LayoutDensity::Full,
                panel: layout.panel.is_some(),
                help: layout.help.is_some(),
                legend: layout.legend.is_some(),
                drawer: layout.settings.is_some(),
                capsules: density == LayoutDensity::Compact,
                favorites: density == LayoutDensity::Compact && layout.capsule.is_some(),
                listening: density == LayoutDensity::Minimal,
            },
            frame.settings.reduce_motion,
        );

        // Пульс считается один раз на кадр для кольца Listening; глобус
        // от аудио больше не пульсирует (снято до Этапа 9).
        spectrum_overlay.update(frame.spectrum_target, ui.input(|input| input.stable_dt));
        let pulse = spectrum_overlay.pulse();

        // Эфир живёт только в воспроизведении; пауза и Stop возвращают
        // планету в idle-дыхание (ADR-0028).
        let motion_target = if matches!(
            &frame.playback,
            PlaybackState::Playing | PlaybackState::Buffering | PlaybackState::Reconnecting
        ) {
            frame.spectrum_target
        } else {
            None
        };
        let motion = motion_mixer.update(
            motion_target,
            frame.motion_reactivity,
            frame.motion_style,
            ui.input(|input| input.stable_dt),
        );

        // Кольцо — часть Focus-сцены: в капсулах оно не рисуется и гаснет.
        let ring_live = density == LayoutDensity::Full
            && session.visualizer_enabled
            && matches!(
                &frame.playback,
                PlaybackState::Playing | PlaybackState::Buffering | PlaybackState::Reconnecting
            );
        orbit_ring.update(
            if ring_live {
                frame.spectrum_target
            } else {
                None
            },
            ring_live,
            ui.input(|input| input.unstable_dt),
        );

        let mut actions = Vec::new();

        if let Some(globe_rect) = layout.globe {
            let country_enabled = frame.country_mode && density == LayoutDensity::Full;
            let globe = ui
                .scope_builder(egui::UiBuilder::new().max_rect(globe_rect), |ui| {
                    globe_view::show(
                        ui,
                        palette,
                        GlobeState {
                            camera: &mut session.globe_camera,
                            motion: &motion,
                            ready: self.globe_ready,
                            markers: &session.markers,
                            current_uuid: session
                                .current_station
                                .as_ref()
                                .map(|station| station.stationuuid.as_str()),
                            geo: &session.geo,
                            selected_country: session.search.context.country.as_ref(),
                            country_enabled,
                            backdrop: density == LayoutDensity::Full,
                            ring: &*orbit_ring,
                            layout,
                            allow_flips: frame.settings.allow_flips
                                && density == LayoutDensity::Full
                                && frame.motion_reactivity.is_enabled(),
                            callout,
                            reduce_motion: ui_motion.reduce,
                        },
                    )
                })
                .inner;
            actions.extend(globe.actions);
            if country_enabled
                && let Some((x, y, z)) = globe.clicked_point
                && let Some(country) = session.geo.country_at_sphere_point(x, y, z)
            {
                actions.push(UiAction::SelectCountry(country.clone()));
            }
        }

        match density {
            LayoutDensity::Full => {
                // Вход стола: один stagger на прогон окна (ADR-0032 §4).
                let hud_in = animation::alpha(animation::stagger(0, ui_motion.scene));
                let panel_in = animation::alpha(animation::stagger(1, ui_motion.scene));
                let help_in = animation::alpha(animation::stagger(2, ui_motion.scene));
                let transport_in = animation::alpha(animation::stagger(3, ui_motion.scene));

                if let Some(hud_rect) = layout.hud {
                    widgets::glass_panel(
                        ui.painter(),
                        hud_rect,
                        palette,
                        GLASS_RADIUS_MAJOR,
                        hud_in,
                    );
                    actions.extend(
                        ui.scope_builder(horizontal(hud_rect), |ui| {
                            hud::show(
                                ui,
                                palette,
                                &mut ui_motion,
                                hud::HudState {
                                    query: &mut session.search.query,
                                    country_query: &mut session.search.country_query,
                                    genre_query: &mut session.search.genre_query,
                                    mode: session.search.context.mode,
                                    selected_genre: session.search.context.genre.as_deref(),
                                    context_active: session.search.context.is_active(),
                                    geo: &session.geo,
                                    now_playing: session.current_station.as_ref(),
                                    favicons,
                                    playback: &frame.playback,
                                    panel_open: session.panel_open,
                                    help_open: session.show_help,
                                    language: frame.language,
                                },
                            )
                        })
                        .inner,
                    );
                }

                if let Some(panel_rect) = layout.panel {
                    let panel_a = animation::alpha(panel_in * ui_motion.panel);
                    widgets::glass_panel(
                        ui.painter(),
                        panel_rect.translate(egui::vec2(-animation::slide_x(panel_a), 0.0)),
                        palette,
                        GLASS_RADIUS_MAJOR,
                        panel_a,
                    );
                    actions.extend(
                        ui.scope_builder(
                            egui::UiBuilder::new()
                                .max_rect(panel_rect)
                                .layout(egui::Layout::top_down(egui::Align::Min)),
                            |ui| {
                                station_panel::show(
                                    ui,
                                    palette,
                                    &mut ui_motion,
                                    station_panel::PanelState {
                                        tab: &mut session.library_tab,
                                        country: session.search.context.country.as_ref(),
                                        genre: session.search.context.genre.as_deref(),
                                        markers_total: frame.markers_total,
                                        results: &session.search.results,
                                        favorites: session.library.favorites(),
                                        history: session.library.history(),
                                        pending: session.search.pending,
                                        empty: session.search.empty,
                                        error: session.search.error.as_deref(),
                                        filter: &mut session.country_filter,
                                        favicons,
                                        playing_uuid: session
                                            .current_station
                                            .as_ref()
                                            .map(|station| station.stationuuid.as_str()),
                                        language: frame.language,
                                    },
                                )
                            },
                        )
                        .inner,
                    );
                }

                if let Some(transport_rect) = layout.transport {
                    widgets::glass_panel(
                        ui.painter(),
                        transport_rect,
                        palette,
                        GLASS_RADIUS_MAJOR,
                        transport_in,
                    );
                    actions.extend(
                        ui.scope_builder(horizontal(transport_rect), |ui| {
                            transport::show(
                                ui,
                                palette,
                                &mut ui_motion,
                                TransportState {
                                    playback: &frame.playback,
                                    current: session.current_station.as_ref(),
                                    favorite: frame.current_favorite,
                                    can_resume: frame.can_resume,
                                    spectrum_enabled: session.visualizer_enabled,
                                    roulette_pending: session.search.roulette_pending,
                                    volume: frame.volume,
                                    visualizer,
                                    favicons,
                                    language: frame.language,
                                },
                            )
                        })
                        .inner,
                    );
                }

                if let Some(help_rect) = layout.help {
                    let help_a = animation::alpha(help_in * ui_motion.help);
                    widgets::glass_panel(
                        ui.painter(),
                        help_rect.translate(egui::vec2(0.0, animation::rise_y(help_a))),
                        palette,
                        GLASS_RADIUS_MINOR,
                        help_a,
                    );
                    ui.scope_builder(vertical(help_rect), |ui| {
                        overlays::help_card(ui, palette, frame.language, help_a);
                    });
                }
                if let Some(legend_rect) = layout.legend {
                    let legend_a = animation::alpha(ui_motion.legend);
                    widgets::glass_panel(
                        ui.painter(),
                        legend_rect.translate(egui::vec2(0.0, animation::rise_y(legend_a))),
                        palette,
                        GLASS_RADIUS_MINOR,
                        legend_a,
                    );
                    ui.scope_builder(vertical(legend_rect), |ui| {
                        overlays::density_legend(ui, palette, frame.language, legend_a);
                    });
                }
            }
            LayoutDensity::Compact => {
                if let Some(hud_rect) = layout.hud {
                    let capsule_a = animation::alpha(ui_motion.capsules);
                    widgets::glass_panel(
                        ui.painter(),
                        hud_rect,
                        palette,
                        GLASS_RADIUS_MAJOR,
                        capsule_a,
                    );
                    actions.extend(
                        ui.scope_builder(horizontal(hud_rect), |ui| {
                            capsule::overlay_top(
                                ui,
                                palette,
                                &mut ui_motion,
                                capsule::OverlayTop {
                                    playback: &frame.playback,
                                    current: session.current_station.as_ref(),
                                    can_resume: frame.can_resume,
                                    volume: frame.volume,
                                    favicons,
                                    language: frame.language,
                                },
                            )
                        })
                        .inner,
                    );
                }
                if let Some(capsule_rect) = layout.capsule {
                    let favorites_a = animation::alpha(ui_motion.favorites);
                    widgets::glass_panel(
                        ui.painter(),
                        capsule_rect,
                        palette,
                        GLASS_RADIUS_MAJOR,
                        favorites_a,
                    );
                    actions.extend(
                        ui.scope_builder(vertical(capsule_rect), |ui| {
                            capsule::overlay_favorites(
                                ui,
                                palette,
                                &mut ui_motion,
                                capsule::OverlayFavorites {
                                    favorites: session.library.favorites(),
                                    playing_uuid: session
                                        .current_station
                                        .as_ref()
                                        .map(|station| station.stationuuid.as_str()),
                                    favicons,
                                    language: frame.language,
                                },
                            )
                        })
                        .inner,
                    );
                }
            }
            LayoutDensity::Minimal => {
                if let Some(row_rect) = layout.hud {
                    let row_a = animation::alpha(ui_motion.listening);
                    widgets::glass_panel(
                        ui.painter(),
                        row_rect,
                        palette,
                        GLASS_RADIUS_MAJOR,
                        row_a,
                    );
                    actions.extend(
                        ui.scope_builder(horizontal(row_rect), |ui| {
                            capsule::listening(
                                ui,
                                palette,
                                &mut ui_motion,
                                capsule::Listening {
                                    playback: &frame.playback,
                                    current: session.current_station.as_ref(),
                                    favorite: frame.current_favorite,
                                    can_resume: frame.can_resume,
                                    pulse,
                                    favicons,
                                    language: frame.language,
                                },
                            )
                        })
                        .inner,
                    );
                }
            }
        }

        actions.extend(draw_settings(ui, frame, &mut ui_motion, self.drawer_rect));

        if frame.motion_reactivity.is_enabled() {
            // Активный эфир — 30 fps; idle-дыханию хватает 15 fps.
            let cadence = if motion.intensity > 0.01 || session.globe_camera.is_flipping() {
                SPECTRUM_REPAINT
            } else {
                MOTION_IDLE_REPAINT
            };
            ui.ctx().request_repaint_after(cadence);
        }

        actions
    }

    fn apply_actions(&mut self, ctx: &egui::Context, actions: Vec<UiAction>) {
        for action in actions {
            self.apply_action(ctx, action);
        }
    }

    /// Действие UI превращается в команду ядра; побочные эффекты — только здесь.
    fn apply_action(&mut self, ctx: &egui::Context, action: UiAction) {
        match action {
            UiAction::Play(station) => {
                self.session.mark_help_seen();
                if let Err(err) = self.session.start_station(station) {
                    match err {
                        StationStartError::NoUrl => {
                            self.error_toast(ctx, NoticeKey::StationWithoutUrl, None);
                        }
                        StationStartError::Audio(detail) => {
                            self.error_toast(ctx, NoticeKey::PlaybackFailed, Some(detail));
                        }
                    }
                }
            }
            UiAction::ToggleFavorite(station) => self.session.toggle_favorite(&station),
            UiAction::SelectCountry(country) => {
                self.session.mark_help_seen();
                self.session.focus_country(&country);
                self.session.start_country_search(country);
            }
            UiAction::SwitchMode(mode) => self.session.switch_mode(mode),
            UiAction::SelectGenre(tag) => self.session.start_genre_search(tag),
            UiAction::RunSearch => self.session.start_search(),
            UiAction::Roulette => self.session.start_roulette(),
            UiAction::ClearHistory => self.session.clear_history(),
            UiAction::SetTheme(theme) => {
                if let Err(err) = self.session.set_theme(theme) {
                    self.error_toast(ctx, NoticeKey::ThemeNotSaved, Some(err));
                }
                palette::configure_style(ctx, self.session.settings.settings());
            }
            UiAction::SetAccent(accent) => {
                if let Err(err) = self.session.set_accent(accent) {
                    self.error_toast(ctx, NoticeKey::AccentNotSaved, Some(err));
                }
                palette::configure_style(ctx, self.session.settings.settings());
            }
            UiAction::SetLanguage(language) => {
                if let Err(err) = self.session.set_language(language) {
                    self.error_toast(ctx, NoticeKey::LanguageNotSaved, Some(err));
                }
                palette::configure_style(ctx, self.session.settings.settings());
            }
            UiAction::ToggleSpectrum(enabled) => {
                if let Err(err) = self.session.set_visualizer_enabled(enabled) {
                    self.error_toast(ctx, NoticeKey::FftUnavailable, Some(err));
                }
            }
            UiAction::SetMotion(motion) => {
                if let Err(err) = self.session.set_motion(motion) {
                    self.error_toast(ctx, NoticeKey::MotionNotSaved, Some(err));
                }
            }
            UiAction::SetMotionStyle(style) => {
                if let Err(err) = self.session.set_motion_style(style) {
                    self.error_toast(ctx, NoticeKey::MotionNotSaved, Some(err));
                }
            }
            UiAction::SetAllowFlips(allow) => {
                if let Err(err) = self.session.set_allow_flips(allow) {
                    self.error_toast(ctx, NoticeKey::MotionNotSaved, Some(err));
                }
            }
            UiAction::SetReduceMotion(reduce) => {
                if let Err(err) = self.session.set_reduce_motion(reduce) {
                    self.error_toast(ctx, NoticeKey::MotionNotSaved, Some(err));
                }
                palette::configure_style(ctx, self.session.settings.settings());
            }
            UiAction::SetVizBars(bars) => {
                if let Err(err) = self.session.set_viz_bars(bars) {
                    self.error_toast(ctx, NoticeKey::SettingNotSaved, Some(err));
                }
                let bars = self.session.settings.settings().viz_bars;
                self.visualizer = VisualizerModel::new(usize::from(bars));
            }
            UiAction::PreviewVolume(volume) => {
                if let Err(err) = self.session.preview_volume(volume) {
                    self.error_toast(ctx, NoticeKey::VolumeNotSaved, Some(err));
                }
            }
            UiAction::CommitVolume(volume) => {
                if let Err(err) = self.session.commit_volume(volume) {
                    self.error_toast(ctx, NoticeKey::VolumeNotSaved, Some(err));
                }
            }
            UiAction::PlayRelative(step) => self.session.play_relative(step),
            UiAction::TogglePlayback => {
                if let Err(err) = self.session.toggle_playback() {
                    self.error_toast(ctx, NoticeKey::PlaybackFailed, Some(err));
                }
            }
            UiAction::Stop => {
                if let Err(err) = self.session.stop_playback() {
                    self.error_toast(ctx, NoticeKey::PlaybackFailed, Some(err));
                }
            }
            UiAction::Hide => self.session.hide(),
            UiAction::Quit => self.session.quit(),
            UiAction::TogglePanel => self.session.panel_open = !self.session.panel_open,
            UiAction::ToggleHelp => {
                self.session.show_help = !self.session.show_help;
                self.session.mark_help_seen();
            }
            UiAction::ToggleSettings => self.session.toggle_settings(),
            UiAction::CloseSettings => self.session.close_settings(),
            UiAction::SetWindowMode(mode) => {
                self.session.set_window_mode(mode);
                // В Listening drawer не помещается: выбор режима закрывает его.
                if mode == WindowMode::Listening {
                    self.session.close_settings();
                }
            }
        }
        ctx.request_repaint();
    }

    /// Тост ошибки по ключу каталога: `detail` — техническая вставка (ADR-0030).
    fn error_toast(&mut self, ctx: &egui::Context, key: NoticeKey, detail: Option<String>) {
        let language = self.session.effective_language();
        self.toast
            .error(ctx, notice_text(language, &key, detail.as_deref()));
    }
}

/// Снимок читаемого состояния на один кадр: зоны берут данные из него, а не из ядра.
struct Frame {
    palette: Palette,
    settings: Settings,
    playback: PlaybackState,
    volume: f64,
    markers_total: usize,
    current_favorite: bool,
    can_resume: bool,
    spectrum_target: Option<SpectrumTarget>,
    /// Эффективная реактивность движения с поправкой на режим окна.
    motion_reactivity: MotionReactivity,
    motion_style: MotionStyle,
    /// Клик по сфере выбирает страну только в режиме страны (ТЗ §5.2).
    country_mode: bool,
    layout: DeskLayout,
    /// Разрешённый язык: из него drawer берёт подписи (ADR-0030).
    language: Language,
    window_mode: WindowMode,
    show_help: bool,
    /// Всё окно: по нему рисуется затемнение под drawer.
    viewport: egui::Rect,
}

impl Frame {
    fn capture(
        ctx: &egui::Context,
        session: &mut Session,
        visualizer: &mut VisualizerModel,
        viewport: egui::Rect,
    ) -> Self {
        let settings = session.settings.settings();
        let playback = session.audio.state();
        let spectrum_target = session.audio.spectrum().map(|frame| SpectrumTarget {
            centroid_hz: frame.centroid_hz,
            spread_hz: frame.spread_hz,
            flux: frame.flux,
            flatness: frame.flatness,
            rms_db: frame.rms_db,
            bands: frame
                .bands
                .map(|bands| BandTargets::from_db(bands.bass_db, bands.mid_db, bands.high_db)),
        });
        // Визуализатор живёт кадрами: без данных или в паузе — idle-дыхание.
        let viz_active = session.visualizer_enabled
            && matches!(
                playback,
                PlaybackState::Playing | PlaybackState::Buffering | PlaybackState::Reconnecting
            );
        visualizer.advance(
            if viz_active {
                spectrum_target.as_ref()
            } else {
                None
            },
            ctx.input(|input| input.stable_dt),
            ctx.input(|input| input.time),
        );
        Self {
            palette: Palette::for_ui(ctx.theme(), settings.accent),
            motion_reactivity: session.motion_reactivity(),
            motion_style: settings.motion_style,
            settings,
            volume: session.audio.volume(),
            markers_total: session.markers.iter().map(|marker| marker.count).sum(),
            current_favorite: session
                .current_station
                .as_ref()
                .is_some_and(|station| session.library.is_favorite(&station.stationuuid)),
            can_resume: session.current_url.is_some(),
            country_mode: session.search.context.mode == SearchMode::Country,
            language: session.effective_language(),
            window_mode: session.window_mode(),
            show_help: session.show_help,
            viewport,
            layout: DeskLayout::compute(
                viewport,
                session.panel_open,
                session.show_help,
                session.settings_open,
                session.window_mode(),
            ),
            playback,
            spectrum_target,
        }
    }
}

/// Скрим + drawer настроек поверх всех зон (ADR-0031, ADR-0032).
///
/// Layer `Foreground`: затемнение перехватывает клики и scroll стола, а drawer
/// как аллоцирован позже — забирает ввод у скрима. Закрытием считаются клик по
/// скриму и Escape, который обрабатывает [`eframe::App::logic`].
///
/// На выходе `layout.settings` уже `None`, поэтому каркас берёт последний
/// прямоугольник `last_drawer_rect`: геометрия кадра не меняется, zones стола
/// не дёргаются. Гаснущий drawer рисует содержимое, но не принимает ввод.
fn draw_settings(
    ui: &mut egui::Ui,
    frame: &Frame,
    motion: &mut FrameMotion<'_>,
    last_drawer_rect: Option<egui::Rect>,
) -> Vec<UiAction> {
    let palette = &frame.palette;
    let mut volume = frame.volume;
    let mut actions = Vec::new();

    let open = frame.layout.settings.is_some();
    let Some(rect) = frame.layout.settings.or(last_drawer_rect) else {
        return actions;
    };
    let scrim = animation::alpha(motion.scrim);
    let drawer = animation::alpha(motion.drawer);
    if scrim == 0.0 && drawer == 0.0 {
        return actions;
    }

    if scrim > 0.0 {
        egui::Area::new(egui::Id::unique("orbitfm_settings_scrim"))
            .fixed_pos(frame.viewport.min)
            .default_size(frame.viewport.size())
            .order(egui::Order::Foreground)
            .show(ui.ctx(), |ui| {
                // Гаснущий scrim не должен ловить клики: окно уже закрывается.
                let sense = if open {
                    egui::Sense::click()
                } else {
                    egui::Sense::hover()
                };
                let (rect, response) = ui.allocate_exact_size(frame.viewport.size(), sense);
                let dim = if matches!(ui.ctx().theme(), egui::Theme::Dark) {
                    egui::Color32::from_black_alpha(SCRIM_ALPHA)
                } else {
                    egui::Color32::from_white_alpha(SCRIM_ALPHA)
                };
                ui.painter()
                    .rect_filled(rect, 0.0, widgets::faded(dim, scrim));
                if response.clicked() {
                    actions.push(UiAction::CloseSettings);
                }
            });
    }

    // Drawer приходит справа: в закрытом состоянии смещён на SLIDE_X вправо.
    let shifted = rect.translate(egui::vec2(animation::slide_x(drawer), 0.0));
    egui::Area::new(egui::Id::unique("orbitfm_settings_drawer"))
        .fixed_pos(shifted.min)
        .default_size(shifted.size())
        .interactable(open)
        .order(egui::Order::Foreground)
        .show(ui.ctx(), |ui| {
            widgets::glass_panel(ui.painter(), shifted, palette, GLASS_RADIUS_MAJOR, drawer);
            ui.set_min_size(rect.size());
            ui.set_max_size(rect.size());
            actions.extend(settings_panel::show(
                ui,
                palette,
                motion,
                SettingsState {
                    settings: frame.settings,
                    language: frame.language,
                    window_mode: frame.window_mode,
                    volume: &mut volume,
                    show_help: frame.show_help,
                },
            ));
        });

    actions
}

/// Макет зоны: содержимое слева направо по центру.
fn horizontal(rect: egui::Rect) -> egui::UiBuilder {
    egui::UiBuilder::new()
        .max_rect(rect)
        .layout(egui::Layout::left_to_right(egui::Align::Center))
}

/// Макет зоны: содержимое сверху вниз по левому краю.
fn vertical(rect: egui::Rect) -> egui::UiBuilder {
    egui::UiBuilder::new()
        .max_rect(rect)
        .layout(egui::Layout::top_down(egui::Align::Min))
}

/// Ставит системный цветной emoji-шрифт в запас Proportional, чтобы флаги стран
/// (`flag_emoji`) рисовались глифами, а не tofu-квадратами. Чтение файла —
/// побочный эффект, поэтому живёт в App, а не в UI-модулях. Если шрифт не найден,
/// молча пропускаем: UI работает без флагов, как и раньше.
fn install_emoji_font(ctx: &egui::Context) {
    const PATHS: &[&str] = &[
        "/usr/share/fonts/noto/NotoColorEmoji.ttf",
        "/usr/share/fonts/truetype/noto/NotoColorEmoji.ttf",
        "/usr/share/fonts/google-noto-emoji/NotoColorEmoji.ttf",
        "/usr/share/fonts/TTF/NotoColorEmoji.ttf",
    ];
    let Some(bytes) = PATHS
        .iter()
        .map(std::path::Path::new)
        .find(|path| path.is_file())
        .and_then(|path| std::fs::read(path).ok())
    else {
        debug!("цветной emoji-шрифт не найден: флаги стран будут пропущены");
        return;
    };
    ctx.add_font(egui::FontInsert::new(
        "Noto Color Emoji",
        egui::FontData::from_owned(bytes),
        vec![egui::InsertFontFamily {
            family: egui::FontFamily::Proportional,
            priority: egui::FontPriority::Lowest,
        }],
    ));
    debug!("цветной emoji-шрифт подключён: флаги стран будут рисоваться");
}

impl eframe::App for OrbitApp<'_> {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.session.tick();
        self.drain_notices(ctx);
        self.poll_favicons(ctx);

        if let Some(rect) = ctx.input(|input| input.viewport().inner_rect) {
            self.session.set_window_size([rect.width(), rect.height()]);
        }

        match self.session.take_intent() {
            // Окно уже открыто: остаётся вернуть ему фокус.
            Some(UiIntent::Show) => ctx.send_viewport_cmd(egui::ViewportCommand::Focus),
            Some(UiIntent::Close) => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            None => {}
        }

        // Escape сначала снимает самый верхний модальный слой: drawer, затем help.
        if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            if self.session.settings_open {
                self.session.close_settings();
            } else if self.session.show_help {
                self.session.show_help = false;
                self.session.mark_help_seen();
            }
        }

        // Ctrl+, — стандартный горячий вызов настроек (ADR-0031).
        if ctx.input(|input| input.modifiers.command && input.key_pressed(egui::Key::Comma)) {
            self.session.toggle_settings();
        }

        if self.session.visualizer_enabled
            && matches!(
                self.session.audio.state(),
                PlaybackState::Playing | PlaybackState::Buffering
            )
        {
            ctx.request_repaint_after(SPECTRUM_REPAINT);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.draw(ui);
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }
}

fn favicon_url_allowed(url: &str) -> bool {
    url::Url::parse(url)
        .ok()
        .is_some_and(|url| matches!(url.scheme(), "http" | "https"))
}

fn favicon_mime_allowed(value: &str) -> bool {
    let mime = value
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    ALLOWED_FAVICON_MIME.contains(&mime.as_str())
}

fn favicon_content_type_allowed(response: &reqwest::Response) -> bool {
    match response.headers().get(reqwest::header::CONTENT_TYPE) {
        None => true,
        Some(value) => value.to_str().ok().is_some_and(favicon_mime_allowed),
    }
}

async fn read_favicon_bytes(mut response: reqwest::Response) -> Option<Vec<u8>> {
    if response
        .content_length()
        .is_some_and(|length| length > MAX_FAVICON_BYTES as u64)
    {
        return None;
    }

    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.ok()? {
        if bytes.len().saturating_add(chunk.len()) > MAX_FAVICON_BYTES {
            return None;
        }
        bytes.extend_from_slice(&chunk);
    }
    Some(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn favicon_urls_accept_only_http_and_https() {
        assert!(favicon_url_allowed("https://example.com/icon.png"));
        assert!(favicon_url_allowed("http://example.com/icon.png"));
        assert!(!favicon_url_allowed("file:///etc/passwd"));
        assert!(!favicon_url_allowed("data:image/png;base64,AAAA"));
        assert!(!favicon_url_allowed("javascript:alert(1)"));
        assert!(!favicon_url_allowed("not a url"));
    }

    #[test]
    fn favicon_mime_allowlist_rejects_active_content() {
        assert!(favicon_mime_allowed("image/png"));
        assert!(favicon_mime_allowed("IMAGE/JPEG; charset=binary"));
        assert!(favicon_mime_allowed("image/webp"));
        assert!(favicon_mime_allowed("application/octet-stream"));
        assert!(!favicon_mime_allowed("image/svg+xml"));
        assert!(!favicon_mime_allowed("image/gif"));
        assert!(!favicon_mime_allowed("image/x-icon"));
        assert!(!favicon_mime_allowed("text/html"));
        assert!(!favicon_mime_allowed(""));
    }
}
