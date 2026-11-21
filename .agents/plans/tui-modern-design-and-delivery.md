# План: OrbitFM TUI — современный дизайн и поставка лёгкого терминального приложения

**Организация:** ModernByte  
**Дата:** 2026-11-21  
**Статус:** план принят к работе; **волна 1 запущена** по решению владельца от 2026-11-21  
**Роль:** мастер-план трека TUI. Дорожная карта и оценки — `docs/tui/roadmap.md`; язык дизайна — `docs/ui/tui-design-system.md`; хаб приложения — `docs/tui/README.md`.

---

## 1. Контекст и что уже сделано

К 0.23.0-rc.3 проектная команда (T0, 2026-11-03) уже закрыла **документальный**
фундамент TUI:

| Артефакт | Состояние |
|----------|-----------|
| `docs/adr/0047-tui-ratatui.md` | принят (рев. 2) |
| `docs/interfaces/16-tui.md` | принят (рев. 2) |
| `docs/modules/19-tui.md` | принят, 575 строк |
| `docs/audit/OrbitFM-tui-ratatui.md` | актуализирован |
| `docs/ROADMAP.md` → «Трек TUI на ratatui» | T0 закрыт, T1–T6 открыты |
| `.agents/skills/orbitfm-tui/SKILL.md` | есть, маршрутизация описана |

**Чего нет (пробелы, закрываемые этим планом):**

1. **Языка дизайна.** Каталог `docs/ui/` описывает только desktop; визуальный
   стиль TUI был «минимальная палитра + один accent» одной строкой. Нет токенов,
   компонентов, глифов, движения, доступности, эталонных композиций.
2. **Главных документов отдельного приложения.** Нет `docs/tui/` хаба и
   карты версий TUI: читатель не понимает, что это за продукт, как ставится, как
   версионируется и чем отличается от desktop.
3. **Плана и декомпозиции.** Не было mastered-плана с волнами, задачами,
   приоритетами, оценками и зависимостями.
4. **Команды субагентов.** Роли в `docs/AGENTS.md` §2 — это скиллы, а не
   запускаемые тимейты для трека.

**Кода нет ни строки:** `crates/` отсутствует, workspace в `Cargo.toml` не
объявлен, `ratatui` в дереве зависимостей нет. Никакая другая команда код не
начинала — проверено `find` по репозиторию.

---

## 2. Цель

Довести трек TUI от документации до **лёгкого отдельного приложения
`orbitfm-tui`** с современным терминальным дизайном, не нарушая ни одного
инварианта OrbitFM:

- один процесс с аудио (single-instance, ADR-0005);
- TUI — IPC-клиент, не второй libmpv/MPRIS (ADR-0047);
- никакого чтения `library.json`, Radio Browser и сетевых портов из TUI;
- `q` не трогает эфир, `Q` требует подтверждения;
- desktop-пакет `orbitfm` не получает зависимость на `ratatui`.

---

## 3. Доказательная база (проверено в этой сессии)

| Факт | Источник |
|------|----------|
| `crates/` нет, `workspace` в `Cargo.toml` не объявлен | `ls`, `grep` по репозиторию |
| Нет `ratatui`/`crossterm` в `Cargo.toml`/`Cargo.lock` | `grep` |
| T0-документы на месте, T1–T6 не начаты | `docs/ROADMAP.md:1519+`, `docs/modules/19-tui.md` §12 |
| Ядро отдаёт `events` `ev: 1` с `snapshot`/`state`/`search`/`keepalive` | ADR-0039, interface 13 |
| Ядро имеет `play`/`playpause`/`stop`/`next`/`previous`/`roulette`/`volume-up|down`/`favorite`/`library`/`search`/`play-result`/`play-library`/`sleep`/`quit` | interface 05 |
| `library` отдаёт ≤ 20 записей на список и ≤ 64 КиБ | interface 16 §5.1 |
| Планируемый `events --spectrum` ещё **не** реализован в ядре | ADR-0047 §9.3, interface 16 §4.1 |
| MSRV проекта — Rust **1.100**, edition 2024 | `Cargo.toml`, CHANGELOG 0.23.0-rc.3 |
| Целевые версии TUI-крейтов: ratatui 0.30.2, crossterm 0.29.0, unicode-width 0.2.3 | `docs/modules/19-tui.md` §2.1 |
| Субагенты pi читаются из `.pi/agents/*.md` (scope `project`/`both`) | README расширения `subagent` |

---

## 4. Волны

### Волна 0 — Дизайн и контракты (сделано в этой сессии)

- [x] **DSN-0** — `docs/ui/tui-design-system.md`: токены, палитра ANSI-16,
      темы, глифы unicode/ascii, сетка, компоненты, визуализаторы, движение,
      доступность, эталонные композиции, маппинг на `tui.toml` и `theme.rs`.
- [x] **DOC-HUB** — `docs/tui/README.md`: хаб отдельного приложения.
- [x] **DOC-ROAD** — `docs/tui/roadmap.md`: версионная карта TUI с приоритетами
      и оценками.
- [x] **PLAN-0** — этот план.
- [x] **TEAM-0** — команда тимейтов в `.pi/agents/tui-*.md`.
- [x] **ADR-R3** — ревизия 3 ADR-0047 + интерфейс 16: поля `ui.theme`,
      `ui.border`, `ui.glyphs = auto`, порядок разрешения темы/акцента/глифов,
      статус `spectrum`. Выполнено 2026-11-21 (`tui-architect`).

### Волна 1 — Фундамент (milestone **TUI 0.1.0**)

- [x] **FND-1** — Cargo workspace + `crates/orbitfm-tui` + CI-джоба. Корневой
      пакет не тянет ratatui.
- [x] **FND-2** — terminal lifecycle: raw mode, alternate screen, guard,
      panic hook, min size 60×18.
- [x] **FND-3** — `src/ui/theme.rs` по `docs/ui/tui-design-system.md` §9.2
      (ANSI-16, `NO_COLOR`, `TERM=dumb`, локаль, `ORBITFM_TUI_THEME`).
- [x] **FND-4** — компонентный кит: header, panel, station row, gauge,
      status bar, footer, overlay, toast/empty/error states.
- [x] **FND-5** — `config.rs`: схема 1, чтение `[ui]`/`[visualizer]`,
      атомарная запись (`tmp`→`rename`) с debounce 500 мс.
- [x] **VER-1** — динамическая версионность: `build.rs` (git `sha`/`dirty`,
      дата, `SOURCE_DATE_EPOCH`), `src/version.rs`, `--version`/`--help`,
      схема — `docs/tui/versioning.md`; установлен локально.
- [x] **FND-6** — `app.rs`: состояние, `AppEvent`, цикл, `dirty`-перерисовка,
      дедлайны.
- [x] **FND-7** — `ui/layout.rs`: базовые `desk`/`minimal` + адаптивность.

### Волна 2 — Клиент ядра (milestone **TUI 0.1.0**)

- [x] **CLC-1** — трейт `CoreClient`, модели, `CoreCommand/Reply/Event`.
- [x] **CLC-2** — поток `orbitfm events`: NDJSON, первый `snapshot`, stale 45 с,
      backoff 1→30 с, `R`.
- [x] **CLC-3** — worker one-shot команд: сериализация, таймаут 15 с, `kill`,
      классификация `NotRunning`/`Command`/`Protocol`/`Timeout`/`Io`.
- [x] **CLC-4** — гейты совместимости `schema: 1`, `app: orbitfm`, `ev: 1`;
      блокировка управления при несовпадении.
- [x] **CLC-5** — парсеры `snapshot`/`state`/`keepalive`/`search`/`library`/
      `favorite`/`ok sleep`; лимит 64 КиБ; NaN/out-of-range отбрасываются.

### Волна 3 — Экраны (milestone **TUI 0.1.0**)

- [x] **SCR-1** — Now Playing + строка статуса; пустое состояние.
- [x] **SCR-2** — транспорт: `p`/`Space`, `s`, `[`/`]`, `r`, `+`/`-`.
- [x] **SCR-3** — Favorites/History: `library`, `play-library`, `favorite`,
      refresh, потолок 20.
- [x] **SCR-4** — Search: ввод, `search <id>`, корреляция по `id`, буферизация
      обратного порядка, таймаут 15 с, `play-result` → `play` fallback.
- [x] **SCR-5** — Help: клавиши, версии TUI/ядра, ограничения v1.
- [x] **SCR-6** — оверлеи: `QuitConfirm`, `SleepPrompt`, `Modules`.
- [x] **SCR-7** — `q`/`Q`-семантика и честный отказ без ядра.

**Критерий TUI 0.1.0** — критерии готовности из `docs/ROADMAP.md` (живое ядро
видно, играют избранное/история/поиск, `q`/`Q`, нет паники при resize).

### Волна 4 — Визуализаторы, компоновки, движение (milestone **TUI 0.2.0**)

- [x] **VIZ-1** — модель визуализатора: сглаживание, idle-дыхание, procedural.
- [x] **VIZ-2** — `bars` (default).
- [x] **VIZ-3** — `ring`.
- [x] **VIZ-4** — `scope`.
- [x] **VIZ-5** — `pulse` + авто-выбор для узкого/`motion=off`.
- [x] **LAY-1** — пресеты `desk`/`focus`/`library`/`minimal`/`dashboard`.
- [ ] **LAY-2** — менеджер модулей + сохранение в `tui.toml`.
- [x] **MOT-1** — `motion full|reduced|off`, лимит FPS, скрытый модуль не тикает.
- [ ] **CRX-1** — ядро: `events --spectrum` (interface 13/16, ADR). Если
      владелец откладывает — TUI остаётся в procedural, это законный режим.

### Волна 5 — Качество и поставка (milestone **TUI 0.2.0**)

- [ ] **QAT-1** — unit-тесты parse/model/text/config/ui/visualizer.
- [ ] **QAT-2** — интеграционные тесты с fake `ProcessRunner`/`CoreClient`.
- [ ] **QAT-3** — live-приёмка на живом ядре (чек-лист модуля 19 §10).
- [ ] **SEC-1** — ревью терминальной инъекции, shell-less `Command`,
      подтверждение `Q`, отсутствие сети/файлов.
- [ ] **DOC-1** — README/установка TUI, синхронизация хаба и модуля 19.
- [ ] **DOC-2** — CHANGELOG/ROADMAP/VERSIONMAP.
- [ ] **PKG-1** — решение владельца: source-only (`cargo install`) или пятый
      релизный артефакт.

### Волна 6 — Later (milestone **TUI 0.3.0 → 1.0.0**)

- [ ] **T6-1** — визуализаторы `stars`/`heat`.
- [ ] **T6-2** — прямой сокет-клиент (замена CLI-адаптера без смены UI).
- [ ] **T6-3** — мышь.
- [ ] **T6-4** — image protocol (Kitty/sixel) — favicon, только fun.
- [ ] **HLR-1** — headless-ядро `--headless`/`ORBITFM_HEADLESS=1` (отдельный
      трек, разблокирует чистый SSH).
- [ ] **LOC-1** — локализация TUI (после стабилизации UX), если владелец решит.
- [ ] **STB-1** — стабилизация 1.0.0: заморозка контракта, нагрузка, память.

---

## 5. Реестр задач (приоритет · сложность · агент)

| ID | Задача | Приоритет | Сложность | Агент | Зависит от |
|----|--------|-----------|-----------|-------|------------|
| DSN-0 | Язык дизайна TUI | P0 | L | tui-design | — |
| DOC-HUB | Хаб приложения | P0 | S | tui-docs | DSN-0 |
| DOC-ROAD | Карта версий TUI | P0 | S | tui-docs | DSN-0 |
| PLAN-0 | Мастер-план | P0 | M | tui-lead | — |
| TEAM-0 | Команда тимейтов | P0 | M | tui-lead | PLAN-0 |
| ADR-R3 | ADR-0047 рев. 3 + interface 16 | P0 | M | tui-architect | DSN-0 |
| FND-1 | Workspace + crate + CI | P0 | M | tui-foundation | ADR-R3 |
| FND-2 | Terminal lifecycle | P0 | M | tui-foundation | FND-1 |
| FND-3 | Theme/tokens | P0 | M | tui-design | ADR-R3 |
| FND-4 | Компонентный кит | P0 | L | tui-foundation | FND-3 |
| FND-5 | `tui.toml` (чтение сделано; запись — остаётся) | P1 | M | tui-foundation | FND-1 |
| VER-1 | Динамическая версионность | P0 | M | tui-foundation | FND-1 |
| FND-6 | App state + loop | P0 | L | tui-foundation | FND-2 |
| FND-7 | Базовые компоновки | P1 | M | tui-layout | FND-4/6 |
| CLC-1 | `CoreClient` + модели | P0 | M | tui-coreclient | FND-1 |
| CLC-2 | Поток events + reconnect | P0 | L | tui-coreclient | CLC-1 |
| CLC-3 | Worker команд | P0 | M | tui-coreclient | CLC-1 |
| CLC-4 | Гейты совместимости | P0 | S | tui-coreclient | CLC-2 |
| CLC-5 | Парсеры | P0 | M | tui-coreclient | CLC-1 |
| SCR-1 | Now Playing + status | P0 | M | tui-screens | FND-4, CLC-2 |
| SCR-2 | Транспорт | P0 | S | tui-screens | SCR-1, CLC-3 |
| SCR-3 | Favorites/History | P0 | M | tui-screens | SCR-1, CLC-5 |
| SCR-4 | Search | P0 | L | tui-screens | SCR-1, CLC-5 |
| SCR-5 | Help | P1 | S | tui-screens | FND-4 |
| SCR-6 | Оверлеи | P0 | M | tui-screens | FND-4, CLC-3 |
| SCR-7 | `q`/`Q`-семантика | P0 | S | tui-screens | SCR-6 |
| VIZ-1 | Модель визуализатора | P1 | M | tui-visualizer | FND-6 |
| VIZ-2 | `bars` | P1 | M | tui-visualizer | VIZ-1 |
| VIZ-3 | `ring` | P1 | M | tui-visualizer | VIZ-1 |
| VIZ-4 | `scope` | P1 | M | tui-visualizer | VIZ-1 |
| VIZ-5 | `pulse` | P1 | S | tui-visualizer | VIZ-1 |
| LAY-1 | Пресеты компоновки | P1 | M | tui-layout | FND-7 |
| LAY-2 | Менеджер модулей | P1 | M | tui-layout | FND-5, LAY-1 |
| MOT-1 | Motion-режимы | P1 | S | tui-layout | FND-6 |
| CRX-1 | `events --spectrum` (ядро) | P1 | L | tui-spectrum | ADR-R3 |
| QAT-1 | Unit-тесты | P0 | L | tui-qa | FND/CLC/SCR |
| QAT-2 | Интеграционные тесты | P0 | M | tui-qa | CLC |
| QAT-3 | Live-приёмка | P0 | M | tui-qa | MVP |
| SEC-1 | Безопасность | P0 | M | tui-qa | SCR/CLC |
| DOC-1 | README/установка | P1 | S | tui-docs | MVP |
| DOC-2 | Sync CHANGELOG/ROADMAP/VERSIONMAP | P1 | S | tui-docs | MVP |
| PKG-1 | Решение о поставке | P0 | XS | tui-lead | владелец |
| T6-1..4 | Later | P2 | L/XL | tui-visualizer | 0.2.0 |
| HLR-1 | Headless core | P2 | XL | tui-architect | отдельный ADR |

Сложность: XS ≤ полдня, S ≈ 1 день, M ≈ 2–3 дня, L ≈ 4–6 дней, XL > недели.
Оценки — для одного исполнителя; параллелить можно по волнам и агентам.

---

## 6. Команда тимейтов (`.pi/agents/tui-*.md`)

Каждый тимейт — markdown с frontmatter (`name`, `description`, при
необходимости `tools`) и системным промптом: **скиллы, обязательные документы,
роль, контекст, методы, границы, отчёт, Definition of Done**. Scope — `project`
(union) или `both` в конфигурации расширения `subagent`; файлы доверены только
для этого репозитория.

| Тимейт | Зона | Ключевые скиллы | Волны |
|--------|------|-----------------|-------|
| `tui-lead` | координация, scope, приёмка, роадмап | product, core, architect | все |
| `tui-design` | язык дизайна, tokens/theme | tui, core, gui (эстетика) | 0, 1 |
| `tui-architect` | ADR, публичные контракты | architect, core, ipc, docs | 0, 4 |
| `tui-foundation` | workspace, loop, guard, UI-кит, config | tui, rust, core | 1 |
| `tui-coreclient` | CoreClient, events, worker, parse | tui, ipc, rust | 2 |
| `tui-screens` | Now Playing, library, search, help, overlays | tui, rust, ipc | 3 |
| `tui-visualizer` | модель и режимы визуализаторов | tui, gui (приёмы), rust | 4, 6 |
| `tui-layout` | пресеты, модули, motion | tui, rust | 1, 4 |
| `tui-spectrum` | `events --spectrum` в ядре | audio, ipc, architect | 4 |
| `tui-qa` | тесты, fakes, live-приёмка, безопасность | rust, tui, ipc | 5 |
| `tui-docs` | README, roadmap, sync | docs, core, product | 0, 5 |

Правила передачи (из `docs/AGENTS.md` §2.2): `orbitfm-core` первым; профильный
скилл по матрице; отчёт начинается со строки «Скиллы: …»; при конфликте скилла и
ADR действует ADR.

---

## 7. Критические файлы

**Новые (код):** `crates/orbitfm-tui/Cargo.toml` ·
`crates/orbitfm-tui/src/{main,app,model,config,text,keymap}.rs` ·
`crates/orbitfm-tui/src/core_client/{mod,cli,parse,worker}.rs` ·
`crates/orbitfm-tui/src/ui/{mod,layout,modules,now_playing,search,library,help,overlay,theme,widgets}.rs` ·
`crates/orbitfm-tui/src/ui/visualizer/{mod,bars,ring,scope,pulse,stars,heat}.rs`.

**Новые (доки):** `docs/ui/tui-design-system.md` ·
`docs/tui/README.md` · `docs/tui/roadmap.md` ·
`.agents/plans/tui-modern-design-and-delivery.md` · `.pi/agents/tui-*.md`.

**Правки:** `Cargo.toml` (workspace) · `Cargo.lock` · `.github/workflows/*`
(TUI-джоба) · `docs/adr/0047-tui-ratatui.md` (рев. 3) ·
`docs/interfaces/13-events.md`, `16-tui.md` ·
`docs/modules/19-tui.md` · `src/ipc/*`, `src/session/*` (только `--spectrum`) ·
`docs/ROADMAP.md` · `docs/VERSIONMAP.md` · `docs/CHANGELOG.md` ·
`docs/README.md`, `docs/dev/README.md` (ссылки на хаб TUI) ·
`docs/AGENTS.md` (§2 и §9 — регистрация тимейтов и плана).

---

## 8. Верификация

Каждая волна отдельно:

```bash
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo clippy --all-targets --no-default-features -- -D warnings
cargo test --all-targets
bash packaging/cargo-deny-check.sh
cargo check --all-targets --locked
```

Дополнительно для TUI: `TERM=xterm-256color cargo run -p orbitfm-tui` на живом
ядре; прогон с `NO_COLOR=1`, `glyphs="ascii"`, `motion=off`,
`COLUMNS=60 LINES=18`; `R` при убитом ядре; `q`/`Q`; resize во время эфира.

Правило честности (ADR-0052): ни один README/док не утверждает работу того,
для чего нет лога или замера в этом репозитории.

---

## 9. Решения владельца (2026-11-21)

1. **Старт кода — да.** Волна 1 запущена; правило «не начинать T1 без
   решения» выполнено явным указанием владельца.
2. **Android на паузе.** Трек TUI получает приоритет; parallel-track 10–20%
   больше не ограничение. Android возобновляется отдельным решением.
3. **`events --spectrum` — обязательно.** `CRX-1` переводится в P0 для
   milestone 0.2.0; procedural остаётся только аварийным fallback и режимом
   для ядра без флага, но не целью.
4. **Поставка — и так, и так.** TUI распространяется и как source-only
   (`cargo install --path crates/orbitfm-tui`), и как отдельный релизный
   артефакт. `PKG-1` закрыт: пятый артефакт добавляется в релизный конвейер
   (формат уточняет `tui-lead` вместе с владельцем).
5. **Первое имя темы.** `orbit-dark` как default — подтверждается
   ADR-0047 рев. 3; список тем (`ice`/`amber`/`green`/`magenta`/`mono`)
   фиксируется там же.
6. **Локализация TUI** (`LOC-1`) — остаётся после 0.2.0 (P2), решается
   позже.

---

## 10. Чего сознательно не делаем

Второй аудио-движок/libmpv в TUI; MPRIS из TUI; чтение `library.json`,
`settings.json`, кэша обложек; Radio Browser и favicon из TUI; сетевой сервер;
прямой сокет в v1; мышь/image protocol в v1; полный список избранного больше 20
записей до аддитивного расширения IPC; локализация до стабилизации UX;
обещания паритета с Orbital Desk; `play-url` и `library-import` в TUI.
