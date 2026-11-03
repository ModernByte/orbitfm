# OrbitFM — идеи развития и новые возможности

| Поле | Значение |
|------|----------|
| **Дата** | 2026-11-03 |
| **База** | OrbitFM `0.17.0-rc.2` (коммит `704b77c`) |
| **Статус** | Продуктовые предложения; не ADR и не обязательство релиза |
| **Приоритет владельца** | **Android-приложение — в приоритете** |

Документ фиксирует направления после закрытия широкого desktop-контура (IPC, шеллы, browser companion, offline snapshot, i18n, motion, packaging). Цель — не размыть фокус «3D radio on the globe», а выстроить следующие ставки по ценности и стоимости.

---

## 0. Контекст: что уже есть

Краткий снимок, чтобы не предлагать дубли:

| Область | Состояние (0.17) |
|---------|------------------|
| Desktop Linux-first | eframe/egui/wgpu, libmpv, single-instance IPC |
| Окно | Focus / Overlay / Listening / Panel |
| Каталог | Radio Browser + stream import + SSRF |
| UX | Orbital Desk, heatmap, markers, favorites/history |
| Аудио-UI | cava-like visualizer, globe motion, orbit ring |
| Локализация | System + 10 языков |
| Интеграции | Noctalia, Waybar, Plasma6, GNOME, Omarchy, polybar, swaybar, i3*, browser (Chrome/Firefox native host) и др. |
| Устойчивость | offline stations snapshot, adaptive bitrate path |
| Packaging | AppImage, Flatpak, MSI, DMG |

**Вывод:** desktop-ядро насыщено. Рост — через **мобильный companion**, ритуалы слушания и гигиену кода; не через «ещё один DE-виджет» как главный трек.

---

## 1. Приоритет A — Android-приложение

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

### 1.7. Технический контур (предложение, не ADR)

| Слой | Вариант | Комментарий |
|------|---------|-------------|
| Язык UI | Kotlin + Jetpack Compose | Стандарт Android 2026 |
| Плеер | Media3 (ExoPlayer) | Нативные stream; HLS/icy по возможности |
| Фон | `MediaSessionService` + FGS | Требование Android 8+ / 14+ политики |
| Сеть | OkHttp / Ktor | RB API, DNS discovery по аналогии с desktop |
| Данные | Room / DataStore | favorites, history, settings |
| Карта (v1.5) | MapLibre Native или platform Map | Легче, чем порт wgpu-глобуса |
| DI | Рекомендуется один стиль (Hilt/Koin) | Не принципиально для v1 |
| Модули | `:app`, `:core:radio`, `:core:player`, `:feature:search` | Не монолит Activity |

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
| Монорепо vs отдельный | **Отдельный** `ModernByte/orbitfm-android` **или** каталог `android/` в monorepo после 0.18 desktop stable |
| Versioning | Независимое `1.0.0-alpha` приложения; в README desktop — ссылка «Companion» |
| CI | GitHub Actions: assembleDebug, lint, unit tests; release AAB отдельно |
| Минимум SDK | API 26+ (или 29+ для упрощения FGS) — решить в ADR Android |

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

### 2.1. Sleep timer

**Суть:** остановить воспроизведение через N минут (15 / 30 / 45 / 60 / custom).

**Desktop:** пункт в transport / settings; IPC `sleep <minutes>` / `sleep clear`; MPRIS не обязан дублировать.  
**Android:** must в v1.5, should в v1 если успевает.

**Детали:**

- Пауза vs stop — по умолчанию **stop** (освобождает сеть).
- Опция «уменьшать громкость последние 2 минуты» (fade) — nice-to-have.
- Уведомление «эфир остановлен по таймеру».

### 2.2. Очередь и осмысленный Next/Previous

**Суть:** не только roulette, а контекстная очередь.

| Контекст | Next |
|----------|------|
| Страна выбрана | следующая из текущего списка (sort-aware) |
| Жанр (global) | следующая из выдачи жанра |
| Избранное | следующая в favorites |
| История | previous = назад по history stack |

**IPC:** `next`, `previous` с гарантированной семантикой (документировать в interfaces).  
**UI:** не показывать next, если очередь из одного элемента без wrap-режима (настройка wrap on/off).

### 2.3. Экспорт / импорт библиотеки

**Суть:** JSON (и опционально простой OPML-like список URL) для избранного и истории.

- Схема versioned (`schema: 1`), совместима с Android import.
- Desktop: Settings → About/Data → Export / Import.
- Безопасность: import только http(s) URL через существующий SSRF classifier перед play.

### 2.4. Профиль сети (Network profile)

| Профиль | Поведение |
|---------|-----------|
| Unlimited | как сейчас |
| Metered / Mobile | max bitrate cap, меньше prefetch favicon, реже world refresh |
| Offline-first |优先 snapshot (ADR-0043), сеть только по действию пользователя |

На Android — чтение `ConnectivityManager` / OS metered flag + ручной override.

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

**Общие артефакты (документировать в `docs/interfaces/`):**

1. Schema `library` export/import v1.  
2. Правила publishable URL (ссылка на ADR-0025 / ssrf policy).  
3. Семантика metadata станции (uuid, name, url, countrycode, tags, bitrate).  
4. Deep link format (v2): `orbitfm://station/<uuid>` на desktop; `https://` app links на Android — отдельное решение.

---

## 7. Предлагаемая дорожная карта (сводная)

### Волна 1 — Android foundation + desktop enablers

1. ADR: Android Companion (scope v1 / non-goals).  
2. Desktop: **export/import library** (разблокирует перенос избранного).  
3. Android A1–A2: player + search + favorites.  
4. Desktop: **sleep timer** + IPC (паритет с будущим Android).

### Волна 2 — Companion usable daily

1. Android MediaSession polish, BT, notification actions.  
2. Sleep timer на Android.  
3. Queue/next-prev semantics на desktop (и упрощённо на Android).  
4. Network profile (особенно Android metered).

### Волна 3 — delight & scale

1. Android map/country mode (v1.5).  
2. Local listening stats (desktop → optional Android).  
3. Split monster files на desktop.  
4. Web Explorer как отдельный репозиторий (если нужен рост аудитории).

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
| Фокус продукта | README по-прежнему описывает «3D desktop radio» + «Android companion», не «всё для всех» |

---

## 9. Открытые решения владельца

1. **Репозиторий Android:** monorepo `android/` vs `ModernByte/orbitfm-android`?  
2. **Минимальные языки Android v1:** только en+ru или полный паритет 10?  
3. **Карта в v1 или строго v1.5?**  
4. **Google Play в первой публикации** или сначала GitHub APK / F-Droid?  
5. **Связка library:** только file export или сразу design под Syncthing path?

---

## 10. История документа

| Дата | Изменение |
|------|-----------|
| 2026-11-03 | Первая версия: идеи после 0.17.0-rc.2; Android Companion как приоритет A |

---

*Документ не заменяет ADR. Любая реализация Android или sleep/queue/export начинается с короткого ADR и обновления ROADMAP/VERSIONMAP.*
