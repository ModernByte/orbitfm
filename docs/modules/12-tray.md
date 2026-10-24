# Модуль: Системный трей (`src/tray/`)

**Путь в коде:** `src/tray/*`  
**Ответственность:** StatusNotifierItem для SNI-хостов: иконка, тултип, меню,
клики и колесо; приём `TrayCommand` от пользователя и публикация `TraySnapshot`
от Application Core.  
**Зависимости:** `ksni` (только Linux, optional-фича `tray`), `tokio::sync::mpsc`,
`tracing`, `crate::ipc::WakeFn` (тип пробуждения), `crate::settings::ThemeMode`  
**Решение:** ADR-0020 «Иконка в системном трее (StatusNotifierItem на ksni)»

---

## 1. Назначение и границы

Правило проекта — «закрытие окна = Hide, процесс живёт» (ADR-0018), поэтому у
пользователя должен оставаться способ управлять радио без окна. Трей — третий
источник команд наравне с IPC (ADR-0005) и MPRIS (ADR-0006): он не заводит ни
своего протокола, ни своей бизнес-логики.

| Живёт в `src/tray/` | Не живёт в `src/tray/` |
|---|---|
| Регистрация SNI-item, меню `com.canonical.dbusmenu`, pixmap иконки, поток-владелец, каналы снимков и команд | Решение, что означает команда (`Session::execute_tray_command`), состояние эфира, станции, темы, окно |
| `TrayCommand` / `TraySnapshot` / `TrayHandle` / `spawn` — контракт слоя | `IpcCommand`-протокол (`src/ipc/`), MPRIS-метаданные (`src/media/`), egui (`src/ui/`) |

Модуль не знает ни про egui, ни про `Station`: наружу идут только доменные
типы-сообщения (`docs/interfaces/11-tray.md`). Обратная зависимость тоже
односторонняя: `Session` вызывает `tray::spawn` и работает с `TrayHandle`, а
`src/tray/` про `Session` ничего не импортирует.

## 2. Состав пакета

| Файл | Ответственность | Наружу (`pub(crate)`) |
|------|-----------------|------------------------|
| `mod.rs` | Контракт слоя: типы-сообщения, `TrayHandle`, точка входа и её no-op-заглушка | `VOLUME_STEP`, `TrayCommand`, `TraySnapshot`, `TrayHandle::{publish, commands}`, `spawn` |
| `sni.rs` | Реализация `ksni::Tray`, поток-владелец, меню, деградация (`#[cfg(all(target_os = "linux", feature = "tray"))]`) | `OrbitTray`, `start` — только для `mod.rs` |
| `icon.rs` | Процедурный орбитальный знак 22/32/48 в ARGB32, собранный один раз (`#[cfg(all(target_os = "linux", feature = "tray"))]`) | `ICONS` — только для `sni.rs` |

`mod.rs` компилируется всегда, чтобы `src/session/` не обрастал `cfg`-ветками:
на других платформах и в сборке без фичи `spawn` возвращает `None` с той же
сигнатурой.

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

Пробуждение GUI: каждый колбэк ksni после `commands.send(..)` зовёт `wake`. При
открытом окне слот `WakeHandle` держит `request_repaint`, и команды вычерпывает
`tick()`; при закрытом слот пуст, и команды ждёт ветка `tray_recv` в
`Session::park`.

## 4. Витрина

| Что видит хост | Откуда |
|----------------|--------|
| `Id`, `IconName` | `app_id` = `org.modernbyte.OrbitFM` |
| `Title`, `ToolTip.title` | `WINDOW_TITLE` = `OrbitFM` |
| `Category` | `ApplicationStatus` |
| `Status` | `Active`, если окно видно или идёт эфир |
| `ToolTip.description` | `station` из снимка, иначе `"Nothing playing"` |
| `IconPixmap` | `icon::ICONS` — 22/32/48, ARGB32 network byte order |
| `IconThemePath` | первый существующий `.../icons/hicolor` (XDG_DATA_HOME, затем `/usr/share`), иначе пусто |
| `Menu` | `/MenuBar`, собирается из снимка на каждое обновление |
| `ItemIsMenu` | `false` (константа `MENU_ON_ACTIVATE` по умолчанию) — только тогда хост вызывает `Activate` |

Иконка рисуется процедурно (диск планеты, наклонённое кольцо, точка эфира —
пропорции `ui::widgets::logo_mark`) с 4× суперсэмплингом и src-over
композицией слоёв. Новый бинарный ассет в репозиторий не добавлялся.

Подписи меню — английский (конвенция UI-строк): Show/Hide window · Play/Pause ·
Stop · Previous · Next · Radio Roulette · Volume Up/Down · System/Dark/Light ·
Quit. `theme_label` в `sni.rs` — своя функция, а не `ui::palette::theme_label`:
presentation-слой модулю трея недоступен.

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

## 6. Текущие ограничения

- `Volume Up/Down` не показывают текущую громкость: у SNI нет витрины для неё.
- Flatpak/sandbox: признак, при котором нужен `disable_dbus_name(true)`, в коде
  не зашит — он должен быть уточнён live-проверкой flatpak-сборки (ADR-0020 п. 8).
- `ksni::current_xdg_activation_token()` существует, но не потребляется: eframe
  0.37 + winit 0.30.13 не читают токен, поэтому показ окна из трея по-прежнему
  зависит от композитора (фокус на Wayland не гарантируется).
- Разные dbusmenu-хосты по-разному реагируют на `Category`/`ItemIsMenu` —
  проверено на Noctalia, другие хосты (KDE, waybar) не тестировались.

## 7. Тестирование

Unit-тесты покрывают контракт, не касаясь D-Bus: `mod.rs` — значения `Default`
и сравнение снимков; `sni.rs` — `Status` по снимку, тултип, состав и состояние
меню (чекбокс Play/Pause, выбранная тема), правило «одно действие = одна
команда + одно пробуждение» и байтовый порядок пикселей; `icon.rs` — все
размерности, alpha-first в ARGB32, непрозрачное ядро планеты и пустые углы,
выход орбиты за диск, наличие точки эфира.

Живая проверка — ADR-0020, «Проверка реализации».

## 8. Связанные документы

- `docs/interfaces/11-tray.md` — формальный контракт модуля
- ADR-0020 — протокол, зависимость, меню, иконка, деградация
- ADR-0018 — зачем трей нужен и куда впадают его команды
- `docs/modules/11-session.md` § 4.13 — исполнение `TrayCommand` в Core
- `docs/modules/08-noctalia-wm.md` — Noctalia как SNI-хост и виджет
