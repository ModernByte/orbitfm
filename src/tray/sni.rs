//! Трей на `ksni` (ADR-0020, п. 2, 4–6, 8): SNI-item с процедурной иконкой,
//! меню и колесом громкости.

use std::path::PathBuf;
use std::sync::LazyLock;
use std::sync::mpsc::Receiver;

use ksni::blocking::TrayMethods;
use ksni::menu::{CheckmarkItem, MenuItem, RadioGroup, RadioItem, StandardItem};
use ksni::{Category, Icon, OfflineReason, Orientation, Status, ToolTip, Tray};
use tokio::sync::mpsc::UnboundedSender;
use tracing::{info, warn};

use super::{TrayCommand, TraySnapshot, icon, text};
use crate::i18n;
use crate::ipc::WakeFn;
use crate::settings::ThemeMode;

/// Каталог hicolor, если он установлен: тогда хост может взять векторную иконку
/// из упаковки вместо нашего pixmap'а.
static THEME_PATH: LazyLock<String> = LazyLock::new(find_hicolor);

/// Пиксели знака в ARGB32 (network byte order), как требует SNI: растеризатор
/// общий с macOS-треем и отдаёт RGBA (ADR-0026, п. 8).
static ICONS: LazyLock<Vec<Icon>> = LazyLock::new(|| {
    icon::SIZES
        .iter()
        .map(|&size| Icon {
            width: size,
            height: size,
            data: icon::rgba(size)
                .as_chunks::<4>()
                .0
                .iter()
                .flat_map(|pixel| [pixel[3], pixel[0], pixel[1], pixel[2]])
                .collect(),
        })
        .collect()
});

/// Подпись, которую рисует трей, и канал, по которому он отдаёт команды ядру.
pub(crate) struct OrbitTray {
    app_id: &'static str,
    title: &'static str,
    snapshot: TraySnapshot,
    commands: UnboundedSender<TrayCommand>,
    wake: WakeFn,
}

impl OrbitTray {
    /// Колбэки ksni обязаны быть лёгкими: только non-blocking send в тот же
    /// dispatch, что у IPC и MPRIS (ADR-0020, п. 3). `wake` будит UI-цикл,
    /// который эти команды и читает.
    fn send(&self, command: TrayCommand) {
        if let Err(err) = self.commands.send(command) {
            warn!(?err, "Application Core больше не слушает трей");
        }
        (self.wake)();
    }
}

/// Запускает поток-владелец трея. `false`, если поток не создан — трей при этом
/// просто отсутствует, запуск приложения это не отменяет.
///
/// Отдельный обычный `std::thread` обязателен: `ksni::blocking` входит в общий
/// process-global current-thread runtime (ADR-0020, п. 4), и вызов с воркера
/// нашего `rt-multi-thread` panicked' бы «Cannot start a runtime from within a runtime».
pub(crate) fn start(
    app_id: &'static str,
    title: &'static str,
    snapshots: Receiver<TraySnapshot>,
    commands: UnboundedSender<TrayCommand>,
    wake: WakeFn,
) -> bool {
    let spawned = std::thread::Builder::new()
        .name("orbitfm-tray".to_owned())
        .spawn(move || serve(app_id, title, snapshots, commands, wake));
    match spawned {
        Ok(_) => true,
        Err(err) => {
            warn!(error = %err, "не удалось создать поток трея");
            false
        }
    }
}

/// Цикл потока-владельца: ждёт снимки, сворачивает очередь до последнего и
/// обновляет SNI-item. `update` блокирует этот поток до dbus-ответа, поэтому он
/// никогда не вызывается из UI-цикла.
#[allow(clippy::needless_pass_by_value)] // каналы переходят в поток-владелец
fn serve(
    app_id: &'static str,
    title: &'static str,
    snapshots: Receiver<TraySnapshot>,
    commands: UnboundedSender<TrayCommand>,
    wake: WakeFn,
) {
    let tray = OrbitTray {
        app_id,
        title,
        snapshot: TraySnapshot::default(),
        commands,
        wake,
    };
    // `disable_dbus_name` не ставим: sandbox-признак flatpak-сборки уточняется
    // live-проверкой и не хардкодится (ADR-0020, п. 8).
    let Ok(handle) = tray.spawn() else {
        warn!("системный трей недоступен, продолжаем без него");
        return;
    };
    if handle.is_closed() {
        warn!("трей зарегистрировался на мёртвом соединении, продолжаем без него");
        return;
    }
    info!("системный трей запущен");

    while let Ok(first) = snapshots.recv() {
        let mut latest = first;
        while let Ok(next) = snapshots.try_recv() {
            latest = next;
        }
        if handle.update(|tray| tray.snapshot = latest).is_none() {
            break;
        }
    }
    handle.shutdown().wait();
}

impl Tray for OrbitTray {
    fn id(&self) -> String {
        self.app_id.to_owned()
    }

    fn category(&self) -> Category {
        Category::ApplicationStatus
    }

    fn title(&self) -> String {
        self.title.to_owned()
    }

    fn status(&self) -> Status {
        if self.snapshot.playing || self.snapshot.window_visible {
            Status::Active
        } else {
            Status::Passive
        }
    }

    fn icon_name(&self) -> String {
        self.app_id.to_owned()
    }

    fn icon_theme_path(&self) -> String {
        THEME_PATH.clone()
    }

    /// Пиксели строятся один раз: хост спрашивает их на каждое событие трея.
    fn icon_pixmap(&self) -> Vec<Icon> {
        ICONS.clone()
    }

    /// Название станции — внешние данные Radio Browser, оно не переводится
    /// (ADR-0030, п. 4); переводится только заглушка, когда станции нет.
    fn tool_tip(&self) -> ToolTip {
        ToolTip {
            icon_name: self.app_id.to_owned(),
            icon_pixmap: Vec::new(),
            title: self.title.to_owned(),
            description: self
                .snapshot
                .station
                .clone()
                .unwrap_or_else(|| text::nothing_playing(&self.snapshot)),
        }
    }

    /// Левый клик — toggle окна, средний — показать, колесо — громкость.
    /// Работает только при `MENU_ON_ACTIVATE = false` (значение по умолчанию):
    /// иначе `activate` не вызывается никогда (ADR-0020, п. 5).
    fn activate(&mut self, _x: i32, _y: i32) {
        self.send(TrayCommand::Toggle);
    }

    fn secondary_activate(&mut self, _x: i32, _y: i32) {
        self.send(TrayCommand::Show);
    }

    fn scroll(&mut self, delta: i32, orientation: Orientation) {
        match orientation {
            Orientation::Vertical if delta > 0 => self.send(TrayCommand::VolumeUp),
            Orientation::Vertical if delta < 0 => self.send(TrayCommand::VolumeDown),
            _ => {}
        }
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        // Язык берётся из снимка: `Session` публикует его разрешённым, а смена
        // языка меняет сам снимок, поэтому меню перестраивается без перезапуска
        // (ADR-0030, п. 8–9).
        let catalog = i18n::catalog(self.snapshot.language);
        vec![
            MenuItem::Standard(StandardItem {
                label: text::now_playing_label(&self.snapshot),
                enabled: false,
                ..Default::default()
            }),
            MenuItem::Separator,
            standard(text::toggle_label(&self.snapshot), |tray| {
                tray.send(TrayCommand::Toggle);
            }),
            MenuItem::Separator,
            MenuItem::Checkmark(CheckmarkItem {
                label: catalog.tray.play_pause.to_owned(),
                checked: self.snapshot.playing,
                activate: Box::new(|tray: &mut Self| tray.send(TrayCommand::PlayPause)),
                ..Default::default()
            }),
            standard(catalog.tray.stop, |tray| tray.send(TrayCommand::Stop)),
            standard(catalog.tray.previous, |tray| {
                tray.send(TrayCommand::Previous);
            }),
            standard(catalog.tray.next, |tray| tray.send(TrayCommand::Next)),
            standard(catalog.tray.radio_roulette, |tray| {
                tray.send(TrayCommand::Roulette);
            }),
            MenuItem::Separator,
            standard(catalog.tray.volume_up, |tray| {
                tray.send(TrayCommand::VolumeUp);
            }),
            standard(catalog.tray.volume_down, |tray| {
                tray.send(TrayCommand::VolumeDown);
            }),
            MenuItem::Separator,
            MenuItem::RadioGroup(RadioGroup {
                selected: ThemeMode::ALL
                    .iter()
                    .position(|mode| *mode == self.snapshot.theme)
                    .unwrap_or_default(),
                select: Box::new(|tray: &mut Self, index| {
                    if let Some(mode) = ThemeMode::ALL.get(index) {
                        tray.send(TrayCommand::SetTheme(*mode));
                    }
                }),
                options: ThemeMode::ALL
                    .iter()
                    .map(|mode| RadioItem {
                        label: i18n::theme_label(self.snapshot.language, *mode).to_owned(),
                        ..Default::default()
                    })
                    .collect(),
            }),
            MenuItem::Separator,
            standard(catalog.tray.settings, |tray| {
                tray.send(TrayCommand::Settings);
            }),
            standard(catalog.tray.quit, |tray| tray.send(TrayCommand::Quit)),
        ]
    }

    /// Хост ушёл (перезапуск оболочки, выключенный плагин): соединение держим и
    /// ждём `watcher_online` — `false` выключило бы трей до конца жизни процесса.
    fn watcher_offline(&self, reason: OfflineReason) -> bool {
        warn!(?reason, "SNI-хост offline");
        true
    }
}

fn standard(
    label: &str,
    activate: impl Fn(&mut OrbitTray) + Send + 'static,
) -> MenuItem<OrbitTray> {
    MenuItem::Standard(StandardItem {
        label: label.to_owned(),
        activate: Box::new(activate),
        ..Default::default()
    })
}

/// Первый существующий каталог тем: пользовательский XDG, затем системный.
fn find_hicolor() -> String {
    data_home()
        .into_iter()
        .chain([PathBuf::from("/usr/share")])
        .find_map(|root| {
            let dir = root.join("icons/hicolor");
            dir.is_dir().then(|| dir.to_string_lossy().into_owned())
        })
        .unwrap_or_default()
}

fn data_home() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("XDG_DATA_HOME") {
        let path = PathBuf::from(path);
        if path.is_absolute() {
            return Some(path);
        }
    }
    std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share"))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use crate::settings::Language;

    fn tray(snapshot: TraySnapshot) -> OrbitTray {
        let (commands, _sink) = tokio::sync::mpsc::unbounded_channel();
        OrbitTray {
            app_id: "org.modernbyte.OrbitFM",
            title: "OrbitFM",
            snapshot,
            commands,
            wake: Arc::new(|| {}),
        }
    }

    /// Подписи сверяются с английским источником каталога (ADR-0030, п. 3):
    /// язык ставится явно, чтобы тесты не зависели от локали сборки.
    fn english() -> TraySnapshot {
        TraySnapshot {
            language: Language::English,
            ..Default::default()
        }
    }

    fn labels(menu: &[MenuItem<OrbitTray>]) -> Vec<String> {
        menu.iter()
            .filter_map(|item| match item {
                MenuItem::Standard(item) => Some(item.label.clone()),
                MenuItem::Checkmark(item) => Some(item.label.clone()),
                MenuItem::RadioGroup(group) => Some(
                    group
                        .options
                        .iter()
                        .map(|option| option.label.as_str())
                        .collect::<Vec<_>>()
                        .join("/"),
                ),
                MenuItem::Separator | MenuItem::SubMenu(_) => None,
            })
            .collect()
    }

    #[test]
    fn status_is_active_only_when_something_happens() {
        assert_eq!(
            tray(TraySnapshot::default()).status(),
            Status::Passive,
            "скрытое окно и тишина в эфире"
        );
        let playing = TraySnapshot {
            playing: true,
            ..Default::default()
        };
        let visible = TraySnapshot {
            window_visible: true,
            ..Default::default()
        };
        assert_eq!(tray(playing).status(), Status::Active);
        assert_eq!(tray(visible).status(), Status::Active);
    }

    #[test]
    fn tooltip_shows_the_station_or_silence() {
        assert_eq!(tray(english()).tool_tip().description, "Nothing playing");
        let snapshot = TraySnapshot {
            station: Some("WFMU".to_owned()),
            ..english()
        };
        assert_eq!(tray(snapshot).tool_tip().description, "WFMU");
    }

    #[test]
    fn menu_offers_window_playback_volume_theme_settings_and_quit() {
        let menu = tray(english()).menu();
        let items = labels(&menu);
        assert_eq!(items.first(), Some(&"Nothing playing".to_owned()));
        assert!(items.contains(&"Show window".to_owned()));
        assert!(items.contains(&"Play/Pause".to_owned()));
        assert!(items.contains(&"Radio Roulette".to_owned()));
        assert!(items.contains(&"Volume Up".to_owned()));
        assert!(items.contains(&"System/Dark/Light".to_owned()));
        assert!(items.contains(&"Settings…".to_owned()));
        assert_eq!(items.last(), Some(&"Quit".to_owned()));

        let visible = tray(TraySnapshot {
            window_visible: true,
            ..english()
        })
        .menu();
        assert!(labels(&visible).contains(&"Hide window".to_owned()));
    }

    #[test]
    fn menu_header_shows_the_current_stream_without_being_a_command() {
        let playing = tray(TraySnapshot {
            playing: true,
            station: Some("WFMU".to_owned()),
            ..english()
        })
        .menu();
        let MenuItem::Standard(header) = &playing[0] else {
            panic!("первым идёт заголовок");
        };
        assert_eq!(header.label, "Now playing: WFMU");
        assert!(!header.enabled, "заголовок — витрина, а не команда");

        let paused = tray(TraySnapshot {
            station: Some("WFMU".to_owned()),
            ..english()
        })
        .menu();
        let MenuItem::Standard(header) = &paused[0] else {
            panic!("первым идёт заголовок");
        };
        assert_eq!(header.label, "Paused: WFMU");
    }

    /// Контракт ADR-0030 п. 9: трей читает каталог языка снимка. Проверяем
    /// равенство с полями `tray` для каждого языка — так видно и литерал,
    /// и перепутанное поле. `title` тултипа остаётся незакрытым контрактом
    /// app-name (ADR-0030, п. 4).
    #[test]
    fn every_label_comes_from_the_catalog_for_the_snapshot_language() {
        for language in Language::ALL {
            let text = i18n::catalog(language);
            let themes = ThemeMode::ALL
                .iter()
                .map(|mode| i18n::theme_label(language, *mode))
                .collect::<Vec<_>>()
                .join("/");
            let snapshot = TraySnapshot {
                language,
                window_visible: true,
                theme: ThemeMode::Dark,
                ..Default::default()
            };
            assert_eq!(
                labels(&tray(snapshot.clone()).menu()),
                vec![
                    text.tray.nothing_playing.to_owned(),
                    text.tray.hide_window.to_owned(),
                    text.tray.play_pause.to_owned(),
                    text.tray.stop.to_owned(),
                    text.tray.previous.to_owned(),
                    text.tray.next.to_owned(),
                    text.tray.radio_roulette.to_owned(),
                    text.tray.volume_up.to_owned(),
                    text.tray.volume_down.to_owned(),
                    themes,
                    text.tray.settings.to_owned(),
                    text.tray.quit.to_owned(),
                ],
                "меню на {language:?} расходится с каталогом"
            );

            let tip = tray(snapshot).tool_tip();
            assert_eq!(tip.title, "OrbitFM", "имя приложения не переводится");
            assert_eq!(tip.description, text.tray.nothing_playing);
        }
    }

    #[test]
    fn theme_radio_marks_the_current_mode() {
        let menu = tray(TraySnapshot {
            theme: ThemeMode::Light,
            ..Default::default()
        })
        .menu();
        let selected = menu.iter().find_map(|item| match item {
            MenuItem::RadioGroup(group) => Some(group.selected),
            _ => None,
        });
        assert_eq!(selected, Some(2));
    }

    #[test]
    fn play_pause_check_follows_the_stream() {
        let menu = tray(TraySnapshot {
            playing: true,
            ..Default::default()
        })
        .menu();
        let checked = menu.iter().find_map(|item| match item {
            MenuItem::Checkmark(item) => Some(item.checked),
            _ => None,
        });
        assert_eq!(checked, Some(true));
    }

    #[test]
    fn icon_sizes_match_the_specification_and_byte_order() {
        let icons = ICONS.clone();
        assert_eq!(icons.len(), icon::SIZES.len());
        for (item, size) in icons.iter().zip(icon::SIZES) {
            assert_eq!(item.data.len(), (item.width * item.height * 4) as usize);
            assert_eq!((item.width, item.height), (size, size));
        }
        let small = &icons[0];
        assert_eq!((small.width, small.height), (22, 22));
        let center = usize::try_from((small.width / 2) * small.width + small.height / 2)
            .expect("иконка помещается в usize")
            * 4;
        assert_eq!(small.data[center], 255, "ARGB32: alpha идёт первым");
    }

    #[test]
    fn theme_path_is_empty_or_a_hicolor_root() {
        // Зависит от окружения сборки, поэтому проверяется только контракт:
        // пустая строка допустима, хост тогда возьмёт `icon_name`.
        assert!(THEME_PATH.is_empty() || THEME_PATH.ends_with("hicolor"));
    }

    /// Контракт с Core: каждое действие пользователя — ровно одна команда в
    /// общую очередь и ровно одно пробуждение цикла, который её читает.
    #[test]
    fn clicks_publish_a_command_and_wake_the_core() {
        let (commands, mut sink) = tokio::sync::mpsc::unbounded_channel();
        let hits = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&hits);
        let mut tray = OrbitTray {
            app_id: "org.modernbyte.OrbitFM",
            title: "OrbitFM",
            snapshot: TraySnapshot::default(),
            commands,
            wake: Arc::new(move || {
                counter.fetch_add(1, Ordering::SeqCst);
            }),
        };

        tray.activate(0, 0);
        tray.secondary_activate(0, 0);
        tray.scroll(-1, Orientation::Vertical);
        tray.scroll(1, Orientation::Horizontal);

        assert_eq!(sink.try_recv(), Ok(TrayCommand::Toggle));
        assert_eq!(sink.try_recv(), Ok(TrayCommand::Show));
        assert_eq!(sink.try_recv(), Ok(TrayCommand::VolumeDown));
        assert_eq!(
            sink.try_recv(),
            Err(tokio::sync::mpsc::error::TryRecvError::Empty),
            "горизонтальное колесо ничего не меняет"
        );
        assert_eq!(hits.load(Ordering::SeqCst), 3);
    }
}
