//! Состояние интеграций для окна настроек (ADR-0042 §1).
//!
//! Страница получает снимок состояний, а не ходит по файлам на каждый кадр.
//! Установка, удаление, правка конфига и перезагрузка идут в
//! `Session::runtime`; результат возвращается пробуждением GUI и обновляет
//! снимок. Файловые операции остаются в установщике ADR-0037.

use std::sync::mpsc::{self, Receiver, TryRecvError};

use tokio::runtime::Handle;

use super::reload::{self, ReloadOutcome};
use super::waybar::{self, WaybarStatus};
use super::{InstalledState, Layout, Target, current_binary, inspect, install_one, uninstall_one};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Availability {
    Detected,
    NotDetected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InstallState {
    NotInstalled,
    Installed,
    UpdateAvailable,
    Problem,
}

#[derive(Debug, Clone)]
pub(crate) struct TargetStatus {
    pub target: Target,
    pub availability: Availability,
    pub state: InstallState,
    /// Техническая подробность для hover/тоста; в подписи не показывается.
    pub detail: Option<String>,
    /// Заполнено только для Waybar: путь, включённость и ошибка разбора.
    pub waybar: Option<WaybarStatus>,
    /// Найденные браузеры цели (Chrome/Chromium или Firefox).
    pub browsers: Vec<&'static str>,
}

/// Снимок без копирования списка на каждый кадр.
#[derive(Debug, Clone, Copy)]
pub(crate) struct IntegrationsView<'a> {
    pub statuses: &'a [TargetStatus],
    pub busy: Option<Target>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PanelAction {
    Install(Target),
    Uninstall(Target),
    EnableWaybar,
    Reload(Target),
}

impl PanelAction {
    pub(crate) fn target(self) -> Target {
        match self {
            Self::Install(target) | Self::Uninstall(target) | Self::Reload(target) => target,
            Self::EnableWaybar => Target::Waybar,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PanelOutcome {
    Completed,
    ManualReload,
}

#[derive(Debug)]
pub(crate) enum PanelEvent {
    Done {
        action: PanelAction,
        outcome: PanelOutcome,
    },
    Failed {
        action: PanelAction,
        detail: String,
    },
}

#[derive(Default)]
pub(crate) struct IntegrationPanel {
    statuses: Vec<TargetStatus>,
    loaded: bool,
    busy: Option<PanelAction>,
    receiver: Option<Receiver<PanelEvent>>,
}

impl IntegrationPanel {
    pub(crate) fn ensure_loaded(&mut self) {
        if !self.loaded {
            self.refresh();
        }
    }

    pub(crate) fn refresh(&mut self) {
        self.statuses = collect();
        self.loaded = true;
    }

    pub(crate) fn view(&self) -> IntegrationsView<'_> {
        IntegrationsView {
            statuses: &self.statuses,
            busy: self.busy.map(PanelAction::target),
        }
    }

    /// Запускает операцию в фоне. Пока одна операция жива, вторая не стартует:
    /// квитанции и конфиг не должны меняться параллельно.
    pub(crate) fn start(
        &mut self,
        action: PanelAction,
        runtime: &Handle,
        wake: crate::ipc::WakeFn,
    ) -> bool {
        if self.busy.is_some() {
            return false;
        }
        self.ensure_loaded();
        let (sender, receiver) = mpsc::channel();
        self.busy = Some(action);
        self.receiver = Some(receiver);
        runtime.spawn_blocking(move || {
            let event = match perform(action) {
                Ok(outcome) => PanelEvent::Done { action, outcome },
                Err(detail) => PanelEvent::Failed { action, detail },
            };
            let _ = sender.send(event);
            wake();
        });
        true
    }

    /// Забирает готовый результат; `None` — задача ещё идёт.
    pub(crate) fn poll(&mut self) -> Option<PanelEvent> {
        let receiver = self.receiver.as_ref()?;
        match receiver.try_recv() {
            Ok(event) => {
                self.receiver = None;
                self.busy = None;
                self.refresh();
                Some(event)
            }
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                self.receiver = None;
                let action = self.busy.take();
                self.refresh();
                action.map(|action| PanelEvent::Failed {
                    action,
                    detail: "фоновая задача завершилась без результата".to_owned(),
                })
            }
        }
    }
}

pub(crate) fn collect() -> Vec<TargetStatus> {
    let layout = match Layout::from_env() {
        Ok(layout) => layout,
        Err(err) => {
            let detail = format!("{err:#}");
            return Target::ALL
                .into_iter()
                .map(|target| TargetStatus {
                    target,
                    availability: Availability::NotDetected,
                    state: InstallState::Problem,
                    detail: Some(detail.clone()),
                    waybar: None,
                    browsers: Vec::new(),
                })
                .collect();
        }
    };
    Target::ALL
        .into_iter()
        .map(|target| status_for(&layout, target))
        .collect()
}

fn status_for(layout: &Layout, target: Target) -> TargetStatus {
    let availability = if layout.target_detected(target) {
        Availability::Detected
    } else {
        Availability::NotDetected
    };
    let waybar = (target == Target::Waybar).then(|| waybar::status(layout));
    let browsers = layout.detected_browsers(target);

    let (state, detail) = match inspect(layout, target) {
        Ok(inspection) => match inspection.state {
            InstalledState::NotInstalled => (InstallState::NotInstalled, None),
            InstalledState::Installed => {
                if inspection.version == env!("CARGO_PKG_VERSION") {
                    (InstallState::Installed, None)
                } else {
                    (
                        InstallState::UpdateAvailable,
                        Some(format!(
                            "{} → {}",
                            inspection.version,
                            env!("CARGO_PKG_VERSION")
                        )),
                    )
                }
            }
            InstalledState::Changed => (
                InstallState::Problem,
                Some("файлы изменены пользователем".to_owned()),
            ),
            InstalledState::Missing => (
                InstallState::Problem,
                Some("часть файлов установки отсутствует".to_owned()),
            ),
        },
        Err(err) => (InstallState::Problem, Some(format!("{err:#}"))),
    };

    TargetStatus {
        target,
        availability,
        state,
        detail,
        waybar,
        browsers,
    }
}

fn perform(action: PanelAction) -> Result<PanelOutcome, String> {
    let layout = Layout::from_env().map_err(|err| format!("{err:#}"))?;
    match action {
        PanelAction::Install(target) => {
            let binary = current_binary().map_err(|err| format!("{err:#}"))?;
            install_one(&layout, target, false, &binary).map_err(|err| format!("{err:#}"))?;
            Ok(PanelOutcome::Completed)
        }
        PanelAction::Uninstall(target) => {
            uninstall_one(&layout, target, false).map_err(|err| format!("{err:#}"))?;
            Ok(PanelOutcome::Completed)
        }
        PanelAction::EnableWaybar => {
            let report = waybar::enable(&layout).map_err(|err| format!("{err:#}"))?;
            tracing::info!(
                config = %report.path.display(),
                backup = report.backup.as_ref().map(|path| path.display().to_string()),
                already_enabled = report.already_enabled,
                "модуль OrbitFM включён в конфиг Waybar"
            );
            Ok(PanelOutcome::Completed)
        }
        PanelAction::Reload(target) => Ok(match reload::reload(target) {
            ReloadOutcome::Manual => PanelOutcome::ManualReload,
            ReloadOutcome::Reloaded | ReloadOutcome::Signaled(_) => PanelOutcome::Completed,
        }),
    }
}
