---
name: orbitfm-listening-ux
description: Ритуалы слушания OrbitFM — sleep timer, контекстные next/previous и очередь, network profile (metered), локальная статистика, URL-алиасы станций. Desktop и паритет с Android companion.
---

# OrbitFM Listening UX

Ты добавляешь функции **повседневного слушания**, а не новый 3D-эффект и не
новый шелл. База sleep/очереди и переноса библиотеки реализована в
0.18.0-rc.1; быстрый доступ к таймеру и профиль сети — в 0.19.0-rc.1.
Локальная статистика и URL-алиасы ещё проектируются.

## Источники

1. `docs/audit/OrbitFM-future-features.md` — §2 (приоритет B)
2. `docs/adr/0046-ритуалы-слушания.md` — решения по sleep, очереди и схеме
3. `docs/adr/0048-профиль-сети.md` — профили `unlimited`/`metered`/`offline-first`
4. `docs/interfaces/01-audio.md`, `05-ipc.md`, `09-settings.md`, `10-session.md`
5. ADR-0003 (аудио), ADR-0043 (офлайн/адаптивное аудио), ADR-0025 (URL)
6. `docs/modules/11-session.md`, `docs/interfaces/13-events.md`

## Маршрутизация

- **Когда:** sleep timer, `next`/`previous` и края очереди, network profile,
  локальная статистика, URL-алиасы, паритет этих сценариев с Android.
- **Обязательные спутники:** `orbitfm-core` + слой реализации
  (`orbitfm-rust`, `orbitfm-gui`, `orbitfm-ipc`); `orbitfm-android` для
  мобильного паритета; `orbitfm-library-portability` при экспорте;
  `orbitfm-architect` + `orbitfm-docs` при смене контракта.
- **Не подменяет:** продуктовое планирование (`orbitfm-product`), аудио-ядро
  (`orbitfm-audio`) и интеграции шеллов (`orbitfm-integration`).
- **Правила:** `docs/AGENTS.md` §2.2; при конфликте скилла и принятого ADR
  действует ADR.

## Текущий статус

| Возможность | Статус |
|-------------|--------|
| Sleep timer, IPC, `status.sleep` | реализовано в 0.18.0-rc.1 |
| Быстрый sleep: шапки, панель, трей, browser popup, GNOME | реализовано в 0.19.0-rc.1 |
| `queue_wrap`, края очереди, `can`/MPRIS | реализовано в 0.18.0-rc.1 |
| Экспорт/импорт библиотеки | реализовано, см. `orbitfm-library-portability` |
| Network profile (desktop) | реализовано в 0.19.0-rc.1 (ADR-0048) |
| Network profile (Android metered flag) | не начато, вместе с A1–A2 |
| Локальная статистика | не начато |
| URL-алиасы | не начато |

## Sleep timer

- Единый ряд `settings::SLEEP_CHOICES = [0, 15, 30, 45, 60, 90]`, где `0` —
  выключен. UI показывает те же подписи через `i18n::sleep_option_label`.
- IPC принимает любое целое `1..=720`: `sleep <minutes>` заменяет дедлайн,
  `sleep clear` снимает; без активного таймера `clear` — no-op.
- По истечении — **stop**, не pause: эфир освобождает сеть и аудио-устройство.
  Fade-out в v1 не делается. Уведомление — `NoticeKey::SleepExpired`.
- Дедлайн живёт в `Session`, переживает закрытие окна, проверяется в `tick()`
  и в `park()`; выход процесса его не переживает.
- `status --json` схемы 1: аддитивное `sleep: { minutes, remaining_secs } | null`.
- Быстрый доступ: часы в шапках HUD/Overlay/Listening и панели, подменю трея
  (SNI `RadioGroup`, macOS `CheckMenuItem`), popup браузерного companion,
  подменю GNOME. Noctalia открывает панель, отдельная кнопка не нужна.
- Android: sleep timer — v1.5 must / v1 should, если успевает; действие в
  notification.

## Очередь и Next / Previous

Видимая очередь — та же, что показывает активная вкладка панели: избранное,
история или результаты поиска с действующим фильтром.

- `queue_wrap = true` (по умолчанию): `next` после последней станции идёт к
  первой, `previous` с первой — к последней.
- `queue_wrap = false`: шаг за край — no-op; кнопки, MPRIS
  `CanGoNext/CanGoPrevious` и поля `can` в `status --json` гаснут по позиции.
- Если текущей станции нет в видимой очереди, оба шага ведут к первой/последней.
- Семантика зафиксирована в интерфейсах 05/10; отдельных IPC-команд нет.
- Расширенные контексты из аудита (страна/жанр, «назад по history stack») —
  будущее развитие, не повод ломать текущую семантику.
- Roulette — отдельный сценарий; он не подменяет явный `next`.

## Network profile

| Профиль | Поведение |
|---------|-----------|
| Unlimited | полный каталог, снимок 1000 станций, 8 favicon-запросов |
| Metered | `bitrateMax=128` в discovery, снимок 250 станций, 2 favicon-запроса; эфир и библиотека без cap |
| Offline-first | snapshot first (ADR-0043), фоновый мир и новые favicon не стартуют, сеть по действию пользователя |

- Desktop реализован (ADR-0048): `settings.network_profile`, IPC `network`,
  `status --json.network`, карточка «Сеть» в категории «Аудио»; смена профиля
  сразу влияет на мир, discovery и `FaviconCache`.
- Отмена летящего мира при `offline-first` — `Session::cancel_world_fetch`
  (отбрасывание приёмника), без поколений.
- Android читает OS metered flag и ручной override вместе с ADR-0045.
- Не отключать SSRF/publishable checks и не ломать адаптивный битрейт ADR-0043
  «облегчённым» режимом.

## Локальная статистика

- Только on-device: минуты по станциям, странам и грубым тегам.
- Без сервера, скрытой телеметрии и аккаунта; export — только opt-in.
- Desktop-визуализация может позже переиспользовать heatmap API.

## URL-алиасы

- Локальный override stream URL для `stationuuid` с пометкой «ручная ссылка».
- Перед play — та же SSRF/publishable проверка, что для URL Radio Browser.
- Алиас не меняет экспортируемую станцию без явного решения о схеме.

## Порядок внедрения

1. Sleep timer — сделано.
2. Семантика next/previous и `queue_wrap` — сделано.
3. Экспорт/импорт библиотеки — сделано.
4. Network profile — desktop сделано в 0.19.0-rc.1; Android metered — вместе
   с A1–A2.
5. Локальная статистика.
6. URL-алиасы.

## Чего нельзя

- Считать stats облачным product analytics без продукта и consent.
- Ломать adaptive bitrate ADR-0043 противоречивыми «профилями».
- Добавлять next как «ещё один roulette» без контекста списка.
- Ослаблять SSRF/publishable policy в metered или offline-режиме.
- Менять схему `status`/IPC без ADR, интерфейса и CHANGELOG.
