//! Подписи трея, общие для SNI (ADR-0020) и строки меню macOS (ADR-0026):
//! оба бэкенда обязаны показывать одинаковый текст из каталога языка снимка
//! (ADR-0030, п. 8–9).
//!
//! Название станции — внешние данные Radio Browser, оно не переводится;
//! переводится только заглушка, когда станции нет (ADR-0030, п. 4).

use super::TraySnapshot;
use crate::i18n;

/// Первая строка меню — витрина, а не команда.
pub(crate) fn now_playing_label(snapshot: &TraySnapshot) -> String {
    let text = &i18n::catalog(snapshot.language).tray;
    match (&snapshot.station, snapshot.playing) {
        (Some(station), true) => i18n::fill(text.now_playing, &[("station", station.as_str())]),
        (Some(station), false) => i18n::fill(text.paused, &[("station", station.as_str())]),
        (None, _) => nothing_playing(snapshot),
    }
}

/// Заглушка, когда станция не играет: общая для тултипа и первой строки меню.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Language;

    #[test]
    fn header_reflects_the_stream() {
        let playing = TraySnapshot {
            playing: true,
            station: Some("WFMU".to_owned()),
            language: Language::English,
            ..Default::default()
        };
        assert_eq!(now_playing_label(&playing), "Now playing: WFMU");
        let paused = TraySnapshot {
            playing: false,
            ..playing.clone()
        };
        assert_eq!(now_playing_label(&paused), "Paused: WFMU");
        assert_eq!(
            now_playing_label(&TraySnapshot::default()),
            "Nothing playing"
        );
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
}
