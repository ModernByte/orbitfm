# OrbitFM TUI (`orbitfm-tui`) — лёгкий терминальный клиент

**Организация:** ModernByte  
**Статус:** спроектирован (T0, D0); волна 1 запущена 2026-11-21, Android на паузе, `events --spectrum` обязателен, поставка — source-only + артефакт  
**Бинарь:** `orbitfm-tui` · **Crate:** `crates/orbitfm-tui` (Cargo workspace)  
**Ядро:** `orbitfm >= 0.18.0-rc.1`, `ev: 1`, `status.schema: 1`

OrbitFM TUI — **отдельное лёгкое приложение** для терминала. Оно показывает тот
же эфир и ту же библиотеку, что и desktop OrbitFM, но управляет ими через
публичный CLI/IPC запущенного ядра. Это не второй плеер и не второй процесс с
аудио.

Это главный документ приложения. Язык дизайна — в
[`docs/ui/tui-design-system.md`](../ui/tui-design-system.md), версии и оценки —
в [карте версий TUI](roadmap.md), план работ —
в [`.agents/plans/tui-modern-design-and-delivery.md`](../../.agents/plans/tui-modern-design-and-delivery.md).

---

## 1. Зачем он нужен

Desktop закрывает «витрину»: 3D-глобус, стекло, motion. Android Companion
закрывает карман. Между ними остаётся терминал:

- SSH на домашний box, VPS или машину без рабочего стола;
- tmux/screen рядом с `htop`, `ncmpcpp`, `newsboat`;
- минимальный WM, tty и recovery-сценарии;
- слабый GPU или удалённый X11, где `wgpu` избыточен;
- сценарии «скрипт + человек», где CLI уже есть, но нужен обозреватель станций;
- терминальная эстетика (рисование, шрифты, прозрачность) как отдельный вкус.

TUI не заменяет Orbital Desk и не подменяет Android-приоритет. Это parallel
track для terminal-native аудитории.

---

## 2. Что это и чего это не есть

| Это | Это не |
|-----|--------|
| IPC-клиент к работающему `orbitfm` | второй плеер со своим libmpv |
| отдельный бинарь и crate в workspace | feature-флаг desktop-пакета |
| ratatui + crossterm, 16 цветов | truecolor-графика и image protocol в v1 |
| один акцент, уважение к фону терминала | заливка экрана и навязанная палитра |
| `q` выйти из TUI, эфир жив | `q` = убить радио |
| `Q` + подтверждение = `orbitfm quit` | тихий `quit` ядра |
| поиск и библиотека через IPC ядра | чтение `library.json`, Radio Browser, favicon |

**Один процесс с аудио.** Аудио, MPRIS, трей, каталог Radio Browser и
`library.json` остаются за `orbitfm` (ADR-0005, ADR-0047).

---

## 3. Установка

TUI распространяется и как исходная сборка, и как отдельный релизный артефакт
(решение владельца 2026-11-21):

```bash
cargo install --path crates/orbitfm-tui
orbitfm-tui --version
```

`--version` показывает динамическую версию: semver, git-состояние (`sha`,
`sha-dirty`), дату сборки и совместимость с ядром. Схема версионирования —
[версионирование TUI](versioning.md).

Требуется запущенное ядро `orbitfm` в `PATH` или переменная `ORBITFM_BIN`
с путём к бинарнику ядра. TUI **не** запускает ядро и не поднимает GUI.

> Сценарий «чистый SSH без графической сессии» требует отдельного трека
> headless-ядра (`--headless`/`ORBITFM_HEADLESS=1`); до него TUI честно
> сообщает об отсутствии ядра. Это не деталь TUI, а отдельный ADR.

---

## 4. Быстрый старт

```bash
# 1. В одной сессии — desktop-ядро (эфир живёт)
orbitfm

# 2. В терминале/tmux — лёгкий клиент
orbitfm-tui
```

Клавиши (полная таблица — модуль 19 §7 и интерфейс 16):

| Клавиша | Действие |
|---------|----------|
| `j`/`k`, `↓`/`↑` | список |
| `g`/`G` | начало/конец списка |
| `Enter` | играть выбранную станцию |
| `p`/`Space` | play/pause |
| `s` | stop |
| `[`/`]` | previous/next |
| `r` | радиорулетка |
| `+`/`-` | громкость |
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
| `q` | выйти из TUI, **эфир продолжает играть** |
| `Q` | подтвердить и выполнить `quit` ядра |
| `R` | переподключиться к ядру |

`q` никогда не останавливает эфир; это прямо написано в подвале. `Q` сначала
спрашивает `y`/`n`; при ошибке TUI остаётся открытым.

---

## 5. Экраны и модули

MVP-экраны: **Now Playing**, **Search**, **Favorites**, **History**, **Help**.
Поверх — строка статуса, подтверждение `Q` и ввод таймера сна.

Визуализатор — **модуль любого экрана**, не отдельный экран. Модули
`now_playing`, `visualizer`, `search`, `results`, `favorites`, `history`,
`status`, `help` включаются независимо (btop-подобно). Скрытый модуль не
рисуется, не тикает и не запускает запросов.

Визуализаторы v1: `bars` (Orbital Bars, default), `ring` (Orbit Ring), `scope`
(Braille Scope), `pulse` (Core Pulse, fallback для узкого экрана и
`motion=off`). `stars` (Signal Constellation) и `heat` (Spectrogram Heat) —
этап T6.

Пресеты компоновки: `desk` (default), `focus`, `library`, `minimal`,
`dashboard`. Минимум 60×18; адаптивность описана в
[языке дизайна](../ui/tui-design-system.md) §3 и модуле 19 §6.7.

---

## 6. Конфигурация

Файл: `$XDG_CONFIG_HOME/orbitfm/tui.toml` (fallback `~/.config/orbitfm/tui.toml`).
Ядро его не читает и не валидирует. Волна 1 уже **читает** `[ui].theme` и
`[ui].glyphs`; атомарная запись с debounce 500 мс — задача FND-5. Битый файл
не перезаписывается и даёт defaults с предупреждением в статусе.

```toml
schema = 1

[ui]
layout = "desk"        # desk|focus|library|minimal|dashboard
motion = "full"        # full|reduced|off
glyphs = "auto"        # auto|unicode|ascii
border = "rounded"     # rounded|square|none
theme = "orbit-dark"   # orbit-dark|ice|amber|green|magenta|mono
accent = "cyan"        # совместимость; theme главнее
frame_fps = 15         # 1..=30

[modules]
now_playing = true
visualizer = true
search = true
results = true
favorites = true
history = true
status = true
help = false

[visualizer]
kind = "bars"          # bars|ring|scope|pulse|stars|heat
bars = 32              # 8..=64
```

Правила: неизвестные поля игнорируются; `schema != 1` — предупреждение и
defaults; невалидные значения зажимаются, а не роняют TUI; повреждённый файл не
перезаписывается без действия пользователя.

---

## 7. Совместимость и безопасность

- Целевое ядро `>= 0.18.0-rc.1`; TUI требует `ev: 1` и `status.schema: 1`.
  При `schema: 2`/`ev: 2` — ошибка «обновите TUI», управление блокируется.
- Неизвестные поля кадров игнорируются; аддитивные расширения не ломают TUI.
- TUI не слушает сеть, не ходит в каталог, не читает файлы библиотеки и не
  скачивает favicon.
- Все строки от ядра очищаются от C0/C1/DEL/ESC и bidi-переопределений перед
  выводом в терминал (defense-in-depth; интерфейс 16 §8).
- Аргументы передаются в `Command` без shell.
- Мышь в v1 отключена; OSC 8 и динамический заголовок окна не используются.

---

## 8. Поставка

| Вопрос | Решение |
|--------|---------|
| Имя | `orbitfm-tui` |
| Crate | `crates/orbitfm-tui` в workspace; desktop не тянет ratatui |
| Установка | `cargo install --path crates/orbitfm-tui` (source-only) |
| Зависимость runtime | запущенный `orbitfm` в `PATH` или `ORBITFM_BIN` |
| Релизный артефакт | **добавляется** отдельным решением владельца (2026-11-21); и source-only, и артефакт |
| Версия TUI | независима (`0.1.0` → `1.0.0`), см. [карту версий](roadmap.md) |

---

## 9. Ограничения v1

- Требуется запущенное ядро; чистый SSH ждёт headless-тренд.
- `library` отдаёт ≤ 20 записей на список (favorites/история).
- Нет мыши, favicon, изображений, настроек desktop и `play-url`.
- Нет локализации: служебные подписи английские; строки `status.ui` не
  смешиваются.
- Нет собственного кэша: при разрыве показывается последний снимок с пометкой
  `stale`.
- Нет прямого сокета: v1 зависит от CLI в `PATH` или `ORBITFM_BIN`.
- Reactive-визуализаторы требуют `events --spectrum`; до него работает
  procedural-режим с честной пометкой `viz procedural`.
- `dashboard` недоступен на терминалах `< 120` колонок и откатывается к `desk`.

---

## 10. Карта документов

| Документ | Что внутри |
|----------|------------|
| [Язык дизайна TUI](../ui/tui-design-system.md) | токены, палитра, компоненты, композиции, доступность |
| [Карта версий TUI](roadmap.md) | версии, задачи, приоритеты, сложность, критерии |
| [Версионирование TUI](versioning.md) | динамическая версия: semver + git + дата, тег `tui-vX.Y.Z` |
| [ADR-0047](../adr/0047-tui-ratatui.md) | архитектурное решение |
| [Интерфейс 16](../interfaces/16-tui.md) | контракт TUI ↔ ядро |
| [Модуль 19](../modules/19-tui.md) | экраны, event loop, визуализаторы, конфиг |
| [Аудит TUI](../audit/OrbitFM-tui-ratatui.md) | продуктовая база и аналогии |
| [План трека](../../.agents/plans/tui-modern-design-and-delivery.md) | волны, реестр задач, команда |

Разработчику: входная точка — [`docs/dev/README.md`](../dev/README.md).
