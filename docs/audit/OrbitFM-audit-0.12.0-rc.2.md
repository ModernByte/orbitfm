# OrbitFM — аудит кодовой базы, архитектуры и безопасности

| Поле | Значение |
|------|----------|
| **Дата** | 2026-10-28 |
| **Версия** | `0.12.0-rc.2` |
| **Коммит** | `71e9bd8` (`fix(ui): строки настроек складываются в узком окне`) |
| **Объём** | ~33 051 LOC Rust (`src/`), 77 `.rs` файлов, 36 ADR |
| **Источник** | https://github.com/ModernByte/orbitfm |

---

## 1. Резюме для руководства

OrbitFM вырос из прототипа в **плотный product-grade RC**: i18n (11 языков), drawer настроек, анимации, packaging (AppImage/Flatpak/MSI/DMG), macOS в packaging и CI-контуре, расширенный IPC (`panel` / `overlay`), heatmap, vector icons, согласованность списка и глобуса.

**Сильные стороны:** документация и ADR-дисциплина; границы IPC; favicon pipeline с лимитами; thin Noctalia plugin; Linux-first packaging.

**Критичные долги:**

1. **SSRF на favicon** — по-прежнему только scheme allowlist (`http`/`https`), без блокировки private/link-local/metadata IP.
2. **Монстр-файлы** — `widgets.rs` (~1813), `motion.rs` (~1499), `app.rs` (~1380), `globe_view.rs` (~1343) — хуже, чем на аудите 2026-10-25 по `app`/`widgets`.
3. **Экосистема шеллов** — в репо по-прежнему один first-class plugin (Noctalia); Plasma/GNOME/Omarchy — в планах, не в коде.

**Оценка (1–5):**

| Измерение | Оценка |
|-----------|--------|
| Архитектура слоёв | 4.3 |
| Модульность файлов | 3.0 |
| Безопасность (artwork/IPC) | 3.7 |
| Производительность (дизайн) | 4.0 |
| Документация | 5.0 |
| Packaging / release | 4.5 |
| Shell ecosystem | 3.2 |
| Готовность 0.12 stable | 4.0 (после P0 SSRF + smoke macOS/Win) |

---

## 2. Что изменилось с аудита 2026-10-25

| Было (~20.6k LOC, 29 ADR) | Стало (~33k LOC, 36 ADR, 0.12.0-rc.2) |
|---------------------------|--------------------------------------|
| i18n в планах | `src/i18n/*` + ADR-0030 |
| Settings popup | drawer + ADR-0031, `settings_panel.rs` |
| Анимации в планах | `animation.rs` + ADR-0032 |
| — | ADR-0033 panel dropdown, 0034 vector icons, 0035 heatmap, 0036 list↔globe |
| packaging зачаточный | linux / windows / macos scripts + desktop/metainfo |
| app ~846 | app ~1380 (регресс модульности) |
| motion ~1485 | motion ~1499 (без split) |

Вывод: **продуктовая зрелость выросла быстрее, чем гигиена размера файлов.**

---

## 3. Архитектура

### 3.1. Слои (факт кода)

```
main / run_session
    → Session (core: playback, search, library, settings, window mode)
    → OrbitApp / ui/* (presentation)
    → audio/* (libmpv worker)
    → render/* (wgpu globe, motion, spectrum)
    → ipc/* (socket/pipe protocol)
    → tray/* (SNI Linux)
    → media/* (MPRIS / SMTC host window)
    → services/* (Radio Browser + discovery)
```

Направление зависимостей в целом соблюдается. UI разросся (`controls`, `country_map`, `settings_panel`, `animation`) — ожидаемо после 0.10–0.12.

### 3.2. IPC как правильный «API продукта»

Команды (фрагмент): `toggle`, `show`, `hide`, `play`, `stop`, `status`, `quit`, `volume`, `mode`, **`panel`**, **`overlay`**.

Это правильный фундамент для **всех** внешних оболочек: виджеты не содержат audio, только вызывают CLI/IPC.

### 3.3. Риски архитектуры

| Риск | Деталь |
|------|--------|
| Presentation bloat | `app.rs` снова оркестратор на 1.3k+ строк (favicon HTTP, drawing, actions) |
| Motion monolith | Вся ADR-0028 логика в одном файле |
| Widgets catch-all | 1.8k строк — glass, sliders, shared chrome |
| Двойная роль Session | settings + playback + notices + tray snapshot — следить за god-object |

Рекомендация: **заморозить** новые фичи в `widgets.rs` / `motion.rs` / `app.rs` до split (см. §5).

---

## 4. Безопасность

### 4.1. Favicon / artwork

**Есть (хорошо):**

- scheme: только `http` / `https` (`favicon_url_allowed`);
- timeout, max 3 redirects;
- body ≤ 512 KiB (streaming cut-off);
- decode: PNG/JPEG/WebP, max dimension 512, max alloc 8 MiB;
- resize → 64×64; кэш LRU + in-flight limit.

**Нет (P0):**

```rust
// фактическая проверка
matches!(url.scheme(), "http" | "https")
// нет: DNS → IpAddr::is_global(), block 127/10/172.16/192.168/169.254/::1/fc00::/fe80::
// нет: re-validate IP на каждом redirect hop
```

**Также:**

- HTTP по умолчанию разрешён — для artwork лучше default HTTPS-only + opt-in http.
- MPRIS `artUrl`: убедиться, что наружу уходит локальный cache path или поле пустое (не сырой URL станции).

### 4.2. IPC

- Local single-user model: любой клиент с доступом к `$XDG_RUNTIME_DIR/orbitfm.sock` может `play`/`quit`/`volume`.
- Это приемлемо для desktop, но должно быть **явно** в threat model README/SECURITY.
- Проверить mode `0600` на socket и отсутствие world-writable path.

### 4.3. Прочее

| Тема | Статус |
|------|--------|
| `unsafe` | Только Win32 host window + `setlocale` — узко |
| TLS | reqwest + rustls |
| Supply chain | Рекомендуется `cargo audit` / `cargo deny` в CI |
| Packaging | MSI/DMG **не подписаны** коммерчески — честно описано в README (SmartScreen/Gatekeeper) |

---

## 5. Модульность и «монстр-файлы»

Порог: >600–800 LOC с несколькими ответственностями → split.

| Файл | LOC | Приоритет | Предлагаемый split |
|------|-----|-----------|-------------------|
| `src/ui/widgets.rs` | **1813** | P1 | `glass`, `slider`, `chips`, `buttons` |
| `src/render/motion.rs` | **1499** | P1 | `features`, `spring`, `styles`, `onset` |
| `src/app.rs` | **1380** | P1 | `app/{mod,frame,actions,favicon}.rs` (план 0.10 частично не дожат) |
| `src/ui/globe_view.rs` | **1343** | P1 | callout / hit / paint |
| `src/render/camera.rs` | 1216 | P2 | много tests — вынести `camera/tests.rs` |
| `src/ui/settings_panel.rs` | 890 | OK/watch | не растить без секций-файлов |
| `src/audio/worker.rs` | 923 | OK/watch | |

**Регресс:** `app.rs` и `widgets.rs` стали тяжелее с октября — фичи добавлялись «в тот же файл».

---

## 6. Производительность

- Глобус: wgpu callback — ок.
- Motion: springs, clamp dt — ок; стоимость растёт с markers + heatmap + orbit ring одновременно.
- Favicon: batch in-flight — ок.
- Анимации UI (ADR-0032): не должны держать permanent 60 fps; проверить idle CPU с `reduce_motion` и без playing.

**Рекомендация:** tracing spans на `paint_globe`, `MotionMixer::tick`, marker project; бюджет frame на целевом laptop iGPU.

---

## 7. Плагины и экосистема шеллов

### 7.1. Сейчас в репозитории

| Интеграция | Статус |
|------------|--------|
| Noctalia (`orbitfm/`) | **Есть** — thin `toggle`, translations |
| wm-configs niri/hyprland | **Есть** |
| Tray SNI | **Есть** (Linux) |
| MPRIS | **Есть** |
| KDE Plasmoid | План / нет кода |
| GNOME extension | План / нет кода |
| Omarchy | План / нет кода |
| Waybar / dwm | Нет contrib scripts |

### 7.2. Стратегия (рекомендация)

**Не** делать «полноценный UI OrbitFM» внутри каждого шелла.  
**Делать** thin clients:

```text
[Shell widget] --exec/IPC--> orbitfm (single instance) --libmpv--> audio
                \--MPRIS--> DE media controls
```

Приоритет внедрения:

1. **Waybar / universal script** — максимальный охват при минимуме кода (`status` + click `toggle`).
2. **Omarchy** — `manifest.json` + QML bar-widget (официальная plugin model).
3. **KDE Plasmoid** — QML + `ExecutableDataSource` / Process; плюс MPRIS закрывает now-playing.
4. **GNOME extension** — JS + `GLib.spawn_command_line_async('orbitfm toggle')`; MPRIS для media.
5. **Caelestia / Hyprland rice** — binds + optional QML module; не first-class store.
6. **dwmblocks / polybar / eww** — shell one-liners в `contrib/`.

Общий контракт уже готов (`panel`/`overlay`/`mode` расширяют UX виджетов).

---

## 8. Browser extensions (Chrome / Firefox) — мнение

### 8.1. Имеет ли смысл?

| Аргумент за | Аргумент против |
|-------------|-----------------|
| Пользователь «живёт» в браузере | OrbitFM — **desktop native** + 3D globe; расширение **не заменит** приложение |
| Быстрый toggle / now playing | Нужен мост к локальному `orbitfm` (Native Messaging) |
| Магазины расширений = дистрибуция | Review policy, поддержка двух браузеров, обновления |
| | Без native host расширение **не** управляет libmpv |

**Вывод:** расширение имеет смысл только как **пульт** к уже установленному OrbitFM (play/pause/station, badge), не как веб-плеер с глобусом.

### 8.2. Как делать правильно

1. **Native Messaging host** (маленький JSON stdin/stdout) → те же команды, что IPC (`status`, `toggle`, `play`).
2. MV3 service worker: popup с now-playing + кнопки.
3. Firefox + Chrome из **одного** общего JS + тонкие manifest отличия.
4. Не тащить Radio Browser API и audio в extension — дублирование и CORS/квоты.

### 8.3. Приоритет относительно шеллов

| Направление | Приоритет для OrbitFM |
|-------------|------------------------|
| Shell plugins (Plasma/GNOME/Omarchy/Waybar) | **Выше** — пользователь уже слушает с десктопа |
| Browser extension | **Средний / позже** — удобный remote control |
| Web-only player без native | **Низкий** — другой продукт, другая угрозная модель |

Сначала закройте desktop shell matrix на IPC; extension — следующий слой, когда native host стабилен.

---

## 9. Другие варианты расширения экосистемы

1. **MPRIS-first** — уже даёт виджеты DE «бесплатно»; довести metadata + art (local).
2. **`.desktop` Actions** — Play/Stop/Show в контекстном меню лаунчера.
3. **D-Bus API** (опционально, Linux) — помимо unix socket, для GNOME/KDE интеграций.
4. **Discord Rich Presence / playerctl docs** — нулевая цена, высокая видимость у части аудитории.
5. **Home Assistant / MQTT sensor** — ниша self-hosted; только если есть спрос.
6. **Android companion** — отдельный продукт; не смешивать с 0.12.
7. **Stream Deck / HTTP localhost control** — тонкий `orbitfm` subcommand или unix→http bridge с auth token (осторожно с security).

---

## 10. Packaging и платформы

| Платформа | Артефакт | Замечания |
|-----------|----------|-----------|
| Linux | AppImage, Flatpak yml | Rootless AppImage; Noctalia требует `orbitfm` в PATH |
| Windows | MSI | Unsigned — SmartScreen; SMTC host window |
| macOS | DMG + scripts | Ad-hoc sign; Gatekeeper; ADR-0026 runtime — проверять на реальном Mac |

ROADMAP должен явно разделять: **собирается в CI** vs **проверено runtime на железе**.

---

## 11. Приоритеты работ

### P0

1. SSRF-защита favicon (global unicast only, redirect recheck).
2. HTTPS-only default для artwork (http — setting).
3. Документировать IPC threat model (local user = full control).

### P1

1. Split `widgets.rs`, `motion.rs`, `app.rs`, `globe_view.rs`.
2. Waybar + Omarchy thin plugins в `contrib/` или `shells/`.
3. KDE plasmoid MVP (toggle + status).
4. `cargo audit` в CI.

### P2

1. GNOME extension MVP.
2. Noctalia: status text на виджете, не только toggle.
3. Native Messaging + browser extension MVP.
4. Idle CPU / frame budget regression test (ручной чеклист → скрипт).

### P3

1. D-Bus optional interface.
2. Desktop Actions.
3. Подпись Windows/macOS при появлении сертификатов.

---

## 12. Критерии «можно резать 0.12.0 stable»

- [ ] P0 SSRF влит и покрыт тестами (synthetic IPs / URL cases)
- [ ] `cargo fmt` / `clippy -D warnings` / `test` green
- [ ] Smoke: Linux niri/Noctalia hide/show/panel/overlay
- [ ] Smoke: Windows SMTC хотя бы на одной машине/VM
- [ ] Smoke: macOS tray/Now Playing на реальном Mac или явный «experimental»
- [ ] README: threat model одной секцией; shell matrix «supported / planned»

---

## 13. Итог

OrbitFM **0.12.0-rc.2** — серьёзный RC с сильной документацией и широким UI-функционалом. Главные угрозы качеству сейчас не «нет фич», а **безопасность artwork (SSRF)** и **энтропия размера ключевых UI/render файлов**. Экосистему шеллов правильно строить как **тонкие клиенты к IPC**, не как порты приложения. Browser extension — полезный пульт **после** desktop plugins, не вместо них.

---

*Отчёт подготовлен по дереву исходников commit `71e9bd8`. Не заменяет pen-test и не является аудитом supply-chain зависимостей с полным `cargo deny`.*
