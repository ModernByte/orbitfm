//! Трей в строке меню macOS на `tray-icon`/`muda` (ADR-0026, п. 5–8).
//!
//! Контракт общий с SNI: наружу торчат те же [`TraySnapshot`] и
//! [`TrayCommand`], а `Session` публикует снимки и исполняет команды без
//! платформенных веток. Отличие одно — иконка создаётся не в своём потоке, а на
//! главном потоке после старта run loop: этого требует `tray-icon`. Поэтому
//! [`super::spawn`] отдаёт «неподключённый» [`Slot`], а `OrbitApp::new` зовёт
//! [`Slot::attach`], когда eframe уже крутит цикл событий.
//!
//! Снимок, пришедший до `attach`, копится в `pending` (последний побеждает):
//! меню строится сразу с актуальными подписями. После `attach` каждый снимок
//! обновляет пункты на месте — иконка и `MenuId` не пересоздаются.

use std::cell::Cell;

use tokio::sync::mpsc::UnboundedSender;
use tracing::warn;
use tray_icon::menu::{
    CheckMenuItem, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem, Submenu,
};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

use super::{TrayCommand, TraySnapshot, icon, text};
use crate::i18n;
use crate::ipc::WakeFn;
use crate::settings::ThemeMode;

/// Пиксельный размер знака: `tray-icon` ужимает иконку до 22 pt строки меню,
/// а запас в 48 px даёт чёткость на Retina. Число берётся из верхнего края
/// ряда `icon::SIZES`, чтобы SNI- и macOS-бэкенды не разъезжались
/// (ADR-0026, п. 8).
const ICON_SIZE: i32 = icon::SIZES[icon::SIZES.len() - 1];

/// Стабильные ID пунктов: меню не пересобирается, обработчик переводит ID в
/// команду (ADR-0026, п. 7).
const ID_TOGGLE: &str = "orbitfm.toggle";
const ID_PLAY_PAUSE: &str = "orbitfm.play-pause";
const ID_STOP: &str = "orbitfm.stop";
const ID_PREVIOUS: &str = "orbitfm.previous";
const ID_NEXT: &str = "orbitfm.next";
const ID_ROULETTE: &str = "orbitfm.roulette";
const ID_VOLUME_UP: &str = "orbitfm.volume-up";
const ID_VOLUME_DOWN: &str = "orbitfm.volume-down";
const ID_THEME_SYSTEM: &str = "orbitfm.theme.system";
const ID_THEME_DARK: &str = "orbitfm.theme.dark";
const ID_THEME_LIGHT: &str = "orbitfm.theme.light";
const ID_TOGGLE_FAVORITE: &str = "orbitfm.favorite-toggle";
const ID_OPEN_HOMEPAGE: &str = "orbitfm.homepage";
const ID_SETTINGS: &str = "orbitfm.settings";
const ID_QUIT: &str = "orbitfm.quit";

/// Неподключённый или живой элемент строки меню.
pub(crate) struct Slot {
    app_id: &'static str,
    title: &'static str,
    commands: UnboundedSender<TrayCommand>,
    wake: WakeFn,
    /// Снимок, пришедший до `attach`: обновлять ещё нечего.
    pending: Option<TraySnapshot>,
    menu: Option<MenuHandle>,
    tray: Option<TrayIcon>,
}

impl Slot {
    pub(crate) fn new(
        app_id: &'static str,
        title: &'static str,
        commands: UnboundedSender<TrayCommand>,
        wake: WakeFn,
    ) -> Self {
        Self {
            app_id,
            title,
            commands,
            wake,
            pending: None,
            menu: None,
            tray: None,
        }
    }

    /// Публикует снимок. До `attach` копит последний, после — обновляет пункты
    /// на главном потоке: меню macOS нельзя трогать ни из какого другого.
    pub(crate) fn publish(&mut self, snapshot: &TraySnapshot) -> bool {
        if let Some(menu) = self.menu.as_ref() {
            menu.apply(snapshot);
        } else {
            self.pending = Some(snapshot.clone());
        }
        true
    }

    /// Создаёт иконку и меню. `false` — создать не удалось: приложение
    /// продолжает работать без трея (ADR-0026, п. 6).
    pub(crate) fn attach(&mut self) -> bool {
        if self.tray.is_some() {
            return true;
        }
        let snapshot = self.pending.clone().unwrap_or_default();
        let Some((menu, handle)) = build_menu(&snapshot) else {
            return false;
        };
        install_handler(self.commands.clone(), self.wake.clone());

        let icon = match Icon::from_rgba(icon::rgba(ICON_SIZE), ICON_SIZE as u32, ICON_SIZE as u32)
        {
            Ok(icon) => icon,
            Err(err) => {
                warn!(error = %err, "не удалось построить иконку строки меню");
                return false;
            }
        };
        // Template-режим: монохромный знак сам подстраивается под светлую и
        // тёмную тему (ADR-0026, п. 8).
        match TrayIconBuilder::new()
            .with_id(self.app_id)
            .with_menu(Box::new(menu))
            .with_icon_templated(icon)
            .with_tooltip(self.title)
            .build()
        {
            Ok(tray) => {
                self.menu = Some(handle);
                self.tray = Some(tray);
                true
            }
            Err(err) => {
                warn!(error = %err, "не удалось создать элемент строки меню");
                false
            }
        }
    }
}

/// Подписи и переключатели меню. Пункты создаются один раз со стабильными
/// `MenuId`; дальше меняются только тексты, состояния и наличие строки
/// сведений о станции.
struct MenuHandle {
    menu: Menu,
    header: MenuItem,
    details: MenuItem,
    details_shown: Cell<bool>,
    toggle: MenuItem,
    playback: MenuItem,
    play_pause: CheckMenuItem,
    stop: MenuItem,
    previous: MenuItem,
    next: MenuItem,
    roulette: MenuItem,
    volume: MenuItem,
    volume_up: MenuItem,
    volume_down: MenuItem,
    appearance: Submenu,
    themes: [CheckMenuItem; 3],
    about: Submenu,
    favorite: CheckMenuItem,
    homepage: MenuItem,
    settings: MenuItem,
    quit: MenuItem,
}

impl MenuHandle {
    fn apply(&self, snapshot: &TraySnapshot) {
        let catalog = i18n::catalog(snapshot.language);
        self.header.set_text(text::now_playing_label(snapshot));
        self.apply_details(snapshot);
        self.toggle.set_text(text::toggle_label(snapshot));
        self.playback.set_text(catalog.tray.playback);
        self.play_pause.set_text(catalog.tray.play_pause);
        self.play_pause.set_checked(snapshot.playing);
        self.stop.set_text(catalog.tray.stop);
        self.previous.set_text(catalog.tray.previous);
        self.next.set_text(catalog.tray.next);
        self.roulette.set_text(catalog.tray.radio_roulette);
        self.volume.set_text(text::volume_label(snapshot));
        self.volume_up.set_text(catalog.tray.volume_up);
        self.volume_down.set_text(catalog.tray.volume_down);
        self.appearance.set_text(catalog.tray.appearance);
        for (item, mode) in self.themes.iter().zip(ThemeMode::ALL) {
            item.set_text(i18n::theme_label(snapshot.language, mode));
            item.set_checked(snapshot.theme == mode);
        }
        self.about.set_text(catalog.tray.about_station);
        self.about.set_enabled(snapshot.station.is_some());
        self.favorite.set_text(text::favorite_label(snapshot));
        self.favorite.set_checked(snapshot.favorite);
        self.homepage.set_text(catalog.tray.open_homepage);
        self.homepage.set_enabled(
            snapshot
                .station
                .as_ref()
                .is_some_and(|station| station.homepage.is_some()),
        );
        self.settings.set_text(catalog.tray.settings);
        self.quit.set_text(catalog.tray.quit);
    }

    /// Строка `страна · битрейт · теги` показывается только когда есть что
    /// показать: `muda` не умеет скрывать пункты, поэтому он снимается с меню
    /// и возвращается на своё место.
    fn apply_details(&self, snapshot: &TraySnapshot) {
        let summary = snapshot.station.as_ref().and_then(text::station_summary);
        match (summary, self.details_shown.get()) {
            (Some(summary), false) => {
                self.details.set_text(summary);
                if self.menu.insert(&self.details, 1).is_ok() {
                    self.details_shown.set(true);
                }
            }
            (Some(summary), true) => self.details.set_text(summary),
            (None, true) => {
                if self.menu.remove(&self.details).is_ok() {
                    self.details_shown.set(false);
                }
            }
            (None, false) => {}
        }
    }
}

/// Строит меню в порядке SNI (ADR-0020, ревизия 0.16): карточка, окно,
/// транспорт, громкость, тема, «О станции», настройки, выход.
fn build_menu(snapshot: &TraySnapshot) -> Option<(Menu, MenuHandle)> {
    let catalog = i18n::catalog(snapshot.language);
    let header = MenuItem::with_id(
        "orbitfm.header",
        text::now_playing_label(snapshot),
        false,
        None,
    );
    let details = MenuItem::with_id("orbitfm.details", "", false, None);
    let details_summary = snapshot.station.as_ref().and_then(text::station_summary);
    if let Some(summary) = &details_summary {
        details.set_text(summary);
    }
    let toggle = MenuItem::with_id(ID_TOGGLE, text::toggle_label(snapshot), true, None);
    let playback = MenuItem::with_id("orbitfm.playback", catalog.tray.playback, false, None);
    let play_pause = CheckMenuItem::with_id(
        ID_PLAY_PAUSE,
        catalog.tray.play_pause,
        true,
        snapshot.playing,
        None,
    );
    let stop = MenuItem::with_id(ID_STOP, catalog.tray.stop, true, None);
    let previous = MenuItem::with_id(ID_PREVIOUS, catalog.tray.previous, true, None);
    let next = MenuItem::with_id(ID_NEXT, catalog.tray.next, true, None);
    let roulette = MenuItem::with_id(ID_ROULETTE, catalog.tray.radio_roulette, true, None);
    let volume = MenuItem::with_id("orbitfm.volume", text::volume_label(snapshot), false, None);
    let volume_up = MenuItem::with_id(ID_VOLUME_UP, catalog.tray.volume_up, true, None);
    let volume_down = MenuItem::with_id(ID_VOLUME_DOWN, catalog.tray.volume_down, true, None);
    let themes = [
        CheckMenuItem::with_id(
            ID_THEME_SYSTEM,
            i18n::theme_label(snapshot.language, ThemeMode::System),
            true,
            snapshot.theme == ThemeMode::System,
            None,
        ),
        CheckMenuItem::with_id(
            ID_THEME_DARK,
            i18n::theme_label(snapshot.language, ThemeMode::Dark),
            true,
            snapshot.theme == ThemeMode::Dark,
            None,
        ),
        CheckMenuItem::with_id(
            ID_THEME_LIGHT,
            i18n::theme_label(snapshot.language, ThemeMode::Light),
            true,
            snapshot.theme == ThemeMode::Light,
            None,
        ),
    ];
    let appearance = Submenu::with_items(
        catalog.tray.appearance,
        true,
        &[&themes[0], &themes[1], &themes[2]],
    )
    .ok()?;
    let favorite = CheckMenuItem::with_id(
        ID_TOGGLE_FAVORITE,
        text::favorite_label(snapshot),
        true,
        snapshot.favorite,
        None,
    );
    let homepage = MenuItem::with_id(
        ID_OPEN_HOMEPAGE,
        catalog.tray.open_homepage,
        snapshot
            .station
            .as_ref()
            .is_some_and(|station| station.homepage.is_some()),
        None,
    );
    let about = Submenu::with_items(
        catalog.tray.about_station,
        snapshot.station.is_some(),
        &[&favorite, &homepage],
    )
    .ok()?;
    let settings = MenuItem::with_id(ID_SETTINGS, catalog.tray.settings, true, None);
    let quit = MenuItem::with_id(ID_QUIT, catalog.tray.quit, true, None);

    let menu = Menu::new();
    let mut appended = menu.append(&header);
    if details_summary.is_some() {
        appended = appended.and_then(|()| menu.append(&details));
    }
    appended = appended
        .and_then(|()| menu.append(&PredefinedMenuItem::separator()))
        .and_then(|()| menu.append(&toggle))
        .and_then(|()| menu.append(&PredefinedMenuItem::separator()))
        .and_then(|()| menu.append(&playback))
        .and_then(|()| menu.append(&play_pause))
        .and_then(|()| menu.append(&stop))
        .and_then(|()| menu.append(&previous))
        .and_then(|()| menu.append(&next))
        .and_then(|()| menu.append(&roulette))
        .and_then(|()| menu.append(&PredefinedMenuItem::separator()))
        .and_then(|()| menu.append(&volume))
        .and_then(|()| menu.append(&volume_up))
        .and_then(|()| menu.append(&volume_down))
        .and_then(|()| menu.append(&PredefinedMenuItem::separator()))
        .and_then(|()| menu.append(&appearance))
        .and_then(|()| menu.append(&PredefinedMenuItem::separator()))
        .and_then(|()| menu.append(&about))
        .and_then(|()| menu.append(&PredefinedMenuItem::separator()))
        .and_then(|()| menu.append(&settings))
        .and_then(|()| menu.append(&quit));
    if let Err(err) = appended {
        warn!(error = %err, "не удалось собрать меню строки меню");
        return None;
    }

    let handle = MenuHandle {
        menu,
        header,
        details,
        details_shown: Cell::new(details_summary.is_some()),
        toggle,
        playback,
        play_pause,
        stop,
        previous,
        next,
        roulette,
        volume,
        volume_up,
        volume_down,
        appearance,
        themes,
        about,
        favorite,
        homepage,
        settings,
        quit,
    };
    Some((menu, handle))
}

/// Глобальный обработчик `muda`: пункт меню не несёт замыкания, поэтому ID
/// переводится в команду, кладётся в общую очередь и будит GUI-цикл — тот же
/// путь, что у SNI и IPC (ADR-0026, п. 7).
fn install_handler(commands: UnboundedSender<TrayCommand>, wake: WakeFn) {
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        let Some(command) = command_for(&event.id) else {
            return;
        };
        if let Err(err) = commands.send(command) {
            warn!(?err, "Application Core больше не слушает трей");
        }
        (wake)();
    }));
}

fn command_for(id: &MenuId) -> Option<TrayCommand> {
    match id.0.as_str() {
        ID_TOGGLE => Some(TrayCommand::Toggle),
        ID_PLAY_PAUSE => Some(TrayCommand::PlayPause),
        ID_STOP => Some(TrayCommand::Stop),
        ID_PREVIOUS => Some(TrayCommand::Previous),
        ID_NEXT => Some(TrayCommand::Next),
        ID_ROULETTE => Some(TrayCommand::Roulette),
        ID_VOLUME_UP => Some(TrayCommand::VolumeUp),
        ID_VOLUME_DOWN => Some(TrayCommand::VolumeDown),
        ID_THEME_SYSTEM => Some(TrayCommand::SetTheme(ThemeMode::System)),
        ID_THEME_DARK => Some(TrayCommand::SetTheme(ThemeMode::Dark)),
        ID_THEME_LIGHT => Some(TrayCommand::SetTheme(ThemeMode::Light)),
        ID_TOGGLE_FAVORITE => Some(TrayCommand::ToggleFavorite),
        ID_OPEN_HOMEPAGE => Some(TrayCommand::OpenHomepage),
        ID_SETTINGS => Some(TrayCommand::Settings),
        ID_QUIT => Some(TrayCommand::Quit),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_menu_id_maps_to_its_command() {
        assert_eq!(
            command_for(&MenuId::new(ID_TOGGLE)),
            Some(TrayCommand::Toggle)
        );
        assert_eq!(
            command_for(&MenuId::new(ID_PLAY_PAUSE)),
            Some(TrayCommand::PlayPause)
        );
        assert_eq!(command_for(&MenuId::new(ID_STOP)), Some(TrayCommand::Stop));
        assert_eq!(
            command_for(&MenuId::new(ID_PREVIOUS)),
            Some(TrayCommand::Previous)
        );
        assert_eq!(command_for(&MenuId::new(ID_NEXT)), Some(TrayCommand::Next));
        assert_eq!(
            command_for(&MenuId::new(ID_ROULETTE)),
            Some(TrayCommand::Roulette)
        );
        assert_eq!(
            command_for(&MenuId::new(ID_VOLUME_UP)),
            Some(TrayCommand::VolumeUp)
        );
        assert_eq!(
            command_for(&MenuId::new(ID_VOLUME_DOWN)),
            Some(TrayCommand::VolumeDown)
        );
        assert_eq!(
            command_for(&MenuId::new(ID_THEME_SYSTEM)),
            Some(TrayCommand::SetTheme(ThemeMode::System))
        );
        assert_eq!(
            command_for(&MenuId::new(ID_THEME_DARK)),
            Some(TrayCommand::SetTheme(ThemeMode::Dark))
        );
        assert_eq!(
            command_for(&MenuId::new(ID_THEME_LIGHT)),
            Some(TrayCommand::SetTheme(ThemeMode::Light))
        );
        assert_eq!(
            command_for(&MenuId::new(ID_TOGGLE_FAVORITE)),
            Some(TrayCommand::ToggleFavorite)
        );
        assert_eq!(
            command_for(&MenuId::new(ID_OPEN_HOMEPAGE)),
            Some(TrayCommand::OpenHomepage)
        );
        assert_eq!(
            command_for(&MenuId::new(ID_SETTINGS)),
            Some(TrayCommand::Settings)
        );
        assert_eq!(command_for(&MenuId::new(ID_QUIT)), Some(TrayCommand::Quit));
        assert_eq!(command_for(&MenuId::new("orbitfm.header")), None);
        assert_eq!(command_for(&MenuId::new("orbitfm.details")), None);
        assert_eq!(command_for(&MenuId::new("foreign.item")), None);
    }

    #[test]
    fn ids_are_unique() {
        let ids = [
            ID_TOGGLE,
            ID_PLAY_PAUSE,
            ID_STOP,
            ID_PREVIOUS,
            ID_NEXT,
            ID_ROULETTE,
            ID_VOLUME_UP,
            ID_VOLUME_DOWN,
            ID_THEME_SYSTEM,
            ID_THEME_DARK,
            ID_THEME_LIGHT,
            ID_TOGGLE_FAVORITE,
            ID_OPEN_HOMEPAGE,
            ID_SETTINGS,
            ID_QUIT,
        ];
        let mut unique = ids.to_vec();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), ids.len());
    }
}
