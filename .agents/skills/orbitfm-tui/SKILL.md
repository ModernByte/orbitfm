---
name: orbitfm-tui
description: Terminal UI OrbitFM на ratatui — IPC-клиент к single-instance daemon, не второй audio engine. Использовать при проектировании и реализации orbitfm-tui, клавишах, экранах Now Playing/Search/Favorites, визуализаторах и компоновках.
---

# OrbitFM TUI (ratatui)

Ты проектируешь или пишешь **терминальный клиент** OrbitFM. T0 (ADR-0047,
interface 16, module 19) выполнен; код T1 ещё не начат.

## Источники

1. `docs/audit/OrbitFM-tui-ratatui.md` — исследование и отклонённые варианты
2. `docs/adr/0047-tui-ratatui.md` — принятая архитектура, клавиши, этапы
3. `docs/interfaces/16-tui.md` — контракт `CoreClient`, команды и кадры
4. `docs/modules/19-tui.md` — crate, event loop, визуализаторы, `tui.toml`
5. ADR-0005 (single-instance + IPC), ADR-0039 (поток событий), ADR-0046
   (sleep, очередь, library)
6. `orbitfm-core`, `orbitfm-ipc`

## Маршрутизация

- **Когда:** `orbitfm-tui`, ratatui/crossterm, экраны, клавиши, визуализаторы,
  компоновки, `events --spectrum`.
- **Обязательные спутники:** `orbitfm-core`, `orbitfm-rust`, `orbitfm-ipc`;
  `orbitfm-product` для scope; `orbitfm-architect` + `orbitfm-docs` при смене
  IPC-контракта.
- **Не подменяет:** desktop GUI, Android Companion и аудио-подсистему.
- **Правила:** `docs/AGENTS.md` §2.2; при конфликте скилла и принятого ADR
  действует ADR.

## Главное решение

**Вариант A (обязательный для v1):** TUI — frontend к уже запущенному
`orbitfm` (libmpv в core).

```
orbitfm-tui  --CLI/IPC-->  orbitfm (session + audio)
```

**Запрещено в v1:** второй libmpv/rodio внутри TUI (два MPRIS, две library,
гонки за устройство), прямое чтение `library.json`, обращение к Radio Browser
и прямой Unix-сокет в обход CLI.

## Бинарь и структура

- Имя: `orbitfm-tui`; отдельный crate `crates/orbitfm-tui/` в workspace, чтобы
  корневой `orbitfm` не зависел от ratatui.
- Backend: `crossterm` + `ratatui`; версия TUI независима (`0.1.0`).
- Граница `CliCoreClient`: долгоживущий `orbitfm events` + one-shot команды в
  worker-потоке. Прямой сокет — будущая реализация того же `CoreClient`.
- Конфиг: `$XDG_CONFIG_HOME/orbitfm/tui.toml` (fallback `~/.config`), атомарная
  запись с debounce 500 мс; не пересекается с настройками core.

## MVP-экраны

1. **Now Playing** — title, country, bitrate, volume, state.
2. **Search** — query + results.
3. **Favorites** / **History**.
4. **Help**.

Поверх — строка статуса, подтверждение `Q` и ввод таймера сна. Визуализатор —
модуль любого экрана, не отдельный экран. Не включать 3D, desktop settings
drawer и менеджер интеграций.

## Клавиши (ADR-0047)

| Клавиша | Действие |
|---------|----------|
| `j`/`k`, `↓`/`↑` | список |
| `g`/`G` | начало/конец списка |
| `Enter` | играть выбранную станцию |
| `p`/`Space` | play/pause |
| `s` | stop |
| `[`/`]` | previous/next |
| `r` | радиорулетка |
| `+`/`-` | громкость шагом протокола |
| `/` | поиск |
| `f`/`h` | избранное/история |
| `a` | переключить избранное выбранной станции |
| `t`/`T` | задать/снять таймер сна |
| `v`/`V` | следующий визуализатор / вкл-выкл |
| `l` | следующий пресет компоновки |
| `m` | менеджер модулей и пресетов |
| `M` | motion: full → reduced → off |
| `Ctrl+R` | сброс компоновки к `desk` |
| `?` | помощь |
| `q` | **выйти из TUI, эфир продолжает играть** |
| `Q` | подтвердить и выполнить `quit` ядра |
| `R` | переподключиться к ядру |

`q` никогда не останавливает эфир. Footer это поясняет. `Q` отправляет
`orbitfm quit` только после `y`; при ошибке TUI остаётся открытым.

## Данные

| Нужда | v1 подход |
|-------|-----------|
| status | `events`: первый `snapshot`, затем полные `state`, `keepalive` |
| play/stop/volume | one-shot IPC-команды |
| favorites/history | IPC `library`, не более 20 записей на список |
| search | IPC `search`, корреляция по `id`; запуск `play-result <uuid>` с fallback на `play <uuid>` |
| library play | `play-library <uuid>` без обращения к каталогу |
| favorite | `favorite <uuid>`, затем refresh списков |
| sleep | `sleep <1..=720>` / `sleep clear`; остаток из `status.sleep` |
| разрыв | backoff 1/2/4… до 30 с, новый `snapshot`, баннер |

Не ослаблять SSRF/publishable policy. Неизвестные поля и типы кадров
игнорируются; чужие `app`/`schema`/`ev` — понятная ошибка совместимости.

## Визуализаторы и компоновки

- v1: `bars`, `ring`, `scope`, `pulse`; T6: `stars`, `heat`.
- Пресеты: `desk` (default), `focus`, `library`, `minimal`, `dashboard`.
- Модули: `now_playing`, `visualizer`, `search`, `results`, `favorites`,
  `history`, `status`, `help`; включаются независимо (btop-подобно).
- 16-цветный fallback, `NO_COLOR`, `glyphs = "unicode" | "ascii"`; тик ≤ 15 FPS;
  `motion = full | reduced | off`; скрытый модуль не тикает.
- `events --spectrum` — планируемое аддитивное расширение (`ev: 1`, ≤ 30 Hz,
  только `playing`). Без него визуализатор работает в procedural-режиме и
  сообщает об этом.
- Минимум 60×18; при высоте сцены < 3 строк визуализатор скрывается.

## Headless core

TUI v1 требует уже запущенный `orbitfm` и честно сообщает об отсутствии ядра.
Headless-режим (`--headless`/`ORBITFM_HEADLESS=1`) — отдельный ADR и отдельный
трек, не часть TUI v1.

## Безопасность

- Только локальный пользователь; аргументы `Command` без shell.
- Все строки от ядра очищаются от C0/C1/DEL/ESC и bidi-переопределений перед
  выводом в терминал.
- TUI не слушает сеть, не ходит в каталог, не читает файлы библиотеки и не
  скачивает favicon.
- `Q` требует подтверждения; `q` и Ctrl-C завершают только TUI.

## Этапы

1. T1: workspace, `crates/orbitfm-tui`, ratatui loop на mock-данных, terminal
   guard, минимальный размер.
2. T2: `CliCoreClient`, `events`, worker команд, транспорт, `q`/`Q`.
3. T3: `library`, избранное/история, `play-library`, `favorite`.
4. T4: `search`, `play-result` с fallback.
5. T5: визуализаторы, пресеты, менеджер модулей, `tui.toml`, `spectrum` с
   fallback, Help, быстрый sleep, live-приёмка.
6. T6: `stars`/`heat`, прямой сокет, мышь, image protocol.

## Поставка

- Source-only `cargo install --path crates/orbitfm-tui` до решения владельца о
  пятом релизном артефакте.
- Целевое ядро `>= 0.18.0-rc.1`; совместимость по `ev: 1` и
  `status.schema: 1`.
- Служебные подписи TUI v1 — английские; строки `status.ui` не смешиваются.

## Чего нельзя

- `q` = kill radio без предупреждения.
- Обещать паритет с Orbital Desk.
- Ставить TUI выше Android Companion без решения владельца.
- Писать TUI с собственной сессией воспроизведения, поиском RB или чтением
  `library.json` в v1.
- Начинать код T1 без явного решения владельца о старте трека.
