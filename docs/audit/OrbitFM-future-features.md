# OrbitFM — идеи развития и новые возможности

| Поле | Значение |
|------|----------|
| **Дата** | 2026-11-03 |
| **База** | OrbitFM `0.17.0-rc.3`; enabler-пункты §2 реализованы в `0.18.0-rc.1`–`0.19.0-rc.1` |
| **Статус** | Продуктовые предложения; не ADR и не обязательство релиза |
| **Приоритет владельца** | **Android-приложение — в приоритете** |

Документ фиксирует направления после закрытия широкого desktop-контура (IPC, шеллы, browser companion, offline snapshot, i18n, motion, packaging). Цель — не размыть фокус «3D radio on the globe», а выстроить следующие ставки по ценности и стоимости.

---

## 0. Контекст: что уже есть

Краткий снимок, чтобы не предлагать дубли:

| Область | Состояние (0.19.0-rc.1) |
|---------|--------------------------|
| Desktop Linux-first | eframe/egui/wgpu, libmpv, single-instance IPC |
| Окно | Focus / Overlay / Listening / Panel |
| Каталог | Radio Browser + stream import + SSRF |
| UX | Orbital Desk, heatmap, markers, favorites/history |
| Аудио-UI | cava-like visualizer, globe motion, orbit ring |
| Локализация | System + 16 языков |
| Интеграции | Noctalia, Waybar, Plasma6, GNOME, Omarchy, polybar, swaybar, i3*, browser (Chrome/Firefox native host) и др. |
| Устойчивость | offline stations snapshot, adaptive bitrate path |
| Ритуалы | sleep timer, `queue_wrap`, экспорт/импорт библиотеки |
| Сеть | профиль `unlimited`/`metered`/`offline-first` (ADR-0048) |
| Packaging | AppImage, Flatpak, MSI, DMG |

**Вывод:** desktop-ядро насыщено. Рост — через **мобильный companion**, ритуалы слушания и гигиену кода; не через «ещё один DE-виджет» как главный трек.

---

## 1. Приоритет A — Android-приложение (ADR-0045 принят, кода нет)

### 1.1. Зачем

- Слушание радио уходит в карман: дорога, работа, без ноутбука.
- Desktop OrbitFM остаётся «центром» (глобус, библиотека, сложные фильтры); Android — **удобный клиент эфира**.
- Конкуренты (простые radio apps) не дают связки «та же библиотека / тот же вкус станций», если сделать аккуратный companion.

### 1.2. Продуктовая роль (важно зафиксировать)

| Вариант | Описание | Рекомендация |
|---------|----------|--------------|
| **A. Companion** | Управление и воспроизведение; глобус упрощён или список+карта | **Выбрать для v1** |
| **B. Полный порт** | 1:1 desktop с 3D wgpu | Дорого, слабо на середине Android; отложить |
| **C. Только remote** | Пульт к домашнему desktop по сети | Узкий сценарий; можно later |

**Решение для v1:** Android = **Companion player + library sync light**, не клон 3D-десктопа.

### 1.3. Сценарии v1 (must)

1. Пользователь открывает приложение → видит **сейчас играет** / пусто.
2. Поиск станции по имени / стране / жанру (Radio Browser API с тем же User-Agent policy, что desktop).
3. Воспроизведение в фоне (**Foreground Service** + MediaStyle notification).
4. **MediaSession** — кнопки на шторке, Bluetooth, Android Auto (базовый уровень).
5. Избранное локально; опционально **импорт JSON** из desktop (`library` export — см. §3).
6. История последних станций.
7. «Случайная станция» (roulette light).
8. Настройки: язык (система), качество (макс. битрейт), theme (system/dark/light).

### 1.4. Сценарии v1.5 (should)

1. Упрощённая **карта** (не полный wgpu-глобус): MapLibre / ОС-карта + маркеры топ-станций или стран.
2. Страна → список станций (как режим A на desktop).
3. Жанровые чипы.
4. Sleep timer (см. §3) — на мобиле критичен.
5. Карточка станции: bitrate, codec, country, favicon (с тем же SSRF-классом политики на клиенте или через безопасный decode path).

### 1.5. Сценарии v2+ (could)

1. QR / deep link «открыть станцию» с desktop.
2. Опциональная синхронизация избранного через **файл пользователя** (Syncthing, папка), не свой облачный бэкенд.
3. Wear OS controls (optional).
4. Car-optimized UI (Android Auto full projection), если v1 MediaSession стабилен.

### 1.6. Что сознательно не делать в Android v1

- Полный 3D-глобус на GLES/Vulkan «как в desktop».
- Собственный аккаунт OrbitFM и сервер синхронизации.
- Дублирование всех WindowMode desktop.
- Встраивание всех bar-plugin концепций.

### 1.7. Технический контур (зафиксирован в ADR-0045)

| Слой | Решение ADR-0045 | Комментарий |
|------|------------------|-------------|
| Язык UI | Kotlin + Jetpack Compose | Стандарт Android 2026 |
| Плеер | Media3 (ExoPlayer) | Нативные stream, ICY через `IcyHeaders`/`IcyInfo` |
| Фон | `MediaSessionService` + FGS `mediaPlayback` | Требование Android 8+ / 14+ политики |
| Сеть | OkHttp / Ktor | RB API, DNS discovery по аналогии с desktop |
| Данные | Room / DataStore | favorites, history, settings |
| Карта (v1.5) | MapLibre Native или platform Map | Легче, чем порт wgpu-глобуса |
| DI | Один стиль (Hilt/Koin) | Не принципиально для v1 |
| Модули | `:app`, `:core:radio`, `:core:player`, `:feature:search` | Не монолит Activity |
| SDK | `minSdk 26`, `targetSdk 36` | Зафиксировано в ADR-0045 |

**Связь с desktop-кодом:**

- Не ожидать прямого шаринга Rust UI.
- Можно вынести **общие правила** в документацию: SSRF URL policy, schema library JSON, IPC не нужен на телефоне.
- Опционально позже: shared Rust core через `uniffi`/`jni` только для catalog client — **не блокер v1**.

### 1.8. Безопасность и политика магазинов

- HTTPS для API и по возможности для streams; cleartext — только явный fallback и с осторожностью (Android Network Security Config).
- Те же классы запрета, что ADR-0025: не грузить `file://`, не ходить на link-local из пользовательских URL без политики.
- Privacy: нет трекинга; Radio Browser click-tracking — как на desktop, с документированием.
- Foreground service type `mediaPlayback`; корректные уведомления — иначе отказ в Google Play.
- Лицензии: MIT desktop ≠ автоматический выбор лицензии для store listing; указать в отдельном `android/` NOTICE.

### 1.9. Репозиторий и версионирование

| Вопрос | Рекомендация |
|--------|--------------|
| Монорепо vs отдельный | **Отдельный** `ModernByte/orbitfm-android` **или** каталог `android/` в monorepo — открытое решение владельца в ADR-0045 |
| Versioning | Независимое `1.0.0-alpha` приложения; в README desktop — ссылка «Companion» |
| CI | GitHub Actions: assembleDebug, lint, unit tests; release AAB отдельно |
| Минимум SDK | `minSdk 26`, `targetSdk 36` — зафиксировано в ADR-0045 |

### 1.10. Критерии приёмки Android v1

- [ ] Play/pause/stop из UI и из notification
- [ ] Воспроизведение не убивается при сворачивании (FGS)
- [ ] Поиск RB возвращает результаты на хорошей сети; понятная ошибка на плохой
- [ ] Избранное переживает перезапуск
- [ ] MediaSession: headset / BT play-pause
- [ ] Нет краша на пустом поиске и на мёртвом URL (error state)
- [ ] Базовый TalkBack на главных кнопках
- [ ] Политика конфиденциальности (короткая) для store

### 1.11. Оценка порядка работ (ориентир)

| Этап | Содержание | Ориентир |
|------|------------|----------|
| A0 | ADR-Android, UX wireframes (список/плеер/поиск), package id | 3–5 дней |
| A1 | Player + MediaSession + notification | 1–2 недели |
| A2 | RB search + station screen + favorites/history | 1–2 недели |
| A3 | Polish, i18n subset (en+ru minimum), store listing draft | 1 неделя |
| A4 | v1.5 map/country mode | отдельно |

Параллельно desktop не обязан замирать: export library JSON с desktop ускоряет companion.

---

## 2. Приоритет B — ритуалы слушания (desktop + общие)

Фичи с высокой ежедневной ценностью при умеренной стоимости. На Android большинство из них тоже нужны.

### 2.1. Sleep timer — реализовано в 0.18.0-rc.1 (ADR-0046)

**Суть:** остановить воспроизведение через N минут (15 / 30 / 45 / 60 / custom).

**Desktop:** пункт в настройках («Аудио», 15/30/45/60/90 и «выключен»), IPC
`sleep <minutes>` / `sleep clear`, поле `sleep` в `status --json`; срабатывает
и при закрытом окне.  
**Android:** must в v1.5, should в v1 если успевает.

**Детали:**

- Пауза vs stop — по умолчанию **stop** (освобождает сеть).
- Опция «уменьшать громкость последние 2 минуты» (fade) — nice-to-have.
- Уведомление «эфир остановлен по таймеру».

### 2.2. Очередь и осмысленный Next/Previous — база в 0.18.0-rc.1 (ADR-0046)

**Суть:** не только roulette, а контекстная очередь. Семантика видимой очереди,
краёв и `queue_wrap` уже зафиксирована; остаются расширенные контексты
(например, «назад по history stack»).

| Контекст | Next |
|----------|------|
| Страна выбрана | следующая из текущего списка (sort-aware) |
| Жанр (global) | следующая из выдачи жанра |
| Избранное | следующая в favorites |
| История | previous = назад по history stack |

**IPC:** `next`, `previous` с гарантированной семантикой — реализовано в
`docs/interfaces/05-ipc.md` и `docs/interfaces/10-session.md` (0.18.0-rc.1).  
**UI:** кнопки, MPRIS `CanGoNext/CanGoPrevious` и `can` в статусе гаснут по
позиции при `queue_wrap = false` (ADR-0046).

### 2.3. Экспорт / импорт библиотеки — реализовано в 0.18.0-rc.1 (ADR-0046)

**Суть:** JSON (и опционально простой OPML-like список URL) для избранного и истории.

- Схема versioned (`schema: 1`), совместима с Android import — контракт
  `docs/interfaces/15-library-export.md`.
- Desktop: IPC/CLI `library-export`/`library-import`; GUI-кнопок и file picker
  в 0.18.0-rc.1 нет.
- Безопасность: импорт принимает только публичные http(s) URL (та же политика
  `publishable`, ADR-0025), лимиты 2 МиБ / 5000 станций, санитизация строк;
  на Unix файл пишется с правами `0600`.

### 2.4. Профиль сети (Network profile) — desktop в 0.19.0-rc.1 (ADR-0048)

| Профиль | Поведение |
|---------|-----------|
| Unlimited | как раньше: полный каталог, снимок 1000 станций, 8 favicon-запросов |
| Metered | `bitrateMax=128` в discovery, снимок 250 станций, 2 favicon-запроса; эфир и библиотека без ограничения |
| Offline-first | snapshot first (ADR-0043), фоновый мир и новые favicon не стартуют, сеть — по действию пользователя |

Desktop реализован: `network_profile` в `settings.json`, IPC `network`,
`status --json.network`, карточка «Сеть» в настройках. Android metered flag
(`ConnectivityManager` + ручной override) остаётся за треком ADR-0045 и
подключается вместе с A1–A2.

### 2.5. Локальная статистика прослушивания

**Суть:** privacy-first, только на устройстве.

- Минуты по станциям / странам / жанрам (грубые теги).
- «Карта года» — heatmap из локальных счётчиков (desktop globe).
- Без аккаунта, без отправки на сервер.
- Export opt-in.

### 2.6. Алиасы URL станций

Ручная подмена stream URL, если каталог мёртв, с пометкой «userчная ссылка» в UI.  
Хранить в library local overrides; play path тот же SSRF.

---

## 3. Приоритет C — desktop polish и техдолг

Без этого скорость любых фич (включая сопровождение Android docs/schemas) падает.

### 3.1. Распил монстр-файлов

| Файл (порядок ~0.17) | Направление split |
|----------------------|-------------------|
| `src/ui/widgets.rs` (~1.8k) | glass / slider / chips / buttons |
| `src/integration/mod.rs` (~1.8k) | status formatters / wm / reload по файлам |
| `src/render/motion.rs` (~1.5k) | features / spring / styles / onset |
| `src/app.rs` (~1.5k) | frame / actions / favicon glue |
| `src/ui/globe_view.rs` (~1.3k) | callout / hit / paint |

**Правило:** не добавлять крупный функционал в файлы >800 LOC без split (зафиксировать в CONTRIBUTING, если ещё мягко).

### 3.2. Критерии производительности

- Idle CPU с reduce_motion и без playing — зафиксировать baseline.
- Длинная сессия (memory footprint) — раз в релиз-кандидат.
- Не блокирует Android, но укрепляет бренд «лёгкий native».

### 3.3. Desktop UX-хвосты

- Станции **без координат** — fallback-маркер на центроид страны + список.
- Явный UI adaptive bitrate / offline snapshot (не только в логах).
- Coach marks после first-run wizard (одноразовые подсказки: орбита ПКМ, жанр vs страна).
- Share-карточка «сейчас играет» (PNG) — optional.

### 3.4. Интеграции шеллов — режим сопровождения

Уже закрыт широкий набор. Дальше:

- не плодить новые шеллы без запроса;
- углублять **существующие**: status text, scroll-volume там, где API стабилен;
- Noctalia: richer status, если ещё thin toggle.

---

## 4. Приоритет D — экосистема и «без версии в репо»

Из ROADMAP и обсуждений; не смешивать с Android v1.

| Идея | Суть | Когда |
|------|------|--------|
| OrbitFM Web Explorer | Отдельный репо, Pages, read-only каталог+глобус web | Маркетинг / SEO |
| Telegram-бот | Remote play к домашнему instance или только links | После стабильного remote API |
| Discord Rich Presence | Now playing в профиле | Дешёвый nice-to-have desktop |
| D-Bus интерфейс | Дополнение к unix socket для GNOME/KDE | Linux polish |
| `.desktop` Actions | Quick actions в лаунчере | Дешёвый win |
| UPnP/DLNA | Выход на колонки | Высокая стоимость, узкая ниша |
| Integration SDK | Публичный пакет для виджет-авторов | После freeze IPC schema |
| Browser extension | Уже есть companion path — доводить UX, не дублировать player | Сопровождение |
| TUI на ratatui | Отдельный `orbitfm-tui` — IPC-клиент к ядру: состояние, поиск, библиотека, визуализаторы, btop-подобные модули | Параллельный трек 10–20%, после T0 |

### 4.1. TUI на ratatui — спроектирован, код не начат

- Решение: ADR-0047; контракт — `docs/interfaces/16-tui.md`; модуль —
  `docs/modules/19-tui.md`; продуктовый аудит —
  `docs/audit/OrbitFM-tui-ratatui.md`.
- TUI — **клиент** к single-instance core, а не второй плеер: без libmpv,
  MPRIS, Radio Browser и чтения `library.json`.
- v1: Now Playing, Search, Favorites, History, Help; визуализаторы
  `bars`/`ring`/`scope`/`pulse`; пресеты `desk`/`focus`/`library`/`minimal`;
  менеджер модулей и `tui.toml`; `stars`/`heat` — T6.
- Reactive-визуализаторы ждут аддитивного `events --spectrum` (после
  0.18.0-rc.1); до этого — procedural с явной маркировкой.
- Чистый SSH требует отдельного headless-ядра; TUI v1 работает при уже
  запущенном desktop-ядре.
- Поставка — `cargo install --path crates/orbitfm-tui`; пятый релизный
  артефакт без решения владельца не добавляется.
- Приоритет: не впереди Android v1; хороший parallel track или работа между
  волнами.

---

## 5. Приоритет E — сознательно не делать (сейчас)

1. Смена GUI-стека desktop (egui → X) ради «красоты» — UI-kit внутри egui дешевле.
2. Собственный облачный аккаунт и сервер синхронизации до доказанного спроса.
3. Полноценный подкаст-клиент / music library file player — другой продукт.
4. Полный порт 3D-глобуса на Android в v1.
5. Ещё 10 bar-плагинов «для галочки».

---

## 6. Связность Android ↔ Desktop

```
┌─────────────────────┐         export JSON          ┌─────────────────────┐
│  Desktop OrbitFM    │ ───────────────────────────► │  Android Companion  │
│  globe, power UI   │                               │  player, search     │
│  library.json       │ ◄─── optional re-import ──── │  favorites          │
└──────────┬──────────┘                               └──────────┬──────────┘
           │                                                     │
           │              Radio Browser / streams                │
           └───────────────────────┬─────────────────────────────┘
                                   ▼
                            public HTTPS streams
```

**Общие артефакты:**

1. Schema `library` export/import v1 — `docs/interfaces/15-library-export.md`
   (реализовано в 0.18.0-rc.1).  
2. Правила publishable URL — ADR-0025 и `crate::services::publishable`.  
3. Семантика metadata станции (uuid, name, url, countrycode, tags, bitrate) —
   ADR-0045 § 3.  
4. Deep link format (v2): `orbitfm://station/<uuid>` на desktop; `https://`
   app links на Android — отдельное решение, в ADR-0045 не входит.

---

## 7. Предлагаемая дорожная карта (сводная)

### Волна 1 — Android foundation + desktop enablers

1. [x] ADR: Android Companion (ADR-0045, scope v1 / non-goals).  
2. [x] Desktop: **export/import library** (0.18.0-rc.1, интерфейс 15).  
3. [ ] Android A1–A2: player + search + favorites.  
4. [x] Desktop: **sleep timer** + IPC (0.18.0-rc.1, ADR-0046).

### Волна 2 — Companion usable daily

1. Android MediaSession polish, BT, notification actions.  
2. Sleep timer на Android.  
3. Queue/next-prev semantics на desktop (и упрощённо на Android).  
4. [x] Desktop: **Network profile** (0.19.0-rc.1, ADR-0048).  
5. [ ] Android: системный metered flag и ручной override (вместе с A1–A2).

### Волна 3 — delight & scale

1. Android map/country mode (v1.5).  
2. Local listening stats (desktop → optional Android).  
3. Split monster files на desktop.  
4. Web Explorer как отдельный репозиторий (если нужен рост аудитории).
5. TUI ratatui T1–T5 параллельным треком (визуализаторы, модули,
   `tui.toml`); не блокирует Android v1.

### Волна 4 — optional ecosystem

Discord RPC, D-Bus, desktop Actions, Telegram bot — по одному, по спросу.

---

## 8. Метрики успеха

| Направление | Индикатор |
|-------------|-----------|
| Android v1 | Стабильный фоновый playback; <1% crash-free не ниже целевого; избранное не теряется |
| Sleep / queue | Использование хотя бы 1 раз за сессию у активных пользователей (если появится opt-in analytics — иначе качественные опросы) |
| Library export | Успешный перенос desktop → Android у бета-тестеров |
| Техдолг | Ни одного нового файла >1000 LOC без review-исключения |
| TUI | Установка одной командой; управление эфиром и библиотекой; визуализаторы и компоновки не мешают Android-треку |
| Фокус продукта | README по-прежнему описывает «3D desktop radio» + «Android companion», не «всё для всех» |

---

## 9. Открытые решения владельца

1. **Репозиторий Android:** monorepo `android/` vs `ModernByte/orbitfm-android`?  
2. **Минимальные языки Android v1:** только en+ru или полный паритет 10?  
3. **Карта в v1 или строго v1.5?**  
4. **Google Play в первой публикации** или сначала GitHub APK / F-Droid?  
5. **Связка library:** только file export или сразу design под Syncthing path?
6. **TUI:** остаётся source-only (`cargo install`) или получает отдельный
   артефакт? Когда начинается headless-ядро для чистого SSH?
7. **Spectrum:** принимать ли `events --spectrum` в core и на какой версии?

---

## 10. История документа

| Дата | Изменение |
|------|-----------|
| 2026-11-03 | Первая версия: идеи после 0.17.0-rc.2; Android Companion как приоритет A |
| 2026-11-03 | Волна 1: Android Companion зафиксирован ADR-0045; sleep timer, `queue_wrap` и экспорт/импорт библиотеки реализованы в 0.18.0-rc.1 (ADR-0046, интерфейс 15); отмечены статусы §2 и артефакты §6 |
| 2026-11-03 | Добавлен TUI-трек: ADR-0047, интерфейс 16, модуль 19; визуализаторы, btop-подобные модули, `spectrum`-расширение; поставка source-only |
| 2026-11-03 | Волна 2: профиль сети реализован на desktop в 0.19.0-rc.1 (ADR-0048); Android metered — за ADR-0045 |

---

*Документ не заменяет ADR. Любая реализация Android или sleep/queue/export начинается с короткого ADR и обновления ROADMAP/VERSIONMAP.*
