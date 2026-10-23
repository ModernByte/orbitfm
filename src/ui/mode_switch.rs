//! Переключатель режимов поиска и выбор глобального жанра в HUD.

use eframe::egui;

use super::palette::Palette;
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

/// Отображаемое имя тега: первая буква заглавная.
pub(crate) fn genre_label(tag: &str) -> String {
    let tag = tag.trim();
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
}

pub(crate) fn show(ui: &mut egui::Ui, palette: &Palette, state: ModeState<'_>) -> Vec<UiAction> {
    let ModeState {
        mode,
        selected_genre,
        genre_query,
    } = state;
    let mut actions = Vec::new();

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        if ui
            .selectable_label(mode == SearchMode::Country, "Страна")
            .clicked()
            && mode != SearchMode::Country
        {
            actions.push(UiAction::SwitchMode(SearchMode::Country));
        }
        if ui
            .selectable_label(mode == SearchMode::GlobalGenre, "Жанр")
            .clicked()
            && mode != SearchMode::GlobalGenre
        {
            actions.push(UiAction::SwitchMode(SearchMode::GlobalGenre));
        }

        if mode == SearchMode::GlobalGenre {
            let label = selected_genre.map_or_else(|| "Выбрать жанр".to_owned(), genre_label);
            let button = ui
                .button(label)
                .on_hover_text("Глобальный поиск станций по жанру");
            let popup_id = egui::Popup::default_response_id(&button);
            egui::Popup::menu(&button)
                .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                .show(|ui| {
                    ui.set_min_width(260.0);
                    ui.label(egui::RichText::new("Глобальный жанр").strong());
                    ui.horizontal_wrapped(|ui| {
                        for tag in GENRE_PRESETS {
                            let selected = selected_genre == Some(tag);
                            if ui.selectable_label(selected, genre_label(tag)).clicked() {
                                actions.push(UiAction::SelectGenre(tag.to_owned()));
                                egui::Popup::close_id(ui.ctx(), popup_id);
                            }
                        }
                    });
                    ui.add_space(4.0);
                    let response = ui.add(
                        egui::TextEdit::singleline(genre_query)
                            .hint_text("Свой тег, например ambient")
                            .desired_width(240.0),
                    );
                    let enter = response.lost_focus()
                        && ui.input(|input| input.key_pressed(egui::Key::Enter));
                    ui.label(
                        egui::RichText::new(
                            "Поиск по всему миру, станции без координат остаются в списке.",
                        )
                        .size(10.5)
                        .color(palette.muted),
                    );
                    if ui.button("Искать").clicked() || enter {
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
    fn genre_labels_are_capitalized_and_trimmed() {
        assert_eq!(genre_label("rock"), "Rock");
        assert_eq!(genre_label("  synthwave "), "Synthwave");
        assert_eq!(genre_label(""), "");
    }

    #[test]
    fn genre_presets_are_unique_lowercase_tags() {
        let mut tags = GENRE_PRESETS.to_vec();
        tags.sort_unstable();
        tags.dedup();
        assert_eq!(tags.len(), GENRE_PRESETS.len());
        assert!(
            GENRE_PRESETS
                .iter()
                .all(|tag| *tag == tag.to_lowercase() && !tag.trim().is_empty())
        );
    }
}
