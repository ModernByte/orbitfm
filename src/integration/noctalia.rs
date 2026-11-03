//! Мост к Noctalia (ADR-0051 §2): регистрация источника плагина, не файлы.
//!
//! Граница безопасности одна: argv внешнего commands строят только функции этого
//! модуля, и вызов идёт фиксированным вектором без shell-оболочки (ADR-0033).
//! Ни одна внешняя строка в argv не оказывается — имя и URL источника заданы
//! кодом, вывод оболочки только читается.

use std::process::Command;

use super::command_in_path;

/// Программа обращения к оболочке Noctalia.
pub(crate) const PROGRAM: &str = "noctalia";
/// Целевой plugin id после регистрации источника (ADR-0051 §2).
pub(crate) const PLUGIN_ID: &str = "modernbyte/orbitfm";
/// Фиксированное имя источника OrbitFM в Noctalia (ADR-0051 §2).
pub(crate) const SOURCE: &str = "orbitfm";
/// Публичный git-источник Noctalia (ADR-0051 §2).
pub(crate) const SOURCE_URL: &str = "https://github.com/ModernByte/orbitfm";

/// Принадлежности установки: что OrbitFM сам менял в оболочке (ADR-0051 §4).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Flags {
    /// Источник `orbitfm` зарегистрировали мы, а не пользователь заранее.
    pub(crate) source_added: bool,
    /// `modernbyte/orbitfm` был включён до нашей установки.
    pub(crate) was_enabled: bool,
}

/// Сборка argv: программа плюс фиксированные подсказки.
fn argv(args: &[&str]) -> Vec<String> {
    let mut argv = vec![PROGRAM.to_string()];
    argv.extend(args.iter().map(|arg| (*arg).to_string()));
    argv
}

/// Чтение списка плагинов (по нему решаем `was_enabled`).
fn argv_list() -> Vec<String> {
    argv(&["msg", "plugins", "list"])
}

/// Чтение списка источников (по нему решаем `source_added`).
fn argv_source_list() -> Vec<String> {
    argv(&["msg", "plugins", "source", "list"])
}

/// Регистрация git-источника OrbitFM.
fn argv_source_add() -> Vec<String> {
    argv(&["msg", "plugins", "source", "add", SOURCE, "git", SOURCE_URL])
}

/// Удаление источника OrbitFM.
fn argv_source_remove() -> Vec<String> {
    argv(&["msg", "plugins", "source", "remove", SOURCE])
}

/// Включение виджета.
fn argv_enable() -> Vec<String> {
    argv(&["msg", "plugins", "enable", PLUGIN_ID])
}

/// Выключение виджета.
fn argv_disable() -> Vec<String> {
    argv(&["msg", "plugins", "disable", PLUGIN_ID])
}

/// Установка: `source add` (только если источника нет) и `enable`.
///
/// Причина отказа — `Err`: без отвечающей оболочки регистрировать нечего, и
/// установщик ничего не пишет (ADR-0051 §2). Песочница (`ORBITFM_INTEGRATION_ROOT`)
/// живую оболочку не трогает: владение фиксируется как «источник наш».
pub(crate) fn register(sandbox: bool) -> Result<Flags, String> {
    if sandbox {
        return Ok(Flags {
            source_added: true,
            was_enabled: false,
        });
    }
    if !command_in_path(PROGRAM) {
        return Err(format!("нет команды {PROGRAM} в PATH"));
    }
    let list = run(&argv_list())?;
    let sources = run(&argv_source_list())?;
    let flags = Flags {
        source_added: !source_present(&sources),
        was_enabled: plugin_enabled(&list),
    };
    if flags.source_added {
        run(&argv_source_add())?;
    }
    run(&argv_enable())?;
    Ok(flags)
}

/// Снятие: `disable`, затем `source remove` только за наш источник, и возврат
/// включённости, если виджет всё ещё доступен из чужого источника (ADR-0051 §2).
///
/// Отказ оболочки не отменяет расписку: причина уходит в `detail` подсказкой.
pub(crate) fn unregister(flags: Flags, sandbox: bool) -> Option<String> {
    if sandbox {
        return None;
    }
    if !command_in_path(PROGRAM) {
        return Some(format!("нет команды {PROGRAM}: оболочке не командовали"));
    }
    let mut notes = Vec::new();
    if let Err(reason) = run(&argv_disable()) {
        notes.push(reason);
    }
    if flags.source_added
        && let Err(reason) = run(&argv_source_remove())
    {
        notes.push(reason);
    }
    if flags.was_enabled {
        match run(&argv_list()) {
            // Виджет остался доступным: включаем обратно, как включал пользователь.
            Ok(list) if plugin_present(&list) => {
                if let Err(reason) = run(&argv_enable()) {
                    notes.push(reason);
                }
            }
            Ok(_) => {}
            Err(reason) => notes.push(reason),
        }
    }
    (!notes.is_empty()).then(|| notes.join("; "))
}

/// Запуск фиксированного argv; stdout при успехе, причина иначе.
fn run(argv: &[String]) -> Result<String, String> {
    let Some((program, args)) = argv.split_first() else {
        return Err("пустой argv".to_string());
    };
    match Command::new(program).args(args).output() {
        Ok(out) if out.status.success() => Ok(String::from_utf8_lossy(&out.stdout).into_owned()),
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            let stdout = String::from_utf8_lossy(&out.stdout);
            let reason = [stderr.trim(), stdout.trim()]
                .into_iter()
                .find(|text| !text.is_empty())
                .unwrap_or("команда ничего не ответила");
            Err(format!("{}: {reason}", argv.join(" ")))
        }
        Err(error) => Err(format!("{program}: {error}")),
    }
}

/// Строка списка плагинов про наш виджет: поле, равное его id.
fn plugin_row(list: &str) -> Option<&str> {
    list.lines()
        .find(|row| row.split_whitespace().any(|field| field == PLUGIN_ID))
}

/// Виден ли виджет в списке плагинов.
pub(crate) fn plugin_present(list: &str) -> bool {
    plugin_row(list).is_some()
}

/// Включён ли виджет: поле `enabled` без поля `disabled`.
pub(crate) fn plugin_enabled(list: &str) -> bool {
    plugin_row(list).is_some_and(|row| {
        row.split_whitespace().any(|field| field == "enabled")
            && !row.split_whitespace().any(|field| field == "disabled")
    })
}

/// Зарегистрирован ли источник `orbitfm`: строка `<имя> <вид> <адрес>`.
pub(crate) fn source_present(sources: &str) -> bool {
    sources
        .lines()
        .any(|row| row.split_whitespace().any(|field| field == SOURCE))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Формат `plugins list` снят с живой оболочки: `<id> [<источник>] <версия> enabled`.
    const LIST: &str = "\
launcher [official] 1.0.0 enabled requires n/a
modernbyte/orbitfm [orbitfm] 1.1.0 enabled requires orbitfm
weather [community] 0.9.0 disabled requires n/a
";

    /// Отключённый или отсутствующий виджет не читается как включённый.
    #[test]
    fn plugin_parsers_read_the_shell_listing() {
        assert!(plugin_present(LIST));
        assert!(plugin_enabled(LIST));
        let our_disabled = "modernbyte/orbitfm [orbitfm] 1.1.0 disabled requires orbitfm\n";
        assert!(plugin_present(our_disabled));
        assert!(!plugin_enabled(our_disabled));
        assert!(!plugin_present("weather [community] 0.9.0 enabled\n"));
        assert!(!plugin_enabled(""));
        // Подстрока не считается: чужой id с нашим префиксом нам не виджет.
        assert!(!plugin_present(
            "modernbyte/orbitfm-extra [x] 1.0.0 enabled\n"
        ));
    }

    /// Источники различаются по имени, а не по упоминанию внутри адреса.
    #[test]
    fn source_parser_reads_the_source_listing() {
        let sources = "\
official git https://github.com/noctalia-dev/official-plugins
warpdev path /home/user/DEV/cloudflare-warp-plugin
orbitfm git https://github.com/ModernByte/orbitfm
";
        assert!(source_present(sources));
        assert!(source_present(
            "orbitfm path /home/user/DEV/noctalia-orbitfm"
        ));
        assert!(!source_present(
            "official git https://github.com/noctalia-dev/official-plugins\n"
        ));
        assert!(!source_present(""));
    }

    /// argv фиксирован и совпадает с ADR-0051 §5; shell-символов в нём нет.
    #[test]
    fn argv_is_the_fixed_command_vector() {
        assert_eq!(argv_list(), ["noctalia", "msg", "plugins", "list"]);
        assert_eq!(
            argv_source_list(),
            ["noctalia", "msg", "plugins", "source", "list"]
        );
        assert_eq!(
            argv_source_add(),
            [
                "noctalia",
                "msg",
                "plugins",
                "source",
                "add",
                "orbitfm",
                "git",
                "https://github.com/ModernByte/orbitfm"
            ]
        );
        assert_eq!(
            argv_source_remove(),
            ["noctalia", "msg", "plugins", "source", "remove", "orbitfm"]
        );
        assert_eq!(
            argv_enable(),
            ["noctalia", "msg", "plugins", "enable", "modernbyte/orbitfm"]
        );
        assert_eq!(
            argv_disable(),
            [
                "noctalia",
                "msg",
                "plugins",
                "disable",
                "modernbyte/orbitfm"
            ]
        );
        for argv in [
            argv_list(),
            argv_source_list(),
            argv_source_add(),
            argv_source_remove(),
            argv_enable(),
            argv_disable(),
        ] {
            assert_eq!(argv.first().map(String::as_str), Some(PROGRAM));
            for arg in &argv {
                assert!(!arg.contains([' ', ';', '&', '|', '$', '`']), "{arg}");
            }
        }
    }

    /// Песочница не командует живой оболочке, но фиксирует владение источником.
    #[test]
    fn sandbox_never_commands_the_shell() {
        assert_eq!(
            register(true).expect("песочница регистрирует источник"),
            Flags {
                source_added: true,
                was_enabled: false,
            }
        );
        assert_eq!(unregister(Flags::default(), true), None);
        assert_eq!(
            unregister(
                Flags {
                    source_added: true,
                    was_enabled: true,
                },
                true,
            ),
            None
        );
    }
}
