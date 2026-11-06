//! Windows: лок сна держит сам процесс через `SetThreadExecutionState`
//! (ADR-0054 §3), действие таймера — `SetSuspendState` и
//! `InitiateSystemShutdownExW` (ADR-0055 §3).
//!
//! ⚠ UNVERIFIED: ветка компилируется под `cfg(windows)`, на живой Windows не
//! проверялась; проверка — в live-протоколе. Привилегия `SeShutdownPrivilege`
//! может отсутствовать — тогда действие вернёт ошибку, но процесс не упадёт.
//!
//! На не-Windows сборках файл пуст: все элементы скрыты за `cfg`.

#[cfg(windows)]
mod imp {
    use std::io;

    use windows::Win32::System::Power::{
        ES_CONTINUOUS, ES_SYSTEM_REQUIRED, SetSuspendState, SetThreadExecutionState,
    };
    use windows::Win32::System::Shutdown::{
        InitiateSystemShutdownExW, SHTDN_REASON_FLAG_PLANNED, SHTDN_REASON_MAJOR_OTHER,
        SHTDN_REASON_MINOR_OTHER,
    };
    use windows::core::PCWSTR;

    use crate::power::{InhibitBackend, InhibitionGuard, PowerAction, PowerError};

    /// Win32 API ядра есть всегда: отдельной утилиты, как на Linux, нет.
    pub(crate) fn available() -> bool {
        true
    }

    pub(crate) fn backend() -> InhibitBackend {
        InhibitBackend::SetThreadExecutionState
    }

    /// Захватывает лок: `ES_CONTINUOUS | ES_SYSTEM_REQUIRED`.
    ///
    /// `ES_DISPLAY_REQUIRED` не ставим — гашение экрана не задача таймера.
    /// Состояние привязано к вызвавшему потоку, поэтому вызывать с главного.
    pub(crate) fn acquire(_why: &str, _ttl_secs: u64) -> Result<InhibitionGuard, PowerError> {
        // SAFETY: у `SetThreadExecutionState` нет предусловий и указателей;
        // вызов меняет только состояние питания текущего потока.
        let previous = unsafe { SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED) };
        if previous.0 == 0 {
            return Err(PowerError::Spawn(format!(
                "SetThreadExecutionState: {}",
                io::Error::last_os_error()
            )));
        }
        Ok(InhibitionGuard::new_process(
            InhibitBackend::SetThreadExecutionState,
        ))
    }

    /// Снимает лок: `ES_CONTINUOUS` без флагов.
    ///
    /// Вызывать с того же потока, что и [`acquire`]: лок живёт в его состоянии.
    pub(crate) fn release() {
        // SAFETY: см. `acquire`; возврат игнорируется, как и в `Drop` стража.
        unsafe {
            SetThreadExecutionState(ES_CONTINUOUS);
        }
    }

    /// Выполняет действие таймера: одна попытка, без ретраев.
    pub(crate) fn perform(action: PowerAction) -> Result<(), String> {
        match action {
            PowerAction::Nothing => Ok(()),
            PowerAction::Suspend => {
                // SAFETY: параметры — три bool, указателей нет; hibernate=false,
                // force=false, события пробуждения не отключаем.
                let ok = unsafe { SetSuspendState(false, false, false) };
                if ok {
                    Ok(())
                } else {
                    Err(format!("SetSuspendState: {}", io::Error::last_os_error()))
                }
            }
            PowerAction::PowerOff => {
                let reason =
                    SHTDN_REASON_MAJOR_OTHER | SHTDN_REASON_MINOR_OTHER | SHTDN_REASON_FLAG_PLANNED;
                // SAFETY: обе строки — null (API их не разыменовывает),
                // таймаут 0 — немедленное выключение, перезагрузки нет.
                unsafe {
                    InitiateSystemShutdownExW(
                        PCWSTR::null(),
                        PCWSTR::null(),
                        0,
                        false,
                        false,
                        reason,
                    )
                }
                .map_err(|err| format!("InitiateSystemShutdownExW: {err}"))
            }
        }
    }

    /// У Win32-пути argv нет: dry-run печатает только имя действия.
    pub(crate) fn action_argv(_action: PowerAction) -> Option<Vec<&'static str>> {
        None
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn backend_is_the_thread_execution_state() {
            assert_eq!(backend(), InhibitBackend::SetThreadExecutionState);
            assert!(available());
            assert_eq!(action_argv(PowerAction::Suspend), None);
        }
    }
}

#[cfg(windows)]
pub(crate) use imp::{acquire, action_argv, available, backend, perform, release};
