//! Встроенные fallback-шрифты интерфейса (ADR-0041).
//!
//! Шрифты лежат в бинарнике и подключаются последними в `Proportional` и
//! `Monospace`: основные глифы по-прежнему берёт `Ubuntu-Light`, а встроенный
//! набор закрывает CJK, арабскую вязь и флаги на системе без Noto-пакетов.
//! Системный Noto Color Emoji остаётся предпочтительным, если установлен.

use std::path::Path;

use eframe::egui;

pub(crate) const NOTO_SANS: &[u8] = include_bytes!("../../assets/fonts/NotoSans-Regular.ttf");
pub(crate) const NOTO_SANS_ARABIC: &[u8] =
    include_bytes!("../../assets/fonts/NotoSansArabic-Regular.ttf");
pub(crate) const NOTO_SANS_CJK: &[u8] =
    include_bytes!("../../assets/fonts/NotoSansCJK-Regular-subset.otf");
pub(crate) const NOTO_EMOJI_FLAGS: &[u8] =
    include_bytes!("../../assets/fonts/NotoColorEmoji-flags-subset.ttf");

/// Потолок встроенного набора, байт: вышел за него — превышен бюджет ADR-0041.
#[cfg(test)]
const FONT_BUDGET: usize = 6 * 1024 * 1024;

const SYSTEM_EMOJI_PATHS: &[&str] = &[
    "/usr/share/fonts/noto/NotoColorEmoji.ttf",
    "/usr/share/fonts/truetype/noto/NotoColorEmoji.ttf",
    "/usr/share/fonts/google-noto-emoji/NotoColorEmoji.ttf",
    "/usr/share/fonts/TTF/NotoColorEmoji.ttf",
];

#[cfg(test)]
fn embedded_bytes() -> usize {
    NOTO_SANS.len() + NOTO_SANS_ARABIC.len() + NOTO_SANS_CJK.len() + NOTO_EMOJI_FLAGS.len()
}

/// Ставит встроенный набор и, если есть, системный цветной emoji.
pub(crate) fn install(ctx: &egui::Context) {
    add(ctx, "orbitfm-noto-sans", NOTO_SANS);
    add(ctx, "orbitfm-noto-arabic", NOTO_SANS_ARABIC);
    add(ctx, "orbitfm-noto-cjk", NOTO_SANS_CJK);
    add(ctx, "orbitfm-noto-emoji-flags", NOTO_EMOJI_FLAGS);
    install_system_emoji(ctx);
}

fn add(ctx: &egui::Context, name: &str, bytes: &'static [u8]) {
    ctx.add_font(egui::FontInsert::new(
        name,
        egui::FontData::from_static(bytes),
        vec![
            egui::InsertFontFamily {
                family: egui::FontFamily::Proportional,
                priority: egui::FontPriority::Lowest,
            },
            egui::InsertFontFamily {
                family: egui::FontFamily::Monospace,
                priority: egui::FontPriority::Lowest,
            },
        ],
    ));
}

/// Полный системный emoji-шрифт предпочтительнее сабсета: флаги уже покрыты,
/// зато остальные эмодзи рисуются цветными. Чтение файла — побочный эффект App.
fn install_system_emoji(ctx: &egui::Context) {
    let Some(bytes) = SYSTEM_EMOJI_PATHS
        .iter()
        .map(Path::new)
        .find(|path| path.is_file())
        .and_then(|path| std::fs::read(path).ok())
    else {
        tracing::debug!("системный emoji-шрифт не найден: остаются встроенные флаги");
        return;
    };
    ctx.add_font(egui::FontInsert::new(
        "orbitfm-system-emoji",
        egui::FontData::from_owned(bytes),
        vec![egui::InsertFontFamily {
            family: egui::FontFamily::Proportional,
            priority: egui::FontPriority::Lowest,
        }],
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_fonts_stay_within_the_agreed_budget() {
        let total = embedded_bytes();
        assert!(total > 0);
        assert!(
            total <= FONT_BUDGET,
            "встроенные шрифты занимают {total} байт при потолке {FONT_BUDGET}"
        );
    }

    #[test]
    fn every_embedded_file_is_a_real_font() {
        for (name, bytes) in [
            ("NotoSans", NOTO_SANS),
            ("NotoSansArabic", NOTO_SANS_ARABIC),
            ("NotoSansCJK", NOTO_SANS_CJK),
            ("NotoColorEmoji", NOTO_EMOJI_FLAGS),
        ] {
            assert!(bytes.len() > 10_000, "{name}: подозрительно маленький файл");
            assert!(
                bytes.starts_with(b"\x00\x01\x00\x00")
                    || bytes.starts_with(b"OTTO")
                    || bytes.starts_with(b"true"),
                "{name}: нет сигнатуры sfnt"
            );
        }
    }
}
