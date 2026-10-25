//! Краткие сообщения поверх интерфейса: стек до трёх записей (ADR-0032).
//!
//! Переходом владеет сам тост: `Presence` живёт внутри записи, потому что
//! сообщение появляется в любой кадр и обязано доиграть уход даже тогда, когда
//! ядро уже не отдаёт `Notice`. Кадры перехода запрашивает ядро, наш
//! `request_repaint_after` остаётся единственным будильником по таймеру жизни.

use std::time::Duration;

use eframe::egui;

use super::animation::{self, Presence};
use super::palette::Palette;
use super::widgets::faded;

/// Больше трёх слоёв контракт не допускает: четвёртое вытесняет самое старое.
const STACK_LIMIT: usize = 3;
/// Секунды, которые сообщение висит до начала ухода.
const TOAST_DURATION: f64 = 3.0;
/// Вертикальный зазор между записями стека.
const TOAST_GAP: f32 = 6.0;
/// Шире сообщение не рисуем: длинный текст переносится.
const TOAST_WIDTH: f32 = 420.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ToastKind {
    Info,
    Error,
}

/// Одна запись стека: текст, срок жизни и собственный переход.
#[derive(Debug)]
struct ToastEntry {
    text: String,
    kind: ToastKind,
    /// Момент `egui`-времени, с которого запись начинает уходить.
    until: f64,
    /// Слот перехода: по нему считаются и `Id` области, и `Presence`.
    slot: usize,
    presence: Presence,
    /// Прозрачность последнего кадра: `0.0` до входа и после ухода.
    shown: f32,
}

#[derive(Debug, Default)]
pub(crate) struct Toast {
    /// От самого старого к самому новому.
    entries: Vec<ToastEntry>,
    /// Слоты доигравших уход записей. `Id::unique` детерминирован по источнику,
    /// поэтому ключей перехода ровно столько, сколько слоёв стека, а не столько,
    /// сколько сообщений прошло за сессию.
    freed: [usize; STACK_LIMIT],
    freed_len: usize,
    /// Следующий слот, ещё не выходивший на экран.
    next_slot: usize,
}

impl Toast {
    pub(crate) fn info(&mut self, ctx: &egui::Context, text: impl Into<String>) {
        self.push(ctx, text.into(), ToastKind::Info);
    }

    pub(crate) fn error(&mut self, ctx: &egui::Context, text: impl Into<String>) {
        self.push(ctx, text.into(), ToastKind::Error);
    }

    /// Добавляет сообщение, а дубликат — обновляет уже стоящее: новый слой при
    /// повторе не появляется (ADR-0032 §4).
    fn push(&mut self, ctx: &egui::Context, text: String, kind: ToastKind) {
        let until = ctx.input(|input| input.time) + TOAST_DURATION;
        let duplicate = self.entries.iter().position(|entry| entry.text == text);
        if let Some(index) = duplicate {
            let entry = &mut self.entries[index];
            entry.text = text;
            entry.kind = kind;
            entry.until = until;
        } else {
            if self.entries.len() == STACK_LIMIT {
                // Вытеснение самого старого слоя. Его слот в пул не возвращается:
                // переход оборван, а значение egui на нём неизвестно.
                self.entries.remove(0);
            }
            let slot = self.take_slot();
            let id = slot_id(slot);
            self.entries.push(ToastEntry {
                text,
                kind,
                until,
                slot,
                presence: Presence::new(id),
                shown: 0.0,
            });
        }
        ctx.request_repaint_after(Duration::from_secs_f64(TOAST_DURATION));
    }

    /// Берёт свободный слот: сначала из пула доигравших, иначе новый.
    fn take_slot(&mut self) -> usize {
        if self.freed_len > 0 {
            self.freed_len -= 1;
            return self.freed[self.freed_len];
        }
        let slot = self.next_slot;
        self.next_slot += 1;
        slot
    }

    /// Возвращает слот пула: запись снята со стека с нулевой прозрачностью,
    /// поэтому следующий тост начнёт вход с нуля.
    fn release_slot(&mut self, slot: usize) {
        if self.freed_len < STACK_LIMIT {
            self.freed[self.freed_len] = slot;
            self.freed_len += 1;
        }
    }

    /// Стек у центра нижнего края viewport: свежее сообщение сидит на отступе
    /// `offset` (его даёт [`super::layout::DeskLayout::toast_offset`]), старые
    /// поднимаются над ним. Смещения — только краски (`rise_y`), область
    /// неинтерактивна, поэтому ни hit-зон, ни чужих аллокаций не двигается.
    pub(crate) fn draw(
        &mut self,
        ctx: &egui::Context,
        palette: &Palette,
        offset: f32,
        reduce: bool,
    ) {
        if self.entries.is_empty() {
            return;
        }
        let now = ctx.input(|input| input.time);

        // Прогресс — ровно один опрос `Presence` на запись за кадр: второй опрос
        // того же `Id` рассинхронизировал бы переход (docs/modules/15-ui-motion).
        for entry in &mut self.entries {
            let alive = now < entry.until;
            let duration = if alive {
                animation::TOAST_IN
            } else {
                animation::TOAST_OUT
            };
            entry.shown = animation::alpha(entry.presence.value(ctx, alive, duration, reduce));
        }

        // Доигранная запись снята со стека: слот свободен, кадры больше не нужны.
        let mut index = self.entries.len();
        while index > 0 {
            index -= 1;
            if self.entries[index].shown <= 0.0 && now >= self.entries[index].until {
                let slot = self.entries.remove(index).slot;
                self.release_slot(slot);
            }
        }

        // Обход снизу вверх: высота строки известна сразу после её отрисовки,
        // поэтому следующий поднимается ровно на неё без запаса на угадывание.
        let mut stack = 0.0;
        for entry in self.entries.iter().rev() {
            let (edge, ink) = match entry.kind {
                ToastKind::Info => (palette.accent, palette.accent),
                ToastKind::Error => (palette.error, palette.error),
            };
            let area = egui::Area::new(slot_id(entry.slot))
                .anchor(
                    egui::Align2::CENTER_BOTTOM,
                    row_offset(entry.shown, stack, offset),
                )
                .order(egui::Order::Foreground)
                .interactable(false)
                // Длительность задаёт ядро: собственный fade Area не добавляем.
                .fade_in(false)
                .show(ctx, |ui| {
                    egui::Frame::default()
                        .fill(faded(palette.card_fill, entry.shown))
                        .stroke(egui::Stroke::new(1.0, faded(edge, entry.shown)))
                        .corner_radius(egui::CornerRadius::same(10))
                        .inner_margin(egui::Margin::symmetric(14, 8))
                        .show(ui, |ui| {
                            ui.set_max_width(TOAST_WIDTH);
                            ui.label(
                                egui::RichText::new(&entry.text)
                                    .size(12.0)
                                    .color(faded(ink, entry.shown)),
                            );
                        });
                });
            stack += area.response.rect.height() + TOAST_GAP;
        }

        // Таймер жизни: будит окно к ближайшему сроку, дальше работают `Presence`.
        let wake = self
            .entries
            .iter()
            .filter(|entry| entry.until > now)
            .map(|entry| entry.until - now)
            .fold(f64::INFINITY, f64::min);
        if wake.is_finite() {
            ctx.request_repaint_after(Duration::from_secs_f64(wake));
        }
    }
}

/// Якорь строки стека: центр низа viewport с отступом `offset`, подъём
/// `RISE_Y` пикселей снизу в закрытом состоянии и ноль смещения в покое.
fn row_offset(shown: f32, stack: f32, offset: f32) -> egui::Vec2 {
    egui::vec2(0.0, -(offset + stack) + animation::rise_y(shown))
}

/// Ключ перехода и области слоя стека. `egui::Id::unique` хеширует источник
/// детерминированно (одни и те же семена), поэтому один и тот же текст дал бы
/// один `Id` на все слои: различаются только номера слотов.
fn slot_id(slot: usize) -> egui::Id {
    egui::Id::unique(("orbitfm_toast_entry", slot))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::AccentColor;

    /// Отступ стека в тестах: в живом кадре его считает `DeskLayout::toast_offset`.
    const TEST_OFFSET: f32 = 78.0;

    /// Один кадр с фиксированным временем — как в тестах модуля `animation`.
    fn pass(ctx: &egui::Context, seconds: f64, mut body: impl FnMut(&egui::Context)) {
        let input = egui::RawInput {
            time: Some(seconds),
            predicted_dt: 1.0 / 60.0,
            ..Default::default()
        };
        let output = ctx.run_pass(input, |ctx| body(ctx));
        // Дельты текстур (шрифт на первом кадре) в тесте не применяются.
        output.drop_without_applying_deltas();
    }

    fn palette() -> Palette {
        Palette::for_ui(egui::Theme::Dark, AccentColor::Blue)
    }

    /// Публикация сообщения внутри кадра: время берётся из `RawInput`.
    fn publish(ctx: &egui::Context, toast: &mut Toast, seconds: f64, text: &str) {
        pass(ctx, seconds, |ctx| toast.info(ctx, text));
    }

    #[test]
    fn duplicate_updates_the_existing_entry_instead_of_a_new_layer() {
        let ctx = egui::Context::default();
        let mut toast = Toast::default();
        publish(&ctx, &mut toast, 0.0, "настройки сохранены");
        publish(&ctx, &mut toast, 1.0, "эфир потерян");
        assert_eq!(toast.entries.len(), 2);

        publish(&ctx, &mut toast, 2.0, "настройки сохранены");
        assert_eq!(toast.entries.len(), 2, "дубликат не добавляет слой");
        let first = &toast.entries[0];
        assert_eq!(first.text, "настройки сохранены");
        assert_eq!(first.until, 2.0 + TOAST_DURATION, "срок пересчитан");
        assert_eq!(
            toast.entries[1].until,
            1.0 + TOAST_DURATION,
            "сосед не задет"
        );
    }

    #[test]
    fn duplicate_keeps_its_place_in_the_stack() {
        let ctx = egui::Context::default();
        let mut toast = Toast::default();
        publish(&ctx, &mut toast, 0.0, "первое");
        publish(&ctx, &mut toast, 0.5, "второе");
        let first_slot = toast.entries[0].slot;

        publish(&ctx, &mut toast, 1.0, "первое");
        assert_eq!(toast.entries.len(), 2);
        assert_eq!(toast.entries[0].slot, first_slot, "запись не пересоздана");
        assert_eq!(
            toast
                .entries
                .iter()
                .map(|entry| entry.text.as_str())
                .collect::<Vec<_>>(),
            vec!["первое", "второе"],
            "стек остаётся в порядке появления"
        );
        assert_slots_are_distinct(&toast);
    }

    #[test]
    fn recycled_slot_starts_a_clean_entrance() {
        let ctx = egui::Context::default();
        let palette = palette();
        let mut toast = Toast::default();
        publish(&ctx, &mut toast, 0.0, "первое");
        // Первый слой доигрывает уход и освобождает слот.
        for frame in 1..=200 {
            if step(&ctx, &mut toast, &palette, frame).is_none() {
                break;
            }
        }
        assert!(toast.entries.is_empty(), "слой не снялся со стека");
        let released = toast.freed[..toast.freed_len].to_vec();
        assert_eq!(released.len(), 1, "слот не вернулся в пул");

        publish(&ctx, &mut toast, 4.0, "второе");
        assert_eq!(
            toast.entries[0].slot, released[0],
            "слот должен повториться"
        );
        // Вход нового сообщения начинается с нуля, а не с остатков старого.
        assert_eq!(step(&ctx, &mut toast, &palette, 241), Some(0.0));
        assert!(
            step(&ctx, &mut toast, &palette, 242).is_some_and(|shown| shown < 0.5),
            "переход унаследовал прозрачность предыдущего слоя"
        );
        assert_slots_are_distinct(&toast);
    }

    /// Живые слои не могут делить один ключ: иначе `Presence` и область стека
    /// перетаскивали бы состояние друг друга.
    fn assert_slots_are_distinct(toast: &Toast) {
        for (left, a) in toast.entries.iter().enumerate() {
            for (right, b) in toast.entries.iter().enumerate() {
                if left == right {
                    continue;
                }
                assert_ne!(a.slot, b.slot, "слои {left} и {right} разделили слот");
                assert_ne!(
                    slot_id(a.slot),
                    slot_id(b.slot),
                    "разные слоты дали один Id"
                );
            }
        }
    }

    #[test]
    fn stack_never_grows_past_three() {
        let ctx = egui::Context::default();
        let mut toast = Toast::default();
        for (seconds, text) in [(0.0, "а"), (1.0, "б"), (2.0, "в"), (3.0, "г")] {
            publish(&ctx, &mut toast, seconds, text);
        }
        assert_eq!(toast.entries.len(), STACK_LIMIT);
        assert_eq!(
            toast
                .entries
                .iter()
                .map(|entry| entry.text.as_str())
                .collect::<Vec<_>>(),
            vec!["б", "в", "г"],
            "вытесняется самая старая запись"
        );
        assert_eq!(
            toast.entries.last().expect("стек не пуст").until,
            3.0 + TOAST_DURATION
        );
        assert_slots_are_distinct(&toast);
        // Вытесненный слой слот в пул не вернули: значение его перехода нам
        // неизвестно, а новый тост должен входить с нуля.
        assert_eq!(toast.freed_len, 0, "вытесненный слот вернулся в пул");
    }

    #[test]
    fn reduce_motion_gives_final_state_without_a_transition() {
        let ctx = egui::Context::default();
        let palette = palette();
        let mut toast = Toast::default();
        publish(&ctx, &mut toast, 0.0, "громкость сохранена");

        // Кадр сразу после публикации: без reduce motion здесь был бы ноль.
        pass(&ctx, 0.001, |ctx| {
            toast.draw(ctx, &palette, TEST_OFFSET, true)
        });
        assert_eq!(toast.entries[0].shown, 1.0, "reduce motion — сразу видно");
        assert_eq!(
            row_offset(toast.entries[0].shown, 0.0, TEST_OFFSET),
            egui::vec2(0.0, -TEST_OFFSET),
            "смещения нет"
        );

        // Срок вышел: запись снята в том же кадре, цикла ухода нет.
        pass(&ctx, TOAST_DURATION + 0.001, |ctx| {
            toast.draw(ctx, &palette, TEST_OFFSET, true)
        });
        assert!(toast.entries.is_empty(), "reduce motion не держит слой");
    }

    /// Кадры строго по `1/60` с: `Presence` считает прогресс по `stable_dt`,
    /// поэтому скачки времени исказили бы длительность перехода.
    fn step(ctx: &egui::Context, toast: &mut Toast, palette: &Palette, frame: u32) -> Option<f32> {
        let seconds = f64::from(frame) / 60.0;
        let mut shown = None;
        pass(ctx, seconds, |ctx| {
            toast.draw(ctx, palette, TEST_OFFSET, false);
            shown = toast.entries.first().map(|entry| entry.shown);
        });
        shown
    }

    #[test]
    fn entrance_takes_toast_in_and_rests_on_the_anchor() {
        let ctx = egui::Context::default();
        let palette = palette();
        let mut toast = Toast::default();
        publish(&ctx, &mut toast, 0.0, "станция найдена");

        // Presence сидирует закрытое состояние: первый кадр — ноль.
        assert_eq!(step(&ctx, &mut toast, &palette, 1), Some(0.0));
        let mut previous = 0.0;
        let mut settled = None;
        for frame in 2..=20 {
            let shown = step(&ctx, &mut toast, &palette, frame).expect("запись жива");
            assert!(
                previous <= shown && shown <= 1.0,
                "вход идёт назад на кадре {frame}: {shown} после {previous}"
            );
            previous = shown;
            if shown >= 1.0 && settled.is_none() {
                settled = Some(frame);
            }
        }
        let entered_at = settled.expect("вход не завершился за 20 кадров");
        assert!(
            (11..=13).contains(&entered_at),
            "вход должен занять TOAST_IN, вышло {entered_at} кадров"
        );
        assert_eq!(
            row_offset(toast.entries[0].shown, 0.0, TEST_OFFSET),
            egui::vec2(0.0, -TEST_OFFSET),
            "в покое смещения нет"
        );
    }

    #[test]
    fn exit_takes_toast_out_and_frees_the_slot() {
        let ctx = egui::Context::default();
        let palette = palette();
        let mut toast = Toast::default();
        publish(&ctx, &mut toast, 0.0, "эфир потерян");
        for frame in 1..=12 {
            step(&ctx, &mut toast, &palette, frame);
        }
        assert_eq!(toast.entries[0].shown, 1.0, "запись должна успеть войти");

        // Пауза в покое ничего не ломает: прогресс остаётся конечным.
        assert_eq!(step(&ctx, &mut toast, &palette, 179), Some(1.0));

        // Срок — кадр 180; дальше только убыывание до нуля и снятие со стека.
        let mut previous = 1.0;
        let mut gone_at = None;
        for frame in 180..=200 {
            let Some(shown) = step(&ctx, &mut toast, &palette, frame) else {
                gone_at = Some(frame - 179);
                break;
            };
            assert!(
                (0.0..=previous).contains(&shown),
                "уход должен убывать: {shown} после {previous}"
            );
            previous = shown;
        }
        assert!(toast.entries.is_empty(), "запись не освободила слот");
        let gone_at = gone_at.expect("уход не завершился за 20 кадров");
        assert!(
            (11..=14).contains(&gone_at),
            "уход должен занять TOAST_OUT, вышло {gone_at} кадров"
        );
    }

    #[test]
    fn stack_offsets_climb_upwards_from_the_anchor() {
        let row = 30.0;
        assert_eq!(
            row_offset(1.0, 0.0, TEST_OFFSET),
            egui::vec2(0.0, -TEST_OFFSET)
        );
        assert_eq!(
            row_offset(1.0, row + TOAST_GAP, TEST_OFFSET),
            egui::vec2(0.0, -(TEST_OFFSET + row + TOAST_GAP))
        );
        // Якорь задаёт вызывающий: плотность окна меняет отступ, а не стек.
        assert_eq!(
            row_offset(1.0, 0.0, TEST_OFFSET + 40.0).y - row_offset(1.0, 0.0, TEST_OFFSET).y,
            -40.0
        );
        // Закрытая запись лежит на RISE_Y ниже своей итоговой позиции.
        assert_eq!(
            row_offset(0.0, 0.0, TEST_OFFSET).y - row_offset(1.0, 0.0, TEST_OFFSET).y,
            animation::RISE_Y
        );
    }
}
