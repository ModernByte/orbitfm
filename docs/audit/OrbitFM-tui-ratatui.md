# OrbitFM TUI — клиент на ratatui

| Поле | Значение |
|------|----------|
| **Дата** | 2026-11-03 |
| **Статус** | Принято к проектированию (ADR-0047); код не начат |
| **Стек** | Rust, ratatui 0.30.2, crossterm 0.29.0; MSRV проекта 1.95 |
| **База** | OrbitFM desktop `0.18.0-rc.1`, single-instance + CLI/IPC |

Связанные документы: `docs/audit/OrbitFM-future-features.md` (Android —
приоритет A), ADR-0005 (IPC), ADR-0039 (события), ADR-0046 (ритуалы),
ADR-0047 (решение о TUI), `docs/interfaces/05-ipc.md`,
`docs/interfaces/13-events.md`, `docs/interfaces/16-tui.md`,
`docs/modules/19-tui.md`.

---

## 1. Зачем TUI OrbitFM

Desktop OrbitFM уже закрывает «витрину»: 3D-глобус, стекло, motion, heatmap. Android companion закрывает карман. Между ними остаётся устойчивая ниша:

| Сценарий | GUI тяжёл / неудобен | TUI уместен |
|----------|----------------------|-------------|
| SSH на сервер / VPS / домашний box | Нет дисплея / нет Wayland | Да |
| Минимальный WM, tty, recovery | Глобус не нужен | Да |
| tmux/screen рядом с htop, ncmpcpp | Хочется тот же UX-паттерн | Да |
| Слабый GPU / удалённый X | wgpu избыточен | Да |
| Скрипты + человек | Только CLI; нужен обозреватель станций | Да |
| Rice-эстетика (шрифты, прозрачность терминала) | Отдельный вкус аудитории | Да |

**Роль продукта:** не заменить глобус, а дать **быстрый terminal-native клиент**
к тому же эфиру и той же библиотеке.

**Аналогии:** `ncmpcpp` / `cmus` для музыки, `newsboat` для лент. TUI не
заменяет Orbital Desk и не подменяет Android-приоритет.

---

## 2. Мнение по архитектуре: два честных варианта

### Вариант A — Remote control (принят)

```
┌──────────────┐  CLI → IPC (socket/pipe)  ┌─────────────────────┐
│ orbitfm-tui  │ ────────────────────────► │ orbitfm (core)      │
│ ratatui UI   │ ◄── events / ответы ───── │ libmpv + session    │
└──────────────┘                           └─────────────────────┘
```

- Один процесс с аудио — уже есть single-instance.
- TUI не нужны `show`/`hide`/`mode`; нужны состояние и команды эфира.
- Поток состояния уже есть: `orbitfm events` (ADR-0039), опрос `status` не
  нужен.
- Плюсы: нет второго libmpv, нет гонок за устройство вывода, library общая.
- Минусы: без запущенного ядра TUI «пустой»; чистый SSH ждёт headless-режим.

**Headless core:** `--headless`/`ORBITFM_HEADLESS=1` — отдельный трек.
`orbitfm` стартует через `run_window` и требует дисплея; `events` не входит
в cold-start allowlist. TUI v1 не запускает ядро и не поднимает GUI.

### Вариант B — Standalone player в терминале (отклонён)

- Свой audio stack (mpv/rodio) внутри TUI-бинаря.
- Минусы: дубль сессии, расхождение library, два MPRIS-агента, больше
  поддержки и рисков.

**Вердикт:** Вариант A, ADR-0047.

---

## 3. Что входит в TUI v1 (MVP)

### 3.1. Экраны

1. **Now Playing** — станция, страна, bitrate/codec, volume, state, sleep.
2. **Search** — строка запроса + до 20 результатов.
3. **Favorites** — избранное из `library` (до 20 записей).
4. **History** — последние станции из `library` (до 20 записей).
5. **Help** — ключи, версии, ограничения.

Опционально в v1: строка статуса команд и подтверждение `Q`.

### 3.2. Клавиши

| Клавиша | Действие |
|---------|----------|
| `j` / `k`, `↓` / `↑` | список |
| `g` / `G` | начало / конец списка |
| `Enter` | play выбранной станции |
| `p` / Space | play/pause |
| `s` | stop |
| `[` / `]` | previous / next |
| `r` | roulette |
| `+` / `-` | volume |
| `/` | поиск |
| `f` | экран избранного |
| `h` | экран истории |
| `a` | переключить избранное выбранной станции |
| `t` / `T` | задать / снять таймер сна |
| `v` / `V` | следующий визуализатор / вкл-выкл |
| `l` | следующий пресет компоновки |
| `m` | менеджер модулей |
| `M` | motion: full → reduced → off |
| `Ctrl+R` | сброс компоновки |
| `R` | переподключиться к ядру |
| `?` | help |
| `q` | quit TUI (**не** quit daemon) |
| `Q` | quit TUI **и** `orbitfm quit` (с confirm) |

### 3.3. Не входит в TUI v1

- 3D-глобус и desktop-сцены; спектрограмма `heat` — T6.
- Настройки desktop (темы, accent, globe style).
- Редактор интеграций шеллов.
- Мышь, favicon и изображения.
- Полный список избранного больше 20 записей.
- `play-url`, `library-export`/`library-import`.
- Локализация: v1 — английские служебные подписи.
- Запуск ядра или GUI из TUI.

---

## 4. Данные и IPC

### 4.1. Что уже есть в ядре

- `events` — `snapshot`/`state`/`search`/`keepalive`, конверт `ev: 1`.
- `status --json` схемы 1 — запасной путь, TUI его не опрашивает.
- `play <uuid>`, `playpause`, `stop`, `next`, `previous`, `roulette`,
  `volume-up`, `volume-down`.
- `search <query>` → `search <id>` + кадр `search` в `events`.
- `library` — избранное и история, по 20 записей.
- `favorite [uuid]` → `favorite true|false`.
- `play-result <uuid>` — только последняя выдача.
- `play-library <uuid>` — только избранное/история, без каталога.
- `sleep <1..=720>` / `sleep clear`, поле `sleep` в статусе.
- `quit`.

### 4.2. Решения по данным

| Потребность TUI | Контракт | Решение TUI |
|-----------------|----------|-------------|
| Список favorites | `library`, 20 записей | показываем 20; полный список — будущее аддитивное расширение |
| История | `library`, 20 записей | то же |
| Поиск RB | `search` + кадр `search` | используем ядро, свой клиент не пишем |
| Live updates | `events` | snapshot/state, keepalive, переподключение |
| Данные визуализатора | `events --spectrum` (план) | кадры `spectrum`, procedural fallback |
| Ошибки play | текст `error …` | строка статуса + повтор для `play-result` |
| Таймер сна | `sleep`, `status.sleep` | ввод `t`, остаток рисуем локально |

**Запрещено для TUI:** читать `library.json` напрямую, ходить в Radio
Browser, скачивать favicon, открывать homepage, использовать `file://`,
дублировать SSRF-политику.

**Адаптер v1:** CLI (`orbitfm events` + one-shot команды) за границей
`CoreClient`. Прямой сокет — будущая реализация без изменений UI.

### 4.3. SSRF и безопасность

- TUI не принимает URL и не загружает сетевые ресурсы; `play-url` не
  выставляется.
- Строки станции перед выводом очищаются от C0/C1/ESC/bidi — защита от
  terminal escape injection.
- TUI не поднимает открытый сетевой сервер.
- IPC остаётся local-user trust model; `Q` требует подтверждения.
- `ORBITFM_BIN` — доверенная переменная локального пользователя, без shell.

---

## 5. UX и раскладка (ratatui)

### 5.1. Каркас

```
┌ OrbitFM TUI ─────────────────────────────── 0.1 ┐
│ ▶ Radio XYZ  DE  128 kbps  vol 70%  sleep 14:32 │
├─────────────────────────┬───────────────────────┤
│ Orbital Bars            │ Search / Results      │
│  ▂▃▅█▇▅▃▂  ▂▄▆█▆▄▂       │ > Station A   US 192  │
│  ▁▂▄▆█▆▄▂  ▁▃▅▇▅▃▁       │   Station B   JP 128  │
│                         │   Station C   FR  96  │
├─────────────────────────┴───────────────────────┤
│ Status  ok sleep 30       visualizer: reactive  │
│ j/k move  enter play  p pause  v/V visualizer   │
│ l layout  m modules  ? help  q quit  Q quit core│
└─────────────────────────────────────────────────┘
```

### 5.2. Стиль

- Минимальная палитра: default terminal + один accent.
- Не зависеть от truecolor (fallback 16 colors); `NO_COLOR` уважается.
- `ratatui` + backend `crossterm`; crossterm берётся из ре-экспорта ratatui.
- Состояние передаётся символами и текстом, не только цветом.

### 5.3. Accessibility

- Не только цвет: `▶` `‖` `■` `…` `↻` `!`.
- Совместимость со screen reader ограничена природой TUI — не обещать
  паритет с AccessKit desktop.
- Минимальный размер 60×18; меньше — понятное сообщение.
- `motion = reduced/off`, `glyphs = ascii`, `NO_COLOR` — читаемость важнее
  анимации; вспышки ограничены 3 Hz.

---

## 6. Поставка и имя

| Вопрос | Решение |
|--------|---------|
| Имя бинарника | `orbitfm-tui` |
| Crate | `crates/orbitfm-tui` в Cargo workspace; desktop не тянет ratatui |
| Установка | `cargo install --path crates/orbitfm-tui`; AUR/пакет — later |
| Зависимость runtime | запущенный `orbitfm` в `PATH` или `ORBITFM_BIN` |
| Релизный артефакт | не добавляется без решения владельца; четыре артефакта остаются |

Структура репо (план):

```text
orbitfm/                 # desktop (как сейчас)
crates/orbitfm-tui/      # ratatui client
docs/audit/...-tui...    # этот документ
```

---

## 7. Визуализаторы, компоновки и модули

### 7.1. Принципы

- Полотно TUI — ячейки терминала, half blocks (`▀▄█`) и Braille; пиксельной
  графики нет.
- Два источника движения: **reactive** — кадры `spectrum` от core (расширение
  интерфейса 16), **procedural** — локальная анимация без спектра. TUI v1
  обязан работать в procedural; reactive включается, когда ядро отдаёт кадры.
- Визуализатор — обычный модуль: его можно выключить, layout пересобирается
  без пустой дыры.
- Анимация не блокирует ввод: тик ≤ 15 FPS, перерисовка только видимой сцены,
  при `motion = "off"` — статичный кадр 1 Hz, при `reduced` — 4 FPS.
- Никаких полнокадровых инверсий и вспышек чаще 3 Hz: терминальная
  светочувствительность важнее эффекта.
- Данные core уже дают `SpectrumFrame` (centroid, spread, flatness, flux,
  rms_db и три полосы, ADR-0010/0019). Новый FFT не изобретается — TUI
  повторяет синтетическую огибающую desktop-визуализатора.

### 7.2. Набор визуализаторов

| id | Название | Данные | Полотно | Поведение |
|----|----------|--------|---------|-----------|
| `bars` | **Orbital Bars** | level, centroid, spread, flux, flatness | half blocks, 24–64 столбца | гравитация и peak-hold как в desktop; accent-градиент; процедурная фактура от flux |
| `ring` | **Orbit Ring** | level, bands, centroid | Braille/точки, полярное кольцо | bass расширяет радиус, mid вращает кольцо, high мерцает; в центре — код страны или `▶` |
| `scope` | **Braille Scope** | level, centroid, flux | Braille 2×4 | бегущая осциллограмма и Lissajous; амплитуда от level, частота от centroid, дрожание от flux |
| `stars` | **Signal Constellation** | level, flux, bands | Braille/точки | звёздное поле с параллаксом; flux задаёт «варп», в тишине — медленный дрейф |
| `heat` | **Spectrogram Heat** | level, centroid, bands | half blocks, кольцевой буфер истории | спектрограмма во времени: три полосы + маркер центроида, прокрутка справа налево |
| `pulse` | **Core Pulse** | level, state, sleep | block gauge, 1–3 строки | минимальный орбитальный индикатор для узкого layout и слабого CPU |

`bars` — режим по умолчанию. `ring`, `scope`, `stars`, `heat` — «витрина»,
`pulse` — fallback для `< 80` колонок и `motion = "off"`.

Эскизы (не пиксель-перфект, а композиция):

```text
bars                          ring
┌───────────────────────┐     ┌───────────────────────┐
│   ▂▄▆█▇▅▃▂  ▁▃▅▇█▆▄▂  │     │      ⢀⣀⣤⣤⣀⢀       │
│   ▁▂▄▆█▆▄▂  ▁▂▃▅█▇▅▃  │     │   ⢀⣴⠟⠉  ▶  ⠉⠻⣦⡀   │
│   ▁▂▃▅▆▇▆▅▃  ▁▂▃▄▅▆▅▄  │     │  ⢠⡿   DE · 128k ⢿⡄  │
│        peak ▔          │     │   ⠙⣦⣄⣀⣀⣠⣤⠋   │
└───────────────────────┘     └───────────────────────┘

scope                         stars
┌───────────────────────┐     ┌───────────────────────┐
│   ~-._     .-~-.      │     │  ·    ✦       ·    ·  │
│      `-.__.~    `--.  │     │    ·     ·   ✦   ·    │
│  .-~`   `-.____.-~`   │     │ ✦      ·   ·      ✦   │
│                       │     │    ·      ✦    ·       │
└───────────────────────┘     └───────────────────────┘

heat                          pulse
┌───────────────────────┐     ┌───────────────────────┐
│ ░░▒▒▓▓██▓▒░  high     │     │ ▶ Radio XYZ           │
│ ▒▒▓▓██▓▒░░   mid      │     │ ●───────○──── 70%     │
│ ▓▓██▓▒░░     bass     │     │ sleep 14:32           │
│ └─ time ─────────►    │     │                       │
└───────────────────────┘     └───────────────────────┘
```

Правила отрисовки:

- 16-цветный fallback: accent + dim, без truecolor; `NO_COLOR` полностью
  отключает цвет, оставляя глифы.
- `glyphs = "ascii"` заменяет Braille/half blocks на `#`, `=`, `*`, `-` для
  шрифтов без Unicode.
- Высота сцены < 3 строк — визуализатор скрывается, а не рисует мусор.
- Пустое состояние (эфир остановлен) — медленное «дыхание», не пустой экран;
  ошибка эфира — статичная полоса `!` без анимации.
- Reactive-кадры приходят не чаще 30 Hz; рендер интерполирует и не хранит
  очередь — медленный терминал видит последний кадр.

### 7.3. Пресеты компоновки

| Пресет | Раскладка | Когда |
|--------|-----------|-------|
| `desk` (default) | Now Playing сверху; слева визуализатор, справа активный список (results/favorites/history) | обычный терминал 100×30 |
| `focus` | Визуализатор на весь экран; Now Playing и статус — нижние 3 строки; списки скрыты | «режим созерцания», tmux-панель |
| `library` | Две колонки favorites/history; Now Playing сверху; визуализатор — полоса 6–8 строк | работа с библиотекой |
| `minimal` | Now Playing 2 строки + активный список + статус; визуализатор выключен | ssh, маленький экран, scripts |
| `dashboard` | Сетка модулей как в btop: NP, visualizer, search, results, favorites/history, status | широкий экран ≥ 120 колонок |

Адаптивность: `< 80` колонок — одна колонка и `pulse`/без визуализатора;
`80–119` — две зоны; `≥ 120` — `dashboard` доступен. Минимум остаётся
60×18; меньше — сообщение и ожидание resize.

```text
desk                                focus
┌ NP: ▶ Radio XYZ · DE · 128k ────┐  ┌ visualizer full ────────────────┐
├───────────────┬─────────────────┤  │                                │
│  Orbital Bars │  Search/Results │  │          Orbit Ring            │
│               │  > Station A    │  │                                │
├───────────────┴─────────────────┤  ├ NP: ▶ Radio XYZ · 70% ─────────┤
│ status · keys                    │  └ status ────────────────────────┘
└──────────────────────────────────┘

library                             minimal
┌ NP: ▶ Radio XYZ ────────────────┐  ▶ Radio XYZ — DE — 128k — 70%
│ visualizer strip                 │  > Station A                    │
├───────────────┬─────────────────┤    Station B                    │
│ Favorites (20)│ History (20)    │  status: ok · q quit · ? help   │
│ > Station A   │   Station C     │
├───────────────┴─────────────────┤  dashboard
│ status · keys                    │  ┌ NP ───────┬ visualizer ─────┐
└──────────────────────────────────┘  ├ search ───┼ results ────────┤
                                      ├ favorites ┴ history ────────┤
                                      └ status ────────────────────┘
```

### 7.4. Модули и управление

Модули: `now_playing`, `visualizer`, `search`, `results`, `favorites`,
`history`, `status`, `help`. Каждый включается/выключается независимо;
скрытый модуль сохраняет состояние и не перерисовывается.

| Клавиша | Действие |
|---------|----------|
| `v` | следующий визуализатор |
| `V` | визуализатор вкл/выкл |
| `l` | следующий пресет компоновки |
| `m` | менеджер модулей (чекбоксы, пресеты `1..5`, apply/cancel) |
| `Tab` / `Shift+Tab` | фокус между зонами |
| `Ctrl+R` | сброс компоновки к `desk` |
| `M` | `motion`: full → reduced → off |

Цифры `1..5` работают только внутри менеджера модулей, чтобы не конфликтовать
с поиском и списками. Изменения компоновки и модулей сохраняются в
`$XDG_CONFIG_HOME/orbitfm/tui.toml` (fallback `~/.config`; атомарная запись,
debounce 500 мс).

```toml
schema = 1

[ui]
layout = "desk"        # desk|focus|library|minimal|dashboard
motion = "full"        # full|reduced|off
glyphs = "unicode"     # unicode|ascii
accent = "cyan"        # имя 16-цветной палитры
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
kind = "bars"          # bars|ring|scope|stars|heat|pulse
bars = 32              # 8..=64, только для bars
```

### 7.5. Контракт `spectrum` (планируемое расширение)

Реальные аудиовизуализаторы требуют кадров спектра. Предлагается опциональная
подписка `orbitfm events --spectrum` (интерфейс 16, ADR-0047 ревизия 2):

```json
{"ev":1,"type":"spectrum","seq":42,"level":0.63,
 "centroid_hz":1720.0,"spread_hz":1600.0,"flatness":0.42,
 "flux":0.0018,"bands":{"bass":0.70,"mid":0.55,"high":0.30}}
```

- Кадры идут только при `playing`, не чаще 30 Hz, latest-wins, без очереди;
  `keepalive` не сбрасывают.
- Подписчик без флага `--spectrum` не получает кадров — старые клиенты и
  `events` без флага не меняются.
- Поля аддитивны к `ev: 1`; неизвестный тип кадра TUI игнорирует.
- Ядро уже считает `SpectrumFrame`; публикация не добавляет FFT-граф и не
  меняет аудио-поток.
- Если флаг недоступен (ядро старше), TUI молча уходит в procedural и пишет
  `visualizer: procedural` в статус.

---

## 8. Этапы реализации

### T0 — Решение и контракт — выполнено (2026-11-03)

- ADR-0047, `docs/interfaces/16-tui.md`, `docs/modules/19-tui.md`.
- Зафиксированы quit, источники library/search, headless как отдельный трек.
- Проверены кадры `events`, `status --json` схемы 1 и команды 0.18.

### T1 — Skeleton (2–4 дня)

- Cargo workspace, `crates/orbitfm-tui`, ratatui app loop, terminal guard
  (restore на выходе и панике).
- `CoreClient` CLI-адаптер: долгоживущий `orbitfm events` + one-shot команды.
- Экраны с mock-данными, затем реальные `snapshot`/`state`.
- Reconnect/backoff, stale-таймер 45 с, экран «core недоступен».

### T2 — Playback (2–3 дня)

- play/pause/stop/roulette, volume ±, `library` (20+20), `play-library`.
- `q` — только TUI, `Q` + confirm — `orbitfm quit`; тосты ошибок.
- Понятный сценарий «ядро не запущено» без попытки поднять GUI.

### T3 — Library actions (2 дня)

- `favorite`, выбор станции, пустые списки, сохранение позиции.
- Синхронизация с `status` без мигания списков.

### T4 — Search (2–3 дня)

- `search <q>` → `search <id>`, корреляция по id, таймаут 15 с.
- `/`, Enter, Esc; пустая выдача и ошибка сети — разные состояния.

### T5 — Визуализаторы, компоновки, полировка (3–5 дней)

- `bars`, `ring`, `scope`, `pulse`; пресеты `desk`/`focus`/`library`/
  `minimal`; менеджер модулей; `tui.toml`.
- `events --spectrum` в core (если принято) и reactive-режим; без него —
  procedural.
- Help overlay, resize, README, `motion` и `NO_COLOR`.

### T6 — Later

- `stars` и `heat`, direct socket client, mouse, image protocol (Kitty/sixel)
  для favicon — только fun, не must.
- `next`/`previous`/`sleep` уже есть в core 0.18; используются с T2.

### Отдельный трек — headless core

`--headless`/`ORBITFM_HEADLESS=1`: запуск аудио-ядра без окна и дисплея.
Без него TUI в чистом SSH требует уже запущенного `orbitfm`; в TUI v1 это
честная ошибка, а не скрытый запуск GUI.

---

## 9. Связь с приоритетами продукта

| Трек | Отношение к TUI |
|------|------------------|
| **Android Companion (Prio A)** | Не конкурирует: мобила vs SSH/tty. Общая library schema 1 усиливает оба |
| Sleep timer / queue | Уже в core 0.18; TUI получает `t`/`T`, `[`/`]` |
| Headless core | Предусловие чистого SSH; отдельный трек, не блокирует T1–T4 |
| Визуализаторы TUI | Дифференциатор против CLI, но не повод сдвигать Android v1 |
| Ещё шеллы, Web Explorer | Другие каналы, TUI не блокируется |

**Когда делать относительно Android:**

- Параллельно можно: TUI опирается на готовый IPC, Android — на Media3.
- Один разработчик: **не** ставить TUI впереди Android v1; TUI — parallel
  track 10–20% времени или между волнами.
- Визуализаторы — после T4: сначала управление и библиотека, потом красота.

---

## 10. Риски

| Риск | Митигация |
|------|-----------|
| Нет headless — чистый SSH не работает | Честная ошибка в v1; отдельный трек `--headless` |
| `library` отдаёт только 20+20 записей | Документировать лимит; later IPC-пагинация, не читать `library.json` |
| Зависимость от CLI-ребёнка и его задержек | Долгоживущий `events`; one-shot команды с таймаутом 15 с; later direct socket |
| Терминальные escape-последовательности из каталога | Санитизация C0/C1/DEL/bidi до отрисовки (как в `status`) |
| Spectrum-кадры жгут CPU/батарею | Opt-in, ≤ 30 Hz, latest-wins, `motion=off`; скрытый визуализатор не тикает |
| Вспышки и быстрый motion | ≤ 15 FPS, запрет инверсий > 3 Hz, `reduced`/`off` |
| Пользователь жмёт `q` и думает, что радио встало | `q` только TUI; footer `daemon still running`; `Q` + confirm |
| «Ненастоящий» визуализатор вводит в заблуждение | В procedural-режиме статус явно пишет `visualizer: procedural` |
| Windows console | crossterm; secondary platform, Linux first |

---

## 11. Критерии успеха MVP

- [ ] При живом `orbitfm` видно now playing и меняется volume с TUI.
- [ ] Play из favorites и из search; `library` не читается напрямую.
- [ ] `q` не убивает эфир; `Q` + confirm — убивает.
- [ ] Понятная ошибка, если core не запущен; GUI не поднимается.
- [ ] Без паники при resize и пустых списках; терминал восстановлен после
  выхода и паники.
- [ ] `v` переключает `bars`/`ring`/`scope`/`pulse`; `V` скрывает
  визуализатор; выбор сохраняется в `tui.toml`.
- [ ] Пресеты `desk`/`focus`/`library`/`minimal` переключаются на лету;
  `m` включает/выключает модули без пустых дыр.
- [ ] `motion=off` и `NO_COLOR` дают статичный читаемый экран; скрытый
  визуализатор не потребляет тики.
- [ ] README: 10 строк «установка + два экрана».

---

## 12. Вердикт

**Принято.** TUI — отдельный трек и отдельный бинарь `orbitfm-tui`, клиент к
single-instance core, а не второй mpv. Визуализаторы и компоновки — часть
продукта, а не «рюшечки»: они отличают TUI от CLI и используют уже
существующие метрики эфира.

**Не делать:** обещать паритет с Orbital Desk, подменять Android-приоритет,
читать `library.json`, звать Radio Browser из TUI или запускать второй GUI.

**Следующий шаг:** T1 — скелет ratatui, `CoreClient` и реальные
`snapshot`/`state`; затем T2–T4, визуализаторы — T5.

---

## 13. История

| Дата | Изменение |
|------|-----------|
| 2026-11-03 | Концепция TUI; вариант IPC-client; scope MVP |
| 2026-11-03 | Принято: ADR-0047, интерфейс 16, модуль 19; library/search через IPC; headless отдельным треком; добавлены визуализаторы и компоновки |

