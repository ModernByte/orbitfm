//! Системный трей: StatusNotifierItem на Linux (ADR-0020) и строка меню на
//! macOS (ADR-0026).
//!
//! Модуль не знает ни про egui, ни про станции: обмен идёт двумя
//! доменно-независимыми типами — [`TraySnapshot`] (что показать) и
//! [`TrayCommand`] (что сделать). Команды впадают в тот же dispatch Application
//! Core, что IPC и MPRIS, поэтому трей — третий источник команд, а не новый
//! протокол (ADR-0005 остаётся как есть).
//!
//! На платформах без трея и в сборке без фичи `tray` [`spawn`] возвращает `None`:
//! вызывающий код не обрастает `cfg`-ветками, поведение не меняется.

#[cfg(any(
    all(target_os = "linux", feature = "tray"),
    all(target_os = "macos", feature = "tray")
))]
mod icon;
#[cfg(all(target_os = "macos", feature = "tray"))]
mod macos;
#[cfg(all(target_os = "linux", feature = "tray"))]
mod sni;
#[cfg(any(
    all(target_os = "linux", feature = "tray"),
    all(target_os = "macos", feature = "tray")
))]
mod text;

use tokio::sync::mpsc;

use crate::ipc::WakeFn;
use crate::settings::{Language, ThemeMode};

/// Шаг громкости: и колесо по иконке, и пункты меню двигают громкость на него.
pub(crate) const VOLUME_STEP: f64 = 0.1;

/// Запрос пользователя из трея. `Hide` нет: единственный способ спрятать окно из
/// трея — тот же toggle, что у Noctalia-виджета и IPC.
///
/// Конструируют их только платформенные бэкенды, поэтому в сборке без фичи
/// `tray` варианты формально никем не создаются; `Session` исполняет их без
/// `cfg`-веток.
#[cfg_attr(
    not(any(
        all(target_os = "linux", feature = "tray"),
        all(target_os = "macos", feature = "tray")
    )),
    allow(dead_code)
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TrayCommand {
    Toggle,
    /// Жест `secondary_activate` есть только у SNI; меню-бар macOS использует
    /// тот же `Toggle` (ADR-0026, п. 7), поэтому на macOS вариант не создаётся.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    Show,
    PlayPause,
    Stop,
    Previous,
    Next,
    Roulette,
    VolumeUp,
    VolumeDown,
    SetTheme(ThemeMode),
    /// Избранное текущей станции (ADR-0020, ревизия 0.16).
    ToggleFavorite,
    /// Домашняя страница станции во внешнем браузере; только `http(s)`.
    OpenHomepage,
    /// Открыть окно настроек (ADR-0031): окно показывается, если спрятано.
    Settings,
    Quit,
}

/// Карточка станции в меню: внешние данные Radio Browser, не переводятся
/// (ADR-0030, п. 4).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct TrayStation {
    pub(crate) name: String,
    pub(crate) country: String,
    /// Теги Radio Browser через запятую; пустая строка — тегов нет.
    pub(crate) tags: String,
    /// Битрейт в kbps; `0` — неизвестен.
    pub(crate) bitrate: u32,
    /// Домашняя страница станции; `None` — открывать нечего.
    pub(crate) homepage: Option<String>,
}

/// Что трей показывает прямо сейчас.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct TraySnapshot {
    /// Окно открыто: определяет пункт «Show/Hide window».
    pub(crate) window_visible: bool,
    /// Станция в эфире (Playing/Buffering/Reconnecting).
    pub(crate) playing: bool,
    /// Активная станция в карточке; `None` — «Nothing playing».
    pub(crate) station: Option<TrayStation>,
    /// Активная станция лежит в избранном: подпись пункта и `About station`.
    pub(crate) favorite: bool,
    /// Громкость в процентах: подпись группы громкости.
    pub(crate) volume_percent: u8,
    /// Текущая тема: активный пункт группы System/Dark/Light.
    pub(crate) theme: ThemeMode,
    /// Разрешённый язык интерфейса: смена языка перестраивает меню (ADR-0030).
    pub(crate) language: Language,
}

/// Живой трей: куда публиковать снимки и откуда читать команды.
///
/// Владелец — `Session`. На Linux, когда он отдаёт handle, закрытие канала
/// снимков останавливает поток-владелец сервиса, а тот гасит SNI-item. На macOS
/// handle держит элемент строки меню сам: он создан на главном потоке после
/// старта run loop (ADR-0026, п. 6) и живёт столько же, сколько `Session`.
pub(crate) struct TrayHandle {
    #[cfg(all(target_os = "linux", feature = "tray"))]
    snapshots: Option<std::sync::mpsc::Sender<TraySnapshot>>,
    #[cfg(all(target_os = "macos", feature = "tray"))]
    macos: Option<macos::Slot>,
    pub(crate) commands: mpsc::UnboundedReceiver<TrayCommand>,
}

impl TrayHandle {
    /// Публикует снимок. `false` означает, что трей умер: больше к нему не идём.
    #[cfg(all(target_os = "linux", feature = "tray"))]
    pub(crate) fn publish(&mut self, snapshot: &TraySnapshot) -> bool {
        self.snapshots
            .as_ref()
            .is_some_and(|sender| sender.send(snapshot.clone()).is_ok())
    }

    /// macOS обновляет меню на главном потоке: `Session` живёт там же, поэтому
    /// публикация — обычный вызов без канала (ADR-0026, п. 6–7).
    #[cfg(all(target_os = "macos", feature = "tray"))]
    pub(crate) fn publish(&mut self, snapshot: &TraySnapshot) -> bool {
        self.macos
            .as_mut()
            .is_some_and(|slot| slot.publish(snapshot))
    }

    /// Платформа без трея: публиковать некуда.
    #[cfg(not(any(
        all(target_os = "linux", feature = "tray"),
        all(target_os = "macos", feature = "tray")
    )))]
    pub(crate) fn publish(&mut self, _snapshot: &TraySnapshot) -> bool {
        false
    }

    /// Создаёт macOS-иконку на главном потоке, когда run loop уже запущен.
    /// На Linux такой стадии нет: SNI-поток поднимается в [`spawn`].
    #[cfg(all(target_os = "macos", feature = "tray"))]
    pub(crate) fn attach(&mut self) -> bool {
        self.macos.as_mut().is_some_and(macos::Slot::attach)
    }
}

/// Регистрирует трей и запускает его поток-владелец.
///
/// `wake` будит GUI-цикл: при открытом окне команды трея читает `tick`,
/// при закрытом их уже ждёт `select!` в `Session::park`.
#[cfg(all(target_os = "linux", feature = "tray"))]
pub(crate) fn spawn(app_id: &'static str, title: &'static str, wake: WakeFn) -> Option<TrayHandle> {
    let (snapshot_tx, snapshot_rx) = std::sync::mpsc::channel();
    let (command_tx, command_rx) = mpsc::unbounded_channel();
    if !sni::start(app_id, title, snapshot_rx, command_tx, wake) {
        return None;
    }
    Some(TrayHandle {
        snapshots: Some(snapshot_tx),
        commands: command_rx,
    })
}

/// macOS: иконка создаётся позже, в `OrbitApp::new` (требование `tray-icon`),
/// поэтому здесь только канал команд и неподключённый слот.
#[cfg(all(target_os = "macos", feature = "tray"))]
pub(crate) fn spawn(app_id: &'static str, title: &'static str, wake: WakeFn) -> Option<TrayHandle> {
    let (command_tx, command_rx) = mpsc::unbounded_channel();
    Some(TrayHandle {
        macos: Some(macos::Slot::new(app_id, title, command_tx, wake)),
        commands: command_rx,
    })
}

/// Платформы без трея: трея нет, остальное приложение работает как раньше.
#[cfg(not(any(
    all(target_os = "linux", feature = "tray"),
    all(target_os = "macos", feature = "tray")
)))]
pub(crate) fn spawn(
    _app_id: &'static str,
    _title: &'static str,
    _wake: WakeFn,
) -> Option<TrayHandle> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_defaults_to_hidden_and_idle() {
        let snapshot = TraySnapshot::default();
        assert!(!snapshot.window_visible);
        assert!(!snapshot.playing);
        assert_eq!(snapshot.station, None);
        assert!(!snapshot.favorite);
        assert_eq!(snapshot.volume_percent, 0);
        assert_eq!(snapshot.theme, ThemeMode::default());
        assert_eq!(snapshot.language, Language::default());
    }

    #[test]
    fn snapshots_compare_by_content() {
        let mut hidden = TraySnapshot {
            window_visible: false,
            playing: true,
            station: Some(TrayStation {
                name: "WFMU".to_owned(),
                country: "US".to_owned(),
                tags: "eclectic".to_owned(),
                bitrate: 128,
                homepage: Some("https://wfmu.org".to_owned()),
            }),
            favorite: true,
            volume_percent: 70,
            theme: ThemeMode::Dark,
            language: Language::French,
        };
        let same = hidden.clone();
        assert_eq!(hidden, same);
        hidden.window_visible = true;
        assert_ne!(hidden, same);
        hidden.window_visible = false;
        hidden.language = Language::German;
        assert_ne!(hidden, same, "смена языка меняет меню");
        hidden.language = Language::French;
        hidden.volume_percent = 60;
        assert_ne!(hidden, same, "громкость видна в подписи группы");
    }
}
