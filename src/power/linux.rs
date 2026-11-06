//! Linux: лок сна держит дочерний `systemd-inhibit` (ADR-0054 §2).
//!
//! `busctl call … Inhibit` не годится: fd лока живёт в таблице `busctl` и
//! исчезает вместе с ним. Носитель — `sleep infinity` под
//! `systemd-inhibit --mode=block`; argv фиксирован, shell не запускается.
//!
//! На не-Linux сборках файл пуст: все элементы скрыты за `cfg`.

#[cfg(target_os = "linux")]
mod imp {
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};

    use crate::power::{InhibitBackend, InhibitionGuard, PowerAction, PowerError};

    /// Команда-носитель лока.
    const INHIBIT: &str = "systemd-inhibit";
    /// Блокируем только сон: выключение и крышка — не наша задача.
    const WHAT: &str = "--what=sleep";
    /// Кто держит лок: имя видно в `systemd-inhibit --list`.
    const WHO: &str = "--who=OrbitFM";
    /// `delay` ограничен `InhibitDelayMaxSec=5`, поэтому только `block`.
    const MODE: &str = "--mode=block";
    /// Носитель лока: живёт, пока жив дочерний процесс (плюс PDEATHSIG).
    const CARRIER: [&str; 2] = ["sleep", "infinity"];

    /// Точный argv `busctl` для сна (ADR-0055 §3).
    ///
    /// `busctl` здесь применим: возврата нет, fd не передаётся, а
    /// `interactive=true` позволяет polkit спросить пользователя.
    pub(crate) const SUSPEND_ARGV: [&str; 9] = [
        "busctl",
        "--user",
        "call",
        "org.freedesktop.login1",
        "/org/freedesktop/login1",
        "org.freedesktop.login1.Manager",
        "Suspend",
        "b",
        "true",
    ];

    /// То же для выключения: отличается только метод.
    pub(crate) const POWER_OFF_ARGV: [&str; 9] = [
        "busctl",
        "--user",
        "call",
        "org.freedesktop.login1",
        "/org/freedesktop/login1",
        "org.freedesktop.login1.Manager",
        "PowerOff",
        "b",
        "true",
    ];

    /// Фолбэк, если `busctl` отказал. Ретраев нет — ровно одна попытка.
    pub(crate) const SUSPEND_FALLBACK: [&str; 2] = ["loginctl", "suspend"];
    pub(crate) const POWER_OFF_FALLBACK: [&str; 2] = ["loginctl", "poweroff"];

    /// `systemd-inhibit` есть в `PATH` и исполняем. Без spawn (ADR-0054 §1).
    pub(crate) fn available() -> bool {
        super::super::find_in_path(INHIBIT).is_some()
    }

    /// Бэкенд выбирается по доступности команды: порталов для сна нет.
    #[allow(dead_code)]
    pub(crate) fn backend() -> InhibitBackend {
        if available() {
            InhibitBackend::LogindInhibit
        } else {
            InhibitBackend::Unavailable
        }
    }

    /// Полный argv ингибитора: меняется только причина (ADR-0054 §2).
    fn inhibit_argv(why: &str) -> Vec<String> {
        let mut argv = vec![
            INHIBIT.to_owned(),
            WHAT.to_owned(),
            WHO.to_owned(),
            format!("--why={why}"),
            MODE.to_owned(),
        ];
        argv.extend(CARRIER.map(str::to_owned));
        argv
    }

    /// Захватывает лок `systemd-inhibit`.
    ///
    /// `ttl_secs` не нужен: время жизни лока совпадает со временем жизни
    /// стража. Причину (`why`) задаёт вызывающий — она фиксированная.
    pub(crate) fn acquire(why: &str, _ttl_secs: u64) -> Result<InhibitionGuard, PowerError> {
        if !available() {
            return Err(PowerError::Unavailable(format!(
                "{INHIBIT} не найден в PATH"
            )));
        }
        let argv = inhibit_argv(why);
        let Some((program, args)) = argv.split_first() else {
            return Err(PowerError::Spawn("пустой argv ингибитора".to_owned()));
        };

        let mut command = Command::new(program);
        command
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        // SAFETY: `pre_exec` выполняется в дочернем процессе после fork и до
        // exec, поэтому замыкание не должно трогать аллокатор и вызывать
        // не-async-signal-safe функции. Здесь только `prctl(2)`, что разрешено.
        // PR_SET_PDEATHSIG гарантирует: родитель умер — носитель `sleep infinity`
        // получил SIGKILL, лок снят, осиротевших лока нет (ADR-0054 §2).
        unsafe {
            command.pre_exec(|| {
                // SAFETY: у `prctl` с PR_SET_PDEATHSIG нет предусловий,
                // указателей нет; SIGKILL — допустимый сигнал.
                if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }

        let child = command
            .spawn()
            .map_err(|err| PowerError::Spawn(format!("{INHIBIT}: {err}")))?;
        Ok(InhibitionGuard::new_child(
            InhibitBackend::LogindInhibit,
            child,
        ))
    }

    /// Выполняет действие таймера: `busctl`, при отказе — `loginctl`.
    pub(crate) fn perform(action: PowerAction) -> Result<(), String> {
        let (argv, fallback) = match action {
            PowerAction::Nothing => return Ok(()),
            PowerAction::Suspend => (SUSPEND_ARGV.as_slice(), SUSPEND_FALLBACK.as_slice()),
            PowerAction::PowerOff => (POWER_OFF_ARGV.as_slice(), POWER_OFF_FALLBACK.as_slice()),
        };
        match super::super::run_fixed(argv) {
            Ok(()) => Ok(()),
            Err(detail) => super::super::run_fixed(fallback)
                .map_err(|fallback_detail| format!("{detail}; фолбэк: {fallback_detail}")),
        }
    }

    /// argv действия без spawn: dry-run и тесты (ADR-0055 §4).
    pub(crate) fn action_argv(action: PowerAction) -> Option<Vec<&'static str>> {
        match action {
            PowerAction::Nothing => None,
            PowerAction::Suspend => Some(SUSPEND_ARGV.to_vec()),
            PowerAction::PowerOff => Some(POWER_OFF_ARGV.to_vec()),
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn inhibit_argv_is_fixed_apart_from_the_reason() {
            assert_eq!(
                inhibit_argv("Radio sleep timer"),
                [
                    "systemd-inhibit",
                    "--what=sleep",
                    "--who=OrbitFM",
                    "--why=Radio sleep timer",
                    "--mode=block",
                    "sleep",
                    "infinity",
                ]
            );
        }

        /// Причина остаётся одним аргументом: shell не участвует.
        #[test]
        fn reason_is_a_single_argument() {
            let argv = inhibit_argv("Radio; sleep timer --mode=delay");
            assert_eq!(argv.len(), 7);
            assert_eq!(argv[3], "--why=Radio; sleep timer --mode=delay");
        }

        #[test]
        fn action_argv_is_exact() {
            assert_eq!(
                action_argv(PowerAction::Suspend),
                Some(vec![
                    "busctl",
                    "--user",
                    "call",
                    "org.freedesktop.login1",
                    "/org/freedesktop/login1",
                    "org.freedesktop.login1.Manager",
                    "Suspend",
                    "b",
                    "true",
                ])
            );
            assert_eq!(
                action_argv(PowerAction::PowerOff),
                Some(vec![
                    "busctl",
                    "--user",
                    "call",
                    "org.freedesktop.login1",
                    "/org/freedesktop/login1",
                    "org.freedesktop.login1.Manager",
                    "PowerOff",
                    "b",
                    "true",
                ])
            );
            assert_eq!(action_argv(PowerAction::Nothing), None);
        }

        #[test]
        fn fallbacks_are_fixed() {
            assert_eq!(SUSPEND_FALLBACK, ["loginctl", "suspend"]);
            assert_eq!(POWER_OFF_FALLBACK, ["loginctl", "poweroff"]);
        }
    }
}

#[cfg(target_os = "linux")]
pub(crate) use imp::{acquire, action_argv, available, backend, perform};
