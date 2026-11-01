//! Обнаружение композитора и правила окна на время сеанса (ADR-0041).
//!
//! Мастер первого запуска не редактирует конфиги пользователя: для Hyprland
//! правила ставятся через `hyprctl keyword` с фиксированными аргументами, для
//! niri показывается готовый KDL-блок из `wm-configs/`. Shell не запускается.

use std::env;
use std::ffi::OsString;
use std::process::{Command, Stdio};

/// KDL-правила niri: runtime-интерфейса для window-rule в IPC нет.
pub(crate) const NIRI_SNIPPET: &str = include_str!("../../wm-configs/orbitfm-niri.kdl");
/// Конфиг Hyprland целиком: показывается кнопкой «Копировать», если IPC отказал.
pub(crate) const HYPRLAND_SNIPPET: &str = include_str!("../../wm-configs/orbitfm-hyprland.conf");

/// Рабочее окружение, для которого мастер умеет предложить правила окна.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Desktop {
    Hyprland,
    Niri,
    Kde,
    Gnome,
    Other,
}

impl Desktop {
    /// Определяет окружение по переменным сеанса, которые выставляет композитор.
    pub(crate) fn detect() -> Self {
        Self::classify(
            env::var_os("HYPRLAND_INSTANCE_SIGNATURE"),
            env::var_os("NIRI_SOCKET"),
            env::var_os("XDG_CURRENT_DESKTOP"),
            env::var_os("DESKTOP_SESSION"),
        )
    }

    /// Чистая классификация: тесты подставляют значения вместо env.
    fn classify(
        hyprland: Option<OsString>,
        niri: Option<OsString>,
        current_desktop: Option<OsString>,
        session: Option<OsString>,
    ) -> Self {
        let present = |value: Option<OsString>| value.is_some_and(|value| !value.is_empty());
        if present(hyprland) {
            return Self::Hyprland;
        }
        if present(niri) {
            return Self::Niri;
        }
        let desktop = current_desktop
            .or(session)
            .map(|value| value.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        if desktop.contains("hyprland") {
            Self::Hyprland
        } else if desktop.contains("niri") {
            Self::Niri
        } else if desktop.contains("kde") {
            Self::Kde
        } else if desktop.contains("gnome") {
            Self::Gnome
        } else {
            Self::Other
        }
    }

    /// Имя окружения для строки «Обнаружено: …». Собственные имена не переводятся.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Hyprland => "Hyprland",
            Self::Niri => "niri",
            Self::Kde => "KDE Plasma",
            Self::Gnome => "GNOME",
            Self::Other => "Linux",
        }
    }

    /// Какие правила можно применить прямо сейчас.
    pub(crate) fn runtime_rules(self) -> RuntimeRules {
        match self {
            Self::Hyprland => RuntimeRules::Hyprland,
            Self::Niri => RuntimeRules::Snippet(NIRI_SNIPPET),
            Self::Kde | Self::Gnome | Self::Other => RuntimeRules::Unsupported,
        }
    }
}

/// Возможности окружения для шага «Окружение и окно».
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RuntimeRules {
    /// `hyprctl keyword` с фиксированным набором правил.
    Hyprland,
    /// Runtime-правил нет: готовый сниппет и копирование.
    Snippet(&'static str),
    /// Окружение не требует правил окна или не поддерживает их.
    Unsupported,
}

/// Правила из `wm-configs/orbitfm-hyprland.conf`; аргументы фиксированы в коде,
/// поэтому правило из файла не может превратиться в произвольную команду.
const HYPRLAND_RULES: [&str; 14] = [
    r"float,class:^(org\.modernbyte\.OrbitFM)$",
    r"center,class:^(org\.modernbyte\.OrbitFM)$",
    r"stayfocused,class:^(org\.modernbyte\.OrbitFM)$",
    "size 1100 800,title:^OrbitFM$",
    "size 500 620,title:^OrbitFM · Overlay$",
    "noborder,title:^OrbitFM · Overlay$",
    "noshadow,title:^OrbitFM · Overlay$",
    "size 420 152,title:^OrbitFM · Listening$",
    "noborder,title:^OrbitFM · Listening$",
    "noshadow,title:^OrbitFM · Listening$",
    "size 400 640,title:^OrbitFM · Panel$",
    "noborder,title:^OrbitFM · Panel$",
    "noshadow,title:^OrbitFM · Panel$",
    "noblur,title:^OrbitFM · Panel$",
];

/// Применяет правила Hyprland на время сеанса. Каждый вызов — отдельный
/// `hyprctl` без shell; ошибка возвращает ответ композитора для подсказки.
pub(crate) fn apply_hyprland_rules() -> Result<(), String> {
    for rule in HYPRLAND_RULES {
        let output = Command::new("hyprctl")
            .arg("keyword")
            .arg("windowrulev2")
            .arg(rule)
            .stdin(Stdio::null())
            .output()
            .map_err(|err| format!("hyprctl: {err}"))?;
        if !output.status.success() {
            let detail = String::from_utf8_lossy(&output.stderr).trim().to_owned();
            let detail = if detail.is_empty() {
                String::from_utf8_lossy(&output.stdout).trim().to_owned()
            } else {
                detail
            };
            return Err(if detail.is_empty() {
                format!("hyprctl завершился с {}", output.status)
            } else {
                detail
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn value(text: &str) -> Option<OsString> {
        Some(OsString::from(text))
    }

    #[test]
    fn compositor_variables_win_over_desktop_names() {
        assert_eq!(
            Desktop::classify(value("sig"), value("/run/niri"), value("GNOME"), None),
            Desktop::Hyprland
        );
        assert_eq!(
            Desktop::classify(None, value("/run/niri"), value("KDE"), None),
            Desktop::Niri
        );
    }

    #[test]
    fn desktop_names_are_matched_case_insensitively() {
        assert_eq!(
            Desktop::classify(None, None, value("KDE"), None),
            Desktop::Kde
        );
        assert_eq!(
            Desktop::classify(None, None, value("ubuntu:GNOME"), None),
            Desktop::Gnome
        );
        assert_eq!(
            Desktop::classify(None, None, None, value("Hyprland")),
            Desktop::Hyprland
        );
        assert_eq!(
            Desktop::classify(None, None, value("sway"), None),
            Desktop::Other
        );
        assert_eq!(
            Desktop::classify(value(""), None, None, None),
            Desktop::Other
        );
    }

    #[test]
    fn runtime_rules_follow_the_environment() {
        assert_eq!(Desktop::Hyprland.runtime_rules(), RuntimeRules::Hyprland);
        assert!(matches!(
            Desktop::Niri.runtime_rules(),
            RuntimeRules::Snippet(snippet) if snippet.contains("app-id=\"org.modernbyte.OrbitFM\"")
        ));
        assert_eq!(Desktop::Kde.runtime_rules(), RuntimeRules::Unsupported);
    }

    #[test]
    fn hyprland_rules_are_single_line_fixed_arguments() {
        assert_eq!(HYPRLAND_RULES.len(), 14);
        for rule in HYPRLAND_RULES {
            assert!(
                !rule.contains('\n'),
                "правило не должно содержать переводов строк"
            );
            assert!(!rule.is_empty());
        }
        assert!(HYPRLAND_SNIPPET.contains("windowrulev2"));
        assert!(NIRI_SNIPPET.contains("window-rule"));
    }
}
