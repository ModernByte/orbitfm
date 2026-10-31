//! Установщик интеграций оболочек и баров (ADR-0037, п. 5).
//!
//! Ассеты встроены в бинарник; установка идёт в каталоги пользователя и
//! сопровождается квитанцией. Чужие конфиги установщик не изменяет.

mod assets;

use std::collections::HashSet;
use std::env;
use std::fs;
use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};

use assets::{Asset, Escape, assets};

const RECEIPT_SCHEMA: u32 = 1;
const DATA_SUBDIR: &str = "orbitfm/integrations";
const HOST_MANIFEST_FILE: &str = "org.modernbyte.orbitfm.json";
const HOST_NAME: &str = "org.modernbyte.orbitfm";
/// Фиксированный публичный ключ расширения (ADR-0038): ID ниже выводится из
/// него и не меняется между установками. Приватный ключ в репозитории не живёт.
const CHROME_EXTENSION_ID: &str = "lfhmfgjblkebbalfdlbmnalghhjplfbg";
const FIREFOX_EXTENSION_ID: &str = "orbitfm@modernbyte";
const TARGET_LIST: &str = "plasma6, gnome, omarchy, waybar, polybar, i3blocks, swaybar, \
i3status-rust, xfce, cinnamon, mate, lxqt, chrome, firefox, all";

const HELP: &str = "\
Управление интеграциями оболочек и баров.

Использование:
  orbitfm integration status [цель]
  orbitfm integration install <цель> [--force]
  orbitfm integration uninstall <цель> [--force]

Цели:
  plasma6, gnome, omarchy, waybar, polybar, i3blocks, swaybar,
  i3status-rust, xfce, cinnamon, mate, lxqt, chrome, firefox, all

Установка идёт в каталоги пользователя и пишет квитанцию
$XDG_DATA_HOME/orbitfm/integrations/<цель>.json. Конфиги оболочек
не изменяются: команда печатает готовый сниппет и путь скрипта.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Target {
    Plasma6,
    Gnome,
    Omarchy,
    Waybar,
    Polybar,
    I3blocks,
    Swaybar,
    I3statusRust,
    Xfce,
    Cinnamon,
    Mate,
    Lxqt,
    Chrome,
    Firefox,
}

impl Target {
    const ALL: [Self; 14] = [
        Self::Plasma6,
        Self::Gnome,
        Self::Omarchy,
        Self::Waybar,
        Self::Polybar,
        Self::I3blocks,
        Self::Swaybar,
        Self::I3statusRust,
        Self::Xfce,
        Self::Cinnamon,
        Self::Mate,
        Self::Lxqt,
        Self::Chrome,
        Self::Firefox,
    ];

    fn id(self) -> &'static str {
        match self {
            Self::Plasma6 => "plasma6",
            Self::Gnome => "gnome",
            Self::Omarchy => "omarchy",
            Self::Waybar => "waybar",
            Self::Polybar => "polybar",
            Self::I3blocks => "i3blocks",
            Self::Swaybar => "swaybar",
            Self::I3statusRust => "i3status-rust",
            Self::Xfce => "xfce",
            Self::Cinnamon => "cinnamon",
            Self::Mate => "mate",
            Self::Lxqt => "lxqt",
            Self::Chrome => "chrome",
            Self::Firefox => "firefox",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "plasma6" | "plasma" | "kde" => Some(Self::Plasma6),
            "gnome" | "gnome-shell" => Some(Self::Gnome),
            "omarchy" | "quickshell" => Some(Self::Omarchy),
            "waybar" => Some(Self::Waybar),
            "polybar" => Some(Self::Polybar),
            "i3blocks" => Some(Self::I3blocks),
            "swaybar" => Some(Self::Swaybar),
            "i3status-rust" | "i3status_rust" | "i3statusrust" => Some(Self::I3statusRust),
            "xfce" => Some(Self::Xfce),
            "cinnamon" => Some(Self::Cinnamon),
            "mate" => Some(Self::Mate),
            "lxqt" => Some(Self::Lxqt),
            "chrome" | "chromium" | "google-chrome" => Some(Self::Chrome),
            "firefox" | "mozilla" => Some(Self::Firefox),
            _ => None,
        }
    }

    fn is_browser(self) -> bool {
        matches!(self, Self::Chrome | Self::Firefox)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Scope {
    One(Target),
    All,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum Command {
    Help,
    Status(Scope),
    Install { scope: Scope, force: bool },
    Uninstall { scope: Scope, force: bool },
}

pub(crate) fn parse(args: &[String]) -> Result<Command> {
    let Some(action) = args.first().map(String::as_str) else {
        return Ok(Command::Help);
    };

    match action {
        "-h" | "--help" | "help" => {
            if args.len() > 1 {
                bail!("лишние аргументы после {action}");
            }
            Ok(Command::Help)
        }
        "status" => {
            if args.len() > 2 {
                bail!("лишние аргументы: status принимает не больше одной цели");
            }
            let scope = match args.get(1) {
                Some(value) => parse_scope(value)?,
                None => Scope::All,
            };
            Ok(Command::Status(scope))
        }
        "install" | "uninstall" => {
            let mut force = false;
            let mut target = None;
            for value in &args[1..] {
                if value == "--force" {
                    force = true;
                } else if value.starts_with('-') {
                    bail!("неизвестный флаг: {value}");
                } else if target.is_some() {
                    bail!("{action} принимает ровно одну цель");
                } else {
                    target = Some(parse_scope(value)?);
                }
            }
            let scope = target.ok_or_else(|| anyhow!("укажите цель: {TARGET_LIST}\n\n{HELP}"))?;
            if action == "install" {
                Ok(Command::Install { scope, force })
            } else {
                Ok(Command::Uninstall { scope, force })
            }
        }
        other => bail!("неизвестное действие integration: {other}\n\n{HELP}"),
    }
}

fn parse_scope(value: &str) -> Result<Scope> {
    if value == "all" {
        return Ok(Scope::All);
    }
    Target::parse(value)
        .map(Scope::One)
        .ok_or_else(|| anyhow!("неизвестная цель интеграции: {value}\nЦели: {TARGET_LIST}"))
}

pub(crate) fn run(command: Command) -> Result<()> {
    match command {
        Command::Help => {
            println!("{HELP}");
            Ok(())
        }
        Command::Status(scope) => run_status(scope),
        Command::Install { scope, force } => run_install(scope, force),
        Command::Uninstall { scope, force } => run_uninstall(scope, force),
    }
}

fn targets(scope: Scope) -> Vec<Target> {
    match scope {
        Scope::One(target) => vec![target],
        Scope::All => Target::ALL.to_vec(),
    }
}

fn run_status(scope: Scope) -> Result<()> {
    let layout = Layout::from_env()?;
    let mut failed = false;

    for target in targets(scope) {
        match inspect(&layout, target) {
            Ok(inspection) => {
                print_inspection(&inspection);
                if matches!(scope, Scope::One(_))
                    && inspection.state != InstalledState::NotInstalled
                {
                    println!("  каталог: {}", inspection.root.display());
                    println!("  квитанция: {}", inspection.receipt_path.display());
                    println!("  бинарник: {}", inspection.binary);
                }
            }
            Err(err) => {
                println!("{:<14} ошибка: {err:#}", target.id());
                failed = true;
            }
        }
    }

    if failed {
        bail!("не удалось прочитать часть квитанций");
    }
    Ok(())
}

fn run_install(scope: Scope, force: bool) -> Result<()> {
    let layout = Layout::from_env()?;
    let binary = current_binary()?;
    let mut failed = Vec::new();

    for target in targets(scope) {
        match install_one(&layout, target, force, &binary) {
            Ok(report) => {
                print_install(&layout, &report);
            }
            Err(err) => {
                eprintln!("Не удалось установить {}: {err:#}", target.id());
                failed.push(target.id());
            }
        }
    }

    if failed.is_empty() {
        Ok(())
    } else {
        bail!("не установлены цели: {}", failed.join(", "))
    }
}

fn run_uninstall(scope: Scope, force: bool) -> Result<()> {
    let layout = Layout::from_env()?;
    let mut failed = Vec::new();

    for target in targets(scope) {
        match uninstall_one(&layout, target, force) {
            Ok(report) if report.not_installed => {
                println!("{:<14} не установлено", target.id());
            }
            Ok(report) => {
                println!(
                    "{:<14} удалено файлов: {} (каталог {})",
                    target.id(),
                    report.removed,
                    report.root.display()
                );
            }
            Err(err) => {
                eprintln!("Не удалось удалить {}: {err:#}", target.id());
                failed.push(target.id());
            }
        }
    }

    if failed.is_empty() {
        Ok(())
    } else {
        bail!("не удалены цели: {}", failed.join(", "))
    }
}

fn current_binary() -> Result<PathBuf> {
    // Внутри AppImage `current_exe` указывает на временный mount, который
    // исчезает после выхода; браузерный хост должен запускать сам образ.
    if let Some(appimage) = env::var_os("APPIMAGE") {
        let path = PathBuf::from(appimage);
        if path.is_absolute() {
            return Ok(path);
        }
    }
    env::current_exe().context("не удалось определить путь к исполняемому файлу orbitfm")
}

#[derive(Clone, Copy)]
struct ChromiumBrowser {
    id: &'static str,
    config_rel: &'static str,
    binaries: &'static [&'static str],
}

const CHROMIUM_BROWSERS: [ChromiumBrowser; 5] = [
    ChromiumBrowser {
        id: "google-chrome",
        config_rel: "google-chrome",
        binaries: &["google-chrome", "google-chrome-stable"],
    },
    ChromiumBrowser {
        id: "chromium",
        config_rel: "chromium",
        binaries: &["chromium", "chromium-browser"],
    },
    ChromiumBrowser {
        id: "brave",
        config_rel: "BraveSoftware/Brave-Browser",
        binaries: &["brave-browser", "brave"],
    },
    ChromiumBrowser {
        id: "edge",
        config_rel: "microsoft-edge",
        binaries: &["microsoft-edge", "microsoft-edge-stable"],
    },
    ChromiumBrowser {
        id: "vivaldi",
        config_rel: "vivaldi",
        binaries: &["vivaldi", "vivaldi-stable"],
    },
];

struct Layout {
    data_home: PathBuf,
    config_home: PathBuf,
    home: PathBuf,
    root_override: Option<PathBuf>,
}

impl Layout {
    fn from_env() -> Result<Self> {
        if let Some(root) = env::var_os("ORBITFM_INTEGRATION_ROOT") {
            let root = PathBuf::from(root);
            if !root.is_absolute() {
                bail!(
                    "ORBITFM_INTEGRATION_ROOT должен быть абсолютным путём: {}",
                    root.display()
                );
            }
            return Ok(Self {
                data_home: root.clone(),
                config_home: root.clone(),
                home: root.clone(),
                root_override: Some(root),
            });
        }

        let home = env::var_os("HOME")
            .map(PathBuf::from)
            .context("переменная HOME не задана: некуда устанавливать интеграции")?;
        Ok(Self {
            data_home: xdg_dir("XDG_DATA_HOME", &home.join(".local/share")),
            config_home: xdg_dir("XDG_CONFIG_HOME", &home.join(".config")),
            home,
            root_override: None,
        })
    }

    fn root(&self, target: Target) -> PathBuf {
        if let Some(root) = &self.root_override {
            return root.join("targets").join(target.id());
        }
        match target {
            Target::Plasma6 => self
                .data_home
                .join("plasma/plasmoids/org.modernbyte.orbitfm"),
            Target::Gnome => self
                .data_home
                .join("gnome-shell/extensions/orbitfm@modernbyte"),
            Target::Omarchy => self
                .config_home
                .join("omarchy/plugins/io.github.modernbyte.orbitfm"),
            _ => self.data_home.join(DATA_SUBDIR).join(target.id()),
        }
    }

    fn receipt(&self, target: Target) -> PathBuf {
        if let Some(root) = &self.root_override {
            return root.join("receipts").join(format!("{}.json", target.id()));
        }
        self.data_home
            .join(DATA_SUBDIR)
            .join(format!("{}.json", target.id()))
    }

    /// Все каталоги, где цель вправе держать файлы. Квитанция проверяется по
    /// этому списку: native-манифесты браузеров лежат вне каталога расширения.
    fn allowed_roots(&self, target: Target) -> Vec<PathBuf> {
        let mut roots = vec![self.root(target)];
        roots.extend(self.manifest_dirs(target));
        roots
    }

    /// Каталоги native messaging для цели — все возможные, включая браузеры,
    /// которых сейчас нет: это нужно для чтения старых квитанций.
    fn manifest_dirs(&self, target: Target) -> Vec<PathBuf> {
        let root = self.root(target);
        match target {
            Target::Chrome => CHROMIUM_BROWSERS
                .iter()
                .map(|browser| {
                    if self.root_override.is_some() {
                        root.join("browsers")
                            .join(browser.id)
                            .join("NativeMessagingHosts")
                    } else {
                        self.config_home
                            .join(browser.config_rel)
                            .join("NativeMessagingHosts")
                    }
                })
                .collect(),
            Target::Firefox => vec![if self.root_override.is_some() {
                root.join("browsers/firefox/NativeMessagingHosts")
            } else {
                self.home.join(".mozilla/native-messaging-hosts")
            }],
            _ => Vec::new(),
        }
    }

    /// Каталоги, куда манифесты ставятся сейчас: только для обнаруженных
    /// браузеров. В тестовом root ставятся все, чтобы прогон был детерминирован.
    fn install_manifest_dirs(&self, target: Target) -> Vec<PathBuf> {
        if self.root_override.is_some() {
            return self.manifest_dirs(target);
        }
        match target {
            Target::Chrome => {
                let dirs = self.manifest_dirs(target);
                CHROMIUM_BROWSERS
                    .iter()
                    .zip(dirs)
                    .filter(|(browser, _)| self.chromium_present(browser))
                    .map(|(_, dir)| dir)
                    .collect()
            }
            Target::Firefox => {
                if self.firefox_present() {
                    self.manifest_dirs(target)
                } else {
                    Vec::new()
                }
            }
            _ => Vec::new(),
        }
    }

    fn chromium_present(&self, browser: &ChromiumBrowser) -> bool {
        self.config_home.join(browser.config_rel).is_dir()
            || browser
                .binaries
                .iter()
                .any(|binary| command_in_path(binary))
    }

    fn firefox_present(&self) -> bool {
        self.home.join(".mozilla").is_dir() || command_in_path("firefox")
    }
}

fn command_in_path(name: &str) -> bool {
    env::var_os("PATH")
        .is_some_and(|paths| env::split_paths(&paths).any(|dir| dir.join(name).is_file()))
}

fn xdg_dir(variable: &str, fallback: &Path) -> PathBuf {
    env::var_os(variable)
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| fallback.to_owned())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum FileKind {
    Asset,
    Generated,
}

#[derive(Serialize, Deserialize)]
struct Receipt {
    schema: u32,
    target: String,
    version: String,
    binary: String,
    files: Vec<ReceiptFile>,
}

#[derive(Serialize, Deserialize)]
struct ReceiptFile {
    path: String,
    kind: FileKind,
    hash: String,
}

struct Rendered {
    absolute: PathBuf,
    content: String,
    executable: bool,
    kind: FileKind,
}

fn render_assets(layout: &Layout, target: Target, binary: &Path) -> Result<Vec<Rendered>> {
    let root = layout.root(target);
    let script = root.join("orbitfm-bar");
    let binary = binary
        .to_str()
        .ok_or_else(|| anyhow!("путь к orbitfm не в UTF-8: {}", binary.display()))?;
    let script = script
        .to_str()
        .ok_or_else(|| anyhow!("путь установки не в UTF-8: {}", script.display()))?;

    let mut rendered = Vec::with_capacity(assets(target).len() + 5);
    for asset in assets(target) {
        let relative = Path::new(asset.path);
        ensure_relative(relative)?;
        let absolute = root.join(relative);
        if absolute.to_str().is_none() {
            bail!("путь установки не в UTF-8: {}", absolute.display());
        }
        rendered.push(Rendered {
            absolute,
            content: render_content(asset, binary, script),
            executable: asset.executable,
            kind: if asset.generated {
                FileKind::Generated
            } else {
                FileKind::Asset
            },
        });
    }

    if target.is_browser() {
        let wrapper = root.join("orbitfm-native-host");
        let content = host_manifest(target, &wrapper)?;
        for dir in layout.install_manifest_dirs(target) {
            let absolute = dir.join(HOST_MANIFEST_FILE);
            if absolute.to_str().is_none() {
                bail!("путь установки не в UTF-8: {}", absolute.display());
            }
            rendered.push(Rendered {
                absolute,
                content: content.clone(),
                executable: false,
                kind: FileKind::Generated,
            });
        }
    }
    Ok(rendered)
}

fn host_manifest(target: Target, wrapper: &Path) -> Result<String> {
    let path = wrapper
        .to_str()
        .ok_or_else(|| anyhow!("путь к native host не в UTF-8: {}", wrapper.display()))?;
    let description = "OrbitFM — 3D internet radio";
    let value = match target {
        Target::Chrome => serde_json::json!({
            "name": HOST_NAME,
            "description": description,
            "path": path,
            "type": "stdio",
            "allowed_origins": [format!("chrome-extension://{CHROME_EXTENSION_ID}/")],
        }),
        Target::Firefox => serde_json::json!({
            "name": HOST_NAME,
            "description": description,
            "path": path,
            "type": "stdio",
            "allowed_extensions": [FIREFOX_EXTENSION_ID],
        }),
        _ => bail!("native-манифест не применим к цели {}", target.id()),
    };
    serde_json::to_string_pretty(&value).context("не удалось сериализовать native-манифест")
}

/// Chrome и Firefox принимают в манифесте расширения только числовые
/// компоненты, поэтому предрелизный суффикс (`-rc.2`) отбрасывается.
fn browser_version() -> &'static str {
    let version = env!("CARGO_PKG_VERSION");
    version.split('-').next().unwrap_or(version)
}

fn render_content(asset: &Asset, binary: &str, script: &str) -> String {
    let content = asset
        .content
        .replace("@VERSION@", env!("CARGO_PKG_VERSION"))
        .replace("@BROWSER_VERSION@", browser_version());
    match asset.escape {
        Escape::None => content,
        Escape::Shell => {
            let binary = shell_quote(binary);
            let script = shell_quote(script);
            content
                .replace("@ORBITFM@", &binary)
                .replace("@BAR_SCRIPT@", &script)
        }
        Escape::Quote => {
            let binary = quote_escape(binary);
            let script = quote_escape(script);
            content
                .replace("@ORBITFM@", &binary)
                .replace("@BAR_SCRIPT@", &script)
        }
    }
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn quote_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

#[derive(Debug)]
struct InstallReport {
    target: Target,
    root: PathBuf,
    receipt_path: PathBuf,
    binary: PathBuf,
    files: Vec<PathBuf>,
}

fn install_one(
    layout: &Layout,
    target: Target,
    force: bool,
    binary: &Path,
) -> Result<InstallReport> {
    ensure_applicable(target)?;
    let root = layout.root(target);
    let roots = layout.allowed_roots(target);
    let receipt_path = layout.receipt(target);
    let old = read_receipt_opt(&receipt_path, target, &roots)?;
    let rendered = render_assets(layout, target, binary)?;
    let new_paths: HashSet<PathBuf> = rendered
        .iter()
        .map(|asset| asset.absolute.clone())
        .collect();

    for asset in &rendered {
        check_writable(asset, old.as_ref(), force)?;
    }
    let stale = stale_files(old.as_ref(), &new_paths, &roots, force)?;

    fs::create_dir_all(&root).with_context(|| format!("не удалось создать {}", root.display()))?;
    for asset in &rendered {
        write_asset(asset)?;
    }
    for path in &stale {
        remove_file(path)?;
        prune_for_roots(path.parent(), &roots);
    }

    let mut files = Vec::with_capacity(rendered.len());
    for asset in &rendered {
        let path = asset
            .absolute
            .to_str()
            .ok_or_else(|| anyhow!("путь не в UTF-8: {}", asset.absolute.display()))?;
        files.push(ReceiptFile {
            path: path.to_owned(),
            kind: asset.kind,
            hash: hash_bytes(asset.content.as_bytes()),
        });
    }
    let receipt = Receipt {
        schema: RECEIPT_SCHEMA,
        target: target.id().to_owned(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
        binary: binary
            .to_str()
            .ok_or_else(|| anyhow!("путь к orbitfm не в UTF-8: {}", binary.display()))?
            .to_owned(),
        files,
    };
    write_receipt(&receipt_path, &receipt)?;

    Ok(InstallReport {
        target,
        root,
        receipt_path,
        binary: binary.to_owned(),
        files: rendered.into_iter().map(|asset| asset.absolute).collect(),
    })
}

fn check_writable(asset: &Rendered, old: Option<&Receipt>, force: bool) -> Result<()> {
    let Some(meta) = metadata_opt(&asset.absolute)? else {
        return Ok(());
    };
    if meta.file_type().is_symlink() {
        if force {
            return Ok(());
        }
        bail!(
            "{} — символическая ссылка; удалите её или используйте --force",
            asset.absolute.display()
        );
    }
    if !meta.is_file() {
        bail!(
            "{} существует и не является обычным файлом",
            asset.absolute.display()
        );
    }

    let bytes = fs::read(&asset.absolute)
        .with_context(|| format!("не удалось прочитать {}", asset.absolute.display()))?;
    let hash = hash_bytes(&bytes);
    let owned = old.and_then(|receipt| {
        receipt
            .files
            .iter()
            .find(|file| Path::new(&file.path) == asset.absolute.as_path())
    });

    if let Some(file) = owned {
        if file.hash != hash && !force {
            bail!(
                "файл изменён пользователем: {}\nПерезапись — с --force",
                asset.absolute.display()
            );
        }
    } else if !force {
        bail!(
            "файл уже существует и не принадлежит OrbitFM: {}\nПерезапись — с --force",
            asset.absolute.display()
        );
    }
    Ok(())
}

fn stale_files(
    old: Option<&Receipt>,
    new_paths: &HashSet<PathBuf>,
    roots: &[PathBuf],
    force: bool,
) -> Result<Vec<PathBuf>> {
    let Some(old) = old else {
        return Ok(Vec::new());
    };

    let mut stale = Vec::new();
    for file in &old.files {
        let path = PathBuf::from(&file.path);
        ensure_within_roots(roots, &path)?;
        if new_paths.contains(&path) {
            continue;
        }
        match metadata_opt(&path)? {
            None => {}
            Some(meta) if meta.file_type().is_symlink() => {
                if !force {
                    bail!(
                        "старый файл {} стал символической ссылкой; удаление — с --force",
                        path.display()
                    );
                }
                stale.push(path);
            }
            Some(meta) if meta.is_file() => {
                let bytes = fs::read(&path)
                    .with_context(|| format!("не удалось прочитать {}", path.display()))?;
                if hash_bytes(&bytes) != file.hash && !force {
                    bail!(
                        "старый файл изменён пользователем: {}; удаление — с --force",
                        path.display()
                    );
                }
                stale.push(path);
            }
            Some(_) => bail!(
                "{} существует и не является обычным файлом; удалите вручную",
                path.display()
            ),
        }
    }
    Ok(stale)
}

fn write_asset(asset: &Rendered) -> Result<()> {
    if let Some(parent) = asset.absolute.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("не удалось создать {}", parent.display()))?;
    }
    if let Some(meta) = metadata_opt(&asset.absolute)?
        && meta.file_type().is_symlink()
    {
        fs::remove_file(&asset.absolute)
            .with_context(|| format!("не удалось удалить {}", asset.absolute.display()))?;
    }
    fs::write(&asset.absolute, asset.content.as_bytes())
        .with_context(|| format!("не удалось записать {}", asset.absolute.display()))?;
    set_mode(
        &asset.absolute,
        if asset.executable { 0o755 } else { 0o644 },
    )
}

fn write_receipt(path: &Path, receipt: &Receipt) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("не удалось создать {}", parent.display()))?;
    }
    let temp = path.with_extension("json.tmp");
    let json =
        serde_json::to_string_pretty(receipt).context("не удалось сериализовать квитанцию")?;
    fs::write(&temp, json.as_bytes())
        .with_context(|| format!("не удалось записать {}", temp.display()))?;
    set_mode(&temp, 0o644)?;
    fs::rename(&temp, path).with_context(|| {
        format!(
            "не удалось переименовать {} в {}",
            temp.display(),
            path.display()
        )
    })
}

fn read_receipt_opt(path: &Path, target: Target, roots: &[PathBuf]) -> Result<Option<Receipt>> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(None),
        Err(err) => {
            return Err(err).with_context(|| format!("не удалось прочитать {}", path.display()));
        }
    };
    let receipt: Receipt = serde_json::from_slice(&bytes)
        .with_context(|| format!("не удалось разобрать квитанцию {}", path.display()))?;
    if receipt.schema != RECEIPT_SCHEMA {
        bail!(
            "квитанция {} имеет схему {}; поддерживается {}",
            path.display(),
            receipt.schema,
            RECEIPT_SCHEMA
        );
    }
    if receipt.target != target.id() {
        bail!(
            "квитанция {} относится к цели {}, а не {}",
            path.display(),
            receipt.target,
            target.id()
        );
    }
    for file in &receipt.files {
        ensure_within_roots(roots, Path::new(&file.path))?;
    }
    Ok(Some(receipt))
}

#[derive(Debug)]
struct UninstallReport {
    root: PathBuf,
    removed: usize,
    not_installed: bool,
}

fn uninstall_one(layout: &Layout, target: Target, force: bool) -> Result<UninstallReport> {
    ensure_applicable(target)?;
    let root = layout.root(target);
    let roots = layout.allowed_roots(target);
    let receipt_path = layout.receipt(target);
    let Some(receipt) = read_receipt_opt(&receipt_path, target, &roots)? else {
        return Ok(UninstallReport {
            root,
            removed: 0,
            not_installed: true,
        });
    };

    let mut removable = Vec::new();
    let mut blocked = Vec::new();
    for file in &receipt.files {
        let path = PathBuf::from(&file.path);
        ensure_within_roots(&roots, &path)?;
        match metadata_opt(&path)? {
            None => {}
            Some(meta) if meta.is_dir() => {
                bail!(
                    "{} — каталог; файлы OrbitFM не удалены, разберитесь вручную",
                    path.display()
                );
            }
            Some(meta) if meta.file_type().is_symlink() => {
                if force {
                    removable.push(path);
                } else {
                    blocked.push(path);
                }
            }
            Some(_) => {
                let bytes = fs::read(&path)
                    .with_context(|| format!("не удалось прочитать {}", path.display()))?;
                if hash_bytes(&bytes) == file.hash || force {
                    removable.push(path);
                } else {
                    blocked.push(path);
                }
            }
        }
    }

    if !blocked.is_empty() && !force {
        bail!(
            "файлы изменены пользователем: {}\nУдаление — с --force",
            join_paths(&blocked)
        );
    }

    let mut removed = 0;
    for path in &removable {
        remove_file(path)?;
        removed += 1;
        prune_for_roots(path.parent(), &roots);
    }
    match fs::remove_file(&receipt_path) {
        Ok(()) => {}
        Err(err) if err.kind() == ErrorKind::NotFound => {}
        Err(err) => {
            return Err(err)
                .with_context(|| format!("не удалось удалить {}", receipt_path.display()));
        }
    }
    prune_empty_dirs(Some(&root), &root);

    Ok(UninstallReport {
        root,
        removed,
        not_installed: false,
    })
}

fn remove_file(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err).with_context(|| format!("не удалось удалить {}", path.display())),
    }
}

fn prune_empty_dirs(mut dir: Option<&Path>, root: &Path) {
    while let Some(current) = dir {
        if !current.starts_with(root) || fs::remove_dir(current).is_err() {
            break;
        }
        dir = current.parent();
    }
}

/// Чистит опустевший каталог до ближайшего разрешённого корня цели.
fn prune_for_roots(dir: Option<&Path>, roots: &[PathBuf]) {
    let Some(dir) = dir else {
        return;
    };
    let Some(root) = roots
        .iter()
        .filter(|root| dir.starts_with(root))
        .max_by_key(|root| root.as_os_str().len())
    else {
        return;
    };
    prune_empty_dirs(Some(dir), root);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InstalledState {
    NotInstalled,
    Installed,
    Changed,
    Missing,
}

#[derive(Debug)]
struct FileInspection {
    path: PathBuf,
    state: FileState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FileState {
    Present,
    Missing,
    Modified,
}

#[derive(Debug)]
struct Inspection {
    target: Target,
    root: PathBuf,
    receipt_path: PathBuf,
    version: String,
    binary: String,
    files: Vec<FileInspection>,
    state: InstalledState,
}

fn inspect(layout: &Layout, target: Target) -> Result<Inspection> {
    let root = layout.root(target);
    let roots = layout.allowed_roots(target);
    let receipt_path = layout.receipt(target);
    let Some(receipt) = read_receipt_opt(&receipt_path, target, &roots)? else {
        return Ok(Inspection {
            target,
            root,
            receipt_path,
            version: String::new(),
            binary: String::new(),
            files: Vec::new(),
            state: InstalledState::NotInstalled,
        });
    };

    let mut files = Vec::with_capacity(receipt.files.len());
    for file in &receipt.files {
        let path = PathBuf::from(&file.path);
        ensure_within_roots(&roots, &path)?;
        let state = match metadata_opt(&path)? {
            None => FileState::Missing,
            Some(meta) if !meta.is_file() || meta.file_type().is_symlink() => FileState::Modified,
            Some(_) => {
                let bytes = fs::read(&path)
                    .with_context(|| format!("не удалось прочитать {}", path.display()))?;
                if hash_bytes(&bytes) == file.hash {
                    FileState::Present
                } else {
                    FileState::Modified
                }
            }
        };
        files.push(FileInspection { path, state });
    }

    let state = if files.iter().any(|file| file.state == FileState::Missing) {
        InstalledState::Missing
    } else if files.iter().any(|file| file.state == FileState::Modified) {
        InstalledState::Changed
    } else {
        InstalledState::Installed
    };

    Ok(Inspection {
        target,
        root,
        receipt_path,
        version: receipt.version,
        binary: receipt.binary,
        files,
        state,
    })
}

fn print_inspection(inspection: &Inspection) {
    match inspection.state {
        InstalledState::NotInstalled => {
            println!("{:<14} не установлено", inspection.target.id());
        }
        InstalledState::Installed => {
            println!(
                "{:<14} установлено {} ({} файлов)",
                inspection.target.id(),
                inspection.version,
                inspection.files.len()
            );
        }
        InstalledState::Changed => {
            println!(
                "{:<14} изменено пользователем: {}",
                inspection.target.id(),
                join_paths(&paths_with(inspection, FileState::Modified))
            );
        }
        InstalledState::Missing => {
            println!(
                "{:<14} отсутствуют файлы: {}",
                inspection.target.id(),
                join_paths(&paths_with(inspection, FileState::Missing))
            );
        }
    }
}

fn paths_with(inspection: &Inspection, wanted: FileState) -> Vec<PathBuf> {
    inspection
        .files
        .iter()
        .filter(|file| file.state == wanted)
        .map(|file| file.path.clone())
        .collect()
}

fn print_install(layout: &Layout, report: &InstallReport) {
    println!(
        "Установлено: {} ({} файлов)",
        report.target.id(),
        report.files.len()
    );
    println!("  каталог: {}", report.root.display());
    println!("  квитанция: {}", report.receipt_path.display());
    if let Err(err) = print_instructions(layout, report) {
        println!("  не удалось напечатать шаги подключения: {err:#}");
    }
    println!();
}

fn print_instructions(layout: &Layout, report: &InstallReport) -> Result<()> {
    let target = report.target;
    println!();
    match target {
        Target::Plasma6 => {
            println!("Подключение Plasma 6:");
            println!("  1. Перезапустите Plasma или выполните kbuildsycoca6 --noincremental.");
            println!("  2. ПКМ по панели → «Добавить виджеты» → OrbitFM.");
        }
        Target::Gnome => {
            println!("Подключение GNOME Shell:");
            println!("  gnome-extensions enable orbitfm@modernbyte");
            println!(
                "  На Wayland после установки может понадобиться выйти из сеанса и войти снова."
            );
        }
        Target::Omarchy => {
            println!("Подключение Omarchy/Quickshell:");
            println!("  omarchy-shell shell rescanPlugins");
            println!("  Включите виджет OrbitFM в конфигурации бара Omarchy.");
        }
        Target::Chrome => {
            println!("Подключение Chrome/Chromium/Brave/Edge/Vivaldi:");
            println!("  1. Откройте chrome://extensions и включите «Режим разработчика».");
            println!(
                "  2. «Загрузить распакованное расширение» → {}",
                report.root.join("extension").display()
            );
            print_browser_manifests(layout, target);
        }
        Target::Firefox => {
            println!("Подключение Firefox:");
            println!("  1. Откройте about:debugging#/runtime/this-firefox.");
            println!(
                "  2. «Загрузить временное дополнение» → {}",
                report.root.join("extension/manifest.json").display()
            );
            println!(
                "  Временное дополнение живёт до перезапуска Firefox; \
                 постоянная установка требует подписи."
            );
            print_browser_manifests(layout, target);
        }
        _ => {
            let snippet = snippet_path(target, &report.root);
            println!("Сниппет: {}", snippet.display());
            println!("---");
            let content = fs::read_to_string(&snippet)
                .with_context(|| format!("не удалось прочитать {}", snippet.display()))?;
            print!("{content}");
            if !content.ends_with('\n') {
                println!();
            }
            println!("---");
            print_bar_hint(target, layout, &report.binary, &report.root);
            println!("Конфиги оболочки установщик не меняет: скопируйте сниппет в указанный файл.");
        }
    }
    Ok(())
}

fn print_browser_manifests(layout: &Layout, target: Target) {
    let dirs = layout.install_manifest_dirs(target);
    if dirs.is_empty() {
        println!(
            "  {} не обнаружен: native-манифест не установлен.",
            browser_family(target)
        );
        println!(
            "  Установите браузер и повторите: orbitfm integration install {}",
            target.id()
        );
        return;
    }
    for dir in dirs {
        println!(
            "  Native-манифест: {}",
            dir.join(HOST_MANIFEST_FILE).display()
        );
    }
    println!("  Хост общается с уже запущенным OrbitFM; сам он окно не открывает.");
}

fn browser_family(target: Target) -> &'static str {
    match target {
        Target::Chrome => "Браузер семейства Chromium",
        Target::Firefox => "Firefox",
        _ => "Браузер",
    }
}

fn snippet_path(target: Target, root: &Path) -> PathBuf {
    let name = match target {
        Target::Waybar => "config.jsonc",
        Target::Polybar => "config.ini",
        Target::I3blocks => "config.conf",
        Target::Swaybar => "config.sway",
        Target::I3statusRust => "config.toml",
        _ => "usage.txt",
    };
    root.join(name)
}

fn print_bar_hint(target: Target, layout: &Layout, binary: &Path, root: &Path) {
    let script = root.join("orbitfm-bar");
    if layout.root_override.is_some()
        && !matches!(
            target,
            Target::Xfce | Target::Cinnamon | Target::Mate | Target::Lxqt
        )
    {
        return;
    }
    match target {
        Target::Waybar => {
            println!(
                "Файл: {} — добавьте модуль в нужную секцию modules-*.",
                layout.config_home.join("waybar/config.jsonc").display()
            );
        }
        Target::Polybar => {
            println!(
                "Файл: {} — подключите module/orbitfm в modules-*.",
                layout.config_home.join("polybar/config.ini").display()
            );
        }
        Target::I3blocks => {
            println!(
                "Файл: {} — секция [orbitfm] уже готова.",
                layout.config_home.join("i3blocks/config").display()
            );
        }
        Target::Swaybar => {
            println!(
                "Файл: {} — добавьте status_command в блок bar.",
                layout.config_home.join("sway/config").display()
            );
        }
        Target::I3statusRust => {
            println!(
                "Файл: {} — добавьте [[block]] из сниппета.",
                layout
                    .config_home
                    .join("i3status-rust/config.toml")
                    .display()
            );
        }
        Target::Xfce => {
            println!(
                "Нужен пакет xfce4-genmon-plugin; команда плагина: {} genmon.",
                script.display()
            );
        }
        Target::Cinnamon | Target::Mate | Target::Lxqt => {
            println!(
                "Апплет вывода команды, период 2 с: {} text",
                script.display()
            );
            println!(
                "Кнопки/ярлыки управления: {} playpause и {} panel",
                binary.display(),
                binary.display()
            );
        }
        Target::Plasma6 | Target::Gnome | Target::Omarchy | Target::Chrome | Target::Firefox => {}
    }
}

fn join_paths(paths: &[PathBuf]) -> String {
    paths
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

fn ensure_applicable(target: Target) -> Result<()> {
    if cfg!(target_os = "linux") {
        Ok(())
    } else {
        bail!(
            "интеграции оболочек и баров доступны только на Linux; цель {} не применима",
            target.id()
        )
    }
}

fn ensure_relative(path: &Path) -> Result<()> {
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        bail!("недопустимый путь ассета: {}", path.display());
    }
    Ok(())
}

fn ensure_within_roots(roots: &[PathBuf], path: &Path) -> Result<()> {
    if !path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, Component::ParentDir))
    {
        bail!("недопустимый путь в квитанции: {}", path.display());
    }
    if roots.iter().any(|root| path.starts_with(root)) {
        return Ok(());
    }
    bail!(
        "путь {} вне каталогов установки цели ({})",
        path.display(),
        join_paths(roots)
    )
}

fn metadata_opt(path: &Path) -> Result<Option<fs::Metadata>> {
    match fs::symlink_metadata(path) {
        Ok(meta) => Ok(Some(meta)),
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err).with_context(|| format!("не удалось прочитать {}", path.display())),
    }
}

fn hash_bytes(bytes: &[u8]) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(mode))
        .with_context(|| format!("не удалось выставить права {}", path.display()))
}

#[cfg(not(unix))]
fn set_mode(_path: &Path, _mode: u32) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_layout(dir: &TempDir) -> Layout {
        Layout {
            data_home: dir.path().join("data"),
            config_home: dir.path().join("config"),
            home: dir.path().join("home"),
            root_override: Some(dir.path().join("root")),
        }
    }

    fn fake_binary() -> PathBuf {
        PathBuf::from("/opt/OrbitFM/orbitfm")
    }

    fn assert_extension_version(value: &serde_json::Value) {
        let version = value.as_str().expect("version string");
        assert!(
            version
                .split('.')
                .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit())),
            "Chrome и Firefox принимают только числовые версии: {version}"
        );
    }

    #[test]
    fn parses_targets_and_aliases() {
        assert_eq!(Target::parse("plasma6"), Some(Target::Plasma6));
        assert_eq!(Target::parse("plasma"), Some(Target::Plasma6));
        assert_eq!(Target::parse("gnome-shell"), Some(Target::Gnome));
        assert_eq!(Target::parse("quickshell"), Some(Target::Omarchy));
        assert_eq!(Target::parse("i3status_rust"), Some(Target::I3statusRust));
        assert_eq!(Target::parse("chromium"), Some(Target::Chrome));
        assert_eq!(Target::parse("google-chrome"), Some(Target::Chrome));
        assert_eq!(Target::parse("mozilla"), Some(Target::Firefox));
        assert_eq!(Target::parse("bogus"), None);
    }

    #[test]
    fn parses_integration_commands() {
        let args = [
            "install".to_owned(),
            "waybar".to_owned(),
            "--force".to_owned(),
        ];
        let command = parse(&args).expect("parse install");
        assert!(matches!(
            command,
            Command::Install {
                scope: Scope::One(Target::Waybar),
                force: true
            }
        ));

        let args = ["install".to_owned(), "bogus".to_owned()];
        assert!(parse(&args).is_err());

        let args = ["status".to_owned()];
        assert!(matches!(
            parse(&args).expect("parse status"),
            Command::Status(Scope::All)
        ));
    }

    #[test]
    fn installs_assets_and_receipt() {
        let dir = TempDir::new().expect("temp dir");
        let layout = test_layout(&dir);
        let report = install_one(&layout, Target::Waybar, false, &fake_binary()).expect("install");

        assert!(report.root.join("orbitfm-bar").is_file());
        assert!(report.root.join("config.jsonc").is_file());
        assert!(report.root.join("style.css").is_file());
        assert!(report.root.join("README.md").is_file());
        assert!(report.receipt_path.is_file());

        let script = fs::read_to_string(report.root.join("orbitfm-bar")).expect("script");
        assert!(script.contains("'/opt/OrbitFM/orbitfm'"));

        let receipt: Receipt =
            serde_json::from_slice(&fs::read(&report.receipt_path).expect("receipt bytes"))
                .expect("receipt json");
        assert_eq!(receipt.schema, RECEIPT_SCHEMA);
        assert_eq!(receipt.target, "waybar");
        assert_eq!(receipt.files.len(), 4);
        assert!(
            receipt
                .files
                .iter()
                .any(|file| file.kind == FileKind::Generated)
        );
        assert!(
            receipt
                .files
                .iter()
                .any(|file| file.kind == FileKind::Asset)
        );

        let inspection = inspect(&layout, Target::Waybar).expect("inspect");
        assert_eq!(inspection.state, InstalledState::Installed);
    }

    #[test]
    fn gnome_extension_ships_stylesheet() {
        let dir = TempDir::new().expect("temp dir");
        let layout = test_layout(&dir);
        let report = install_one(&layout, Target::Gnome, false, &fake_binary()).expect("install");

        assert!(report.root.join("extension.js").is_file());
        assert!(report.root.join("metadata.json").is_file());
        assert!(report.root.join("stylesheet.css").is_file());
        assert_eq!(report.files.len(), 4, "три ассета и README");
    }

    #[test]
    fn second_install_is_idempotent() {
        let dir = TempDir::new().expect("temp dir");
        let layout = test_layout(&dir);
        install_one(&layout, Target::Polybar, false, &fake_binary()).expect("first install");
        install_one(&layout, Target::Polybar, false, &fake_binary()).expect("second install");

        let inspection = inspect(&layout, Target::Polybar).expect("inspect");
        assert_eq!(inspection.state, InstalledState::Installed);
    }

    #[test]
    fn install_refuses_modified_file_without_force() {
        let dir = TempDir::new().expect("temp dir");
        let layout = test_layout(&dir);
        let report =
            install_one(&layout, Target::I3blocks, false, &fake_binary()).expect("install");
        let script = report.root.join("orbitfm-bar");
        fs::write(&script, "#!/bin/sh\necho changed\n").expect("modify");

        let err = install_one(&layout, Target::I3blocks, false, &fake_binary())
            .expect_err("modified must be refused");
        assert!(format!("{err:#}").contains("изменён"));

        install_one(&layout, Target::I3blocks, true, &fake_binary()).expect("force reinstall");
        let content = fs::read_to_string(&script).expect("script");
        assert!(content.contains("ORBITFM='/opt/OrbitFM/orbitfm'"));
        assert!(content.contains(" status 2>/dev/null"));
    }

    #[test]
    fn uninstall_removes_only_receipt_files() {
        let dir = TempDir::new().expect("temp dir");
        let layout = test_layout(&dir);
        let report = install_one(&layout, Target::Swaybar, false, &fake_binary()).expect("install");
        let extra = report.root.join("user-note.txt");
        fs::write(&extra, "keep me").expect("extra file");

        let removed = uninstall_one(&layout, Target::Swaybar, false).expect("uninstall");
        assert_eq!(removed.removed, 3);
        assert!(extra.is_file());
        assert!(!report.receipt_path.exists());
        assert!(
            inspect(&layout, Target::Swaybar).expect("inspect").state
                == InstalledState::NotInstalled
        );
    }

    #[test]
    fn uninstall_refuses_modified_file_without_force() {
        let dir = TempDir::new().expect("temp dir");
        let layout = test_layout(&dir);
        let report = install_one(&layout, Target::Gnome, false, &fake_binary()).expect("install");
        let extension = report.root.join("extension.js");
        fs::write(&extension, "// changed\n").expect("modify");

        let err = uninstall_one(&layout, Target::Gnome, false).expect_err("must refuse");
        assert!(format!("{err:#}").contains("изменены"));
        assert!(report.receipt_path.exists());
        assert!(extension.exists());

        uninstall_one(&layout, Target::Gnome, true).expect("force uninstall");
        assert!(!extension.exists());
        assert!(!report.receipt_path.exists());
    }

    #[test]
    fn status_reports_missing_files() {
        let dir = TempDir::new().expect("temp dir");
        let layout = test_layout(&dir);
        let report = install_one(&layout, Target::Mate, false, &fake_binary()).expect("install");
        fs::remove_file(report.root.join("orbitfm-bar")).expect("remove");

        let inspection = inspect(&layout, Target::Mate).expect("inspect");
        assert_eq!(inspection.state, InstalledState::Missing);
    }

    #[test]
    fn receipt_path_outside_root_is_rejected() {
        let dir = TempDir::new().expect("temp dir");
        let layout = test_layout(&dir);
        let report = install_one(&layout, Target::Xfce, false, &fake_binary()).expect("install");

        let mut receipt: Receipt =
            serde_json::from_slice(&fs::read(&report.receipt_path).expect("receipt bytes"))
                .expect("receipt json");
        receipt.files[0].path = "/tmp/orbitfm-escape".to_owned();
        fs::write(
            &report.receipt_path,
            serde_json::to_vec(&receipt).expect("json"),
        )
        .expect("tamper");

        let err = inspect(&layout, Target::Xfce).expect_err("must reject");
        assert!(format!("{err:#}").contains("вне каталогов"));
    }

    #[test]
    fn installs_chrome_extension_and_native_manifests() {
        let dir = TempDir::new().expect("temp dir");
        let layout = test_layout(&dir);
        let report = install_one(&layout, Target::Chrome, false, &fake_binary()).expect("install");

        assert_eq!(report.files.len(), 11, "6 ассетов и 5 native-манифестов");
        let extension = report.root.join("extension");
        for name in ["manifest.json", "popup.html", "popup.css", "popup.js"] {
            assert!(extension.join(name).is_file(), "{name}");
        }

        let wrapper_path = report.root.join("orbitfm-native-host");
        let wrapper = fs::read_to_string(&wrapper_path).expect("wrapper");
        assert!(wrapper.contains("exec '/opt/OrbitFM/orbitfm' native-host"));

        let manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(extension.join("manifest.json")).expect("manifest"))
                .expect("json");
        assert_eq!(manifest["version"], browser_version());
        assert_extension_version(&manifest["version"]);
        assert_eq!(manifest["manifest_version"], 3);
        assert!(manifest["key"].as_str().is_some_and(|key| !key.is_empty()));

        for manifest_dir in layout.install_manifest_dirs(Target::Chrome) {
            let host: serde_json::Value = serde_json::from_slice(
                &fs::read(manifest_dir.join(HOST_MANIFEST_FILE)).expect("host manifest"),
            )
            .expect("host json");
            assert_eq!(host["name"], HOST_NAME);
            assert_eq!(host["type"], "stdio");
            assert_eq!(host["path"], wrapper_path.to_str().expect("utf-8"));
            assert_eq!(
                host["allowed_origins"][0],
                format!("chrome-extension://{CHROME_EXTENSION_ID}/")
            );
        }

        let receipt: Receipt =
            serde_json::from_slice(&fs::read(&report.receipt_path).expect("receipt bytes"))
                .expect("receipt json");
        assert_eq!(receipt.files.len(), 11);
        assert_eq!(receipt.target, "chrome");
        assert_eq!(
            inspect(&layout, Target::Chrome).expect("inspect").state,
            InstalledState::Installed
        );
    }

    #[test]
    fn installs_firefox_native_manifest() {
        let dir = TempDir::new().expect("temp dir");
        let layout = test_layout(&dir);
        let report = install_one(&layout, Target::Firefox, false, &fake_binary()).expect("install");

        assert_eq!(report.files.len(), 7, "6 ассетов и один native-манифест");
        let manifest: serde_json::Value = serde_json::from_slice(
            &fs::read(
                layout
                    .install_manifest_dirs(Target::Firefox)
                    .first()
                    .expect("manifest dir")
                    .join(HOST_MANIFEST_FILE),
            )
            .expect("host manifest"),
        )
        .expect("host json");
        assert_eq!(manifest["name"], HOST_NAME);
        assert_eq!(manifest["allowed_extensions"][0], FIREFOX_EXTENSION_ID);

        let extension: serde_json::Value = serde_json::from_slice(
            &fs::read(report.root.join("extension/manifest.json")).expect("extension manifest"),
        )
        .expect("extension json");
        assert_eq!(extension["version"], browser_version());
        assert_extension_version(&extension["version"]);
        assert_eq!(
            extension["browser_specific_settings"]["gecko"]["id"],
            FIREFOX_EXTENSION_ID
        );
        assert_eq!(
            inspect(&layout, Target::Firefox).expect("inspect").state,
            InstalledState::Installed
        );
    }

    #[test]
    fn uninstall_browser_removes_extension_and_manifests() {
        let dir = TempDir::new().expect("temp dir");
        let layout = test_layout(&dir);
        let report = install_one(&layout, Target::Chrome, false, &fake_binary()).expect("install");
        let manifest_dir = layout
            .install_manifest_dirs(Target::Chrome)
            .first()
            .expect("manifest dir")
            .clone();

        let removed = uninstall_one(&layout, Target::Chrome, false).expect("uninstall");
        assert_eq!(removed.removed, 11);
        assert!(!report.root.join("extension").exists());
        assert!(!report.root.join("orbitfm-native-host").exists());
        assert!(!manifest_dir.join(HOST_MANIFEST_FILE).exists());
        assert!(!report.receipt_path.exists());
        assert_eq!(
            inspect(&layout, Target::Chrome).expect("inspect").state,
            InstalledState::NotInstalled
        );
    }
}
