# Модуль: TUI (orbitfm-tui)

**Путь в коде (план):** `crates/orbitfm-tui/`  
**Ответственность:** терминальный клиент к работающему ядру OrbitFM: состояние,
поиск, избранное, история, управление эфиром и таймером сна, визуализаторы и
настраиваемая компоновка модулей.  
**Зависимости (план):** ratatui 0.30.2, crossterm 0.29.0 (через ре-экспорт
ratatui), unicode-width 0.2.3, serde, serde_json, toml 1.1, thiserror

---

## 1. Назначение

`orbitfm-tui` — отдельный бинарь и отдельный crate в Cargo workspace. Он не
содержит аудио-движка, не создаёт MPRIS-агента, не читает `library.json`, не
обращается к Radio Browser и не открывает сетевых портов. Все действия идут
через публичный CLI/IPC ядра (ADR-0047, интерфейс 16).

Модуль решает три задачи:

1. Показать состояние эфира в терминале без egui/wgpu.
2. Дать быстрый доступ к поиску, избранному, истории и ритуалам слушания.
3. Не нарушить single-instance: ядро остаётся единственным владельцем аудио,
   MPRIS, трея и библиотеки.

Кода в репозитории пока нет: это модульная спецификация для этапов T1–T6.
Визуализаторы и компоновки — обязательная часть продукта (ADR-0047 §9), а не
необязательный декор: они используют существующие метрики эфира и не меняют
аудио-ядро.

## 2. Состав crate (план)

```text
crates/orbitfm-tui/
├── Cargo.toml
└── src/
    ├── main.rs              # terminal guard, event loop, wiring
    ├── app.rs               # состояние, экраны, обработка клавиш
    ├── model.rs             # UI-модели: станция, состояние, библиотека
    ├── config.rs            # tui.toml: layout/modules/visualizer/ui
    ├── text.rs              # санитизация, unicode-width, обрезка
    ├── keymap.rs            # таблица клавиш и контекстные действия
    ├── core_client/
    │   ├── mod.rs           # CoreClient, CoreCommand, CoreReply, CoreEvent
    │   ├── cli.rs           # CliCoreClient: дочерние процессы orbitfm
    │   ├── parse.rs         # разбор status/library/search/events/spectrum
    │   └── worker.rs        # сериализация one-shot команд
    └── ui/
        ├── mod.rs           # каркас, layout, строка статуса
        ├── layout.rs        # пресеты desk/focus/library/minimal/dashboard
        ├── modules.rs       # менеджер модулей и видимость
        ├── now_playing.rs
        ├── search.rs
        ├── library.rs       # избранное и история
        ├── help.rs
        ├── overlay.rs       # подтверждение Q и ввод sleep
        ├── theme.rs         # палитра терминала и символы
        └── visualizer/
            ├── mod.rs       # VisualizerModel, выбор режима, тик
            ├── bars.rs      # Orbital Bars
            ├── ring.rs      # Orbit Ring
            ├── scope.rs     # Braille Scope
            ├── pulse.rs     # Core Pulse
            ├── stars.rs     # Signal Constellation (T6)
            └── heat.rs      # Spectrogram Heat (T6)
```

Правила:

- файлы держатся в мягком лимите 300–400 строк; `app.rs` и `cli.rs` при
  росте делятся по экранам/адаптерам; каждый визуализатор — отдельный файл;
- UI-модули не знают о процессах и JSON, `core_client` не знает о ratatui;
- `model.rs` содержит только данные, пригодные для отрисовки;
- `config.rs` не знает о ratatui и core, только serde-структуры и запись;
- тесты парсеров живут рядом с парсерами, UI-тесты — в `app.rs`/`ui` или
  `tests/`.

### 2.1. Cargo workspace

Корневой `Cargo.toml` становится workspace-корнем с участниками `"."` и
`"crates/orbitfm-tui"`. Desktop-пакет не получает ratatui. TUI-крейт:

```toml
[package]
name = "orbitfm-tui"
version = "0.1.0"
edition = "2024"
rust-version = "1.95"

[dependencies]
ratatui = "0.30.2"
unicode-width = "0.2.3"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
toml = "1.1"
thiserror = "2.0"
```

crossterm берётся из ре-экспорта ratatui (`ratatui::crossterm`), чтобы версии
backend и event API не разъезжались. Отдельная зависимость `crossterm`
добавляется только если ре-экспорта не хватит; версия тогда фиксируется
`0.29.0`.

## 3. Состояние приложения

```rust
struct App {
    core: CoreLink,                 // Connecting | Live | Disconnected
    screen: Screen,                 // NowPlaying | Search | Favorites | History | Help
    overlay: Overlay,               // None | QuitConfirm | SleepPrompt | Modules
    status: Option<StatusSnapshot>,
    library: LibraryPayload,
    search: SearchState,
    selection: ScreenSelection,
    status_line: StatusLine,
    config: TuiConfig,              // ui/modules/visualizer из tui.toml
    layout: LayoutState,            // пресет, фокус, видимость модулей
    visualizer: VisualizerState,    // режим, модель, источник данных
    should_quit: bool,
}

enum CoreLink {
    Connecting,
    Live { core_version: String },
    Disconnected { message: String, retry_at: Instant },
}

struct SearchState {
    input: String,
    editing: bool,
    pending: Option<PendingSearch>, // id, query, deadline
    buffered_frame: Option<SearchFrame>,
    results: Vec<LibraryStation>,
    error: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum LayoutPreset { Desk, Focus, Library, Minimal, Dashboard }

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Module { NowPlaying, Visualizer, Search, Results, Favorites, History, Status, Help }

struct LayoutState {
    preset: LayoutPreset,
    focus: Module,
    visible: BTreeSet<Module>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum VisualizerKind { Bars, Ring, Scope, Pulse, Stars, Heat }

struct VisualizerState {
    kind: VisualizerKind,
    model: VisualizerModel,          // уровни, пики, фазы
    source: SpectrumSource,          // Procedural | Reactive
    last_frame: Option<SpectrumFrame>,
    last_tick: Instant,
}

enum SpectrumSource { Procedural, Reactive }
```

Инварианты:

- одновременно ожидается не более одного поиска и одной one-shot команды;
- `status == None` рисует экран подключения, а не «остановлено»;
- `status.station == None` — валидное состояние, не паника;
- `library` всегда содержит два вектора, даже если ядро вернуло пустые;
- любой ответ ядра заменяет только свою часть состояния;
- скрытый визуализатор не тикает и не подписывается на `spectrum`;
- ошибка `tui.toml` не блокирует запуск: берутся defaults, предупреждение — в
  статус; пользовательский файл не перезаписывается молча;
- пресет компоновки меняет только раскладку, не данные и не состояние эфира;
- визуализатор не влияет на команды, таймауты и очередь поиска.

## 4. Event loop

Главный поток:

1. Включает raw mode и alternate screen через guard.
2. Запускает поток чтения `orbitfm events`.
3. Запускает worker one-shot команд.
4. Крутит цикл:
   - `crossterm::event::poll(timeout)`; `timeout` — до ближайшего дедлайна
     (анимация, поиск, статус, backoff), но не больше 250 мс;
   - читает клавиатуру, обновляет состояние;
   - drains единый канал `AppEvent` (кадры ядра, результаты команд);
   - проверяет дедлайны поиска, строки статуса и переподключения;
   - двигает визуализатор по тику и перерисовывает кадр только при `dirty`.

Все производители шлют события в один `std::sync::mpsc`:

```rust
enum AppEvent {
    Core(CoreEvent),
    Command(CoreCommandResult),
    Tick(Instant),
}
```

Терминальные события не отправляются в канал: они опрашиваются главным
потоком, чтобы не смешивать ввод с сетевыми/процессными событиями.

Частота:

| Условие | Тик |
|---------|-----|
| `motion = full`, визуализатор виден, эфир играет | 66 мс (≤ 15 FPS) |
| `motion = reduced` | 250 мс (4 FPS) |
| `motion = off`, эфир остановлен или визуализатор скрыт | 1 с / только по событию |
| ввод и кадры `state` | немедленная перерисовка |

Спиннер поиска анимируется тиком, но при простое и скрытом визуализаторе
нагрузка нулевая: кадр не собирается, если `dirty == false`.

### 4.1. Поток `events`

- `std::process::Command::new(bin).arg("events")`, stdout — piped;
- если визуализатор включён и `motion != "off"`, первая попытка —
  `events --spectrum`; при отказе ядра (код 1 до первого кадра, stderr про
  лишний аргумент) TUI перезапускает `events` без флага и ставит
  `SpectrumSource::Procedural`;
- `BufRead::lines()` по stdout, каждая строка — кадр NDJSON;
- stderr читается отдельным потоком и хранит последнюю строку для ошибки;
- EOF/ошибка чтения → `CoreEvent::Disconnected` с последней строкой stderr;
- процесс не перезапускается из потока: решение о backoff принимает `App`;
- при выходе TUI процесс убивается и дожидается.

Кадры `spectrum` не буферизуются: модель визуализатора берёт последний,
`seq` используется только для диагностики пропусков. Кадр с невалидными
полями отбрасывается целиком и не влияет на соединение.

### 4.2. Worker команд

- один поток, один `mpsc::Receiver<CommandJob>`;
- `CommandJob` содержит `CoreCommand` и `reply`-канал;
- для каждой команды запускается `orbitfm <args>`, stdout/stderr piped;
- ожидание с таймаутом 15 секунд; по истечении `child.kill()` и `wait()`;
- код 0 — разбор успешного ответа, код 1 — `CoreError::Command` или
  `NotRunning` по тексту stderr;
- результат уходит в `AppEvent::Command`;
- worker не блокирует UI; пока команда в полёте, экран показывает маленький
  индикатор в строке статуса.

## 5. CoreClient v1

`CliCoreClient` реализует интерфейс 16:

| Метод | Поведение |
|-------|-----------|
| `start_events(spectrum)` | запускает `orbitfm events [--spectrum]`, возвращает поток кадров; fallback без флага |
| `execute(Library)` | `orbitfm library`, парсит JSON |
| `execute(Search)` | `orbitfm search <query>`, парсит `search <id>` |
| `execute(Favorite)` | `orbitfm favorite [uuid]`, парсит `favorite true\|false` |
| `execute(Sleep)` | `orbitfm sleep <n>`, проверяет диапазон до запуска |
| `execute(Quit)` | `orbitfm quit` |
| остальные | `orbitfm <команда>`, успех — `ok` |

Поиск бинарника:

1. `ORBITFM_BIN`, если задан и непуст;
2. `orbitfm` из `PATH`;
3. ошибка `CoreError::Io` с подсказкой про `ORBITFM_BIN`.

Путь не проходит через shell и не склеивается со строкой запроса.

## 6. Экраны

### 6.1. Now Playing

- символ состояния и подпись (`playing`, `paused`, `stopped`, `buffering`,
  `reconnecting`, `error`);
- имя станции, страна/код, codec и bitrate;
- метка `radio`/`stream` и звёздочка избранного;
- полоса громкости и число процентов;
- остаток таймера сна, если `sleep != null`;
- доступность `[`/`]` по `can`;
- версия ядра и состояние соединения;
- подсказка `? help`.

Пустое состояние: «Ничего не играет» и подсказка `/` для поиска.

### 6.2. Search

- строка ввода с курсором, `/` фокусирует;
- `Enter` запускает поиск, `Esc` выходит из ввода/очищает;
- во время ожидания — «Ищу…» и таймаут 15 с;
- результаты: имя, страна, bitrate, source;
- `Enter` на строке — `play-result`, при ошибке — `play`;
- `a` — переключить избранное выбранной станции;
- ошибка кадра `search` показывается в списке, а не в отдельном модале.

### 6.3. Favorites и History

- два независимых экрана, список до 20 записей из `library`;
- `Enter` — `play-library <uuid>`;
- `a` — `favorite <uuid>` и перезапрос библиотеки;
- пустой список — подсказка, а не ошибка;
- после успешного запуска история перезапрашивается;
- потолок 20 явно показан в заголовке: «Favorites (20 max)» / «History (20)».

### 6.4. Help

- таблица клавиш;
- версия TUI и ядра;
- ограничения v1: только запущенное ядро, 20 записей библиотеки, без мыши,
  без favicon, английские подписи;
- `?`/`Esc` закрывают.

### 6.5. Оверлеи

- `QuitConfirm`: «Остановить эфир и выйти из OrbitFM?» с `y`/`n`;
- `SleepPrompt`: ввод минут `1..=720`, `Enter` отправляет `sleep`,
  `Esc` отменяет; `T` без ввода отправляет `sleep clear`;
- `Modules`: чекбоксы модулей, пресеты `1..5`, `Space` — переключить,
  `Enter` — применить и сохранить, `Esc` — отменить.

### 6.6. Визуализаторы

Модель одна на все режимы: `level`, `centroid_hz`, `spread_hz`, `flatness`,
`flux`, `bands`, время и состояние эфира. Отрисовщики не ходят в core и не
читают процессы — только получают `&SpectrumFrame` или `None`.

Общие правила:

- сглаживание как в desktop: attack ≈ 28 мс, линейная гравитация 1.8 ед/с,
  peak-hold падает 0.42 ед/с; `dt` ограничен 0.1 с;
- reactive → `Some(frame)`; procedural/idle → `None` и медленное «дыхание»;
- при `playing` без кадров > 2 с источник помечается procedural, сцена
  продолжает жить, статус пишет `visualizer: procedural`;
- буферы аллоцируются при создании и resize, кадр не выделяет память;
- высота < 3 строк — модуль скрывается; ширина < 8 — тоже;
- ни один режим не рисует полнокадровые инверсии и вспышки > 3 Hz.

| Режим | Алгоритм | Глифы |
|-------|----------|-------|
| `bars` | 24–64 столбца; высота в half-block единицах; пик отдельной меткой; цвет — accent-градиент | `▁▂▃▄▅▆▇█` / `▀▄█` |
| `ring` | полярная сетка вокруг центра; bass расширяет радиус, mid вращает фазу, high мерцает; в центре `▶`/код страны | Braille, fallback `*` |
| `scope` | кольцевой буфер 120 сэмплов; синус/сумма двух синусов; амплитуда от level, частота от centroid, дрожание от flux | Braille 2×4, fallback `-` |
| `pulse` | одна орбитальная линия/шкала; точка бежит по level, остаток сна — в подписи | block gauge |
| `stars` | 80–200 частиц, детерминированный seed от UUID; flux — варп, level — яркость | Braille/точки |
| `heat` | кольцевой буфер истории; X — время, Y — полосы/центроид; цвет — уровень | half blocks |

`bars` — default. `pulse` включается автоматически при `< 80` колонок,
`motion = "off"` или скрытии остальных визуализаторов. `stars` и `heat` —
T6; до них `v` циклит четыре режима.

### 6.7. Компоновки и модули

`LayoutPlan` считается из пресета, размера терминала и видимости модулей.
Нулевые зоны не создаются; строка статуса видна всегда.

| Пресет | ≥ 120 колонок | 80–119 | < 80 |
|--------|---------------|--------|------|
| `desk` | NP сверху, visualizer слева 60%, список справа | NP, visualizer сверху 40%, список снизу | одна колонка, visualizer скрыт |
| `focus` | visualizer на весь экран, NP+статус внизу | то же | visualizer или `pulse`, NP внизу |
| `library` | favorites и history в две колонки, visualizer-полоса | две колонки, visualizer скрыт | один активный список |
| `minimal` | NP + список + статус | то же | то же |
| `dashboard` | сетка модулей (btop-подобная) | две зоны | недоступен, откат к `desk` |

Модули: `now_playing`, `visualizer`, `search`, `results`, `favorites`,
`history`, `status`, `help`. Скрытый модуль не рисуется, не тикает и не
запускает запросов; уже полученные данные сохраняются.

Менеджер модулей (`m`) — оверлей:

- список чекбоксов, `Space` переключает, `Enter` применяет, `Esc` отменяет;
- пресеты выбираются `1..5` только внутри оверлея;
- изменения видимости применяются сразу после `Enter` и сохраняются;
- сброс `Ctrl+R` возвращает `desk` и дефолтную видимость.

Фокус: `Tab`/`Shift+Tab` обходят видимые модули, рамка фокуса — accent.
Ввод поиска и оверлеи забирают клавиши первыми.

### 6.8. Конфиг `tui.toml`

Путь: `$XDG_CONFIG_HOME/orbitfm/tui.toml`, fallback
`$HOME/.config/orbitfm/tui.toml`. Файл создаётся при первом изменении,
пишется атомарно (`tmp` → `rename`), debounce 500 мс. Core его не читает.

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
kind = "bars"          # bars|ring|scope|pulse|stars|heat
bars = 32              # 8..=64, только для bars
```

Правила разбора:

- неизвестные поля игнорируются, `schema != 1` — предупреждение и defaults;
- невалидные значения зажимаются в диапазон, а не роняют TUI;
- при ошибке чтения/разбора файл не перезаписывается без действия
  пользователя; в статусе — `config: defaults`;
- `frame_fps` ограничен сверху 30, чтобы терминал не захлёбывался;
- конфиг не содержит URL, UUID и станций — только параметры отображения.

## 7. Клавиши

| Контекст | Клавиша | Действие |
|----------|---------|----------|
| везде | `q` | выйти из TUI, ядро не трогать |
| везде | `Q` | подтверждение выхода из ядра |
| везде | `?` | Help |
| везде | `Tab`/`Shift-Tab` | фокус следующего/предыдущего видимого модуля |
| везде | `v` | следующий визуализатор |
| везде | `V` | визуализатор вкл/выкл |
| везде | `l` | следующий пресет компоновки |
| везде | `m` | менеджер модулей |
| везде | `M` | motion: full → reduced → off |
| везде | `Ctrl+R` | сброс компоновки к `desk` |
| везде | `R` | переподключиться к ядру |
| везде | `Ctrl-C` | как `q` |
| список | `j`/`k`, `↓`/`↑` | перемещение |
| список | `g`/`G` | начало/конец |
| список | `Enter` | играть выбранную станцию |
| список | `a` | переключить избранное |
| Now Playing | `p`/`Space` | play/pause |
| Now Playing | `s` | stop |
| Now Playing | `[`/`]` | previous/next |
| Now Playing | `r` | рулетка |
| Now Playing | `+`/`-` | громкость |
| Now Playing | `t`/`T` | задать/снять таймер сна |
| Search | `/` | фокус ввода |
| Search | `Enter` | отправить запрос |
| Search | `Esc` | выйти из ввода |
| оверлей | `y`/`n` | подтвердить/отменить `Q` |
| оверлей | `Space` | чекбокс модуля |
| оверлей | `1..5` | выбрать пресет |
| оверлей | `Enter` | применить менеджер модулей |
| оверлей | `Esc` | закрыть оверлей |

Пока поле поиска в фокусе, печатные символы идут в запрос, а `j`/`k` не
двигают список. Горячие клавиши без модификаторов в этот момент не
срабатывают, кроме `Esc` и `Enter`.

## 8. Отрисовка

- alternate screen + raw mode; guard восстанавливает терминал на выходе,
  ошибке и панике;
- минимальный размер 60×18; меньше — центрированное сообщение;
- каркас строится `LayoutPlan`: заголовок, зоны модулей, строка статуса (1),
  подсказка (1); пустые зоны не рисуются;
- палитра: фон терминала, один акцент, без обязательного truecolor;
  при `NO_COLOR` цвет отключается, состояние передаётся символами и текстом;
- состояния: `▶` playing, `‖` paused, `■` stopped, `…` buffering,
  `↻` reconnecting, `!` error;
- ширина считается `unicode-width`; длинные имена обрезаются с `…`;
- все строки проходят `text::sanitize` перед выводом;
- `glyphs = "ascii"` заменяет half blocks/Braille на `#`, `=`, `*`, `-`;
- визуализатор рисуется только в своей зоне, не перекрывает списки и статус;
- `motion = off` — статичный кадр, `reduced` — ≤ 4 FPS, `full` — ≤ 15 FPS;
- динамический заголовок терминала и OSC 8 не используются;
- мышь в v1 отключена.

## 9. Ошибки и устойчивость

- Ошибка команды не меняет последнее известное состояние: показывается
  строка статуса, Now Playing остаётся прежним.
- Ошибка `library` не очищает старый список: показывается предупреждение.
- Ошибка поиска очищает результаты только по новому запросу; старые
  результаты не выдаются за новые.
- При `Disconnected` экран затемняется баннером, но последнее состояние
  остаётся видимым с пометкой «последний снимок».
- После переподключения новый `snapshot` заменяет состояние; поиск и
  библиотека перезапрашиваются при следующем действии.
- Паника при отрисовке не должна оставлять терминал в raw mode: panic hook
  вызывает восстановление.
- SIGTERM/SIGINT: Ctrl-C обрабатывается как `q`; внешний SIGTERM завершает
  процесс, guard восстанавливает терминал, если ОС даёт время.
- Ошибка `tui.toml` — defaults в памяти, предупреждение в статусе; файл не
  перезаписывается, пока пользователь сам не изменит настройку.
- Отказ `events --spectrum` — однократный fallback на `events`, статус
  `visualizer: procedural`; повторных попыток нет.
- Остановка кадров `spectrum` при `playing` > 2 с — сцена гаснет в idle,
  соединение и управление не страдают.
- Ошибка записи `tui.toml` — предупреждение в статусе, настройки остаются в
  памяти до выхода.

## 10. Тестирование

Unit-тесты:

- `parse.rs`: `snapshot`, `state`, `keepalive`, `search`, `spectrum`,
  неизвестный тип, битый JSON, `schema`/`app`/`ev`, `library`, `favorite`,
  `ok sleep`; NaN/out-of-range в `spectrum` отбрасываются;
- `model.rs`: пустая станция, отсутствующий `sleep`, `station: null`;
- `text.rs`: C0/C1/ESC/bidi, переводы строк, обрезка по ширине, эмодзи;
- `app.rs`: клавиши в списке и вводе, `q`/`Q`, подтверждение, таймаут
  поиска, обратный порядок кадра и ответа, чужой `id`, backoff, `R`,
  `v`/`V`/`l`/`m`/`M`, сброс `Ctrl+R`;
- `config.rs`: defaults, roundtrip `tui.toml`, неизвестные поля, `schema != 1`,
  зажим `frame_fps`/`bars`, повреждённый файл;
- `ui`: минимальный размер, пустые списки, длинные имена, `NO_COLOR`,
  `glyphs = ascii`, скрытый модуль не рисуется, `LayoutPlan` без нулевых зон;
- `visualizer`: нормализация кадров, idle-дыхание, границы столбцов,
  переключение режимов, отсутствие аллокаций в кадре (по возможности),
  `motion=off` не двигает модель.

Интеграционные тесты:

- fake `ProcessRunner` для `CliCoreClient`: код 0/1, `OrbitFM не запущен`,
  таймаут, лишний stderr, невалидный JSON, fallback `events --spectrum` →
  `events` без флага;
- fake `CoreClient` для UI-сценариев: snapshot → play → state, поиск,
  кадры `spectrum`, разрыв и переподключение.

Live-приёмка после T1–T5:

- живое ядро: Now Playing, громкость, play/pause, stop;
- запуск из избранного, истории и поиска;
- `q` не останавливает эфир, `Q` + `y` останавливает;
- рестарт ядра: баннер, backoff, новый snapshot;
- resize и пустой поиск без паники;
- переключение `v`/`V`/`l`/`m`, сохранение в `tui.toml`, `motion=off`,
  отсутствие роста CPU при скрытом визуализаторе;
- терминал восстановлен после выхода и после ошибки.

## 11. Ограничения v1

- Требуется запущенное ядро; чистый SSH ждёт headless-режим (отдельный ADR).
- Нет полного списка избранного: `library` отдаёт 20 записей.
- Нет мыши, favicon, изображений, настроек, тем ядра и `play-url`.
- Нет локализации: английские подписи, строки `status.ui` не смешиваются.
- Нет собственного кэша: при разрыве показывается последний снимок с
  пометкой.
- Нет прямого сокета: v1 зависит от CLI в `PATH` или `ORBITFM_BIN`.
- Нет управления окном (`show`/`hide`/`mode`): TUI не поднимает GUI.
- Нет экспорта/импорта библиотеки: это файловые операции desktop-клиента.
- Reactive-визуализаторы требуют будущего `events --spectrum`; до него
  работает только procedural, что явно написано в статусе.
- `stars`/`heat` — T6; в v1 четыре режима (`bars`, `ring`, `scope`, `pulse`).
- `dashboard` недоступен на терминалах < 120 колонок и откатывается к `desk`.

## 12. Этапы

| Этап | Содержание | Статус |
|------|------------|--------|
| T0 | ADR-0047, интерфейс 16, модуль 19, аудит, ROADMAP | сделан документами |
| T1 | workspace, crate, ratatui loop, экраны на mock-данных, terminal guard, min size | не начат |
| T2 | `CliCoreClient`, `events`, worker, Now Playing, play/pause/stop/volume/next/prev/roulette, `q`/`Q` | не начат |
| T3 | `library`, избранное/история, `play-library`, `favorite`, refresh | не начат |
| T4 | `search`, корреляция `id`, `play-result`/`play`, таймаут | не начат |
| T5 | `bars`/`ring`/`scope`/`pulse`, пресеты `desk`/`focus`/`library`/`minimal`, менеджер модулей, `tui.toml`, `events --spectrum` + fallback, Help, sleep prompt, README, приёмка | не начат |
| T6 | `stars`/`heat`, direct socket, mouse, image protocol | не начат |

Headless-ядро — отдельный трек, не блокирует T1–T5, но нужен для сценария
«чистый SSH без графической сессии».

## 13. Связанные документы

- ADR-0047 — решение о TUI как IPC-клиенте
- ADR-0005 — single-instance и IPC
- ADR-0039 — поток событий
- ADR-0046 — таймер сна, очередь и библиотека
- `docs/interfaces/16-tui.md` — контракт TUI ↔ ядро
- `docs/interfaces/05-ipc.md`, `docs/interfaces/13-events.md`
- `docs/audit/OrbitFM-tui-ratatui.md`
