//! Краткие сообщения поверх интерфейса (одно сообщение, 3 секунды).

use std::time::Duration;

use eframe::egui;

use super::palette::Palette;

const TOAST_DURATION: f64 = 3.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ToastKind {
    Info,
    Error,
}

#[derive(Debug)]
struct ToastEntry {
    text: String,
    kind: ToastKind,
    until: f64,
}

#[derive(Debug, Default)]
pub(crate) struct Toast {
    entry: Option<ToastEntry>,
}

impl Toast {
    pub(crate) fn info(&mut self, ctx: &egui::Context, text: impl Into<String>) {
        self.push(ctx, text.into(), ToastKind::Info);
    }

    pub(crate) fn error(&mut self, ctx: &egui::Context, text: impl Into<String>) {
        self.push(ctx, text.into(), ToastKind::Error);
    }

    fn push(&mut self, ctx: &egui::Context, text: String, kind: ToastKind) {
        let now = ctx.input(|input| input.time);
        self.entry = Some(ToastEntry {
            text,
            kind,
            until: now + TOAST_DURATION,
        });
        ctx.request_repaint_after(Duration::from_secs_f64(TOAST_DURATION));
    }

    pub(crate) fn draw(&mut self, ctx: &egui::Context, palette: &Palette) {
        let now = ctx.input(|input| input.time);
        if self.entry.as_ref().is_some_and(|entry| now >= entry.until) {
            self.entry = None;
        }
        let Some(entry) = self.entry.as_ref() else {
            return;
        };
        let (stroke, text_color) = match entry.kind {
            ToastKind::Info => (palette.accent, palette.accent),
            ToastKind::Error => (palette.error, palette.error),
        };

        egui::Area::new(egui::Id::unique("orbitfm_toast"))
            .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -78.0))
            .order(egui::Order::Foreground)
            .interactable(false)
            .show(ctx, |ui| {
                egui::Frame::default()
                    .fill(palette.card_fill)
                    .stroke(egui::Stroke::new(1.0, stroke))
                    .corner_radius(egui::CornerRadius::same(10))
                    .inner_margin(egui::Margin::symmetric(14, 8))
                    .show(ui, |ui| {
                        ui.set_max_width(420.0);
                        ui.label(
                            egui::RichText::new(&entry.text)
                                .size(12.0)
                                .color(text_color),
                        );
                    });
            });

        ctx.request_repaint_after(Duration::from_secs_f64((entry.until - now).max(0.0)));
    }
}
