import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import * as PanelMenu from 'resource:///org/gnome/shell/ui/panelMenu.js';
import * as PopupMenu from 'resource:///org/gnome/shell/ui/popupMenu.js';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import St from 'gi://St';

const POLL_SECONDS = 2;
const MAX_LABEL_LENGTH = 48;

export default class OrbitFMExtension extends Extension {
    enable() {
        this._binary = '@ORBITFM@';
        this._status = null;
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
            style_class: 'panel-button',
        });
        box.add_child(this._icon);
        box.add_child(this._label);
        this._indicator.add_child(box);
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
        this._items = null;
        this._windowItem = null;
        this._summary = null;
        this._status = null;
    }

    _buildMenu() {
        const menu = this._indicator.menu;

        this._summary = new PopupMenu.PopupMenuItem('OrbitFM');
        this._summary.reactive = false;
        menu.addMenuItem(this._summary);
        menu.addMenuItem(new PopupMenu.PopupSeparatorMenuItem());

        this._items = {};
        const playback = [
            ['play_pause', 'playpause'],
            ['previous', 'previous'],
            ['next', 'next'],
            ['stop', 'stop'],
            ['radio_roulette', 'roulette'],
        ];
        for (const [key, command] of playback) {
            const item = new PopupMenu.PopupMenuItem('...');
            item.connect('activate', () => this._run(command));
            this._items[key] = item;
            menu.addMenuItem(item);
        }

        menu.addMenuItem(new PopupMenu.PopupSeparatorMenuItem());

        this._windowItem = new PopupMenu.PopupMenuItem('...');
        this._windowItem.connect('activate', () => this._run('toggle'));
        menu.addMenuItem(this._windowItem);

        this._settingsItem = new PopupMenu.PopupMenuItem('...');
        this._settingsItem.connect('activate', () => this._run('settings'));
        this._items.settings = this._settingsItem;
        menu.addMenuItem(this._settingsItem);
    }

    _shorten(text) {
        if (text.length <= MAX_LABEL_LENGTH)
            return text;
        return `${text.slice(0, MAX_LABEL_LENGTH - 1)}…`;
    }

    _applyLabels(ui) {
        const labels = {
            play_pause: ui.play_pause,
            previous: ui.previous,
            next: ui.next,
            stop: ui.stop,
            radio_roulette: ui.radio_roulette,
            settings: ui.settings,
        };
        for (const [key, item] of Object.entries(this._items)) {
            if (labels[key] && item.label)
                item.label.set_text(labels[key]);
        }
    }

    _apply(status) {
        this._status = status;
        const ui = status.ui || {};
        const station = status.station ? status.station.name : '';
        const text = station || ui.nothing_playing || 'OrbitFM';
        const volume = Math.round((status.volume || 0) * 100);

        this._label.set_text(this._shorten(text));
        this._applyLabels(ui);

        if (this._summary.label)
            this._summary.label.set_text(station ? `${text} · ${volume}%` : text);

        if (this._windowItem && this._windowItem.label) {
            const visible = status.window ? status.window.visible : false;
            this._windowItem.label.set_text(
                visible ? (ui.hide_window || '...') : (ui.show_window || '...'));
        }
    }

    _setOffline() {
        this._status = null;
        this._label.set_text('OrbitFM');
        if (this._summary && this._summary.label)
            this._summary.label.set_text('OrbitFM');
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

    _run(command) {
        try {
            Gio.Subprocess.new(
                [this._binary, command],
                Gio.SubprocessFlags.STDOUT_SILENCE | Gio.SubprocessFlags.STDERR_SILENCE);
        } catch (e) {
            console.error(`OrbitFM: ${e.message}`);
        }
    }
}
