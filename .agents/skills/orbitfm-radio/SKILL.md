---
name: orbitfm-radio
description: Клиент Radio Browser API для OrbitFM. Использовать при работе с discovery серверов, поиском станций, click-tracking и кэшем.
---

# OrbitFM Radio

Ты работаешь с клиентом Radio Browser.

Опирайся на ADR-0004, `docs/modules/02-radio-browser.md`, `docs/interfaces/02-radio.md` и официальный https://api.radio-browser.info/.

## Обязательные правила API

1. DNS-lookup `all.api.radio-browser.info` → список серверов.
2. Рандомизация + failover при 5xx / таймауте.
3. Каждый запрос:
   ```
   User-Agent: OrbitFM/1.0 (ModernByte; Linux; Rust)
   ```
4. Для play всегда `url_resolved`.
5. При старте воспроизведения — `send_click(stationuuid)`.
6. Не использовать устаревшие поля `id` и `country` (только `stationuuid`, `countrycode`).

## Контракт

Реализуй `RadioClient`:

- `search(SearchParams)`
- `get_station(uuid)`
- `send_click(uuid)`
- `refresh_servers()`

## Отказоустойчивость

- Таймаут запроса 5–8 с.
- При ошибке — следующий сервер из пула.
- После серии неудач — повторный discovery.
- Кэш последних успешных ответов допустим (in-memory / disk).

## Чего нельзя

- Хардкодить одно зеркало
- Пропускать User-Agent
- Игнорировать click-tracking
- Парсить устаревшие поля как основные
