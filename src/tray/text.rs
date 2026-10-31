//! Подписи трея, общие для SNI (ADR-0020) и строки меню macOS (ADR-0026):
//! оба бэкенда обязаны показывать одинаковый текст из каталога языка снимка
//! (ADR-0030, п. 8–9).
//!
//! Название станции, страна и теги — внешние данные Radio Browser, они не
//! переводятся; переводятся только заглушка и подписи групп меню (ADR-0030,
//! п. 4).

use super::{TraySnapshot, TrayStation};
use crate::i18n;

/// Пределы подписей карточки: длинные названия и теги Radio Browser не должны
/// растягивать меню на весь экран.
const NAME_LIMIT: usize = 48;
#[cfg_attr(not(target_os = "linux"), allow(dead_code))] // карточка SNI
const DETAILS_LIMIT: usize = 64;
#[cfg_attr(not(target_os = "linux"), allow(dead_code))] // карточка SNI
const TAGS_LIMIT: usize = 56;
#[cfg_attr(not(target_os = "macos"), allow(dead_code))] // строка меню macOS
const SUMMARY_LIMIT: usize = 80;

/// Первая строка карточки: `Now playing: WFMU` или `Paused: WFMU`.
pub(crate) fn now_playing_label(snapshot: &TraySnapshot) -> String {
    let text = &i18n::catalog(snapshot.language).tray;
    let Some(station) = &snapshot.station else {
        return nothing_playing(snapshot);
    };
    let name = truncate(&station.name, NAME_LIMIT);
    if snapshot.playing {
        i18n::fill(text.now_playing, &[("station", name.as_str())])
    } else {
        i18n::fill(text.paused, &[("station", name.as_str())])
    }
}

/// Строки карточки станции в меню: имя, `страна · битрейт`, теги.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))] // карточка SNI
pub(crate) fn card_lines(snapshot: &TraySnapshot) -> Vec<String> {
    let Some(station) = &snapshot.station else {
        return vec![nothing_playing(snapshot)];
    };
    let mut lines = vec![now_playing_label(snapshot)];
    if let Some(details) = station_details(station) {
        lines.push(truncate(&details, DETAILS_LIMIT));
    }
    if let Some(tags) = station_tags(station) {
        lines.push(truncate(&tags, TAGS_LIMIT));
    }
    lines
}

/// Имя станции для тултипа; без станции — заглушка.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))] // тултип SNI
pub(crate) fn station_name(snapshot: &TraySnapshot) -> String {
    snapshot.station.as_ref().map_or_else(
        || nothing_playing(snapshot),
        |station| truncate(&station.name, NAME_LIMIT),
    )
}

/// Строка `страна · битрейт`; `None`, если оба поля пусты (внешний поток).
pub(crate) fn station_details(station: &TrayStation) -> Option<String> {
    let mut parts = Vec::new();
    let country = station.country.trim();
    if !country.is_empty() {
        parts.push(country.to_owned());
    }
    if station.bitrate > 0 {
        parts.push(format!("{} kbps", station.bitrate));
    }
    (!parts.is_empty()).then(|| parts.join(" · "))
}

/// Теги через запятую без пустых сегментов; `None`, если тегов нет.
pub(crate) fn station_tags(station: &TrayStation) -> Option<String> {
    let tags = station
        .tags
        .split(',')
        .map(str::trim)
        .filter(|tag| !tag.is_empty())
        .collect::<Vec<_>>()
        .join(", ");
    (!tags.is_empty()).then_some(tags)
}

/// Одна строка для меню macOS: `страна · битрейт · теги`.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))] // строка меню macOS
pub(crate) fn station_summary(station: &TrayStation) -> Option<String> {
    let parts = [station_details(station), station_tags(station)]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    (!parts.is_empty()).then(|| truncate(&parts.join(" · "), SUMMARY_LIMIT))
}

/// Заглушка, когда станция не играет: общая для тултипа и карточки.
pub(crate) fn nothing_playing(snapshot: &TraySnapshot) -> String {
    i18n::catalog(snapshot.language)
        .tray
        .nothing_playing
        .to_owned()
}

/// Пункт окна: показать или спрятать.
pub(crate) fn toggle_label(snapshot: &TraySnapshot) -> &'static str {
    let catalog = i18n::catalog(snapshot.language);
    let text = &catalog.tray;
    if snapshot.window_visible {
        text.hide_window
    } else {
        text.show_window
    }
}

/// Заголовок группы громкости: `Volume · 70%`.
pub(crate) fn volume_label(snapshot: &TraySnapshot) -> String {
    format!(
        "{} · {}%",
        i18n::catalog(snapshot.language).tray.volume,
        snapshot.volume_percent
    )
}

/// Пункт избранного: добавить или убрать.
pub(crate) fn favorite_label(snapshot: &TraySnapshot) -> &'static str {
    let common = &i18n::catalog(snapshot.language).common;
    if snapshot.favorite {
        common.remove_favorite
    } else {
        common.add_favorite
    }
}

/// Обрезает подпись по символам, а не по байтам: станции бывают на любом языке.
fn truncate(value: &str, limit: usize) -> String {
    let mut chars = value.chars();
    let head: String = chars.by_ref().take(limit).collect();
    if chars.next().is_some() {
        let mut clipped: String = head.chars().take(limit.saturating_sub(1)).collect();
        clipped.push('…');
        clipped
    } else {
        head
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Language;

    fn english(station: Option<TrayStation>) -> TraySnapshot {
        TraySnapshot {
            playing: station.is_some(),
            station,
            language: Language::English,
            ..Default::default()
        }
    }

    fn wfmu() -> TrayStation {
        TrayStation {
            name: "WFMU".to_owned(),
            country: "US".to_owned(),
            tags: "eclectic, freeform".to_owned(),
            bitrate: 128,
            homepage: Some("https://wfmu.org".to_owned()),
        }
    }

    #[test]
    fn card_shows_name_meta_and_tags() {
        assert_eq!(
            card_lines(&english(Some(wfmu()))),
            vec!["Now playing: WFMU", "US · 128 kbps", "eclectic, freeform"]
        );
        let paused = TraySnapshot {
            playing: false,
            ..english(Some(wfmu()))
        };
        assert_eq!(
            card_lines(&paused).first().map(String::as_str),
            Some("Paused: WFMU")
        );
        assert_eq!(
            card_lines(&TraySnapshot {
                language: Language::English,
                ..Default::default()
            }),
            vec!["Nothing playing"]
        );
    }

    #[test]
    fn details_skip_empty_parts() {
        let station = TrayStation {
            country: "  ".to_owned(),
            bitrate: 0,
            ..wfmu()
        };
        assert_eq!(station_details(&station), None);
        assert_eq!(
            station_details(&TrayStation {
                bitrate: 192,
                ..station
            })
            .as_deref(),
            Some("192 kbps")
        );
    }

    #[test]
    fn tags_drop_empty_segments() {
        let station = TrayStation {
            tags: " , jazz, ,soul , ".to_owned(),
            ..wfmu()
        };
        assert_eq!(station_tags(&station).as_deref(), Some("jazz, soul"));
        assert_eq!(
            station_tags(&TrayStation {
                tags: " , ".to_owned(),
                ..wfmu()
            }),
            None
        );
    }

    #[test]
    fn long_labels_are_clipped_by_chars() {
        let station = TrayStation {
            name: "Очень длинное название станции, которое не помещается".to_owned(),
            ..wfmu()
        };
        let line = card_lines(&english(Some(station))).remove(0);
        assert_eq!(
            line.chars().count(),
            "Now playing: ".chars().count() + NAME_LIMIT
        );
        assert!(line.ends_with('…'));
    }

    #[test]
    fn summary_joins_meta_and_tags_for_the_macos_menu() {
        assert_eq!(
            station_summary(&wfmu()).as_deref(),
            Some("US · 128 kbps · eclectic, freeform")
        );
        assert_eq!(
            station_summary(&TrayStation {
                country: String::new(),
                tags: String::new(),
                bitrate: 0,
                ..wfmu()
            }),
            None
        );
    }

    #[test]
    fn volume_and_favorite_labels_follow_the_state() {
        let mut snapshot = english(Some(wfmu()));
        snapshot.volume_percent = 70;
        assert_eq!(volume_label(&snapshot), "Volume · 70%");
        assert_eq!(favorite_label(&snapshot), "Add to favorites");
        snapshot.favorite = true;
        assert_eq!(favorite_label(&snapshot), "Remove from favorites");
    }

    #[test]
    fn toggle_label_follows_the_window() {
        let mut snapshot = TraySnapshot {
            language: Language::English,
            ..Default::default()
        };
        assert_eq!(toggle_label(&snapshot), "Show window");
        snapshot.window_visible = true;
        assert_eq!(toggle_label(&snapshot), "Hide window");
    }

    #[test]
    fn station_name_falls_back_to_nothing_playing() {
        assert_eq!(station_name(&english(Some(wfmu()))), "WFMU");
        assert_eq!(
            station_name(&TraySnapshot {
                language: Language::English,
                ..Default::default()
            }),
            "Nothing playing"
        );
    }
}
