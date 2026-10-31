//! Нормализация недоверенного текста.
//!
//! Имена, страны и теги приходят от Radio Browser, внешних потоков и
//! HTML-страниц. UI, IPC, трей и панели обязаны видеть одинаково очищенную
//! строку: управляющими последовательностями нельзя переписать вывод
//! терминала (bar-виджеты, `status`), а двунаправленными — подменить текст
//! визуально. Модуль не зависит от UI-слоя.

/// C0/C1, DEL и двунаправленные управляющие (U+200E/U+200F, U+202A–U+202E,
/// U+2066–U+2069). Прочие Unicode-символы сохраняются.
fn is_forbidden(character: char) -> bool {
    matches!(
        character,
        '\u{0000}'..='\u{001F}'
            | '\u{007F}'..='\u{009F}'
            | '\u{200E}'..='\u{200F}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2066}'..='\u{2069}'
    )
}

/// Схлопывает пробельные последовательности и заменяет управляющие символы
/// пробелом. Замена, а не удаление: `A\u{1B}B` не должно склеиться в `AB`.
pub(crate) fn single_line(value: &str) -> String {
    let cleaned: String = value
        .chars()
        .map(|character| {
            if is_forbidden(character) {
                ' '
            } else {
                character
            }
        })
        .collect();
    cleaned.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collapses_whitespace() {
        assert_eq!(single_line("Radio\n  Nova\tLive"), "Radio Nova Live");
    }

    #[test]
    fn strips_terminal_and_bidi_controls() {
        assert_eq!(
            single_line("Evil\u{1B}[2JFM\u{7F}"),
            "Evil [2JFM",
            "ESC-последовательность не должна дойти до терминала"
        );
        assert_eq!(
            single_line("Radio\u{202E}FM\u{2066}."),
            "Radio FM .",
            "bidi-переопределения вырезаются"
        );
        assert_eq!(single_line("\u{9B}31mRed"), "31mRed", "C1 тоже");
    }

    #[test]
    fn keeps_ordinary_unicode() {
        assert_eq!(single_line("Радио «Эхо» — 101.2"), "Радио «Эхо» — 101.2");
    }
}
