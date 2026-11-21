//! Динамическая версия TUI.
//!
//! semver-база — из `Cargo.toml` (или `ORBITFM_TUI_VERSION` при релизе);
//! git-состояние и дата сборки вшиты `build.rs`. Схема — `docs/tui/versioning.md`.
//!
//! TUI версионируется **независимо** от ядра (ADR-0047 §8); совместимость
//! определяется конвертом `ev: 1` и `status.schema: 1`.

/// Базовая semver-версия.
pub const VERSION: &str = match option_env!("ORBITFM_TUI_VERSION") {
    Some(version) => version,
    None => env!("CARGO_PKG_VERSION"),
};

/// git-состояние сборки: `sha`, `sha-dirty` или `unknown`.
pub const GIT: &str = env!("ORBITFM_TUI_GIT");

/// Дата сборки (UTC, `ГГГГ-ММ-ДД`).
pub const BUILD_DATE: &str = env!("ORBITFM_TUI_BUILD_DATE");

/// Минимальная совместимая версия ядра.
pub const CORE_MIN: &str = "0.18.0-rc.1";

/// Версия конверта событий.
pub const EV: u32 = 1;

/// Версия схемы `status --json`.
pub const STATUS_SCHEMA: u32 = 1;

/// Сборка из рабочего дерева с незакоммиченными изменениями?
pub fn is_dirty() -> bool {
    GIT.ends_with("-dirty")
}

/// git-состояние известно?
pub fn is_git_known() -> bool {
    GIT != "unknown"
}

/// `0.1.0` — короткая форма для шапки.
pub fn short() -> &'static str {
    VERSION
}

/// `0.1.0+abc1234def0-dirty` — semver с build-метаданными.
pub fn semver() -> String {
    if is_git_known() {
        format!("{VERSION}+{GIT}")
    } else {
        VERSION.to_owned()
    }
}

/// Многострочная строка для `--version`: версия, git, дата и совместимость.
pub fn long() -> String {
    let git = if is_git_known() {
        format!("git {GIT}")
    } else {
        "git unknown".to_owned()
    };
    let dirty = if is_dirty() {
        " · working tree dirty"
    } else {
        ""
    };
    format!(
        "orbitfm-tui {}\n{git} · build {BUILD_DATE}{dirty}\ncore >= {CORE_MIN} (ev {EV}, status schema {STATUS_SCHEMA})\n",
        semver()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semver_carries_version() {
        assert!(semver().starts_with(VERSION));
    }

    #[test]
    fn long_mentions_git_and_core() {
        let text = long();
        assert!(text.contains("orbitfm-tui"));
        assert!(text.contains(CORE_MIN));
        assert!(text.contains(&format!("ev {EV}")));
    }

    #[test]
    fn flags_are_consistent() {
        assert_eq!(is_dirty(), GIT.ends_with("-dirty"));
        assert_eq!(is_git_known(), GIT != "unknown");
    }
}
