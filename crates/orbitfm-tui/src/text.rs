//! Санитизация и обрезка недоверенных строк от ядра.
//!
//! Защита от terminal escape injection (C0/C1/DEL/ESC) и bidi-переопределений;
//! ширина считается по `unicode-width`, чтобы не ломать рамки
//! (`docs/interfaces/16-tui.md` §8, ADR-0047 §7).

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// Убирает управляющие символы и bidi-переопределения; переводы строк и табы
/// заменяет пробелом.
pub fn sanitize(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        if is_forbidden(ch) {
            continue;
        }
        out.push(match ch {
            '\n' | '\r' | '\t' => ' ',
            _ => ch,
        });
    }
    out
}

/// Обрезает строку до `max` терминальных колонок, добавляя `...`.
pub fn truncate(input: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    if UnicodeWidthStr::width(input) <= max {
        return input.to_owned();
    }
    let budget = max.saturating_sub(3);
    let mut out = String::new();
    let mut width = 0;
    for ch in input.chars() {
        let char_width = UnicodeWidthChar::width(ch).unwrap_or(0);
        if width + char_width > budget {
            break;
        }
        out.push(ch);
        width += char_width;
    }
    out.push_str("...");
    out
}

/// `sanitize` + `truncate` одной операцией.
pub fn clean(input: &str, max: usize) -> String {
    truncate(&sanitize(input), max)
}

fn is_forbidden(ch: char) -> bool {
    let code = ch as u32;
    let control = code < 0x20 && !matches!(ch, '\n' | '\r' | '\t');
    control
        || (0x7f..=0x9f).contains(&code)
        || matches!(
            ch,
            '\u{061c}'
                | '\u{200e}'
                | '\u{200f}'
                | '\u{202a}'..='\u{202e}'
                | '\u{2066}'..='\u{2069}'
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_escapes_and_controls() {
        let dirty = "Radio\u{1b}[31mXYZ\u{7f}\u{9b}x";
        assert_eq!(sanitize(dirty), "Radio[31mXYZx");
    }

    #[test]
    fn newlines_and_tabs_become_spaces() {
        assert_eq!(sanitize("a\nb\tc\rd"), "a b c d");
    }

    #[test]
    fn strips_bidi_overrides() {
        let dirty = "a\u{202e}b\u{200f}c";
        assert_eq!(sanitize(dirty), "abc");
    }

    #[test]
    fn truncates_by_display_width() {
        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate("hello world", 8), "hello...");
        assert_eq!(truncate("", 5), "");
        assert_eq!(truncate("abc", 0), "");
    }

    #[test]
    fn wide_characters_count_double() {
        // Две CJK-идеограммы занимают 4 колонки, плюс "..." = 7.
        assert_eq!(truncate("日本日本", 7), "日本...");
    }

    #[test]
    fn clean_combines_steps() {
        assert_eq!(clean("hello\nworld", 8), "hello...");
    }
}
