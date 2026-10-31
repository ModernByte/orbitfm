//! Трей на `ksni` (ADR-0020, п. 2, 4–6, 8): SNI-item с процедурной иконкой,
//! меню и колесом громкости.

use std::path::PathBuf;
use std::sync::LazyLock;
use std::sync::mpsc::Receiver;

use ksni::blocking::TrayMethods;
use ksni::menu::{CheckmarkItem, MenuItem, RadioGroup, RadioItem, StandardItem, SubMenu};
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

    /// Иконка живёт, пока жив процесс: Noctalia v5 скрывает пассивные элементы,
    /// поэтому остановка эфира и закрытое окно не должны её убирать
    /// (ADR-0020, ревизия 0.16).
    fn status(&self) -> Status {
        Status::Active
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
            description: text::station_name(&self.snapshot),
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
        let tray = &catalog.tray;
        let mut items = Vec::with_capacity(24);

        // Карточка станции — витрина: строки не нажимаются и не переводятся.
        for line in text::card_lines(&self.snapshot) {
            items.push(section(line));
        }
        items.push(MenuItem::Separator);
        items.push(standard(
            text::toggle_label(&self.snapshot),
            if self.snapshot.window_visible {
                "view-restore-symbolic"
            } else {
                "view-fullscreen-symbolic"
            },
            |tray| tray.send(TrayCommand::Toggle),
        ));

        items.push(MenuItem::Separator);
        items.push(section(tray.playback.to_owned()));
        items.push(MenuItem::Checkmark(CheckmarkItem {
            label: tray.play_pause.to_owned(),
            checked: self.snapshot.playing,
            icon_name: if self.snapshot.playing {
                "media-playback-pause-symbolic"
            } else {
                "media-playback-start-symbolic"
            }
            .to_owned(),
            activate: Box::new(|tray: &mut Self| tray.send(TrayCommand::PlayPause)),
            ..Default::default()
        }));
        items.push(standard(
            tray.stop,
            "media-playback-stop-symbolic",
            |tray| tray.send(TrayCommand::Stop),
        ));
        items.push(standard(
            tray.previous,
            "media-skip-backward-symbolic",
            |tray| tray.send(TrayCommand::Previous),
        ));
        items.push(standard(tray.next, "media-skip-forward-symbolic", |tray| {
            tray.send(TrayCommand::Next);
        }));
        items.push(standard(
            tray.radio_roulette,
            "media-playlist-shuffle-symbolic",
            |tray| tray.send(TrayCommand::Roulette),
        ));

        items.push(MenuItem::Separator);
        items.push(section(text::volume_label(&self.snapshot)));
        items.push(standard(
            tray.volume_up,
            "audio-volume-high-symbolic",
            |tray| tray.send(TrayCommand::VolumeUp),
        ));
        items.push(standard(
            tray.volume_down,
            "audio-volume-low-symbolic",
            |tray| tray.send(TrayCommand::VolumeDown),
        ));

        items.push(MenuItem::Separator);
        items.push(MenuItem::SubMenu(SubMenu {
            label: tray.appearance.to_owned(),
            icon_name: "preferences-desktop-theme-symbolic".to_owned(),
            submenu: vec![MenuItem::RadioGroup(RadioGroup {
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
            })],
            ..Default::default()
        }));

        items.push(MenuItem::Separator);
        // Без активной станции подменю нечего показывать: карточка пуста.
        items.push(MenuItem::SubMenu(SubMenu {
            label: tray.about_station.to_owned(),
            icon_name: "help-about-symbolic".to_owned(),
            enabled: self.snapshot.station.is_some(),
            submenu: vec![
                MenuItem::Standard(StandardItem {
                    label: text::favorite_label(&self.snapshot).to_owned(),
                    icon_name: "emblem-favorite-symbolic".to_owned(),
                    activate: Box::new(|tray: &mut Self| tray.send(TrayCommand::ToggleFavorite)),
                    ..Default::default()
                }),
                MenuItem::Standard(StandardItem {
                    label: tray.open_homepage.to_owned(),
                    icon_name: "web-browser-symbolic".to_owned(),
                    enabled: self
                        .snapshot
                        .station
                        .as_ref()
                        .is_some_and(|station| station.homepage.is_some()),
                    activate: Box::new(|tray: &mut Self| tray.send(TrayCommand::OpenHomepage)),
                    ..Default::default()
                }),
            ],
            ..Default::default()
        }));

        items.push(MenuItem::Separator);
        items.push(standard(
            tray.settings,
            "preferences-system-symbolic",
            |tray| tray.send(TrayCommand::Settings),
        ));
        items.push(standard(tray.quit, "application-exit-symbolic", |tray| {
            tray.send(TrayCommand::Quit);
        }));
        items
    }

    /// Хост ушёл (перезапуск оболочки, выключенный плагин): соединение держим и
    /// ждём `watcher_online` — `false` выключило бы трей до конца жизни процесса.
    fn watcher_offline(&self, reason: OfflineReason) -> bool {
        warn!(?reason, "SNI-хост offline");
        true
    }
}

/// Пункт меню с freedesktop-иконкой: хост без поддержки иконок покажет только
/// подпись.
fn standard(
    label: &str,
    icon_name: &str,
    activate: impl Fn(&mut OrbitTray) + Send + 'static,
) -> MenuItem<OrbitTray> {
    MenuItem::Standard(StandardItem {
        label: label.to_owned(),
        icon_name: icon_name.to_owned(),
        activate: Box::new(activate),
        ..Default::default()
    })
}

/// Заголовок группы и строка карточки: витрина, а не команда.
fn section(label: String) -> MenuItem<OrbitTray> {
    MenuItem::Standard(StandardItem {
        label,
        enabled: false,
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
    use crate::tray::TrayStation;

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
        let mut labels = Vec::new();
        collect_labels(menu, &mut labels);
        labels
    }

    fn collect_labels(menu: &[MenuItem<OrbitTray>], labels: &mut Vec<String>) {
        for item in menu {
            match item {
                MenuItem::Standard(item) => labels.push(item.label.clone()),
                MenuItem::Checkmark(item) => labels.push(item.label.clone()),
                MenuItem::RadioGroup(group) => labels.push(
                    group
                        .options
                        .iter()
                        .map(|option| option.label.as_str())
                        .collect::<Vec<_>>()
                        .join("/"),
                ),
                MenuItem::SubMenu(submenu) => {
                    labels.push(submenu.label.clone());
                    collect_labels(&submenu.submenu, labels);
                }
                MenuItem::Separator => {}
            }
        }
    }

    fn radio_group(menu: &[MenuItem<OrbitTray>]) -> Option<&RadioGroup<OrbitTray>> {
        menu.iter().find_map(|item| match item {
            MenuItem::RadioGroup(group) => Some(group),
            MenuItem::SubMenu(submenu) => radio_group(&submenu.submenu),
            _ => None,
        })
    }

    fn station() -> TrayStation {
        TrayStation {
            name: "WFMU".to_owned(),
            country: "US".to_owned(),
            tags: "eclectic, freeform".to_owned(),
            bitrate: 128,
            homepage: Some("https://wfmu.org".to_owned()),
        }
    }

    #[test]
    fn icon_stays_active_while_the_process_lives() {
        // Noctalia v5 скрывает пассивные элементы, поэтому остановка эфира и
        // спрятанное окно иконку больше не убирают (ADR-0020, ревизия 0.16).
        assert_eq!(tray(TraySnapshot::default()).status(), Status::Active);
        let with_station = TraySnapshot {
            station: Some(station()),
            ..Default::default()
        };
        assert_eq!(tray(with_station).status(), Status::Active);
    }

    #[test]
    fn tooltip_shows_the_station_or_silence() {
        assert_eq!(tray(english()).tool_tip().description, "Nothing playing");
        let snapshot = TraySnapshot {
            station: Some(station()),
            ..english()
        };
        assert_eq!(tray(snapshot).tool_tip().description, "WFMU");
    }

    #[test]
    fn menu_groups_playback_volume_appearance_and_station() {
        let menu = tray(english()).menu();
        let items = labels(&menu);
        assert_eq!(items.first(), Some(&"Nothing playing".to_owned()));
        assert!(items.contains(&"Show window".to_owned()));
        assert!(items.contains(&"Playback".to_owned()));
        assert!(items.contains(&"Play/Pause".to_owned()));
        assert!(items.contains(&"Radio Roulette".to_owned()));
        assert!(items.contains(&"Volume · 0%".to_owned()));
        assert!(items.contains(&"Volume Up".to_owned()));
        assert!(items.contains(&"Appearance".to_owned()));
        assert!(items.contains(&"System/Dark/Light".to_owned()));
        assert!(items.contains(&"About station".to_owned()));
        assert!(items.contains(&"Add to favorites".to_owned()));
        assert!(items.contains(&"Open homepage".to_owned()));
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
    fn menu_card_shows_the_current_station_without_being_a_command() {
        let playing = tray(TraySnapshot {
            playing: true,
            station: Some(station()),
            favorite: true,
            volume_percent: 70,
            ..english()
        })
        .menu();
        let card = playing[..3]
            .iter()
            .map(|item| match item {
                MenuItem::Standard(item) => item.label.clone(),
                _ => panic!("карточка состоит из стандартных строк"),
            })
            .collect::<Vec<_>>();
        assert_eq!(
            card,
            vec!["Now playing: WFMU", "US · 128 kbps", "eclectic, freeform"]
        );
        let MenuItem::Standard(name) = &playing[0] else {
            panic!("первым идёт имя станции");
        };
        assert!(!name.enabled, "карточка — витрина, а не команда");
        let items = labels(&playing);
        assert!(items.contains(&"Volume · 70%".to_owned()));
        assert!(items.contains(&"Remove from favorites".to_owned()));
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
            let volume = format!("{} · 0%", text.tray.volume);
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
                    text.tray.playback.to_owned(),
                    text.tray.play_pause.to_owned(),
                    text.tray.stop.to_owned(),
                    text.tray.previous.to_owned(),
                    text.tray.next.to_owned(),
                    text.tray.radio_roulette.to_owned(),
                    volume,
                    text.tray.volume_up.to_owned(),
                    text.tray.volume_down.to_owned(),
                    text.tray.appearance.to_owned(),
                    themes,
                    text.tray.about_station.to_owned(),
                    text.common.add_favorite.to_owned(),
                    text.tray.open_homepage.to_owned(),
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
        assert_eq!(radio_group(&menu).map(|group| group.selected), Some(2));
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

    /// Пункты меню шлют те же команды, что и жесты: подменю не обходит общий
    /// канал и пробуждение ядра.
    #[test]
    fn menu_actions_publish_their_commands() {
        let (commands, mut sink) = tokio::sync::mpsc::unbounded_channel();
        let hits = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&hits);
        let mut tray = OrbitTray {
            app_id: "org.modernbyte.OrbitFM",
            title: "OrbitFM",
            snapshot: TraySnapshot {
                playing: true,
                station: Some(station()),
                favorite: true,
                ..english()
            },
            commands,
            wake: Arc::new(move || {
                counter.fetch_add(1, Ordering::SeqCst);
            }),
        };

        let menu = tray.menu();
        let about = menu
            .iter()
            .find_map(|item| match item {
                MenuItem::SubMenu(submenu) if submenu.label == "About station" => Some(submenu),
                _ => None,
            })
            .expect("подменю о станции");
        let MenuItem::Standard(favorite) = &about.submenu[0] else {
            panic!("первым идёт избранное");
        };
        (favorite.activate)(&mut tray);
        let MenuItem::Standard(homepage) = &about.submenu[1] else {
            panic!("вторым идёт домашняя страница");
        };
        (homepage.activate)(&mut tray);
        let group = radio_group(&menu).expect("подменю темы");
        (group.select)(&mut tray, 0);

        assert_eq!(sink.try_recv(), Ok(TrayCommand::ToggleFavorite));
        assert_eq!(sink.try_recv(), Ok(TrayCommand::OpenHomepage));
        assert_eq!(
            sink.try_recv(),
            Ok(TrayCommand::SetTheme(ThemeMode::System))
        );
        assert_eq!(hits.load(Ordering::SeqCst), 3);
    }
}
