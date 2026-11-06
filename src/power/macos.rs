//! macOS: лок сна держит дочерний `caffeinate` (ADR-0054 §3).
//!
//! ⚠ UNVERIFIED: ветка не проверялась на живой macOS; статус «принято» в
//! ADR-0054 не означает «проверено», закрывается live-протоколом. Ошибка
//! ингибитора не фатальна для таймера: он работает и без лока.
//!
//! На не-macOS сборках файл пуст: все элементы скрыты за `cfg`.

#[cfg(target_os = "macos")]
mod imp {
    use crate::power::{InhibitBackend, InhibitionGuard, PowerAction, PowerError};

    /// Системная утилита Apple: входит в состав macOS, новых зависимостей нет.
    const CAFFEINATE: &str = "caffeinate";
    /// `-i` запрещает только idle sleep; гашение дисплея (`-d`) не наше дело.
    const IDLE: &str = "-i";
    /// `-w` привязывает жизнь лока к нашему pid.
    const WAIT_PID: &str = "-w";
    /// `-t` страхует от вечного лока, если `-w` почему-то не сработал.
    const TIMEOUT: &str = "-t";

    /// Сон по требованию: `pmset` — штатный путь macOS.
    pub(crate) const SUSPEND_ARGV: [&str; 2] = ["pmset", "sleepnow"];
    /// Выключение через System Events: AppleScript не требует новых crate.
    pub(crate) const POWER_OFF_ARGV: [&str; 3] =
        ["osascript", "-e", "tell app \"System Events\" to shut down"];

    /// `caffeinate` есть в `PATH` и исполняем. Без spawn (ADR-0054 §1).
    pub(crate) fn available() -> bool {
        super::super::find_in_path(CAFFEINATE).is_some()
    }

    /// Бэкенд выбирается по доступности `caffeinate`.
    pub(crate) fn backend() -> InhibitBackend {
        if available() {
            InhibitBackend::Caffeinate
        } else {
            InhibitBackend::Unavailable
        }
    }

    /// Полный argv `caffeinate`: pid и ttl подставляются как отдельные аргументы.
    fn inhibit_argv(pid: u32, ttl_secs: u64) -> Vec<String> {
        vec![
            CAFFEINATE.to_owned(),
            IDLE.to_owned(),
            WAIT_PID.to_owned(),
            pid.to_string(),
            TIMEOUT.to_owned(),
            ttl_secs.to_string(),
        ]
    }

    /// Захватывает лок `caffeinate`.
    ///
    /// Причину (`why`) macOS не принимает — аргумента для неё нет; она остаётся
    /// в общем логе модуля.
    pub(crate) fn acquire(_why: &str, ttl_secs: u64) -> Result<InhibitionGuard, PowerError> {
        if !available() {
            return Err(PowerError::Unavailable(format!(
                "{CAFFEINATE} не найден в PATH"
            )));
        }
        let argv = inhibit_argv(std::process::id(), ttl_secs);
        let Some((program, args)) = argv.split_first() else {
            return Err(PowerError::Spawn("пустой argv ингибитора".to_owned()));
        };
        let child = std::process::Command::new(program)
            .args(args)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|err| PowerError::Spawn(format!("{CAFFEINATE}: {err}")))?;
        Ok(InhibitionGuard::new_child(
            InhibitBackend::Caffeinate,
            child,
        ))
    }

    /// Выполняет действие таймера: одна попытка, без ретраев.
    pub(crate) fn perform(action: PowerAction) -> Result<(), String> {
        match action {
            PowerAction::Nothing => Ok(()),
            PowerAction::Suspend => super::super::run_fixed(SUSPEND_ARGV.as_slice()),
            PowerAction::PowerOff => super::super::run_fixed(POWER_OFF_ARGV.as_slice()),
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
        fn inhibit_argv_is_exact() {
            assert_eq!(
                inhibit_argv(4242, 3600),
                ["caffeinate", "-i", "-w", "4242", "-t", "3600",]
            );
            assert_eq!(inhibit_argv(1, 90).len(), 6);
        }

        /// `-d` в argv нет: гашение дисплея не задача таймера (ADR-0054 §3).
        #[test]
        fn display_assertion_is_not_requested() {
            assert!(!inhibit_argv(4242, 60).iter().any(|arg| arg == "-d"));
        }

        #[test]
        fn action_argv_is_exact() {
            assert_eq!(
                action_argv(PowerAction::Suspend),
                Some(vec!["pmset", "sleepnow"])
            );
            assert_eq!(
                action_argv(PowerAction::PowerOff),
                Some(vec![
                    "osascript",
                    "-e",
                    "tell app \"System Events\" to shut down",
                ])
            );
            assert_eq!(action_argv(PowerAction::Nothing), None);
        }
    }
}

#[cfg(target_os = "macos")]
pub(crate) use imp::{acquire, action_argv, available, backend, perform};
