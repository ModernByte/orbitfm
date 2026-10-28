//! Состояние окна: режим (`WindowMode`), геометрия и намерения UI (ADR-0018, ADR-0021).
//!
//! Модуль ничего не знает про egui: наружу он отдаёт [`UiIntent`], а решение
//! закрыть или открыть root-вьюпорт принимает presentation-слой. Геометрия
//! привязана к режиму, потому что размер окна можно задать только до его
//! создания (ADR-0018 п. 5), и поэтому смена режима — это перезапуск UI-цикла.

/// Размер окна по умолчанию: применяется при показе после скрытия (ADR-0018).
pub(crate) const WINDOW_SIZE: [f32; 2] = [1100.0, 800.0];
/// Нижний лимит размера окна в обычном (не скрытом) состоянии.
pub(crate) const WINDOW_MIN_SIZE: [f32; 2] = [720.0, 480.0];
/// Капсула Overlay: узкое окно с mini-глобусом и избранным (ADR-0021).
/// Ширина 500 px: правый блок контролов верхнего ряда занимает ≈350 px,
/// аватар ≈38 px, и при прежних 420 px на имя/подпись оставалось 27 px —
/// текст заливал визуализатор и кнопки (ADR-0021, ред. 2026-10-28).
pub(crate) const OVERLAY_SIZE: [f32; 2] = [500.0, 620.0];
pub(crate) const OVERLAY_MIN_SIZE: [f32; 2] = [360.0, 480.0];
/// Окно Listening: одна строка now-playing с пульсом (ADR-0021).
pub(crate) const LISTENING_SIZE: [f32; 2] = [420.0, 152.0];
pub(crate) const LISTENING_MIN_SIZE: [f32; 2] = [360.0, 120.0];
/// Выпадающая панель из бара Noctalia: единая поверхность без полей окна
/// (ADR-0033). Ширина совпадает с типичной колонкой бара, высота — с запасом
/// на список станций.
pub(crate) const PANEL_SIZE: [f32; 2] = [400.0, 640.0];
pub(crate) const PANEL_MIN_SIZE: [f32; 2] = [360.0, 480.0];
/// Заголовок окна и запасной заголовок трека в MPRIS.
pub(crate) const WINDOW_TITLE: &str = "OrbitFM";
/// Заголовки капсул: по ним WM-правила подбирают размер режима (ADR-0021 п. 4).
pub(crate) const OVERLAY_TITLE: &str = "OrbitFM · Overlay";
pub(crate) const LISTENING_TITLE: &str = "OrbitFM · Listening";
pub(crate) const PANEL_TITLE: &str = "OrbitFM · Panel";

/// Режим отображения окна (ADR-0021). Focus — полный Orbital Desk.
///
/// `Panel` — выпадающая панель бар-виджета, команда `panel` (ADR-0033): тот же
/// процесс и сессия, но без декораций и только с зоной станций. Капсула
/// `Overlay` остаётся CLI-поверхностью и вызывается командой `overlay`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum WindowMode {
    #[default]
    Focus,
    Overlay,
    Listening,
    Panel,
}

impl WindowMode {
    /// Режимы, доступные из настроек: `Panel` открывается только командой `panel`
    /// (клик по бар-виджету), потому что это поверхность, а не раскладка стола;
    /// капсула `Overlay` остаётся CLI-переключателем `overlay` (ADR-0033).
    pub(crate) const ALL: [Self; 3] = [Self::Focus, Self::Overlay, Self::Listening];

    /// Размер для `ViewportBuilder` нового окна.
    pub(crate) fn default_size(self) -> [f32; 2] {
        match self {
            Self::Focus => WINDOW_SIZE,
            Self::Overlay => OVERLAY_SIZE,
            Self::Listening => LISTENING_SIZE,
            Self::Panel => PANEL_SIZE,
        }
    }

    pub(crate) fn min_size(self) -> [f32; 2] {
        match self {
            Self::Focus => WINDOW_MIN_SIZE,
            Self::Overlay => OVERLAY_MIN_SIZE,
            Self::Listening => LISTENING_MIN_SIZE,
            Self::Panel => PANEL_MIN_SIZE,
        }
    }

    pub(crate) fn title(self) -> &'static str {
        match self {
            Self::Focus => WINDOW_TITLE,
            Self::Overlay => OVERLAY_TITLE,
            Self::Listening => LISTENING_TITLE,
            Self::Panel => PANEL_TITLE,
        }
    }

    /// Имя режима в IPC-протоколе и в ответе `status`.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Focus => "focus",
            Self::Overlay => "overlay",
            Self::Listening => "listening",
            Self::Panel => "panel",
        }
    }

    /// Капсулы и панель не имеют ни заголовка, ни рамки: CSD рисует полосу
    /// заголовка поверх стеклянной поверхности. Рамка остаётся только у Focus —
    /// единственного режима, который живёт как обычное окно (ADR-0021, ADR-0033).
    pub(crate) fn is_borderless(self) -> bool {
        self != Self::Focus
    }
}

/// Действие с окном, которое Core просит выполнить presentation-слой.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UiIntent {
    /// Окно уже открыто: достаточно переместить фокус (на Wayland `Focus` — no-op).
    Show,
    /// Спрятать окно без завершения прогона UI. На macOS это `Visible(false)`,
    /// на Linux/Windows — закрытие root-вьюпорта, как и раньше (ADR-0026).
    Hide,
    /// Закрыть root-вьюпорт. Скрытие и завершение различает `WindowState::quitting`.
    Close,
}

/// Состояние окна (ADR-0018).
#[derive(Debug, Default)]
pub(crate) struct WindowState {
    /// Окно видимо пользователю. На macOS после `Visible(false)` остаётся
    /// `false` при живом root-вьюпорте (ADR-0026).
    pub(super) visible: bool,
    /// Root-вьюпорт существует. На Linux/Windows совпадает с `visible`,
    /// на macOS скрытое окно продолжает жить в run loop.
    viewport: bool,
    /// Последний известный размер: применяется до создания нового окна.
    /// Относится только к Focus — капсулы живут пресетами режима (ADR-0021 п. 4).
    pub(super) size: Option<[f32; 2]>,
    pub(super) mode: WindowMode,
    /// Режим меняется при живом вьюпорте: вьюпорт закрывается и тут же открывается
    /// заново с геометрией нового режима (ADR-0021 п. 3).
    reopen: bool,
    intent: Option<UiIntent>,
    quitting: bool,
}

impl WindowState {
    /// Состояние на старте процесса: первый прогон создаёт окно сразу (ADR-0018 п. 5).
    pub(super) fn shown() -> Self {
        Self {
            visible: true,
            ..Self::default()
        }
    }

    pub(super) fn intent(&self) -> Option<UiIntent> {
        self.intent
    }

    pub(super) fn take_intent(&mut self) -> Option<UiIntent> {
        self.intent.take()
    }

    /// Признак «пересоздать окно» забирается один раз — читает его цикл `run_session`.
    pub(super) fn take_reopen(&mut self) -> bool {
        std::mem::take(&mut self.reopen)
    }

    /// Идёт ли сейчас пересоздание окна из-за смены режима (ADR-0026).
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub(super) fn is_reopening(&self) -> bool {
        self.reopen
    }

    pub(super) fn hide(&mut self) {
        if self.visible && self.intent != Some(UiIntent::Hide) {
            self.intent = Some(UiIntent::Hide);
        }
    }

    pub(super) fn show(&mut self) {
        self.intent = Some(UiIntent::Show);
    }

    /// Единая точка для всех источников: открыто — закрыть, закрыто — показать.
    pub(super) fn toggle(&mut self) {
        if self.visible {
            self.hide();
        } else {
            self.show();
        }
    }

    /// Переключатель-выпадающий (ADR-0033): открытая поверхность того же режима
    /// скрывается, иначе режим переключается на `mode` и показывается. Зовётся
    /// из бар-виджета (`panel`) и из CLI (`overlay`).
    pub(super) fn toggle_dropdown(&mut self, mode: WindowMode) {
        if self.mode == mode && self.visible {
            self.hide();
        } else {
            self.set_mode(mode);
        }
    }

    /// Смена режима. Тот же режим — просто показ; новый режим при живом
    /// вьюпорте требует перезапуска UI-цикла, потому что размер задаётся до
    /// создания окна. На macOS скрытое окно тоже пересоздаётся: иначе оно
    /// показалось бы в геометрии старого режима (ADR-0026).
    pub(super) fn set_mode(&mut self, mode: WindowMode) {
        if self.mode == mode {
            self.show();
            return;
        }
        self.mode = mode;
        if self.viewport {
            self.reopen = true;
            self.intent = Some(UiIntent::Close);
        } else {
            self.show();
        }
    }

    pub(super) fn quit(&mut self) {
        self.quitting = true;
        self.intent = Some(UiIntent::Close);
    }

    pub(super) fn is_quitting(&self) -> bool {
        self.quitting
    }

    /// Окно спрятано без закрытия root-вьюпорта (macOS, ADR-0026).
    /// `WakeHandle` не трогаем: именно он будит скрытое окно.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub(super) fn mark_hidden(&mut self) {
        self.visible = false;
    }

    /// Окно снова видимо — без пересоздания UI-контекста.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub(super) fn mark_shown(&mut self) {
        self.visible = true;
    }

    /// Начало прогона UI: root-вьюпорт создан, намерений больше нет.
    /// Признак перезапуска снимается — геометрия нового режима уже применена.
    pub(super) fn begin(&mut self, size: [f32; 2]) {
        self.visible = true;
        self.viewport = true;
        self.intent = None;
        self.reopen = false;
        self.remember_size(size);
    }

    /// Конец прогона: окно закрыто, процесс жив.
    pub(super) fn end(&mut self) {
        self.visible = false;
        self.viewport = false;
        self.intent = None;
    }

    /// Размер для `ViewportBuilder` нового окна: сохранённый только для `Focus`,
    /// иначе пресет режима.
    pub(super) fn restore_size(&self) -> [f32; 2] {
        match self.mode {
            WindowMode::Focus => self.size.unwrap_or(WINDOW_SIZE),
            mode => mode.default_size(),
        }
    }

    /// Пользовательский размер относится только к `Focus`: иначе капсула
    /// запомнила бы свою пресетную ширину и вернула её в `Focus`.
    pub(super) fn remember_size(&mut self, size: [f32; 2]) {
        if self.mode == WindowMode::Focus {
            self.size = Some(size);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggle_alternates_between_hide_and_show() {
        let mut window = WindowState {
            visible: true,
            viewport: true,
            ..WindowState::default()
        };
        window.toggle();
        assert_eq!(window.take_intent(), Some(UiIntent::Hide));

        window.visible = false;
        window.toggle();
        assert_eq!(window.take_intent(), Some(UiIntent::Show));
    }

    #[test]
    fn hide_of_a_closed_window_requests_nothing() {
        let mut window = WindowState::default();
        window.hide();
        assert_eq!(window.intent(), None, "окно уже закрыто, закрывать нечего");
    }

    /// Панель и трей могут попросить скрытие дважды до кадра: намерение одно.
    #[test]
    fn repeated_hide_before_the_frame_keeps_a_single_intent() {
        let mut window = WindowState {
            visible: true,
            viewport: true,
            ..WindowState::default()
        };
        window.hide();
        window.hide();
        assert_eq!(window.take_intent(), Some(UiIntent::Hide));
        assert_eq!(window.intent(), None, "очередь намерений пуста");
    }

    #[test]
    fn mark_hidden_and_shown_track_visibility_without_a_viewport_restart() {
        let mut window = WindowState::shown();
        window.begin(WINDOW_SIZE);
        window.mark_hidden();
        assert!(!window.visible, "status и трей видят скрытое окно сразу");
        assert!(!window.take_reopen());
        assert_eq!(window.intent(), None, "mark_hidden не рождает намерений");

        window.mark_shown();
        assert!(window.visible);
        assert_eq!(window.intent(), None);
    }

    #[test]
    fn mode_switch_of_a_closed_window_only_shows_it() {
        let mut window = WindowState::default();
        window.set_mode(WindowMode::Overlay);
        assert_eq!(window.mode, WindowMode::Overlay);
        assert_eq!(window.take_intent(), Some(UiIntent::Show));
        assert!(
            !window.take_reopen(),
            "без окна пересоздавать нечего: следующий показ уже в новом режиме"
        );
    }

    #[test]
    fn mode_switch_of_an_open_window_reopens_it() {
        let mut window = WindowState {
            visible: true,
            viewport: true,
            ..WindowState::default()
        };
        window.set_mode(WindowMode::Listening);
        assert_eq!(
            window.take_intent(),
            Some(UiIntent::Close),
            "размер задаётся до создания окна, поэтому режим ждёт нового прогона"
        );
        assert!(window.take_reopen());
        assert!(!window.take_reopen(), "признак забирается один раз");
    }

    /// На macOS скрытое окно остаётся живым: смена режима обязана пересоздать
    /// вьюпорт, иначе показ вернёт геометрию старого режима (ADR-0026).
    #[test]
    fn mode_switch_of_a_hidden_macos_window_reopens_it() {
        let mut window = WindowState::shown();
        window.begin(WINDOW_SIZE);
        window.mark_hidden();
        assert_eq!(window.take_intent(), None);

        window.set_mode(WindowMode::Overlay);
        assert_eq!(window.take_intent(), Some(UiIntent::Close));
        assert!(window.is_reopening(), "цикл обязан пересоздать вьюпорт");
        assert!(window.take_reopen());
    }

    #[test]
    fn same_mode_requests_show_without_reopen() {
        let mut window = WindowState::default();
        window.set_mode(WindowMode::Focus);
        assert_eq!(window.take_intent(), Some(UiIntent::Show));
        assert!(!window.take_reopen());
    }

    #[test]
    fn panel_dropdown_toggles_open_and_closed() {
        let mut window = WindowState {
            visible: true,
            viewport: true,
            mode: WindowMode::Panel,
            ..WindowState::default()
        };
        window.toggle_dropdown(WindowMode::Panel);
        assert_eq!(window.take_intent(), Some(UiIntent::Hide));

        window.visible = false;
        window.viewport = false;
        window.toggle_dropdown(WindowMode::Panel);
        assert_eq!(window.take_intent(), Some(UiIntent::Show));
        assert!(!window.take_reopen(), "панель уже в режиме Panel");

        // Из другого режима первый клик переключает и показывает панель.
        window.mode = WindowMode::Focus;
        window.visible = true;
        window.viewport = true;
        window.toggle_dropdown(WindowMode::Panel);
        assert_eq!(window.mode, WindowMode::Panel);
        assert_eq!(window.take_intent(), Some(UiIntent::Close));
        assert!(window.take_reopen());
    }

    #[test]
    fn overlay_dropdown_toggles_the_capsule() {
        // Команда `overlay` приходит, когда открыт другой режим: капсула появляется.
        let mut window = WindowState {
            visible: true,
            viewport: true,
            mode: WindowMode::Focus,
            ..WindowState::default()
        };
        window.toggle_dropdown(WindowMode::Overlay);
        assert_eq!(window.mode, WindowMode::Overlay);
        assert!(window.take_reopen());
        assert_eq!(window.take_intent(), Some(UiIntent::Close));

        // Повторный клик по открытому Overlay скрывает его.
        window.visible = true;
        window.toggle_dropdown(WindowMode::Overlay);
        assert_eq!(window.mode, WindowMode::Overlay);
        assert_eq!(window.take_intent(), Some(UiIntent::Hide));
        assert!(
            !window.take_reopen(),
            "режим тот же — пересоздания не нужно"
        );

        // Клик по скрытому Overlay показывает его, не трогая режим.
        window.visible = false;
        window.toggle_dropdown(WindowMode::Overlay);
        assert_eq!(window.take_intent(), Some(UiIntent::Show));
    }

    #[test]
    fn each_mode_carries_its_geometry_and_title() {
        assert_eq!(WindowMode::Focus.min_size(), WINDOW_MIN_SIZE);
        assert_eq!(WindowMode::Overlay.default_size(), OVERLAY_SIZE);
        assert_eq!(WindowMode::Listening.min_size(), LISTENING_MIN_SIZE);
        assert_eq!(WindowMode::Panel.default_size(), PANEL_SIZE);
        assert_eq!(WindowMode::Panel.min_size(), PANEL_MIN_SIZE);
        assert_eq!(WindowMode::Overlay.title(), OVERLAY_TITLE);
        assert_eq!(WindowMode::Focus.title(), WINDOW_TITLE);
        assert_eq!(WindowMode::Listening.as_str(), "listening");
        assert_eq!(WindowMode::Panel.title(), PANEL_TITLE);
        assert_eq!(WindowMode::Panel.as_str(), "panel");
    }

    #[test]
    fn only_focus_keeps_decorations() {
        assert!(
            !WindowMode::Focus.is_borderless(),
            "Focus живёт как обычное окно"
        );
        for mode in [
            WindowMode::Overlay,
            WindowMode::Listening,
            WindowMode::Panel,
        ] {
            assert!(mode.is_borderless(), "{mode:?} обязан быть без рамки");
        }
        assert!(
            !WindowMode::ALL.contains(&WindowMode::Panel),
            "Panel открывается бар-виджетом, а не переключателем раскладки"
        );
    }

    #[test]
    fn modes_are_listed_from_full_to_minimal() {
        let codes: Vec<&str> = WindowMode::ALL.iter().map(|mode| mode.as_str()).collect();
        assert_eq!(codes, ["focus", "overlay", "listening"]);
    }

    #[test]
    fn user_size_is_remembered_only_while_focused() {
        let mut window = WindowState {
            visible: true,
            ..WindowState::default()
        };
        window.begin([1000.0, 700.0]);
        assert_eq!(window.restore_size(), [1000.0, 700.0]);

        window.set_mode(WindowMode::Overlay);
        window.begin(OVERLAY_SIZE);
        assert_eq!(
            window.restore_size(),
            OVERLAY_SIZE,
            "капсула живёт пресетом режима"
        );

        window.set_mode(WindowMode::Focus);
        assert_eq!(
            window.restore_size(),
            [1000.0, 700.0],
            "сохранённый размер Focus не заражается капсулой"
        );
    }
}
