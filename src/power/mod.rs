//! Ингибитор сна и действие по истечении таймера (ADR-0054, ADR-0055).
//!
//! Пока играет эфир и тикает таймер, ОС не должна уходить в сон. Лок держит
//! либо дочерний `systemd-inhibit`/`caffeinate`, либо сам процесс через
//! `SetThreadExecutionState`. Модуль не знает ни про аудио, ни про настройки:
//! вызывающий передаёт готовую причину и время жизни, а получает RAII-страж,
//! который снимает лок в `Drop`.
//!
//! Действие по истечении таймера — одноразовая операция: argv фиксированы в
//! коде, shell не запускается, ошибка не ретраится (ADR-0055 §3–4).

use std::env;
use std::ffi::OsString;
#[cfg(unix)]
use std::fs;
#[cfg(unix)]
use std::path::PathBuf;
#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::process::Child;
#[cfg(unix)]
use std::process::{Command, Stdio};
#[cfg(test)]
use std::sync::Arc;
#[cfg(test)]
use std::sync::atomic::{AtomicBool, Ordering};

use tracing::{info, warn};

#[cfg(test)]
pub(crate) mod fake;
#[cfg(target_os = "linux")]
pub(crate) mod linux;
#[cfg(target_os = "macos")]
pub(crate) mod macos;
#[cfg(windows)]
pub(crate) mod windows;

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
compile_error!("power: поддерживаются только Linux, macOS и Windows (ADR-0054)");

/// Read-only-диагностика: вместо spawn пишется `info!` с argv (ADR-0055 §4).
pub(crate) const DRY_RUN_ENV: &str = "ORBITFM_POWER_DRY_RUN";

/// Способ, которым машина удерживает лок сна (ADR-0054).
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InhibitBackend {
    LogindInhibit,
    Caffeinate,
    SetThreadExecutionState,
    Unavailable,
}

/// Почему лок не удалось взять. Detail уходит в статус и в предупреждение.
#[derive(Debug, thiserror::Error)]
pub(crate) enum PowerError {
    /// Ингибитор недоступен на этой платформе. На Windows и macOS бэкенд
    /// всегда доступен, поэтому вариант мёртвый на них — оставим дляLinux и
    /// кросс-платформенной сигнатуры.
    #[allow(dead_code)]
    #[error("ингибитор сна недоступен: {0}")]
    Unavailable(String),
    #[error("не удалось запустить ингибитор: {0}")]
    Spawn(String),
}

/// Действие по истечении таймера сна (ADR-0055 §1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PowerAction {
    Nothing,
    Suspend,
    PowerOff,
}

impl PowerAction {
    /// Имя для лога и dry-run: совпадает со значениями `settings.json`.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Nothing => "nothing",
            Self::Suspend => "suspend",
            Self::PowerOff => "power-off",
        }
    }
}

/// Владелец лока сна.
///
/// Лок снимается в `Drop`; повторное освобождение ничего не делает. Ошибки
/// освобождения намеренно игнорируются: лок всё равно исчезает вместе с
/// процессом-носителем или самим OrbitFM (ADR-0054 §2).
pub(crate) struct InhibitionGuard {
    backend: InhibitBackend,
    released: bool,
    carrier: Carrier,
}

/// Чем именно удерживается лок: процессом-носителем или состоянием процесса.
enum Carrier {
    /// `systemd-inhibit` / `caffeinate`: лок живёт, пока жив дочерний процесс.
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    Child(Child),
    /// Windows: `SetThreadExecutionState` привязан к потоку, снимает `release`.
    #[cfg(windows)]
    Process,
    /// Тестовый бэкенд: флаг в памяти вместо ОС.
    #[cfg(test)]
    Fake(Arc<AtomicBool>),
}

impl InhibitionGuard {
    /// Носитель лока — дочерний процесс (Linux, macOS).
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn new_child(backend: InhibitBackend, child: Child) -> Self {
        Self {
            backend,
            released: false,
            carrier: Carrier::Child(child),
        }
    }

    /// Носитель лока — состояние текущего потока (Windows).
    #[cfg(windows)]
    fn new_process(backend: InhibitBackend) -> Self {
        Self {
            backend,
            released: false,
            carrier: Carrier::Process,
        }
    }

    /// Носитель лока — флаг в памяти: тесты не трогают ОС.
    #[cfg(test)]
    fn new_fake(backend: InhibitBackend, alive: Arc<AtomicBool>) -> Self {
        Self {
            backend,
            released: false,
            carrier: Carrier::Fake(alive),
        }
    }

    /// Каким способом удерживается лок.
    pub(crate) fn backend(&self) -> InhibitBackend {
        self.backend
    }

    /// Жив ли носитель лока.
    ///
    /// Linux/macOS — `try_wait` по дочернему процессу; Windows и освобождённый
    /// страж — `true`/`false` без системных вызовов. Ошибка `try_wait` означает,
    /// что доверять носителю нельзя: лок считается потерянным.
    pub(crate) fn is_alive(&mut self) -> bool {
        if self.released {
            return false;
        }
        match &mut self.carrier {
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            Carrier::Child(child) => matches!(child.try_wait(), Ok(None)),
            #[cfg(windows)]
            Carrier::Process => true,
            #[cfg(test)]
            Carrier::Fake(alive) => alive.load(Ordering::Relaxed),
        }
    }

    /// Снимает лок. Вызывается из `Drop`, поэтому ошибки не возвращает.
    fn release(&mut self) {
        if self.released {
            return;
        }
        self.released = true;
        match &mut self.carrier {
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            Carrier::Child(child) => {
                // Ошибки игнорируются: лок снимается и смертью носителя.
                let _ = child.kill();
                let _ = child.wait();
            }
            #[cfg(windows)]
            Carrier::Process => windows::release(),
            #[cfg(test)]
            Carrier::Fake(alive) => alive.store(false, Ordering::Relaxed),
        }
        info!(backend = ?self.backend, "лок сна снят");
    }
}

impl Drop for InhibitionGuard {
    fn drop(&mut self) {
        self.release();
    }
}

/// Фабрика лока сна. Состояние принадлежит только [`InhibitionGuard`].
pub(crate) struct PowerInhibit;

impl PowerInhibit {
    /// Берёт лок сна.
    ///
    /// `why` — фиксированная человекопонятная причина (уходит в `--why=`),
    /// `ttl_secs` — страховка `caffeinate` на macOS; Linux время жизни не
    /// использует: лок живёт ровно столько, сколько живёт страж.
    ///
    /// Вызывать с главного потока: `PR_SET_PDEATHSIG` на Linux срабатывает на
    /// завершение потока-родителя, а `SetThreadExecutionState` на Windows
    /// привязан к вызвавшему потоку.
    pub(crate) fn acquire(why: &str, ttl_secs: u64) -> Result<InhibitionGuard, PowerError> {
        #[cfg(target_os = "linux")]
        let guard = linux::acquire(why, ttl_secs);
        #[cfg(target_os = "macos")]
        let guard = macos::acquire(why, ttl_secs);
        #[cfg(windows)]
        let guard = windows::acquire(why, ttl_secs);

        match guard {
            Ok(guard) => {
                info!(backend = ?guard.backend(), why, "лок сна взят");
                Ok(guard)
            }
            Err(err) => {
                warn!(error = %err, "лок сна не взят");
                Err(err)
            }
        }
    }

    /// Бэкенд, выбранный на этой машине. Процессов не запускает (ADR-0054 §1).
    #[allow(dead_code)]
    pub(crate) fn backend() -> InhibitBackend {
        #[cfg(target_os = "linux")]
        let backend = linux::backend();
        #[cfg(target_os = "macos")]
        let backend = macos::backend();
        #[cfg(windows)]
        let backend = windows::backend();
        backend
    }

    /// Доступен ли способ удержать лок на этой машине.
    #[allow(dead_code)]
    pub(crate) fn available() -> bool {
        #[cfg(target_os = "linux")]
        let available = linux::available();
        #[cfg(target_os = "macos")]
        let available = macos::available();
        #[cfg(windows)]
        let available = windows::available();
        available
    }
}

/// Выполняет действие таймера сна. `Nothing` — no-op.
///
/// `ORBITFM_POWER_DRY_RUN=1` печатает argv и не запускает ничего (ADR-0055 §4).
pub(crate) fn perform(action: PowerAction) -> Result<(), String> {
    if matches!(action, PowerAction::Nothing) {
        return Ok(());
    }
    if dry_run(env::var_os(DRY_RUN_ENV)) {
        info!(
            action = action.name(),
            argv = ?action_argv(action),
            "ORBITFM_POWER_DRY_RUN=1: действие таймера сна пропущено"
        );
        return Ok(());
    }

    #[cfg(target_os = "linux")]
    let result = linux::perform(action);
    #[cfg(target_os = "macos")]
    let result = macos::perform(action);
    #[cfg(windows)]
    let result = windows::perform(action);
    result
}

/// argv, который был бы запущен: dry-run и тесты не спавнят процессов.
///
/// `None` — у платформы нет argv (Windows вызывает Win32 API напрямую).
pub(crate) fn action_argv(action: PowerAction) -> Option<Vec<&'static str>> {
    #[cfg(target_os = "linux")]
    let argv = linux::action_argv(action);
    #[cfg(target_os = "macos")]
    let argv = macos::action_argv(action);
    #[cfg(windows)]
    let argv = windows::action_argv(action);
    argv
}

/// Чистая проверка переменной: тесты не трогают env процесса.
fn dry_run(value: Option<OsString>) -> bool {
    value.is_some_and(|value| value.to_str() == Some("1"))
}

/// Ищет исполняемый файл в `PATH` без spawn: `metadata` и бит x (ADR-0054 §2).
#[cfg(unix)]
fn find_in_path(name: &str) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;

    env::split_paths(&env::var_os("PATH")?).find_map(|dir| {
        let candidate = dir.join(name);
        let metadata = fs::metadata(&candidate).ok()?;
        (metadata.is_file() && metadata.permissions().mode() & 0o111 != 0).then_some(candidate)
    })
}

/// Запускает фиксированный argv без shell; stderr читается для detail.
///
/// Общий путь Linux и macOS: ни одна команда не собирается из пользовательских
/// строк, поэтому подстановка аргументов невозможна (ADR-0055 §3).
#[cfg(unix)]
fn run_fixed(argv: &[&str]) -> Result<(), String> {
    let Some((program, args)) = argv.split_first() else {
        return Err("пустой argv действия".to_owned());
    };
    let output = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|err| format!("{program}: {err}"))?;
    if output.status.success() {
        return Ok(());
    }
    let detail = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    Err(if detail.is_empty() {
        format!("{program} завершился с {}", output.status)
    } else {
        detail
    })
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::*;

    #[test]
    fn nothing_action_is_a_noop() {
        assert!(perform(PowerAction::Nothing).is_ok());
        assert_eq!(action_argv(PowerAction::Nothing), None);
    }

    #[test]
    fn dry_run_accepts_only_one() {
        assert!(dry_run(Some(OsString::from("1"))));
        for value in ["0", "true", "", " 1", "01"] {
            assert!(
                !dry_run(Some(OsString::from(value))),
                "{value:?} не должен включать dry-run"
            );
        }
        assert!(!dry_run(None));
    }

    #[test]
    fn availability_matches_the_reported_backend() {
        assert_eq!(
            PowerInhibit::available(),
            PowerInhibit::backend() != InhibitBackend::Unavailable
        );
    }
}
