//! Переключатель режимов поиска и выбор глобального жанра в HUD.

use eframe::egui;

use crate::i18n;
use crate::settings::Language;

use super::palette::Palette;
use super::widgets::pill;
use super::{SearchMode, UiAction};

/// Предустановленные жанры глобального поиска (теги Radio Browser).
pub(crate) const GENRE_PRESETS: [&str; 8] = [
    "rock",
    "pop",
    "jazz",
    "electronic",
    "news",
    "metal",
    "classical",
    "synthwave",
];

/// Подписи `labels.genre_*` в том же порядке, что [`GENRE_PRESETS`] (ADR-0030).
const GENRE_LABELS: [fn(&i18n::LabelText) -> &'static str; GENRE_PRESETS.len()] = [
    |labels| labels.genre_rock,
    |labels| labels.genre_pop,
    |labels| labels.genre_jazz,
    |labels| labels.genre_electronic,
    |labels| labels.genre_news,
    |labels| labels.genre_metal,
    |labels| labels.genre_classical,
    |labels| labels.genre_synthwave,
];

/// Отображаемое имя тега: известный читается из каталога, неизвестный получает
/// заглавную букву — теги Radio Browser переводу не подлежат.
pub(crate) fn genre_label(language: Language, tag: &str) -> String {
    let tag = tag.trim();
    let labels = &i18n::catalog(language).labels;
    let position = GENRE_PRESETS
        .iter()
        .position(|preset| preset.eq_ignore_ascii_case(tag));
    position.map_or_else(
        || capitalize(tag),
        |index| GENRE_LABELS[index](labels).to_owned(),
    )
}

/// Первая буква заглавная.
fn capitalize(tag: &str) -> String {
    let mut chars = tag.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

pub(crate) struct ModeState<'a> {
    pub mode: SearchMode,
    pub selected_genre: Option<&'a str>,
    pub genre_query: &'a mut String,
    /// Разрешённый язык интерфейса (ADR-0030).
    pub language: Language,
}

pub(crate) fn show(ui: &mut egui::Ui, palette: &Palette, state: ModeState<'_>) -> Vec<UiAction> {
    let ModeState {
        mode,
        selected_genre,
        genre_query,
        language,
    } = state;
    let mut actions = Vec::new();
    let text = i18n::catalog(language);

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        if pill(ui, palette, text.hud.country, mode == SearchMode::Country).clicked()
            && mode != SearchMode::Country
        {
            actions.push(UiAction::SwitchMode(SearchMode::Country));
        }
        if pill(ui, palette, text.hud.genre, mode == SearchMode::GlobalGenre).clicked()
            && mode != SearchMode::GlobalGenre
        {
            actions.push(UiAction::SwitchMode(SearchMode::GlobalGenre));
        }

        if mode == SearchMode::GlobalGenre {
            let label = selected_genre.map_or_else(
                || text.hud.pick_genre.to_owned(),
                |tag| genre_label(language, tag),
            );
            let button = ui
                .add(
                    egui::Button::new(egui::RichText::new(label).size(13.0).color(palette.text))
                        .fill(palette.chip_fill)
                        .stroke(egui::Stroke::new(1.0, palette.chip_stroke))
                        .corner_radius(egui::CornerRadius::same(14)),
                )
                .on_hover_text("Search stations by genre worldwide");
            let popup_id = egui::Popup::default_response_id(&button);
            egui::Popup::menu(&button)
                .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                .show(|ui| {
                    ui.set_min_width(260.0);
                    ui.label(egui::RichText::new(text.hud.global_genre).strong());
                    ui.horizontal_wrapped(|ui| {
                        for tag in GENRE_PRESETS {
                            let selected = selected_genre == Some(tag);
                            if ui
                                .selectable_label(selected, genre_label(language, tag))
                                .clicked()
                            {
                                actions.push(UiAction::SelectGenre(tag.to_owned()));
                                egui::Popup::close_id(ui.ctx(), popup_id);
                            }
                        }
                    });
                    ui.add_space(4.0);
                    let response = ui.add(
                        egui::TextEdit::singleline(genre_query)
                            .hint_text(text.hud.custom_tag_hint)
                            .desired_width(240.0),
                    );
                    let enter = response.lost_focus()
                        && ui.input(|input| input.key_pressed(egui::Key::Enter));
                    ui.label(
                        egui::RichText::new(text.hud.genre_search_hint)
                            .size(10.5)
                            .color(palette.muted),
                    );
                    if ui.button(text.common.search).clicked() || enter {
                        let tag = genre_query.trim();
                        if !tag.is_empty() {
                            actions.push(UiAction::SelectGenre(tag.to_owned()));
                            egui::Popup::close_id(ui.ctx(), popup_id);
                        }
                    }
                });
        }
    });

    actions
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_genres_read_the_catalog_and_unknown_keep_capitalization() {
        let ui = Language::English;
        assert_eq!(genre_label(ui, "rock"), "Rock");
        assert_eq!(genre_label(ui, "  synthwave "), "Synthwave");
        assert_eq!(genre_label(ui, "ROCK"), "Rock");
        assert_eq!(genre_label(ui, "ambient"), "Ambient");
        assert_eq!(genre_label(ui, ""), "");
    }

    #[test]
    fn every_language_labels_every_preset() {
        // Проверяем не перевод, а источник строки: таблица подписей обязана идти
        // в такт `GENRE_PRESETS`, а выбор языка — давать непустую подпись.
        for language in Language::ALL
            .into_iter()
            .filter(|language| *language != Language::System)
        {
            let labels = &i18n::catalog(language).labels;
            for (tag, label) in GENRE_PRESETS.iter().zip(GENRE_LABELS) {
                let text = label(labels);
                assert!(!text.trim().is_empty(), "жанр {tag}");
                assert_eq!(genre_label(language, tag), text, "жанр {tag}");
            }
        }
    }

    #[test]
    fn genre_presets_are_unique_lowercase_tags() {
        let mut tags = GENRE_PRESETS.to_vec();
        tags.sort_unstable();
        tags.dedup();
        assert_eq!(tags.len(), GENRE_PRESETS.len());
        assert_eq!(GENRE_LABELS.len(), GENRE_PRESETS.len());
        assert!(
            GENRE_PRESETS
                .iter()
                .all(|tag| *tag == tag.to_lowercase() && !tag.trim().is_empty())
        );
    }
}
