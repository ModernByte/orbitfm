# Модуль: Системный трей (`src/tray/`)

**Путь в коде:** `src/tray/*`  
**Ответственность:** Трей: StatusNotifierItem на Linux и элемент строки меню на
macOS — иконка, тултип/меню, клики и колесо; приём `TrayCommand` от пользователя
и публикация `TraySnapshot` от Application Core.  
**Зависимости:** `ksni` (только Linux, optional-фича `tray`), `tray-icon` и
`muda` (только macOS, optional-фича `tray`), `tokio::sync::mpsc`,
`tracing`, `crate::ipc::WakeFn` (тип пробуждения), `crate::settings::{Language, ThemeMode}`,
`crate::i18n` (каталог подписей меню, ADR-0030)  
**Решение:** ADR-0020 «Иконка в системном трее (StatusNotifierItem на ksni)»,
ADR-0026 «macOS: меню-бар и Now Playing при скрытом окне»

---

## 1. Назначение и границы

Правило проекта — «закрытие окна = Hide, процесс живёт» (ADR-0018), поэтому у
пользователя должен оставаться способ управлять радио без окна. Трей — третий
источник команд наравне с IPC (ADR-0005) и MPRIS (ADR-0006): он не заводит ни
своего протокола, ни своей бизнес-логики.

| Живёт в `src/tray/` | Не живёт в `src/tray/` |
|---|---|
| Регистрация SNI-item, меню `com.canonical.dbusmenu`, pixmap иконки, поток-владелец, каналы снимков и команд | Решение, что означает команда (`Session::execute_tray_command`), состояние эфира, станции, темы, языка, окно |
| `TrayCommand` / `TraySnapshot` / `TrayHandle` / `spawn` — контракт слоя | `IpcCommand`-протокол (`src/ipc/`), MPRIS-метаданные (`src/media/`), egui (`src/ui/`), каталог строк (`src/i18n/`) |

Модуль не знает ни про egui, ни про `Station`: наружу идут только доменные
типы-сообщения (`docs/interfaces/11-tray.md`). Обратная зависимость тоже
односторонняя: `Session` вызывает `tray::spawn` и работает с `TrayHandle`, а
`src/tray/` про `Session` ничего не импортирует.

На macOS SNI не существует; там работает отдельный бэкенд `macos.rs` на
`tray-icon`/`muda` с тем же контрактом `TraySnapshot`/`TrayCommand`
(ADR-0026). Linux-бэкенд (`sni.rs`) не меняется.

## 2. Состав пакета

| Файл | Ответственность | Наружу (`pub(crate)`) |
|------|-----------------|------------------------|
| `mod.rs` | Контракт слоя: типы-сообщения, `TrayHandle`, точка входа и её no-op-заглушка; на macOS — «неподключённый» `Slot` и отложенный `attach` | `VOLUME_STEP`, `TrayCommand`, `TraySnapshot`, `TrayHandle::{publish, attach, commands}`, `spawn` |
| `sni.rs` | Реализация `ksni::Tray`, поток-владелец, меню, деградация (`#[cfg(all(target_os = "linux", feature = "tray"))]`) | `OrbitTray`, `start` — только для `mod.rs` |
| `macos.rs` | Элемент строки меню на `tray-icon`/`muda`: `Slot`, сборка меню, стабильные `MenuId`, глобальный `MenuEvent` (`#[cfg(all(target_os = "macos", feature = "tray"))]`) | `Slot::{new, publish, attach}` — только для `mod.rs` |
| `text.rs` | Общие подписи «сейчас играет» и Show/Hide для обоих бэкендов | `now_playing_label`, `nothing_playing`, `toggle_label` |
| `icon.rs` | Платформенно-нейтральный RGBA-растеризатор орбитального знака 22/32/48; Linux-адаптер конвертирует в ARGB32, macOS — в `tray_icon::Icon` | `SIZES`, `rgba` — для `sni.rs`/`macos.rs` |

`mod.rs` компилируется всегда, чтобы `src/session/` не обрастал `cfg`-ветками:
на других платформах и в сборке без фичи `spawn` возвращает `None` с той же
сигнатурой; `attach` на не-macOS — no-op.

## 3. Жизненный цикл

1. `launch` (`src/main.rs`) после IPC, аудио и MPRIS вызывает
   `tray::spawn(APP_ID, WINDOW_TITLE, wake.waker())`.
2. `start` поднимает `std::thread` с именем `orbitfm-tray` и сразу возвращает
   `true`/`false` — результат регистрации поток узнает позже, и это не отменяет
   запуск приложения.
3. На потоке `serve` вызывает `tray.spawn()` (ksni `blocking::TrayMethods`):
   вся D-Bus-регистрация выполняется синхронно на этом потоке через общий
   process-global current-thread runtime, поэтому потоки `rt-multi-thread` для
   этого пути запрещены (ADR-0020 п. 4).
4. Ошибка регистрации или `Handle::is_closed()` → `warn!` и выход из потока:
   канал снимков просто остаётся без читателя, `publish` вернёт `false` при
   первой же публикации.
5. Живой поток крутит цикл: `recv` снимка → сворачивание накопившейся очереди
   `try_recv` до последнего → `Handle::update` (пересборка свойств и меню,
   рассылка сигналов). `update` блокирует именно этот поток, никогда UI.
6. Закрытие канала снимков (Session дропнут при выходе) → `handle.shutdown().wait()`.

На macOS жизненный цикл другой: `spawn` создаёт канал команд и `Slot` без
иконки; `TrayIcon` строится в `OrbitApp::new` через `Session::attach_tray` →
`Slot::attach`, когда eframe уже запустил run loop (требование `tray-icon`).
Снимок, пришедший до `attach`, копится в `Slot::pending` (последний побеждает),
поэтому меню сразу собирается с актуальными подписями. После `attach` пункты
обновляются на месте `set_text`/`set_checked`; `MenuId` стабильны, меню и иконка
не пересоздаются. `Slot` живёт в `Session` и переживает пересоздание окна при
смене режима. Ошибка создания — `warn!`, `attach` возвращает `false`,
`Session::attach_tray` обнуляет `tray`/`tray_snapshot`: приложение работает без
трея (ADR-0026).

Пробуждение GUI: каждый колбэк ksni и каждый `MenuEvent` macOS после
`commands.send(..)` зовёт `wake`. При открытом окне слот `WakeHandle` держит
`request_repaint`, и команды вычерпывает `tick()`; при закрытом на Linux слот
пуст, и команды ждёт ветка `tray_recv` в `Session::park`; на macOS скрытое окно
само получает logic-проходы, и команды забирает `tick()`.

## 4. Витрина

| Что видит хост | Откуда |
|----------------|--------|
| `Id`, `IconName` | `app_id` = `org.modernbyte.OrbitFM` |
| `Title`, `ToolTip.title` | `WINDOW_TITLE` = `OrbitFM` |
| `Category` | `ApplicationStatus` |
| `Status` | `Active`, если окно видно или идёт эфир |
| `ToolTip.description` | `station` из снимка, иначе `text.tray.nothing_playing` на языке снимка |
| `IconPixmap` | `sni.rs::ICONS` — 22/32/48, RGBA из `icon::rgba`, переставленный в ARGB32 network byte order |
| `IconThemePath` | первый существующий `.../icons/hicolor` (XDG_DATA_HOME, затем `/usr/share`), иначе пусто |
| `Menu` | `/MenuBar`, собирается из снимка на каждое обновление |
| Первый пункт меню | неактивная витрина: шаблон `text.tray.now_playing` или `text.tray.paused` с `{station}`, либо `text.tray.nothing_playing` — из `station`, `playing` и `language` |
| `ItemIsMenu` | `false` (константа `MENU_ON_ACTIVATE` по умолчанию) — только тогда хост вызывает `Activate` |

Иконка рисуется процедурно (диск планеты, наклонённое кольцо, точка эфира —
пропорции `ui::widgets::logo_mark`) с 4× суперсэмплингом и src-over
композицией слоёв. Новый бинарный ассет в репозиторий не добавлялся. На macOS
знак растеризуется из той же геометрии в 48 px и ставится template-иконкой
(`with_icon_templated`): монохромный образ сам подстраивается под светлую и
тёмную тему строки меню (ADR-0026).

Подписи меню — из `i18n::catalog` языка снимка, группа `text.tray` (ADR-0030);
общие строки «сейчас играет» и Show/Hide собирает `text.rs` для обоих бэкендов.
Порядок пунктов SNI: неактивная
первая строка · разделитель · `show_window`/`hide_window` · разделитель ·
`play_pause` (чекмарк) · `stop` · `previous` · `next` · `radio_roulette` ·
разделитель · `volume_up` · `volume_down` · разделитель · радиогруппа тем ·
разделитель · `settings` · `quit`. Меню macOS повторяет тот же порядок и те же
подписи; выбранная тема и `play_pause` — чекбоксы. Подписи темы даёт
`i18n::theme_label`: presentation-слой (`ui::palette`) модулю трея по-прежнему
недоступен.

## 5. Ошибки и деградация

Типизированных ошибок у модуля нет: `spawn` возвращает `Option`, потому что
отсутствие трея — не ошибка, а штатное состояние окружения. `ksni::Error` —
`#[non_exhaustive]` с тремя вариантами (`Dbus`, `Watcher`, `WontShow`), поэтому
`match` обязан иметь catch-all; он есть, и любая ошибка превращается в `warn!`.

- `watcher_offline` возвращает `true`: хост мог уйти временно (перезапуск
  оболочки), а `false` закрыл бы соединение и выключил трей до конца жизни
  процесса.
- Мёртвый поток-владелец `Session` замечает по `publish -> false` и по
  `recv() -> None` в `tray_recv`, после чего ветка `select!` гасится — иначе
  припаркованный цикл крутился бы впустую.
- В сборке без фичи `tray` варианты `TrayCommand` никто не конструирует, и они
  помечены `cfg_attr(..., allow(dead_code))`, а не удалены: `Session` исполняет
  их без `cfg`-веток.
- macOS: `Icon::from_rgba`, сборка меню и `TrayIconBuilder::build` возвращают
  `Result`; любая ошибка — `warn!` и `attach -> false`. `MenuEvent` ставится
  глобальным обработчиком один раз (OnceLock `muda`); если канал команд закрыт,
  событие логируется и игнорируется. `AppKit`-вызовы обязаны идти на главном
  потоке — поэтому `attach` вызывается только из `OrbitApp::new`.

## 6. Текущие ограничения

- `Volume Up/Down` не показывают текущую громкость: у SNI нет витрины для неё.
- Flatpak/sandbox: признак, при котором нужен `disable_dbus_name(true)`, в коде
  не зашит — он должен быть уточнён live-проверкой flatpak-сборки (ADR-0020 п. 8).
- `ksni::current_xdg_activation_token()` существует, но не потребляется: eframe
  0.37 + winit 0.30.13 не читают токен, поэтому показ окна из трея по-прежнему
  зависит от композитора (фокус на Wayland не гарантируется).
- Разные dbusmenu-хосты по-разному реагируют на `Category`/`ItemIsMenu` —
  проверено на Noctalia, другие хосты (KDE, waybar) не тестировались.
- macOS-трей проверен компиляцией и unit-тестами; live-приёмка (template-иконка
  в светлой/тёмной теме, меню, Cmd-W/Cmd-Q, Quit) требует живого Mac и входит в
  матрицу ADR-0026.

## 7. Тестирование

Unit-тесты покрывают контракт, не касаясь D-Bus и AppKit: `mod.rs` — значения
`Default` (тема и язык) и сравнение снимков, где смена языка — тоже изменение;
`sni.rs` — `Status` по снимку, тултип, состав и состояние меню (заголовок-витрина
неактивна, чекмарк `play_pause`, выбранная тема), равенство всех подписей
каталогу на каждом языке, правило «одно действие = одна команда + одно
пробуждение» и байтовый порядок пикселей; `macos.rs` — каждый `MenuId`
переводится в свою `TrayCommand`, ID уникальны, чужие ID игнорируются;
`text.rs` — заголовок Now playing/Paused/Nothing playing и Show/Hide по
видимости; `icon.rs` — все размерности RGBA, непрозрачное ядро планеты и пустые
углы, выход орбиты за диск, наличие точки эфира.

Живая проверка — ADR-0020, «Проверка реализации».

## 8. Связанные документы

- `docs/interfaces/11-tray.md` — формальный контракт модуля
- ADR-0020 — протокол, зависимость, меню, иконка, деградация
- ADR-0030 — каталог `text.tray` и язык снимка (заменяет English-only п. 5 ADR-0020)
- ADR-0026 — macOS-бэкенд строки меню и скрытие окна без остановки run loop
- ADR-0037 — команды `volume-up`/`volume-down` и общий снимок для интеграций
- ADR-0031 — окно настроек, которое открывает команда `Settings`
- ADR-0018 — зачем трей нужен и куда впадают его команды
- `docs/modules/11-session.md` § 4.13 — исполнение `TrayCommand` в Core
- `docs/modules/13-i18n.md` — модуль каталога строк
- `docs/modules/08-noctalia-wm.md` — Noctalia как SNI-хост и виджет
