import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import * as PanelMenu from 'resource:///org/gnome/shell/ui/panelMenu.js';
import * as PopupMenu from 'resource:///org/gnome/shell/ui/popupMenu.js';
import Clutter from 'gi://Clutter';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import St from 'gi://St';

const POLL_SECONDS = 2;
const MAX_LABEL_LENGTH = 48;
const MAX_TAGS = 2;

const STATE_CLASSES = ['playing', 'paused', 'loading', 'error', 'stopped', 'offline'];

export default class OrbitFMExtension extends Extension {
    enable() {
        this._binary = '@ORBITFM@';
        this._status = null;
        this._ui = {};
        this._refreshing = false;
        this._timeoutId = 0;

        this._indicator = new PanelMenu.Button(0.0, this.metadata.name, false);
        const box = new St.BoxLayout({style_class: 'panel-status-menu-box'});
        this._icon = new St.Icon({
            icon_name: 'audio-x-generic-symbolic',
            style_class: 'system-status-icon',
        });
        this._label = new St.Label({
            text: 'OrbitFM',
            style_class: 'panel-button orbitfm-panel-label',
            y_align: Clutter.ActorAlign.CENTER,
        });
        box.add_child(this._icon);
        box.add_child(this._label);
        this._indicator.add_child(box);
        this._stateClass('offline');
        Main.panel.addToStatusArea(this.uuid, this._indicator);

        this._buildMenu();
        this._refresh();
        this._timeoutId = GLib.timeout_add_seconds(
            GLib.PRIORITY_DEFAULT,
            POLL_SECONDS,
            () => {
                this._refresh();
                return GLib.SOURCE_CONTINUE;
            });
    }

    disable() {
        if (this._timeoutId) {
            try {
                GLib.source_remove(this._timeoutId);
            } catch (e) {
                // источник уже снят
            }
            this._timeoutId = 0;
        }
        if (this._indicator) {
            this._indicator.destroy();
            this._indicator = null;
        }
        this._status = null;
        this._ui = {};
        this._items = null;
        this._themeItems = null;
    }

    _buildMenu() {
        const menu = this._indicator.menu;
        this._items = {};

        this._buildCard(menu);
        menu.addMenuItem(new PopupMenu.PopupSeparatorMenuItem());

        this._playItem = this._addItem(
            menu, 'play_pause', 'playpause', 'media-playback-start-symbolic');
        for (const [key, command, icon] of [
            ['previous', 'previous', 'media-skip-backward-symbolic'],
            ['next', 'next', 'media-skip-forward-symbolic'],
            ['stop', 'stop', 'media-playback-stop-symbolic'],
            ['radio_roulette', 'roulette', 'media-playlist-shuffle-symbolic'],
        ])
            this._addItem(menu, key, command, icon);

        menu.addMenuItem(new PopupMenu.PopupSeparatorMenuItem());

        this._volumeItem = new PopupMenu.PopupMenuItem('...');
        this._volumeItem.reactive = false;
        menu.addMenuItem(this._volumeItem);
        this._addItem(menu, 'volume_up', 'volume-up', 'audio-volume-high-symbolic');
        this._addItem(menu, 'volume_down', 'volume-down', 'audio-volume-low-symbolic');

        menu.addMenuItem(new PopupMenu.PopupSeparatorMenuItem());
        this._themeMenu = new PopupMenu.PopupSubMenuMenuItem('...');
        this._themeItems = {};
        for (const mode of ['system', 'dark', 'light']) {
            const item = new PopupMenu.PopupMenuItem('...');
            item.connect('activate', () => this._run(`theme ${mode}`));
            this._themeItems[mode] = item;
            this._themeMenu.menu.addMenuItem(item);
        }
        menu.addMenuItem(this._themeMenu);

        this._aboutMenu = new PopupMenu.PopupSubMenuMenuItem('...');
        this._favoriteItem = new PopupMenu.PopupImageMenuItem(
            '...', 'emblem-favorite-symbolic');
        this._favoriteItem.connect('activate', () => this._run('favorite'));
        this._aboutMenu.menu.addMenuItem(this._favoriteItem);
        this._homepageItem = new PopupMenu.PopupImageMenuItem(
            '...', 'web-browser-symbolic');
        this._homepageItem.connect('activate', () => this._openHomepage());
        this._aboutMenu.menu.addMenuItem(this._homepageItem);
        menu.addMenuItem(this._aboutMenu);

        menu.addMenuItem(new PopupMenu.PopupSeparatorMenuItem());

        this._windowItem = new PopupMenu.PopupImageMenuItem(
            '...', 'view-fullscreen-symbolic');
        this._windowItem.connect('activate', () => this._run('toggle'));
        menu.addMenuItem(this._windowItem);

        this._settingsItem = this._addItem(
            menu, 'settings', 'settings', 'preferences-system-symbolic');
    }

    _buildCard(menu) {
        this._card = new PopupMenu.PopupBaseMenuItem({reactive: false});
        this._card.add_style_class_name('orbitfm-card');

        const row = new St.BoxLayout({
            style_class: 'orbitfm-card-box',
            x_expand: true,
            spacing: 10,
        });
        this._art = new St.Icon({
            icon_name: 'audio-x-generic-symbolic',
            icon_size: 22,
            style_class: 'orbitfm-art',
            y_align: Clutter.ActorAlign.CENTER,
        });

        const text = new St.BoxLayout({
            vertical: true,
            x_expand: true,
            spacing: 1,
            style_class: 'orbitfm-card-text',
        });
        this._title = new St.Label({text: 'OrbitFM', style_class: 'orbitfm-title'});
        this._meta = new St.Label({text: '', style_class: 'orbitfm-meta'});
        this._tags = new St.Label({text: '', style_class: 'orbitfm-meta'});
        text.add_child(this._title);
        text.add_child(this._meta);
        text.add_child(this._tags);

        this._dot = new St.Widget({
            style_class: 'orbitfm-dot',
            width: 8,
            height: 8,
            y_align: Clutter.ActorAlign.CENTER,
        });

        row.add_child(this._art);
        row.add_child(text);
        row.add_child(this._dot);
        this._card.add_child(row);
        menu.addMenuItem(this._card);
    }

    _addItem(menu, key, command, icon) {
        const item = new PopupMenu.PopupImageMenuItem('...', icon);
        item.connect('activate', () => this._run(command));
        this._items[key] = item;
        menu.addMenuItem(item);
        return item;
    }

    _shorten(text) {
        if (text.length <= MAX_LABEL_LENGTH)
            return text;
        return `${text.slice(0, MAX_LABEL_LENGTH - 1)}…`;
    }

    _stateClass(state) {
        for (const name of STATE_CLASSES)
            this._indicator.remove_style_class_name(`orbitfm-state-${name}`);
        this._indicator.add_style_class_name(`orbitfm-state-${state}`);
        if (this._dot)
            this._dot.style_class = `orbitfm-dot orbitfm-dot-${state}`;
    }

    _setEnabled(item, enabled) {
        if (item)
            item.setSensitive(Boolean(enabled));
    }

    _applyLabels(ui, station) {
        const labels = {
            play_pause: ui.play_pause,
            previous: ui.previous,
            next: ui.next,
            stop: ui.stop,
            radio_roulette: ui.radio_roulette,
            volume_up: ui.volume_up,
            volume_down: ui.volume_down,
            settings: ui.settings,
        };
        for (const [key, item] of Object.entries(this._items)) {
            if (labels[key] && item.label)
                item.label.set_text(labels[key]);
        }
        this._themeMenu.label.set_text(ui.theme || 'Theme');
        for (const [mode, item] of Object.entries(this._themeItems)) {
            const label = ui[`theme_${mode}`] || mode;
            item.label.set_text(label);
        }
        this._aboutMenu.label.set_text(ui.about_station || 'About station');
        this._favoriteItem.label.set_text(
            station && station.favorite
                ? (ui.remove_favorite || '...')
                : (ui.add_favorite || '...'));
        this._homepageItem.label.set_text(ui.open_homepage || 'Open homepage');
    }

    _apply(status) {
        this._status = status;
        const ui = status.ui || {};
        this._ui = ui;
        const station = status.station || null;
        const state = this._stateKind(status.state);
        const volume = Math.round((status.volume || 0) * 100);
        const visible = status.window ? Boolean(status.window.visible) : false;

        this._label.set_text(this._shorten(station ? station.name : (ui.nothing_playing || 'OrbitFM')));
        this._stateClass(state);
        this._applyLabels(ui, station);

        this._playItem.icon.icon_name = state === 'playing'
            ? 'media-playback-pause-symbolic'
            : 'media-playback-start-symbolic';
        this._playItem.setSensitive(Boolean(station));
        for (const key of ['previous', 'next'])
            this._setEnabled(this._items[key], status.can ? status.can[key] : false);
        this._setEnabled(this._items.stop, Boolean(station));
        this._setEnabled(this._items.radio_roulette, true);
        this._setEnabled(this._items.volume_up, true);
        this._setEnabled(this._items.volume_down, true);

        this._volumeItem.label.set_text(`${ui.volume || 'Volume'} · ${volume}%`);

        for (const [mode, item] of Object.entries(this._themeItems)) {
            item.setOrnament(status.theme === mode
                ? PopupMenu.Ornament.DOT
                : PopupMenu.Ornament.NONE);
        }

        this._setEnabled(this._favoriteItem, Boolean(station));
        const homepage = station && typeof station.homepage === 'string' && /^https?:\/\//i.test(station.homepage)
            ? station.homepage
            : '';
        this._homepageItem.visible = homepage.length > 0;
        this._setEnabled(this._homepageItem, homepage.length > 0);

        this._windowItem.label.set_text(
            visible ? (ui.hide_window || '...') : (ui.show_window || '...'));
        this._windowItem.icon.icon_name = visible
            ? 'view-restore-symbolic'
            : 'view-fullscreen-symbolic';

        this._title.set_text(station ? station.name : (ui.nothing_playing || 'OrbitFM'));
        const meta = [];
        if (station) {
            if (station.source === 'stream')
                meta.push('stream');
            else if (station.country)
                meta.push(station.country);
            if (station.bitrate)
                meta.push(`${station.bitrate} kbps`);
        }
        this._meta.set_text(meta.join(' · '));
        this._meta.visible = meta.length > 0;

        const tags = station && typeof station.tags === 'string'
            ? station.tags.split(',').map(tag => tag.trim()).filter(tag => tag.length > 0)
            : [];
        this._tags.set_text(tags.slice(0, MAX_TAGS).join(' · '));
        this._tags.visible = tags.length > 0;
    }

    _stateKind(state) {
        switch (state) {
        case 'playing':
            return 'playing';
        case 'paused':
            return 'paused';
        case 'buffering':
        case 'reconnecting':
            return 'loading';
        case 'error':
            return 'error';
        default:
            return 'stopped';
        }
    }

    _setOffline() {
        const ui = this._ui || {};
        this._status = null;
        this._label.set_text('OrbitFM');
        this._stateClass('offline');
        if (this._title)
            this._title.set_text(ui.nothing_playing || 'OrbitFM');
        if (this._meta) {
            this._meta.set_text('');
            this._meta.visible = false;
        }
        if (this._tags) {
            this._tags.set_text('');
            this._tags.visible = false;
        }
        if (this._volumeItem)
            this._volumeItem.label.set_text(`${ui.volume || 'Volume'} · 0%`);
        for (const key of ['play_pause', 'previous', 'next', 'stop', 'radio_roulette'])
            this._setEnabled(this._items[key], false);
        this._setEnabled(this._favoriteItem, false);
        this._setEnabled(this._homepageItem, false);
        this._homepageItem.visible = false;
    }

    _refresh() {
        if (this._refreshing)
            return;
        this._refreshing = true;

        let proc;
        try {
            proc = Gio.Subprocess.new(
                [this._binary, 'status', '--json'],
                Gio.SubprocessFlags.STDOUT_PIPE | Gio.SubprocessFlags.STDERR_SILENCE);
        } catch (e) {
            this._refreshing = false;
            this._setOffline();
            return;
        }

        proc.communicate_utf8_async(null, null, (source, result) => {
            this._refreshing = false;
            try {
                const [, stdout] = source.communicate_utf8_finish(result);
                if (!source.get_successful())
                    throw new Error('orbitfm status failed');
                this._apply(JSON.parse(stdout));
            } catch (e) {
                this._setOffline();
            }
        });
    }

    _openHomepage() {
        const station = this._status ? this._status.station : null;
        const url = station && typeof station.homepage === 'string' ? station.homepage : '';
        if (!/^https?:\/\//i.test(url))
            return;
        try {
            Gio.AppInfo.launch_default_for_uri(url, null);
        } catch (e) {
            console.error(`OrbitFM: ${e.message}`);
        }
    }

    _run(command) {
        try {
            Gio.Subprocess.new(
                [this._binary, ...command.split(' ')],
                Gio.SubprocessFlags.STDOUT_SILENCE | Gio.SubprocessFlags.STDERR_SILENCE);
        } catch (e) {
            console.error(`OrbitFM: ${e.message}`);
        }
    }
}
