//! Обнаружение композитора и правила окна на время сеанса (ADR-0041).
//!
//! Мастер первого запуска не редактирует конфиги пользователя: для Hyprland
//! правила ставятся через `hyprctl keyword` с фиксированными аргументами, для
//! niri показывается готовый KDL-блок из `wm-configs/`. Shell не запускается.

use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};

use super::{Layout, Target, reject_symlink};

/// KDL-правила niri: runtime-интерфейса для window-rule в IPC нет.
pub(crate) const NIRI_SNIPPET: &str = include_str!("../../wm-configs/orbitfm-niri.kdl");
/// Конфиг Hyprland целиком: показывается кнопкой «Копировать», если IPC отказал.
pub(crate) const HYPRLAND_SNIPPET: &str = include_str!("../../wm-configs/orbitfm-hyprland.conf");
/// Файл правил niri относительно `~/.config/niri` (ADR-0051 §2).
pub(crate) const NIRI_RULE_FILE: &str = "cfg/orbitfm.kdl";
/// Главный конфиг niri: единственный чужой файл, который правит установщик.
pub(crate) const NIRI_MAIN_CONFIG: &str = "config.kdl";
/// Файл правил Hyprland относительно `~/.config/hypr` (ADR-0051 §2).
pub(crate) const HYPRLAND_RULE_FILE: &str = "orbitfm.conf";
/// Ровно одна строка, которую установщик добавляет в `config.kdl` (ADR-0051 §5).
pub(crate) const NIRI_INCLUDE: &str = "include \"./cfg/orbitfm.kdl\"";
/// Строка подключения для Hyprland: её только печатают, в конфиг не пишут.
pub(crate) const HYPRLAND_SOURCE: &str = "source = ~/.config/hypr/orbitfm.conf";

/// Главный конфиг niri — путь каталога оболочки, не каталога установщика.
pub(super) fn niri_config_path(layout: &Layout) -> PathBuf {
    layout.root(Target::Niri).join(NIRI_MAIN_CONFIG)
}

/// Добавляет строку подключения, если её ещё нет: идемпотентность ADR-0051 §2.
///
/// Возвращает `true`, когда строка добавлена (только тогда она попадает в
/// квитанцию). Бэкап создаётся перед первой правкой и больше не обновляется.
pub(crate) fn add_include(path: &Path, line: &str) -> Result<bool> {
    reject_symlink(path)?;
    let original = fs::read_to_string(path)
        .with_context(|| format!("не удалось прочитать {}", path.display()))?;
    if has_line(&original, line) {
        return Ok(false);
    }
    super::backup_once(path, original.as_bytes())?;
    let separator = if original.ends_with('\n') || original.is_empty() {
        String::new()
    } else {
        "\n".to_string()
    };
    let updated = format!("{original}{separator}{line}\n");
    // Повторная проверка после сборки текста: строка обязана появиться ровно одна.
    if count_lines(&updated, line) != 1 {
        bail!("строка подключения не появилась в {}", path.display());
    }
    crate::storage::write_atomic(path, &temp_name(path), updated.as_bytes())
        .with_context(|| format!("не удалось записать {}", path.display()))?;
    let read_back = fs::read_to_string(path)
        .with_context(|| format!("не удалось перечитать {}", path.display()))?;
    if count_lines(&read_back, line) != 1 {
        bail!("{} изменился не так, как ожидалось", path.display());
    }
    Ok(true)
}

/// Убирает ровно свою целую строку; чужие строки остаются нетронутыми.
///
/// Файл отсутствует — не ошибка: цель уже снята или оболочка снесла конфиг.
pub(crate) fn remove_include(path: &Path, line: &str) -> Result<bool> {
    reject_symlink(path)?;
    let Ok(original) = fs::read_to_string(path) else {
        return Ok(false);
    };
    let mut dropped = false;
    let kept: Vec<&str> = original
        .lines()
        .filter(|row| {
            // Сравнение целой строки, не подстроки (ADR-0051 §2).
            if !dropped && row.trim_end_matches('\r') == line {
                dropped = true;
                false
            } else {
                true
            }
        })
        .collect();
    if !dropped {
        return Ok(false);
    }
    let updated = format_lines(&kept);
    crate::storage::write_atomic(path, &temp_name(path), updated.as_bytes())
        .with_context(|| format!("не удалось записать {}", path.display()))?;
    Ok(true)
}

/// Есть ли в тексте целая строка `line`.
pub(crate) fn has_line(text: &str, line: &str) -> bool {
    text.lines().any(|row| row.trim_end_matches('\r') == line)
}

/// Сколько раз целая строка `line` встречается в тексте.
fn count_lines(text: &str, line: &str) -> usize {
    text.lines()
        .filter(|row| row.trim_end_matches('\r') == line)
        .count()
}

/// Собирает текст обратно: по строке в конце файла.
fn format_lines(lines: &[&str]) -> String {
    let mut text = lines.join("\n");
    if !text.is_empty() {
        text.push('\n');
    }
    text
}

/// Имя временного файла рядом с конфигом (нужен `write_atomic`).
fn temp_name(path: &Path) -> String {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("config");
    format!("{file_name}.orbitfm.tmp")
}

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
const HYPRLAND_RULES: [&str; 18] = [
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
    // Окна настроек и предупреждения сна — только размер, без float/center/stayfocused:
    // они классовые и применяются ко всем окнам OrbitFM.
    "size 420 560,title:^OrbitFM · Settings · Compact$",
    "size 760 544,title:^OrbitFM · Settings · Medium$",
    "size 1040 720,title:^OrbitFM · Settings · Wide$",
    "size 420 320,title:^OrbitFM · Sleep Warning$",
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
    use crate::integration::BACKUP_SUFFIX;
    use tempfile::TempDir;

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
        assert_eq!(HYPRLAND_RULES.len(), 18);
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

    /// Массив `HYPRLAND_RULES` в коде зеркалит строки `windowrulev2 = …`
    /// в `wm-configs/orbitfm-hyprland.conf`: при добавлении правила нужно
    /// править оба места, и тест ловит расхождение.
    #[test]
    fn hyprland_array_mirrors_the_conf_snippet() {
        let conf_rules: Vec<String> = HYPRLAND_SNIPPET
            .lines()
            .filter_map(|line| line.trim_end().strip_prefix("windowrulev2 = "))
            .map(|rule| rule.replace(' ', ""))
            .collect();
        let code_rules: Vec<String> = HYPRLAND_RULES
            .iter()
            .map(|rule| rule.replace(' ', ""))
            .collect();
        assert_eq!(
            conf_rules.len(),
            code_rules.len(),
            "число правил в конфиге и в массиве не совпадает"
        );
        for (conf, code) in conf_rules.iter().zip(code_rules.iter()) {
            assert_eq!(
                conf, code,
                "правило не совпадает (конфиг vs код): {conf} vs {code}"
            );
        }
    }

    /// Строка include обязана ссылаться на тот файл, что пишет установщик.
    #[test]
    fn niri_include_points_at_the_installed_rule_file() {
        assert_eq!(
            NIRI_INCLUDE,
            format!("include \"./{NIRI_RULE_FILE}\""),
            "расходится с {NIRI_RULE_FILE}"
        );
        assert_eq!(NIRI_RULE_FILE, "cfg/orbitfm.kdl");
        assert_eq!(HYPRLAND_RULE_FILE, "orbitfm.conf");
        assert!(
            HYPRLAND_SOURCE.ends_with(HYPRLAND_RULE_FILE),
            "подсказка Hyprland ссылается на другой файл"
        );
        // Все заголовки новых окон (настройки трёх размеров + предупреждение сна)
        // обязаны присутствовать в сниппете niri.
        for title in [
            "^OrbitFM · Settings · Compact$",
            "^OrbitFM · Settings · Medium$",
            "^OrbitFM · Settings · Wide$",
            "^OrbitFM · Sleep Warning$",
        ] {
            assert!(
                NIRI_SNIPPET.contains(title),
                "сниппет niri не содержит заголовок {title}"
            );
        }
    }

    #[test]
    fn add_include_is_idempotent_and_backs_up_once() {
        let dir = TempDir::new().expect("temp dir");
        let config = dir.path().join("config.kdl");
        let original = "layout {\n  focus-follows-mouse\n}\n";
        fs::write(&config, original).expect("config");

        assert!(add_include(&config, NIRI_INCLUDE).expect("first add"));
        let once = fs::read_to_string(&config).expect("read back");
        assert!(once.contains(NIRI_INCLUDE));
        assert_eq!(count_lines(&once, NIRI_INCLUDE), 1);

        // Повторный вызов не дублирует строку и не трогает бэкап.
        assert!(!add_include(&config, NIRI_INCLUDE).expect("second add"));
        assert_eq!(fs::read_to_string(&config).expect("read back"), once);
        fs::write(&config, "x\n").expect("replace");
        assert!(add_include(&config, NIRI_INCLUDE).expect("third add"));
        let backup = dir
            .path()
            .join(format!("{NIRI_MAIN_CONFIG}{BACKUP_SUFFIX}"));
        assert_eq!(fs::read_to_string(backup).expect("backup"), original);
    }

    #[test]
    fn add_include_appends_to_text_without_trailing_newline() {
        let dir = TempDir::new().expect("temp dir");
        let config = dir.path().join("config.kdl");
        fs::write(&config, "include \"./other.kdl\"").expect("config");

        add_include(&config, NIRI_INCLUDE).expect("add");
        let text = fs::read_to_string(&config).expect("read back");
        assert_eq!(text, format!("include \"./other.kdl\"\n{NIRI_INCLUDE}\n"));
    }

    #[test]
    fn remove_include_drops_only_its_whole_line() {
        let dir = TempDir::new().expect("temp dir");
        let config = dir.path().join("config.kdl");
        fs::write(
            &config,
            format!(
                "// include \"./cfg/orbitfm.kdl\" — комментарий пользователя\n\
                 include \"./cfg/orbitfm-extra.kdl\"\n{NIRI_INCLUDE}\n\
                 layout {{\n}}\n"
            ),
        )
        .expect("config");

        assert!(remove_include(&config, NIRI_INCLUDE).expect("remove"));
        let text = fs::read_to_string(&config).expect("read back");
        assert!(!text.contains(&format!("{NIRI_INCLUDE}\n")));
        // Сравнение целой строки: ни комментарий, ни чужое include не задеты.
        assert!(text.contains("// include \"./cfg/orbitfm.kdl\" — комментарий пользователя"));
        assert!(text.contains("include \"./cfg/orbitfm-extra.kdl\""));
        assert!(text.contains("layout {"));

        // Повторное снятие и отсутствующий файл — не ошибка.
        assert!(!remove_include(&config, NIRI_INCLUDE).expect("second remove"));
        assert!(!remove_include(&dir.path().join("missing.kdl"), NIRI_INCLUDE).expect("missing"));
    }

    #[cfg(unix)]
    #[test]
    fn include_edit_refuses_a_symlinked_config() {
        use std::os::unix::fs::symlink;

        let dir = TempDir::new().expect("temp dir");
        let real = dir.path().join("dotfiles-config.kdl");
        fs::write(&real, "layout {\n}\n").expect("real");
        let config = dir.path().join(NIRI_MAIN_CONFIG);
        symlink(&real, &config).expect("symlink");

        let err = add_include(&config, NIRI_INCLUDE).expect_err("must refuse");
        assert!(format!("{err:#}").contains("символическая"));
        assert_eq!(fs::read_to_string(&real).expect("real"), "layout {\n}\n");
        assert!(
            !dir.path()
                .join(format!("{NIRI_MAIN_CONFIG}{BACKUP_SUFFIX}"))
                .exists()
        );
        let err = remove_include(&config, NIRI_INCLUDE).expect_err("must refuse");
        assert!(format!("{err:#}").contains("символическая"));
    }
}
