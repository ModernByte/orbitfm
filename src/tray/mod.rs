//! Системный трей по StatusNotifierItem (ADR-0020).
//!
//! Модуль не знает ни про egui, ни про станции: обмен идёт двумя
//! доменно-независимыми типами — [`TraySnapshot`] (что показать) и
//! [`TrayCommand`] (что сделать). Команды впадают в тот же dispatch Application
//! Core, что IPC и MPRIS, поэтому трей — третий источник команд, а не новый
//! протокол (ADR-0005 остаётся как есть).
//!
//! На платформах без SNI и в сборке без фичи `tray` [`spawn`] возвращает `None`:
//! вызывающий код не обрастает `cfg`-ветками, поведение не меняется.

#[cfg(all(target_os = "linux", feature = "tray"))]
mod icon;
#[cfg(all(target_os = "linux", feature = "tray"))]
mod sni;

use tokio::sync::mpsc;

use crate::ipc::WakeFn;
use crate::settings::ThemeMode;

/// Шаг громкости: и колесо по иконке, и пункты меню двигают громкость на него.
pub(crate) const VOLUME_STEP: f64 = 0.1;

/// Запрос пользователя из трея. `Hide` нет: единственный способ спрятать окно из
/// трея — тот же toggle, что у Noctalia-виджета и IPC.
///
/// Конструирует их только SNI-путь, поэтому в сборке без фичи `tray` варианты
/// формально никем не создаются; `Session` исполняет их без `cfg`-веток.
#[cfg_attr(not(all(target_os = "linux", feature = "tray")), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TrayCommand {
    Toggle,
    Show,
    PlayPause,
    Stop,
    Previous,
    Next,
    Roulette,
    VolumeUp,
    VolumeDown,
    SetTheme(ThemeMode),
    Quit,
}

/// Что трей показывает прямо сейчас.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct TraySnapshot {
    /// Окно открыто: определяет пункт «Show/Hide window» и `Status`.
    pub(crate) window_visible: bool,
    /// Станция в эфире (Playing/Buffering/Reconnecting).
    pub(crate) playing: bool,
    /// Подпись активной станции в тултипе; `None` — «Nothing playing».
    pub(crate) station: Option<String>,
    /// Текущая тема: активный пункт группы System/Dark/Light.
    pub(crate) theme: ThemeMode,
}

/// Живой трей: куда публиковать снимки и откуда читать команды.
///
/// Владелец — `Session`. Когда он отдаёт handle, закрытие канала снимков
/// останавливает поток-владелец сервиса, а тот гасит SNI-item.
pub(crate) struct TrayHandle {
    snapshots: Option<std::sync::mpsc::Sender<TraySnapshot>>,
    pub(crate) commands: mpsc::UnboundedReceiver<TrayCommand>,
}

impl TrayHandle {
    /// Публикует снимок. `false` означает, что трей умер: больше к нему не идём.
    pub(crate) fn publish(&self, snapshot: &TraySnapshot) -> bool {
        self.snapshots
            .as_ref()
            .is_some_and(|sender| sender.send(snapshot.clone()).is_ok())
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

/// Платформы без SNI: трея нет, остальное приложение работает как раньше.
#[cfg(not(all(target_os = "linux", feature = "tray")))]
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
        assert_eq!(snapshot.theme, ThemeMode::default());
    }

    #[test]
    fn snapshots_compare_by_content() {
        let mut hidden = TraySnapshot {
            window_visible: false,
            playing: true,
            station: Some("WFMU".to_owned()),
            theme: ThemeMode::Dark,
        };
        let same = hidden.clone();
        assert_eq!(hidden, same);
        hidden.window_visible = true;
        assert_ne!(hidden, same);
    }
}
