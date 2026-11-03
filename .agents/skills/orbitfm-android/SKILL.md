---
name: orbitfm-android
description: Android Companion для OrbitFM — Kotlin/Compose, Media3, поиск Radio Browser, избранное, фоновое воспроизведение. Использовать при проектировании, кодировании, ADR и ревью мобильного клиента. Не порт 3D-глобуса.
---

# OrbitFM Android Companion

Ты работаешь над **мобильным companion** к desktop OrbitFM (ModernByte), не над
полным портом Orbital Desk. Android-кода в этом репозитории нет: рамки
зафиксированы ADR-0045, реализация начнётся после решения владельца о
репозитории и стартовом scope.

## Источники

1. `docs/audit/OrbitFM-future-features.md` — §1 (приоритет A)
2. `docs/adr/0045-android-companion.md` — границы v1, стек, безопасность
3. `docs/adr/0046-ритуалы-слушания.md` — sleep, очередь, схема библиотеки
4. `docs/interfaces/15-library-export.md` — единственный контракт импорта
5. ADR-0004 (Radio Browser), ADR-0025 (публичные URL и favicon)
6. `orbitfm-core` — именование ModernByte и общие запреты

## Маршрутизация

- **Когда:** Android-клиент, Kotlin/Compose, Media3/ExoPlayer, FGS и
  MediaSession, мобильный импорт библиотеки, паритет сценариев слушания.
- **Обязательные спутники:** `orbitfm-core`; `orbitfm-product` для scope;
  `orbitfm-library-portability` для импорта; `orbitfm-listening-ux` для sleep
  и очереди; `orbitfm-architect` + `orbitfm-docs` при смене контракта.
- **Не подменяет:** desktop Rust-разработку, TUI и общее продуктовое
  планирование. Не применяй `orbitfm-rust` к Kotlin-коду.
- **Правила:** `docs/AGENTS.md` §2.2; при конфликте скилла и принятого ADR
  действует ADR.

## Продуктовая роль

| Делать в v1 | Не делать в v1 |
|-------------|----------------|
| Фоновый radio player | Полный 3D wgpu-глобус |
| Поиск RB + favorites/history | Все `WindowMode` desktop |
| MediaSession + notification | Собственный облачный аккаунт |
| Импорт библиотеки схемы 1 | Второй «сервер OrbitFM» |
| Roulette light, настройки | Bar-плагины и интеграции шеллов |

Sleep timer по аудиту — must для v1.5 и should для v1, если успевает в
стартовый scope. Не обещать его в store listing до реализации.

**Модель:** телефон — удобный эфир; desktop — глобус, мощность, интеграции.

## Стек (ADR-0045)

| Слой | Решение |
|------|---------|
| UI | Kotlin, Jetpack Compose |
| Player | Media3 (ExoPlayer), ICY через `IcyHeaders`/`IcyInfo` |
| Фон | `MediaSessionService` + FGS `mediaPlayback` |
| Сеть | OkHttp или Ktor; DNS discovery Radio Browser |
| Локальные данные | Room (favorites/history), DataStore (settings) |
| DI | Hilt или Koin — один стиль на проект |
| Модули | `:app`, `:core:radio`, `:core:player`, `:feature:search` |
| SDK | `minSdk 26`, `targetSdk 36` |

Не тянуть egui/Rust UI на Android в v1. Общий Rust core через `uniffi`/JNI —
только отдельным ADR и не блокер MVP.

## Общие контракты

- **Библиотека:** только `docs/interfaces/15-library-export.md`, схема 1.
  Импорт — слияние по `stationuuid`, локальная запись побеждает; проверки URL,
  лимиты 2 МиБ/5000 станций, санитизация строк и `skipped` обязательны.
  Внутренний `library.json` desktop на Android не читается.
- **Radio Browser:** User-Agent `OrbitFM/1.0 (ModernByte; Android; Kotlin)`;
  DNS discovery, без хардкода одного сервера; click-tracking документирован и
  отключаем.
- **Desktop IPC:** Android им не управляет. Связь с desktop — только файл
  библиотеки.
- **Метаданные станции:** `stationuuid`, `name`, `url`, `url_resolved`,
  `countrycode`, `tags`, `bitrate`, `codec`, `source`; пустые поля не
  выдумываются.

## Обязательные сценарии v1

1. Play / pause / stop из UI и из notification.
2. Воспроизведение не умирает при сворачивании (FGS).
3. Поиск станций по имени / стране / тегам через Radio Browser.
4. Избранное и история переживают процесс.
5. MediaSession: BT / headset play-pause, базовый Android Auto.
6. Понятные ошибки сети и мёртвого URL.
7. Базовый TalkBack на главных контролах.
8. Импорт библиотеки из desktop с отчётом о пропущенных станциях.

## Безопасность

- `https` по умолчанию; cleartext только через явный
  `network_security_config` и с тем же классом запретов, что ADR-0025.
- Никаких `file://`, link-local и приватных literal-адресов из
  пользовательских URL; favicon/artwork — по той же политике.
- Нет аккаунта, аналитики и отправки библиотеки на сервер.
- FGS объявляет `android:foregroundServiceType="mediaPlayback"` и
  `FOREGROUND_SERVICE_MEDIA_PLAYBACK`; уведомление соответствует Play.
- Privacy policy обязательна до публикации в магазине.

## Репозиторий и версии

- `ModernByte/orbitfm-android` или каталог `android/` в monorepo — **открытое
  решение владельца** (ADR-0045). Не начинать код до его фиксации.
- Application id `org.modernbyte.orbitfm` или `.companion` — зафиксировать в
  ADR до первого артефакта.
- Версии Android независимы от semver desktop.
- Открытые решения владельца: языки v1, карта, магазин, стратегия синхронизации.

## Порядок работы

1. Прочитай audit §1, ADR-0045 и non-goals.
2. Если меняется scope — ADR-Android или обновление ADR-0045, не «тихо в коде».
3. Сначала player + MediaSession, затем search/favorites/import.
4. Карта и страна — после стабильного playback.
5. Не обещай паритет с desktop UI в store listing.

## Чего нельзя

- Тащить полный visual-spec глобуса в Compose «для галочки».
- Хранить токены/пароли «на будущее».
- Обходить FGS/notification требования Android 14+.
- Ослаблять URL-политику ради «удобного» импорта или cleartext-потоков.
- Ломать MIT/лицензии зависимостей без NOTICE.
