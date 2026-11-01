//! Правка `~/.config/waybar/config.jsonc` через CST (ADR-0042 §2).
//!
//! Регулярные выражения и `serde_json` не применяются: дерево сохраняет
//! комментарии и форматирование, результат перед записью разбирается заново,
//! а первая копия файла остаётся в `config.jsonc.orbitfm.bak`.

use std::fs;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use jsonc_parser::ParseOptions;
use jsonc_parser::cst::{CstArray, CstInputValue, CstRootNode};

use super::{Layout, Target, current_binary, inspect, shell_quote};

pub(crate) const MODULE: &str = "custom/orbitfm";

const CONFIG_DIR: &str = "waybar";
const CONFIG_NAMES: [&str; 3] = ["config.jsonc", "config", "config.json"];
const BACKUP_SUFFIX: &str = ".orbitfm.bak";
/// Сторона вставки фиксирована (ADR-0042 §2): произвольные ключи не правятся.
const SIDE_KEY: &str = "modules-right";

/// Состояние конфига для страницы «Интеграции».
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WaybarStatus {
    pub path: Option<PathBuf>,
    pub enabled: bool,
    pub error: Option<String>,
}

#[derive(Debug)]
pub(crate) struct EnableReport {
    pub path: PathBuf,
    pub backup: Option<PathBuf>,
    pub already_enabled: bool,
}

/// Первый найденный конфиг Waybar: `config.jsonc`, затем `config`,
/// затем `config.json` — тот же порядок, что у самой оболочки.
pub(super) fn config_path(layout: &Layout) -> Option<PathBuf> {
    CONFIG_NAMES
        .iter()
        .map(|name| layout.config_home.join(CONFIG_DIR).join(name))
        .find(|path| path.is_file())
}

/// Читает конфиг без записи: ошибка разбора показывается строкой состояния,
/// а не роняет страницу.
pub(super) fn status(layout: &Layout) -> WaybarStatus {
    let Some(path) = config_path(layout) else {
        return WaybarStatus {
            path: None,
            enabled: false,
            error: None,
        };
    };
    match fs::read_to_string(&path) {
        Ok(text) => match CstRootNode::parse(&text, &ParseOptions::default()) {
            Ok(root) => WaybarStatus {
                enabled: root
                    .object_value()
                    .and_then(|object| object.array_value(SIDE_KEY))
                    .is_some_and(|array| array_has_module(&array)),
                path: Some(path),
                error: None,
            },
            Err(err) => WaybarStatus {
                path: Some(path),
                enabled: false,
                error: Some(format!("не удалось разобрать конфиг: {err}")),
            },
        },
        Err(err) => WaybarStatus {
            path: Some(path),
            enabled: false,
            error: Some(format!("не удалось прочитать конфиг: {err}")),
        },
    }
}

/// Добавляет модуль в выбранный `modules-*` и создаёт его определение.
///
/// Повторный вызов идемпотентен; существующее определение пользователя не
/// перезаписывается. Бэкап создаётся один раз и никогда не обновляется.
pub(super) fn enable(layout: &Layout) -> Result<EnableReport> {
    let path = config_path(layout).ok_or_else(|| {
        anyhow!(
            "конфиг Waybar не найден: {} не существует",
            layout.config_home.join(CONFIG_DIR).display()
        )
    })?;
    reject_symlink(&path)?;

    let original =
        fs::read(&path).with_context(|| format!("не удалось прочитать {}", path.display()))?;
    let text = std::str::from_utf8(&original)
        .map_err(|err| anyhow!("{} не в UTF-8: {err}", path.display()))?;
    let root = parse(text, &path)?;
    let object = root
        .object_value()
        .ok_or_else(|| anyhow!("корень {} — не JSON-объект", path.display()))?;
    let array = object
        .array_value(SIDE_KEY)
        .ok_or_else(|| anyhow!("в {} нет массива {SIDE_KEY}", path.display()))?;

    let in_array = array_has_module(&array);
    let defined = object.get(MODULE).is_some();
    if in_array && defined {
        return Ok(EnableReport {
            path,
            backup: None,
            already_enabled: true,
        });
    }

    let backup = backup_once(&path, &original)?;
    if !in_array {
        array.append(CstInputValue::String(MODULE.to_owned()));
    }
    if !defined {
        object.append(MODULE, module_value(layout)?);
    }

    let output = root.to_string();
    verify(&output, &path)?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| anyhow!("имя конфига не в UTF-8: {}", path.display()))?;
    crate::storage::write_atomic(
        &path,
        &format!("{file_name}.orbitfm.tmp"),
        output.as_bytes(),
    )
    .with_context(|| format!("не удалось записать {}", path.display()))?;

    Ok(EnableReport {
        path,
        backup,
        already_enabled: false,
    })
}

fn reject_symlink(path: &Path) -> Result<()> {
    let meta = fs::symlink_metadata(path)
        .with_context(|| format!("не удалось прочитать {}", path.display()))?;
    if meta.file_type().is_symlink() {
        bail!(
            "{} — символическая ссылка; правка не поддерживается, оставьте конфиг как есть",
            path.display()
        );
    }
    Ok(())
}

fn parse(text: &str, path: &Path) -> Result<CstRootNode> {
    CstRootNode::parse(text, &ParseOptions::default())
        .map_err(|err| anyhow!("не удалось разобрать {}: {err}", path.display()))
}

/// Первая копия — оригинал: повторный вызов не перезаписывает бэкап.
fn backup_once(path: &Path, bytes: &[u8]) -> Result<Option<PathBuf>> {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| anyhow!("имя конфига не в UTF-8: {}", path.display()))?;
    let backup = path.with_file_name(format!("{file_name}{BACKUP_SUFFIX}"));
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&backup)
    {
        Ok(mut file) => {
            file.write_all(bytes)
                .with_context(|| format!("не удалось записать {}", backup.display()))?;
            file.sync_all()
                .with_context(|| format!("не удалось синхронизировать {}", backup.display()))?;
            Ok(Some(backup))
        }
        Err(err) if err.kind() == ErrorKind::AlreadyExists => Ok(None),
        Err(err) => Err(err).with_context(|| format!("не удалось создать {}", backup.display())),
    }
}

fn array_has_module(array: &CstArray) -> bool {
    array.elements().iter().any(|node| {
        node.as_string_lit()
            .and_then(|literal| literal.decoded_value().ok())
            .is_some_and(|value| value == MODULE)
    })
}

/// Определение модуля повторяет установленный сниппет: интервал 2 секунды,
/// JSON-вывод и клики по бару. `exec-if` прячет виджет, если скрипт исчез.
fn module_value(layout: &Layout) -> Result<CstInputValue> {
    let script = layout.root(Target::Waybar).join("orbitfm-bar");
    let binary = installed_binary(layout)?;
    let script = shell_quote(&script.to_string_lossy());
    let binary = shell_quote(&binary.to_string_lossy());
    let string = |value: String| CstInputValue::String(value);

    Ok(CstInputValue::Object(vec![
        ("exec".to_owned(), string(format!("{script} waybar"))),
        ("exec-if".to_owned(), string(format!("test -x {script}"))),
        ("interval".to_owned(), CstInputValue::Number("2".to_owned())),
        ("return-type".to_owned(), string("json".to_owned())),
        ("escape".to_owned(), CstInputValue::Bool(true)),
        ("tooltip".to_owned(), CstInputValue::Bool(true)),
        ("on-click".to_owned(), string(format!("{binary} playpause"))),
        (
            "on-click-middle".to_owned(),
            string(format!("{binary} stop")),
        ),
        (
            "on-click-right".to_owned(),
            string(format!("{binary} panel")),
        ),
        (
            "on-scroll-up".to_owned(),
            string(format!("{binary} volume-up")),
        ),
        (
            "on-scroll-down".to_owned(),
            string(format!("{binary} volume-down")),
        ),
    ]))
}

/// Команды кликов запускают тот же бинарник, который записан в квитанции.
fn installed_binary(layout: &Layout) -> Result<PathBuf> {
    let inspection = inspect(layout, Target::Waybar)?;
    if inspection.binary.is_empty() {
        current_binary()
    } else {
        Ok(PathBuf::from(inspection.binary))
    }
}

/// Запись отменяется, если сериализованное дерево не разбирается или модуль
/// в нём не появился: старый файл остаётся нетронутым.
fn verify(text: &str, path: &Path) -> Result<()> {
    let root = parse(text, path).context("повторный разбор после правки не удался")?;
    let object = root
        .object_value()
        .ok_or_else(|| anyhow!("после правки корень перестал быть объектом"))?;
    let array = object
        .array_value(SIDE_KEY)
        .ok_or_else(|| anyhow!("после правки пропал массив {SIDE_KEY}"))?;
    if !array_has_module(&array) {
        bail!("после правки {SIDE_KEY} не содержит {MODULE}");
    }
    if object.get(MODULE).is_none() {
        bail!("после правки нет объекта {MODULE}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::integration::Layout;
    use tempfile::TempDir;

    const ORIGINAL: &str = r#"// Waybar — мой конфиг
{
  // правый блок
  "modules-right": [
    "clock", // время
  ],
  "layer": "top",
}
"#;

    fn test_layout(dir: &TempDir) -> Layout {
        Layout {
            data_home: dir.path().join("data"),
            config_home: dir.path().join("config"),
            home: dir.path().join("home"),
            root_override: Some(dir.path().join("root")),
        }
    }

    fn write_config(layout: &Layout, text: &str) -> PathBuf {
        let dir = layout.config_home.join(CONFIG_DIR);
        fs::create_dir_all(&dir).expect("config dir");
        let path = dir.join("config.jsonc");
        fs::write(&path, text).expect("config");
        path
    }

    fn parse_enabled(text: &str) -> CstRootNode {
        CstRootNode::parse(text, &ParseOptions::default()).expect("parse")
    }

    #[test]
    fn injects_module_and_preserves_comments_and_layout() {
        let dir = TempDir::new().expect("temp dir");
        let layout = test_layout(&dir);
        let path = write_config(&layout, ORIGINAL);

        let report = enable(&layout).expect("enable");
        assert!(!report.already_enabled);
        assert_eq!(
            report.backup.as_deref(),
            Some(path.with_file_name("config.jsonc.orbitfm.bak").as_path())
        );

        let output = fs::read_to_string(&path).expect("output");
        assert!(output.contains("// Waybar — мой конфиг"));
        assert!(output.contains("// правый блок"));
        assert!(output.contains("// время"));
        assert!(output.contains("\"custom/orbitfm\": {"));
        assert!(output.contains("\"return-type\": \"json\""));

        let root = parse_enabled(&output);
        let object = root.object_value().expect("object");
        let array = object.array_value("modules-right").expect("array");
        assert_eq!(
            array
                .elements()
                .iter()
                .filter(|node| {
                    node.as_string_lit()
                        .and_then(|literal| literal.decoded_value().ok())
                        .is_some_and(|value| value == MODULE)
                })
                .count(),
            1,
            "модуль добавлен ровно один раз"
        );

        let backup = path.with_file_name("config.jsonc.orbitfm.bak");
        assert_eq!(fs::read_to_string(backup).expect("backup"), ORIGINAL);
        assert!(status(&layout).enabled);
    }

    #[test]
    fn second_enable_is_idempotent_and_keeps_first_backup() {
        let dir = TempDir::new().expect("temp dir");
        let layout = test_layout(&dir);
        let path = write_config(&layout, ORIGINAL);
        enable(&layout).expect("first enable");

        let second = enable(&layout).expect("second enable");
        assert!(second.already_enabled);
        assert!(second.backup.is_none());

        // Пользователь заменил конфиг: бэкап первого раза остаётся оригиналом.
        fs::write(&path, "{\n  \"modules-right\": [\"clock\"]\n}\n").expect("replace");
        let third = enable(&layout).expect("third enable");
        assert!(!third.already_enabled);
        assert!(third.backup.is_none(), "бэкап уже существует");
        let backup = path.with_file_name("config.jsonc.orbitfm.bak");
        assert_eq!(fs::read_to_string(backup).expect("backup"), ORIGINAL);
    }

    #[test]
    fn malformed_config_is_left_untouched() {
        let dir = TempDir::new().expect("temp dir");
        let layout = test_layout(&dir);
        let broken = "{\n  \"modules-right\": [\n}\n";
        let path = write_config(&layout, broken);

        let err = enable(&layout).expect_err("must fail");
        assert!(format!("{err:#}").contains("разобрать"));
        assert_eq!(fs::read_to_string(&path).expect("config"), broken);
        assert!(!path.with_file_name("config.jsonc.orbitfm.bak").exists());
    }

    #[test]
    fn missing_modules_array_is_an_error() {
        let dir = TempDir::new().expect("temp dir");
        let layout = test_layout(&dir);
        let text = "{\n  \"layer\": \"top\"\n}\n";
        let path = write_config(&layout, text);

        let err = enable(&layout).expect_err("must fail");
        assert!(format!("{err:#}").contains("modules-right"));
        assert_eq!(fs::read_to_string(&path).expect("config"), text);
    }

    #[test]
    fn root_array_is_rejected() {
        let dir = TempDir::new().expect("temp dir");
        let layout = test_layout(&dir);
        write_config(&layout, "[]\n");
        let err = enable(&layout).expect_err("must fail");
        assert!(format!("{err:#}").contains("не JSON-объект"));
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_config_is_not_replaced() {
        use std::os::unix::fs::symlink;

        let dir = TempDir::new().expect("temp dir");
        let layout = test_layout(&dir);
        let target = dir.path().join("dotfiles-config.jsonc");
        fs::write(&target, ORIGINAL).expect("target");
        let config_dir = layout.config_home.join(CONFIG_DIR);
        fs::create_dir_all(&config_dir).expect("config dir");
        let link = config_dir.join("config.jsonc");
        symlink(&target, &link).expect("symlink");

        let err = enable(&layout).expect_err("must refuse");
        assert!(format!("{err:#}").contains("символическая"));
        assert_eq!(fs::read_to_string(&target).expect("target"), ORIGINAL);
    }

    #[test]
    fn finds_config_without_extension() {
        let dir = TempDir::new().expect("temp dir");
        let layout = test_layout(&dir);
        let config_dir = layout.config_home.join(CONFIG_DIR);
        fs::create_dir_all(&config_dir).expect("config dir");
        let path = config_dir.join("config");
        fs::write(&path, "{\n  \"modules-right\": []\n}\n").expect("config");

        enable(&layout).expect("enable");
        let output = fs::read_to_string(&path).expect("output");
        assert!(output.contains("custom/orbitfm"));
        assert!(!config_dir.join("config.jsonc").exists());
    }
}
