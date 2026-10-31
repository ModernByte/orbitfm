//! Строка списка станций по эскизу: круглый favicon, имя, город,
//! битрейт/кодек и звезда. Играющая строка — accent-рамка с эквалайзером.
//!
//! Подложка строки (`chip_fill`) рисуется вручную в зарезервированный слот
//! отрисовщика, поэтому её можно анимировать, не сдвигая хиттест: hover fade
//! [`animation::ROW_HOVER`] и press scale [`animation::PRESS_SCALE`]
//! (ADR-0032 §4, сценарий Focus).

use eframe::egui;

use crate::i18n;
use crate::services::Station;
use crate::settings::Language;

use super::animation;
use super::icons::{SMALL_SIDE, star_button};
use super::images::FaviconCache;
use super::palette::Palette;
use super::widgets::{avatar, chip, eq_bars_in, faded, focus_ring};
use super::{UiAction, single_line, truncate};

const ROW_HEIGHT: f32 = 58.0;
const TAG_LIMIT: usize = 3;
/// Ширина колонки «битрейт/кодек» в полной строке.
const BITRATE_WIDTH: f32 = 64.0;
/// Скругление и обводка подложки: радиус совпадает с рамкой строки.
const ROW_ROUNDING: u8 = 12;
const PLAYING_STROKE: f32 = 1.4;
/// Насколько ярко подсвечивается hovered-строка. Меньше единицы намеренно:
/// на полной альфе `chip_fill` наведение не отличалось бы от играющей строки.
const HOVER_TINT: f32 = 0.7;

pub(crate) struct RowContext<'a> {
    pub playing: bool,
    /// Станция выбрана на глобусе или в списке: её строка подсвечена, даже
    /// когда эфир уже остановлен (ADR-0036).
    pub selected: bool,
    /// Кадр, в котором список обязан прокрутить строку в центр: маркер глобуса
    /// выбрал станцию, которой может не быть на экране.
    pub reveal: bool,
    pub compact: bool,
    pub favorites: &'a [Station],
    /// Разрешённый язык интерфейса (ADR-0030).
    pub language: Language,
    pub favicons: &'a mut FaviconCache,
    /// Соседи по видимому списку: стрелки вверх/вниз переводят фокус.
    pub previous: Option<&'a str>,
    pub next: Option<&'a str>,
}

/// Стабильный id строки: по нему фокус переходит к соседу при навигации.
pub(crate) fn row_id(stationuuid: &str) -> egui::Id {
    egui::Id::unique(("orbitfm_station_row", stationuuid))
}

/// Единая строка для Results/Favorites/History.
pub(crate) fn station_row(
    ui: &mut egui::Ui,
    palette: &Palette,
    station: &Station,
    state: &mut RowContext<'_>,
    actions: &mut Vec<UiAction>,
) {
    let favorite = state
        .favorites
        .iter()
        .any(|item| item.stationuuid == station.stationuuid);
    let previous = state.previous;
    let next = state.next;
    let id = row_id(&station.stationuuid);

    // Рамка отвечает за отступы и аллокацию, а её фон мы рисуем сами: альфа
    // подсветки и геометрия сжатия известны только после аллокации, а слот под
    // пластину резервируется до содержимого — так подложка лежит под текстом
    // (своего фона рамка не получает, см. `row_frame`).
    let mut prepared = row_frame(state, palette).begin(ui);
    let plate = ui.painter().add(egui::Shape::Noop);
    let content = &mut prepared.content_ui;
    content.set_width(content.available_width());
    if !state.compact {
        content.set_min_height(ROW_HEIGHT - 20.0);
    }
    let star_response = draw_contents(content, palette, station, state, favorite);
    let response = prepared.allocate_space(ui);

    let clicked = ui.interact(response.rect, id, egui::Sense::click());
    let visible = ui.is_rect_visible(clicked.rect);
    let selected = state.selected;
    let reveal = state.reveal;
    // Строки вне вьюпорта не опрашивают переход: список без виртуализации, и
    // гонять AnimationManager на сотнях невидимых строк незачем. Значение
    // анимации при этом просто замерает на месте и догорает, когда строка
    // вернется в поле зрения.
    let (hover, press) = if visible {
        row_motion(ui, id, &clicked)
    } else {
        (0.0, 0.0)
    };
    if visible {
        let rect = clicked.rect;
        ui.painter().set(
            plate,
            row_plate(rect, palette, state.playing || selected, hover, press),
        );
    }

    let place = row_place(station);
    clicked.widget_info(|| {
        let name = row_name(station, state.compact, state.language);
        let label = if place.is_empty() {
            name
        } else {
            format!("{name}, {place}")
        };
        egui::WidgetInfo::labeled(egui::Role::ListItem, true, label)
    });
    focus_ring(ui, &clicked, clicked.rect, f32::from(ROW_ROUNDING));

    if clicked.has_focus() {
        let (up, down) = ui.input(|input| {
            (
                input.key_pressed(egui::Key::ArrowUp),
                input.key_pressed(egui::Key::ArrowDown),
            )
        });
        let target = if up {
            previous
        } else if down {
            next
        } else {
            None
        };
        if let Some(uuid) = target {
            ui.memory_mut(|memory| memory.request_focus(row_id(uuid)));
        }
    }
    if clicked.gained_focus() || reveal {
        clicked.scroll_to_me(Some(egui::Align::Center));
    }

    if clicked.clicked() && !star_response.clicked() {
        actions.push(UiAction::Play(station.clone()));
    } else if star_response.clicked() {
        actions.push(UiAction::ToggleFavorite(station.clone()));
    }
    clicked.on_hover_text(single_line(&station.name));

    ui.add_space(if state.compact { 2.0 } else { 6.0 });
}

/// Каркас строки: только отступы и ширина аллокации.
///
/// `inner_margin` плюс ширина обводки задают размер, и они же задают геометрию
/// пластины в [`row_plate`]. Собственный фон рамки не отрисовывается
/// (`Prepared::paint` не вызываем): альфа наведения и степень сжатия известны
/// только после аллокации, а рисовать надо под содержимым.
fn row_frame(state: &RowContext<'_>, palette: &Palette) -> egui::Frame {
    let stroke = if state.playing || state.selected {
        PLAYING_STROKE
    } else {
        0.0
    };
    egui::Frame::default()
        .fill(egui::Color32::TRANSPARENT)
        .stroke(egui::Stroke::new(stroke, palette.accent))
        .corner_radius(egui::CornerRadius::same(ROW_ROUNDING))
        .inner_margin(egui::Margin::symmetric(
            10,
            if state.compact { 4 } else { 6 },
        ))
}

/// Прогресс наведения и нажатия строки.
///
/// Первое значение — яркость подсветки (уже с множителем [`HOVER_TINT`]),
/// второе — доля press-сжатия; оба в `0..=1`.
///
/// Механизм ровно один — AnimationManager egui (`animate_bool_with_time`), тот
/// же, что проигрывает переходы штатных виджетов через `Style::animation_time`.
/// Штатный переход строке недоступен (подложку рисуем сами), а `PresenceStore`
/// держит не больше `KEYED_LIMIT` именованных переходов и для десятков строк не
/// предназначен. Кадры на время перехода запрашивает сам egui и перестаёт, когда
/// значение упирается в 0.0 или 1.0 — «вечных» анимаций не остаётся
/// (ADR-0032 §6). Reduce motion выключается тем же мастер-переключателем, что и
/// остальной интерфейс: при `Style::animation_time == 0` (см.
/// [`super::palette::configure_style`]) строка принимает конечное состояние сразу
/// и не сжимается вовсе (ADR-0032 §3).
fn row_motion(ui: &egui::Ui, id: egui::Id, response: &egui::Response) -> (f32, f32) {
    if ui.style().animation_time <= 0.0 {
        return (if response.hovered() { HOVER_TINT } else { 0.0 }, 0.0);
    }
    let ctx = ui.ctx();
    let hover = ctx.animate_bool_with_time(
        id.with("row_hover"),
        response.hovered(),
        animation::ROW_HOVER,
    );
    let press = ctx.animate_bool_with_time(
        id.with("row_press"),
        response.is_pointer_button_down_on(),
        animation::PRESS,
    );
    (
        animation::alpha(hover) * HOVER_TINT,
        animation::alpha(press),
    )
}

/// Пластина-подложка строки: подсветка наведения плюс accent-рамка выбранной
/// или играющей строки, сжатая по центру при нажатии.
///
/// Двигается только отрисовка: хиттест и расстановка строк остаются на
/// исходном прямоугольнике (ADR-0032 §7).
fn row_plate(
    rect: egui::Rect,
    palette: &Palette,
    highlighted: bool,
    hover: f32,
    press: f32,
) -> egui::Shape {
    let tint = if highlighted {
        1.0
    } else {
        animation::alpha(hover)
    };
    if tint == 0.0 && press == 0.0 {
        return egui::Shape::Noop;
    }
    let card = pressed_rect(rect, press);
    let radius = egui::CornerRadius::same(ROW_ROUNDING);
    let mut shapes = vec![egui::Shape::rect_filled(
        card,
        radius,
        faded(palette.chip_fill, tint),
    )];
    if highlighted {
        shapes.push(egui::Shape::rect_stroke(
            card,
            radius,
            egui::Stroke::new(PLAYING_STROKE, palette.accent),
            egui::StrokeKind::Inside,
        ));
    }
    egui::Shape::Vec(shapes)
}

/// Прямоугольник пластины под press-сжатие: масштаб вокруг центра, от `1.0` до
/// [`animation::PRESS_SCALE`] (ADR-0032 §4).
fn pressed_rect(rect: egui::Rect, press: f32) -> egui::Rect {
    let scale = 1.0 - (1.0 - animation::PRESS_SCALE) * animation::alpha(press);
    egui::Rect::from_center_size(rect.center(), rect.size() * scale)
}

fn draw_contents(
    ui: &mut egui::Ui,
    palette: &Palette,
    station: &Station,
    state: &mut RowContext<'_>,
    favorite: bool,
) -> egui::Response {
    let text = i18n::catalog(state.language);
    let texture = state
        .favicons
        .texture(station.favicon.as_deref().unwrap_or_default());
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 10.0;
        let avatar_size = (ui.available_height() - 2.0).clamp(22.0, 44.0);
        avatar(ui, palette, texture, &station.name, avatar_size);

        // Правый блок резервируется до текста: иначе длинное имя рисуется под
        // звездой (жалоба 2026-10-28). Ширина текста считается от фактического
        // остатка, а Label::truncate обрезает его многоточием по месту.
        let reserve = right_block_width(state.compact) + ui.spacing().item_spacing.x;
        let text_width = (ui.available_width() - reserve).max(56.0);
        ui.vertical(|ui| {
            ui.set_max_width(text_width);
            ui.spacing_mut().item_spacing.y = if state.compact { 0.0 } else { 1.0 };
            let name_color = if state.playing || state.selected {
                palette.accent
            } else {
                palette.text
            };
            ui.add(
                egui::Label::new(
                    egui::RichText::new(row_name(station, state.compact, state.language))
                        .size(if state.compact { 12.0 } else { 13.5 })
                        .strong()
                        .color(name_color),
                )
                .truncate(),
            );
            if !state.compact {
                let place = row_place(station);
                if !place.is_empty() {
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(place).size(10.5).color(palette.muted),
                        )
                        .truncate(),
                    );
                }
                if state.playing {
                    let bars = egui::vec2(24.0, 12.0);
                    let (bars_rect, _) = ui.allocate_at_least(bars, egui::Sense::hover());
                    eq_bars_in(ui, palette, bars_rect, 5, true);
                }
            }
        });

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let hint = if favorite {
                text.common.remove_favorite
            } else {
                text.common.add_favorite
            };
            let color = if favorite {
                palette.accent
            } else {
                palette.muted
            };
            let star = star_button(ui, palette, favorite, color, hint);
            if !state.compact {
                ui.vertical(|ui| {
                    ui.set_width(BITRATE_WIDTH);
                    ui.label(
                        egui::RichText::new(row_bitrate(station, state.language))
                            .size(11.0)
                            .color(palette.text),
                    );
                    ui.label(
                        egui::RichText::new(row_codec(station))
                            .size(10.0)
                            .color(palette.muted),
                    );
                });
            }
            star
        })
        .inner
    })
    .inner
}

/// Ширина правого блока строки: звезда и, в полном режиме, колонка
/// «битрейт/кодек» с промежутком между ними.
fn right_block_width(compact: bool) -> f32 {
    if compact {
        SMALL_SIDE
    } else {
        SMALL_SIDE + 10.0 + BITRATE_WIDTH
    }
}

/// Чипы тегов станции; клик — глобальный поиск по тегу.
pub(crate) fn station_tag_chips(
    ui: &mut egui::Ui,
    palette: &Palette,
    station: &Station,
    actions: &mut Vec<UiAction>,
) {
    let mut shown = 0usize;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        for tag in station
            .tags
            .split(',')
            .map(str::trim)
            .filter(|tag| !tag.is_empty())
        {
            if shown >= TAG_LIMIT {
                break;
            }
            let label = truncate(tag, 14);
            if chip(ui, palette, &label, false).clicked() {
                actions.push(UiAction::SelectGenre(tag.to_owned()));
            }
            shown += 1;
        }
    });
}

fn row_name(station: &Station, compact: bool, language: Language) -> String {
    let text = i18n::catalog(language);
    let name = single_line(&station.name);
    if name.is_empty() {
        text.common.untitled.to_owned()
    } else {
        truncate(&name, if compact { 26 } else { 32 })
    }
}

pub(crate) fn row_place(station: &Station) -> String {
    let state_name = single_line(&station.state);
    let country = single_line(&station.country);
    match (state_name.is_empty(), country.is_empty()) {
        (false, false) => format!("{state_name}, {country}"),
        (false, true) => state_name,
        (true, false) => country,
        (true, true) => single_line(&station.countrycode),
    }
}

fn row_bitrate(station: &Station, language: Language) -> String {
    if station.bitrate > 0 {
        let text = i18n::catalog(language);
        let count = station.bitrate.to_string();
        i18n::fill(text.transport.kbps, &[("count", &count)])
    } else {
        String::new()
    }
}

fn row_codec(station: &Station) -> String {
    single_line(&station.codec)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_station() -> Station {
        Station {
            stationuuid: "uuid-1".to_owned(),
            name: "  Radio   Nova \nLive".to_owned(),
            url: "http://example.com/live".to_owned(),
            url_resolved: "https://example.com/live".to_owned(),
            homepage: None,
            favicon: Some("https://example.com/icon.png".to_owned()),
            tags: "jazz, smooth, vocal".to_owned(),
            countrycode: "FR".to_owned(),
            language: "french".to_owned(),
            votes: 90,
            country: "France".to_owned(),
            state: "Île-de-France".to_owned(),
            bitrate: 320,
            codec: "MP3".to_owned(),
            geo_lat: None,
            geo_long: None,
            source: crate::services::StationSource::Radio,
        }
    }

    #[test]
    fn row_name_is_single_line_with_placeholder() {
        let text = i18n::catalog(Language::English);
        assert_eq!(
            row_name(&sample_station(), false, Language::English),
            "Radio Nova Live"
        );
        let mut blank = sample_station();
        blank.name = "   ".to_owned();
        assert_eq!(
            row_name(&blank, true, Language::English),
            text.common.untitled,
            "пустое имя берётся из каталога"
        );
        assert_eq!(text.common.untitled, "Untitled", "английский источник");
    }

    #[test]
    fn row_place_prefers_state_then_country_then_code() {
        assert_eq!(row_place(&sample_station()), "Île-de-France, France");
        let mut only_code = sample_station();
        only_code.state.clear();
        only_code.country.clear();
        assert_eq!(row_place(&only_code), "FR");
    }

    #[test]
    fn row_bitrate_and_codec_hide_zero_bitrate() {
        let english = i18n::catalog(Language::English);
        assert_eq!(
            row_bitrate(&sample_station(), Language::English),
            "320 kbps"
        );
        assert_eq!(
            row_bitrate(&sample_station(), Language::English),
            i18n::fill(english.transport.kbps, &[("count", "320")]),
            "битрейт собирается из шаблона каталога"
        );
        assert_eq!(row_codec(&sample_station()), "MP3");
        let mut quiet = sample_station();
        quiet.bitrate = 0;
        assert_eq!(row_bitrate(&quiet, Language::English), "");
    }

    fn palette() -> Palette {
        Palette::for_ui(egui::Theme::Dark, crate::settings::AccentColor::Blue)
    }

    fn row_rect() -> egui::Rect {
        egui::Rect::from_min_size(egui::Pos2::new(12.0, 30.0), egui::vec2(300.0, 58.0))
    }

    #[test]
    fn press_scales_the_plate_about_its_center() {
        let rect = row_rect();
        assert_eq!(
            pressed_rect(rect, 0.0),
            rect,
            "без нажатия геометрия не меняется"
        );

        let flat = pressed_rect(rect, 1.0);
        let expected = rect.size() * animation::PRESS_SCALE;
        assert!(
            (flat.size() - expected).length() < 1e-4,
            "шкала press: {:?}",
            flat.size()
        );
        assert_eq!(
            flat.center(),
            rect.center(),
            "сжатие не должно двигать строку"
        );

        let half = pressed_rect(rect, 0.5);
        assert!(half.width() < rect.width() && half.width() > flat.width());
        assert_eq!(half.center(), rect.center());
        // Reduce motion даёт press = 0.0, то есть исходный прямоугольник.
        assert_eq!(pressed_rect(rect, animation::alpha(0.0)), rect);
    }

    #[test]
    fn idle_row_paints_nothing() {
        let plate = row_plate(row_rect(), &palette(), false, 0.0, 0.0);
        assert!(
            matches!(plate, egui::Shape::Noop),
            "покой строки не рисует пластину"
        );
    }

    #[test]
    fn plate_layers_follow_playing_hover_and_press() {
        // Играющая строка: подложка плюс accent-рамка.
        let playing = row_plate(row_rect(), &palette(), true, 0.0, 0.0);
        let egui::Shape::Vec(shapes) = &playing else {
            panic!("играющая строка обязана рисовать рамку: {playing:?}");
        };
        assert_eq!(shapes.len(), 2);

        // Наведение — одна подложка, яркость ограничена HOVER_TINT.
        let hovered = row_plate(row_rect(), &palette(), false, HOVER_TINT, 0.0);
        let egui::Shape::Vec(shapes) = &hovered else {
            panic!("наведение обязано дать подложку: {hovered:?}");
        };
        assert_eq!(shapes.len(), 1);

        // Сжатие без наведения (например, тач-нажатие) тоже рисует пластину.
        let pressed = row_plate(row_rect(), &palette(), false, 0.0, 1.0);
        assert!(
            matches!(pressed, egui::Shape::Vec(_)),
            "press без hover не должен исчезать"
        );
    }

    #[test]
    fn hover_tint_stays_below_the_playing_fill() {
        let palette = palette();
        let dim = faded(palette.chip_fill, HOVER_TINT);
        let [.., dim_a] = dim.to_srgba_unmultiplied();
        let [.., base_a] = palette.chip_fill.to_srgba_unmultiplied();
        assert!(u16::from(dim_a) < u16::from(base_a));
    }

    fn second_station() -> Station {
        let mut station = sample_station();
        station.stationuuid = "uuid-2".to_owned();
        station.name = "Second Radio".to_owned();
        station
    }

    fn key(key: egui::Key) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }
    }

    /// Один кадр headless-egui с двумя соседними строками списка.
    fn run_frame(
        ctx: &egui::Context,
        events: Vec<egui::Event>,
        request: Option<egui::Id>,
    ) -> Vec<UiAction> {
        let palette = Palette::for_ui(egui::Theme::Dark, crate::settings::AccentColor::Blue);
        let first = sample_station();
        let second = second_station();
        let favorites: [Station; 0] = [];
        let mut actions = Vec::new();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(420.0, 600.0),
            )),
            events,
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| {
            if let Some(id) = request {
                ui.memory_mut(|memory| memory.request_focus(id));
            }
            let mut first_cache = FaviconCache::new();
            let mut first_state = RowContext {
                playing: false,
                selected: false,
                reveal: false,
                compact: false,
                favorites: &favorites,
                language: Language::English,
                favicons: &mut first_cache,
                previous: None,
                next: Some("uuid-2"),
            };
            station_row(ui, &palette, &first, &mut first_state, &mut actions);
            let mut second_cache = FaviconCache::new();
            let mut second_state = RowContext {
                playing: false,
                selected: false,
                reveal: false,
                compact: false,
                favorites: &favorites,
                language: Language::English,
                favicons: &mut second_cache,
                previous: Some("uuid-1"),
                next: None,
            };
            station_row(ui, &palette, &second, &mut second_state, &mut actions);
        });
        output.textures_delta.clear();
        actions
    }

    #[test]
    fn arrow_keys_move_focus_along_the_visible_list() {
        let ctx = egui::Context::default();
        let first = row_id("uuid-1");
        let second = row_id("uuid-2");
        run_frame(&ctx, Vec::new(), Some(first));
        assert_eq!(ctx.memory(|memory| memory.focused()), Some(first));

        run_frame(&ctx, vec![key(egui::Key::ArrowDown)], None);
        assert_eq!(ctx.memory(|memory| memory.focused()), Some(second));

        run_frame(&ctx, vec![key(egui::Key::ArrowUp)], None);
        assert_eq!(ctx.memory(|memory| memory.focused()), Some(first));
    }

    #[test]
    fn space_activates_the_focused_row() {
        let ctx = egui::Context::default();
        run_frame(&ctx, Vec::new(), Some(row_id("uuid-1")));
        let actions = run_frame(&ctx, vec![key(egui::Key::Space)], None);
        assert!(
            matches!(actions.as_slice(), [UiAction::Play(station)] if station.stationuuid == "uuid-1"),
            "Space на сфокусированной строке должен запускать станцию: {actions:?}"
        );
    }
}
