use super::Target;

/// Контекст подстановки абсолютных путей в ассет.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Escape {
    /// Текст без подстановок (README и другие справочные файлы).
    None,
    /// Путь в одинарных кавычках — для shell-команд, JSON-строк и TOML.
    Shell,
    /// Путь с экранированием `\` и `"` — для QML/JS-строк в двойных кавычках.
    Quote,
}

pub(super) struct Asset {
    pub(super) path: &'static str,
    pub(super) content: &'static str,
    pub(super) executable: bool,
    pub(super) escape: Escape,
    pub(super) generated: bool,
}

const fn asset(
    path: &'static str,
    content: &'static str,
    executable: bool,
    escape: Escape,
    generated: bool,
) -> Asset {
    Asset {
        path,
        content,
        executable,
        escape,
        generated,
    }
}

static BAR_SCRIPT: &str = include_str!("../../integrations/common/orbitfm-bar.sh");
static BAR_README: &str = include_str!("../../integrations/common/bar-README.md");
static NATIVE_HOST_WRAPPER: &str = include_str!("../../integrations/browser/orbitfm-native-host");
static POPUP_HTML: &str = include_str!("../../integrations/browser/popup.html");
static POPUP_CSS: &str = include_str!("../../integrations/browser/popup.css");
static POPUP_JS: &str = include_str!("../../integrations/browser/popup.js");

static PLASMA6: [Asset; 3] = [
    asset(
        "metadata.json",
        include_str!("../../integrations/plasma6/metadata.json"),
        false,
        Escape::None,
        true,
    ),
    asset(
        "contents/ui/main.qml",
        include_str!("../../integrations/plasma6/main.qml"),
        false,
        Escape::Quote,
        true,
    ),
    asset(
        "README.md",
        include_str!("../../integrations/plasma6/README.md"),
        false,
        Escape::None,
        false,
    ),
];

static GNOME: [Asset; 3] = [
    asset(
        "metadata.json",
        include_str!("../../integrations/gnome/metadata.json"),
        false,
        Escape::None,
        true,
    ),
    asset(
        "extension.js",
        include_str!("../../integrations/gnome/extension.js"),
        false,
        Escape::Quote,
        true,
    ),
    asset(
        "README.md",
        include_str!("../../integrations/gnome/README.md"),
        false,
        Escape::None,
        false,
    ),
];

static OMARCHY: [Asset; 3] = [
    asset(
        "manifest.json",
        include_str!("../../integrations/omarchy/manifest.json"),
        false,
        Escape::None,
        true,
    ),
    asset(
        "BarWidget.qml",
        include_str!("../../integrations/omarchy/BarWidget.qml"),
        false,
        Escape::Quote,
        true,
    ),
    asset(
        "README.md",
        include_str!("../../integrations/omarchy/README.md"),
        false,
        Escape::None,
        false,
    ),
];

static WAYBAR: [Asset; 3] = [
    asset("orbitfm-bar", BAR_SCRIPT, true, Escape::Shell, true),
    asset(
        "config.jsonc",
        include_str!("../../integrations/waybar/config.jsonc"),
        false,
        Escape::Shell,
        true,
    ),
    asset("README.md", BAR_README, false, Escape::None, false),
];

static POLYBAR: [Asset; 3] = [
    asset("orbitfm-bar", BAR_SCRIPT, true, Escape::Shell, true),
    asset(
        "config.ini",
        include_str!("../../integrations/polybar/config.ini"),
        false,
        Escape::Shell,
        true,
    ),
    asset("README.md", BAR_README, false, Escape::None, false),
];

static I3BLOCKS: [Asset; 3] = [
    asset("orbitfm-bar", BAR_SCRIPT, true, Escape::Shell, true),
    asset(
        "config.conf",
        include_str!("../../integrations/i3blocks/config.conf"),
        false,
        Escape::Shell,
        true,
    ),
    asset("README.md", BAR_README, false, Escape::None, false),
];

static SWAYBAR: [Asset; 3] = [
    asset("orbitfm-bar", BAR_SCRIPT, true, Escape::Shell, true),
    asset(
        "config.sway",
        include_str!("../../integrations/swaybar/config.sway"),
        false,
        Escape::Shell,
        true,
    ),
    asset("README.md", BAR_README, false, Escape::None, false),
];

static I3STATUS_RUST: [Asset; 3] = [
    asset("orbitfm-bar", BAR_SCRIPT, true, Escape::Shell, true),
    asset(
        "config.toml",
        include_str!("../../integrations/i3status-rust/config.toml"),
        false,
        Escape::Shell,
        true,
    ),
    asset("README.md", BAR_README, false, Escape::None, false),
];

static XFCE: [Asset; 3] = [
    asset("orbitfm-bar", BAR_SCRIPT, true, Escape::Shell, true),
    asset(
        "usage.txt",
        include_str!("../../integrations/xfce/usage.txt"),
        false,
        Escape::Shell,
        true,
    ),
    asset("README.md", BAR_README, false, Escape::None, false),
];

static CINNAMON: [Asset; 3] = [
    asset("orbitfm-bar", BAR_SCRIPT, true, Escape::Shell, true),
    asset(
        "usage.txt",
        include_str!("../../integrations/cinnamon/usage.txt"),
        false,
        Escape::Shell,
        true,
    ),
    asset("README.md", BAR_README, false, Escape::None, false),
];

static MATE: [Asset; 3] = [
    asset("orbitfm-bar", BAR_SCRIPT, true, Escape::Shell, true),
    asset(
        "usage.txt",
        include_str!("../../integrations/mate/usage.txt"),
        false,
        Escape::Shell,
        true,
    ),
    asset("README.md", BAR_README, false, Escape::None, false),
];

static LXQT: [Asset; 3] = [
    asset("orbitfm-bar", BAR_SCRIPT, true, Escape::Shell, true),
    asset(
        "usage.txt",
        include_str!("../../integrations/lxqt/usage.txt"),
        false,
        Escape::Shell,
        true,
    ),
    asset("README.md", BAR_README, false, Escape::None, false),
];

static CHROME: [Asset; 6] = [
    asset(
        "extension/manifest.json",
        include_str!("../../integrations/chrome/manifest.json"),
        false,
        Escape::None,
        true,
    ),
    asset(
        "extension/popup.html",
        POPUP_HTML,
        false,
        Escape::None,
        false,
    ),
    asset("extension/popup.css", POPUP_CSS, false, Escape::None, false),
    asset("extension/popup.js", POPUP_JS, false, Escape::None, false),
    asset(
        "orbitfm-native-host",
        NATIVE_HOST_WRAPPER,
        true,
        Escape::Shell,
        true,
    ),
    asset(
        "README.md",
        include_str!("../../integrations/chrome/README.md"),
        false,
        Escape::None,
        false,
    ),
];

static FIREFOX: [Asset; 6] = [
    asset(
        "extension/manifest.json",
        include_str!("../../integrations/firefox/manifest.json"),
        false,
        Escape::None,
        true,
    ),
    asset(
        "extension/popup.html",
        POPUP_HTML,
        false,
        Escape::None,
        false,
    ),
    asset("extension/popup.css", POPUP_CSS, false, Escape::None, false),
    asset("extension/popup.js", POPUP_JS, false, Escape::None, false),
    asset(
        "orbitfm-native-host",
        NATIVE_HOST_WRAPPER,
        true,
        Escape::Shell,
        true,
    ),
    asset(
        "README.md",
        include_str!("../../integrations/firefox/README.md"),
        false,
        Escape::None,
        false,
    ),
];

pub(super) fn assets(target: Target) -> &'static [Asset] {
    match target {
        Target::Plasma6 => &PLASMA6,
        Target::Gnome => &GNOME,
        Target::Omarchy => &OMARCHY,
        Target::Waybar => &WAYBAR,
        Target::Polybar => &POLYBAR,
        Target::I3blocks => &I3BLOCKS,
        Target::Swaybar => &SWAYBAR,
        Target::I3statusRust => &I3STATUS_RUST,
        Target::Xfce => &XFCE,
        Target::Cinnamon => &CINNAMON,
        Target::Mate => &MATE,
        Target::Lxqt => &LXQT,
        Target::Chrome => &CHROME,
        Target::Firefox => &FIREFOX,
    }
}
