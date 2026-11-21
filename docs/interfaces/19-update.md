# Интерфейс: обновления (`update`)

**Статус:** принят (ADR-0067, 2026-11-21); UPD-1 и CLI-проверка реализованы  
**Клиент:** CLI `orbitfm update`, IPC (позже), `status --json`  
**Источник:** GitHub Releases `ModernByte/orbitfm`

## 1. Команды CLI

```text
orbitfm update [--check] [--json] [--channel <stable|dev>]
orbitfm update channel <stable|dev>   # UPD-3
orbitfm update install                # UPD-6
```

- cold-start: не требует запущенного экземпляра и не поднимает GUI;
- `--channel` — разовый override; сохранение канала — `update channel`;
- неизвестные аргументы и невалидный канал — понятная ошибка, код ≠ 0.

## 2. `update --check --json` (схема 1)

```json
{"schema":1,
 "channel":"dev","current":"0.23.0-rc.3","latest":"0.24.0-rc.1",
 "available":true,"checked_at":"2026-11-21T10:00:00Z",
 "release_url":"https://github.com/ModernByte/orbitfm/releases/tag/v0.24.0-rc.1",
 "published_at":"2026-11-20T09:00:00Z",
 "notes":"…",
 "checksums":true,
 "asset":{"name":"OrbitFM-0.24.0-rc.1-x86_64.AppImage","size":108448248,
          "url":"https://github.com/…","sha256":null}}
```

`schema` не растёт: только добавление; `asset` и `sha256` могут быть `null`.

## 3. `status --json` (аддитивно, сделано)

Объект `update: {channel, current, latest, available}` добавляется в каждый
кадр из последнего результата планировщика; сетевого запроса в обработчике
`status` нет. Поле появляется только когда `Session` его выставила
(`skip_serializing_if`), поэтому старые потребители не меняются.
`status.schema` остаётся `1`.

## 4. Ошибки

Сеть, `HTTP != 2xx`, невалидный JSON, недопустимый хост — не фатальны для
эфира: сообщение в CLI/`Notice`. Автоустановка без `SHA256SUMS` отменяется.
