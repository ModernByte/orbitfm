//! Состояние окна: режим (`WindowMode`), геометрия и намерения UI (ADR-0018, ADR-0021).
//!
//! Модуль ничего не знает про egui: наружу он отдаёт [`UiIntent`], а решение
//! закрыть или открыть root-вьюпорт принимает presentation-слой. Геометрия
//! привязана к режиму, потому что размер окна можно задать только до его
//! создания (ADR-0018 п. 5), и поэтому смена режима — это перезапуск UI-цикла.

/// Размер окна по умолчанию: применяется при показе после скрытия (ADR-0018).
pub(crate) const WINDOW_SIZE: [f32; 2] = [920.0, 660.0];
/// Нижний лимит размера окна в обычном (не скрытом) состоянии.
pub(crate) const WINDOW_MIN_SIZE: [f32; 2] = [720.0, 480.0];
/// Капсула Overlay: узкое окно с mini-глобусом и избранным (ADR-0021).
pub(crate) const OVERLAY_SIZE: [f32; 2] = [420.0, 620.0];
pub(crate) const OVERLAY_MIN_SIZE: [f32; 2] = [360.0, 480.0];
/// Окно Listening: одна строка now-playing с пульсом (ADR-0021).
pub(crate) const LISTENING_SIZE: [f32; 2] = [420.0, 152.0];
pub(crate) const LISTENING_MIN_SIZE: [f32; 2] = [360.0, 120.0];
/// Заголовок окна и запасной заголовок трека в MPRIS.
pub(crate) const WINDOW_TITLE: &str = "OrbitFM";
/// Заголовки капсул: по ним WM-правила подбирают размер режима (ADR-0021 п. 4).
pub(crate) const OVERLAY_TITLE: &str = "OrbitFM · Overlay";
pub(crate) const LISTENING_TITLE: &str = "OrbitFM · Listening";

/// Режим отображения окна (ADR-0021). Focus — полный Orbital Desk.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum WindowMode {
    #[default]
    Focus,
    Overlay,
    Listening,
}

impl WindowMode {
    /// Все режимы в порядке возрастания плотности — для переключателя в UI.
    pub(crate) const ALL: [Self; 3] = [Self::Focus, Self::Overlay, Self::Listening];

    /// Размер для `ViewportBuilder` нового окна.
    pub(crate) fn default_size(self) -> [f32; 2] {
        match self {
            Self::Focus => WINDOW_SIZE,
            Self::Overlay => OVERLAY_SIZE,
            Self::Listening => LISTENING_SIZE,
        }
    }

    pub(crate) fn min_size(self) -> [f32; 2] {
        match self {
            Self::Focus => WINDOW_MIN_SIZE,
            Self::Overlay => OVERLAY_MIN_SIZE,
            Self::Listening => LISTENING_MIN_SIZE,
        }
    }

    pub(crate) fn title(self) -> &'static str {
        match self {
            Self::Focus => WINDOW_TITLE,
            Self::Overlay => OVERLAY_TITLE,
            Self::Listening => LISTENING_TITLE,
        }
    }

    /// Имя режима в IPC-протоколе и в ответе `status`.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Focus => "focus",
            Self::Overlay => "overlay",
            Self::Listening => "listening",
        }
    }
}

/// Действие с окном, которое Core просит выполнить presentation-слой.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UiIntent {
    /// Окно уже открыто: достаточно переместить фокус (на Wayland `Focus` — no-op).
    Show,
    /// Закрыть root-вьюпорт. Скрытие и завершение различает `WindowState::quitting`.
    Close,
}

/// Состояние окна (ADR-0018).
#[derive(Debug, Default)]
pub(crate) struct WindowState {
    /// Окно открыто: root-вьюпорт существует.
    pub(super) visible: bool,
    /// Последний известный размер: применяется до создания нового окна.
    /// Относится только к Focus — капсулы живут пресетами режима (ADR-0021 п. 4).
    pub(super) size: Option<[f32; 2]>,
    pub(super) mode: WindowMode,
    /// Режим меняется при открытом окне: вьюпорт закрывается и тут же открывается
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

    pub(super) fn hide(&mut self) {
        if self.visible {
            self.intent = Some(UiIntent::Close);
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

    /// Смена режима. Тот же режим — просто показ; новый режим при открытом окне
    /// требует перезапуска UI-цикла, потому что размер задаётся до создания окна.
    pub(super) fn set_mode(&mut self, mode: WindowMode) {
        if self.mode == mode {
            self.show();
            return;
        }
        self.mode = mode;
        if self.visible {
            self.reopen = true;
            self.hide();
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

    /// Начало прогона UI: root-вьюпорт создан, намерений больше нет.
    /// Признак перезапуска снимается — геометрия нового режима уже применена.
    pub(super) fn begin(&mut self, size: [f32; 2]) {
        self.visible = true;
        self.intent = None;
        self.reopen = false;
        self.remember_size(size);
    }

    /// Конец прогона: окно закрыто, процесс жив.
    pub(super) fn end(&mut self) {
        self.visible = false;
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
    /// запомнила бы 420 px и вернула их в `Focus`.
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
    fn toggle_alternates_between_close_and_show() {
        let mut window = WindowState {
            visible: true,
            ..WindowState::default()
        };
        window.toggle();
        assert_eq!(window.take_intent(), Some(UiIntent::Close));

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

    #[test]
    fn same_mode_requests_show_without_reopen() {
        let mut window = WindowState::default();
        window.set_mode(WindowMode::Focus);
        assert_eq!(window.take_intent(), Some(UiIntent::Show));
        assert!(!window.take_reopen());
    }

    #[test]
    fn each_mode_carries_its_geometry_and_title() {
        assert_eq!(WindowMode::Focus.min_size(), WINDOW_MIN_SIZE);
        assert_eq!(WindowMode::Overlay.default_size(), OVERLAY_SIZE);
        assert_eq!(WindowMode::Listening.min_size(), LISTENING_MIN_SIZE);
        assert_eq!(WindowMode::Overlay.title(), OVERLAY_TITLE);
        assert_eq!(WindowMode::Focus.title(), WINDOW_TITLE);
        assert_eq!(WindowMode::Listening.as_str(), "listening");
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
