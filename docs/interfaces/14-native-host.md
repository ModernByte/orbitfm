# Интерфейс: native messaging host (`orbitfm native-host`)

**Версия протокола:** 1 (конверт `{"ok":…}`)  
**Статус:** принят (ADR-0038, 2026-10-31)  
**Транспорт:** stdio, запускается браузером по native-манифесту  
**Имя хоста:** `org.modernbyte.orbitfm`

## 1. Назначение

`orbitfm native-host` — служебный процесс для расширений Chromium и Firefox.
Браузер запускает его при открытии popup и общается с ним по stdin/stdout.
Хост не поднимает GUI, не открывает сетевых портов и не становится демоном:
команды уходят в уже запущенный OrbitFM по тому же IPC-сокету, что у CLI
(ADR-0005), состояние приходит подпиской `events` (ADR-0039).

```
popup ──порт── браузер ──stdio── orbitfm native-host ──IPC-сокет── OrbitFM
```

Если OrbitFM не запущен, любая команда получает `not-running`. Холодный старт
через браузер запрещён: даже `toggle` и `settings` не создают новый экземпляр.

## 2. Регистрация хоста

Установщик ADR-0037 (`orbitfm integration install chrome|firefox`) пишет
native-манифест с именем `org.modernbyte.orbitfm.json`.

### 2.1. Chromium

Каталоги (только обнаруженные браузеры):

| Браузер | Каталог |
|---------|---------|
| Chrome | `$XDG_CONFIG_HOME/google-chrome/NativeMessagingHosts` |
| Chromium | `$XDG_CONFIG_HOME/chromium/NativeMessagingHosts` |
| Brave | `$XDG_CONFIG_HOME/BraveSoftware/Brave-Browser/NativeMessagingHosts` |
| Edge | `$XDG_CONFIG_HOME/microsoft-edge/NativeMessagingHosts` |
| Vivaldi | `$XDG_CONFIG_HOME/vivaldi/NativeMessagingHosts` |

```json
{
  "name": "org.modernbyte.orbitfm",
  "description": "OrbitFM — 3D internet radio",
  "path": "/home/user/.local/share/orbitfm/integrations/chrome/orbitfm-native-host",
  "type": "stdio",
  "allowed_origins": ["chrome-extension://lfhmfgjblkebbalfdlbmnalghhjplfbg/"]
}
```

### 2.2. Firefox

`~/.mozilla/native-messaging-hosts/org.modernbyte.orbitfm.json`:

```json
{
  "name": "org.modernbyte.orbitfm",
  "description": "OrbitFM — 3D internet radio",
  "path": "/home/user/.local/share/orbitfm/integrations/firefox/orbitfm-native-host",
  "type": "stdio",
  "allowed_extensions": ["orbitfm@modernbyte"]
}
```

Wildcard не используется: браузер запускает хост только для расширения с
точным ID из манифеста. Windows-манифесты через реестр в пакет не входят.

Браузер добавляет к запуску аргумент с origin/ID расширения. Манифест всегда
указывает на wrapper, а wrapper **не пробрасывает `$@`**: `orbitfm native-host`
принимает только ноль аргументов и не использует подсказку браузера для
авторизации. Прямой запуск `orbitfm native-host <origin>` завершается ошибкой.

### 2.3. Нестандартный профиль Chromium

С `--user-data-dir=<каталог>` Chromium ищет native-манифесты в
`<каталог>/NativeMessagingHosts`, а не в `$XDG_CONFIG_HOME`. Установщик пишет
в стандартный профиль; для переносимого профиля манифест нужно положить туда
вручную.

## 3. Фрейминг

Одно сообщение — один кадр:

```text
┌──────────────────────┬─────────────────────────────┐
│ длина: u32           │ payload: UTF-8 JSON         │
│ native-endian, 4 байта│ ровно `длина` байт          │
└──────────────────────┴─────────────────────────────┘
```

- длина — `u32` в порядке байтов хоста (native-endian), как требует native
  messaging;
- `0` и EOF означают конец потока; хост завершает работу;
- предел — **1 МиБ** на сообщение в обе стороны. Кадр длиннее лимита
  отклоняется без чтения payload: хост отвечает `oversized` и завершается;
- кодировка — UTF-8; невалидный JSON в пределах лимита не завершает хост, а
  даёт ответ `invalid-request`;
- stdout хоста содержит только кадры. Диагностика идёт в stderr.

## 4. Запрос

```json
{"id": 7, "command": "status --json"}
```

| Поле | Тип | Обязательно | Смысл |
|------|-----|-------------|-------|
| `id` | любой JSON | нет | Обратно возвращается в ответе; хост его не интерпретирует |
| `command` | строка | да | Команда в синтаксисе IPC/CLI |

Разбор строгий: неизвестные поля и отсутствие `command` дают
`invalid-request`. Строка команды разбирается тем же парсером, что CLI;
неверные аргументы (`volume 2`, лишний аргумент) дают `invalid-command`, а
неизвестный глагол сохраняется как `Raw` и отклоняется allowlist с кодом
`forbidden`.

## 5. Ответ

Успех:

```json
{"id": 7, "ok": true, "response": "playing"}
```

Ошибка:

```json
{"id": 7, "ok": false, "code": "not-running", "error": "OrbitFM не запущен"}
```

`response` — строка ответа IPC. Для `status --json` это компактный JSON
схемы 1 (его нужно разобрать ещё раз), для остальных команд — человекочитаемая
строка.

| Код | Когда |
|-----|-------|
| `invalid-request` | Битая JSON-форма, неизвестное поле, нет `command` |
| `invalid-command` | Неверные аргументы команды (`volume`, `theme`, лишний аргумент) |
| `forbidden` | Команда запрещена allowlist (см. §6), включая неизвестный глагол |
| `oversized` | Входящий кадр больше 1 МиБ; после ответа хост завершается |
| `events-active` | Вторая подписка `events` в этом процессе хоста |
| `not-running` | OrbitFM не запущен |
| `timeout` | IPC-сервер не ответил вовремя |
| `subscription-rejected` | IPC отклонил подписку (исчерпан бюджет) |
| `ipc-error` | Прочая ошибка IPC |
| `invalid-frame` (при `"stream":"invalid"`) | Кадр `events` не разобрался; поток завершён |
| `stream-closed` (при `"stream":"closed"`) | Поток `events` закрыт с ошибкой чтения |

## 6. Allowlist команд

| Разрешено | Запрещено |
|-----------|-----------|
| `toggle`, `show`, `hide` | `quit` |
| `panel`, `overlay` | `play <stationuuid>` |
| `status`, `status --json` | `mode`, `mode <focus\|overlay\|listening\|panel>` |
| `playpause`, `stop`, `next`, `previous`, `roulette` | неизвестная команда |
| `volume <0.0…1.0>`, `volume-up`, `volume-down` | |
| `theme <system\|dark\|light>`, `settings` | |

`quit` запрещён, чтобы popup не завершал процесс; `play` — чтобы расширение не
запускало произвольный поток по UUID; `mode` — чтобы компоновка окна не
переключалась из браузера. Все разрешённые команды выполняются только у
работающего экземпляра.

## 7. Поток событий

Popup отправляет `{"command":"events"}` один раз:

```json
→ {"id": 1, "command": "events"}
← {"id": 1, "ok": true, "response": "events"}
← {"ok": true, "event": {"ev": 1, "type": "snapshot", "status": { … }}}
← {"ok": true, "event": {"ev": 1, "type": "state", "status": { … }}}
← {"ok": true, "event": {"ev": 1, "type": "keepalive"}}
```

- `event` — ровно кадр ADR-0039 (интерфейс 13), без обёртки и без изменений;
- первая подписка подтверждается `response: "events"`, затем идут кадры;
- применить `status` можно из `snapshot` и `state`; `keepalive` состояния не
  несёт и нужен только для обнаружения тишины;
- конец потока — `{"ok": true, "event": null}`; ошибка чтения —
  `{"ok": false, "code": "stream-closed", "stream": "closed", …}`; битый
  кадр — `{"ok": false, "code": "invalid-frame", "stream": "invalid", …}`;
- на процесс хоста разрешена одна подписка; повторная — `events-active`;
- команды управления можно отправлять в тот же порт, пока поток открыт;
- закрытие stdin (popup закрылся) прерывает подписку и завершает хост.

Клиент обязан игнорировать неизвестные `type`, не падать на `station: null` и
переподключаться после `event: null` или `stream-closed`.

## 8. Жизненный цикл и лимиты

| Свойство | Значение |
|----------|----------|
| Процесс | Один на порт расширения; завершается по EOF stdin |
| Аргументы | Не принимает; wrapper не пробрасывает аргументы браузера |
| Runtime | current-thread tokio, без GUI и без второго экземпляра |
| Сеть | Нет слушающих сокетов и портов |
| Входящее сообщение | ≤ 1 МиБ; больше — `oversized` и выход |
| Исходящее сообщение | ≤ 1 МиБ; иначе короткий `{"ok":false,"error":"ответ больше 1 МиБ"}` |
| Очередь вывода | 64 сообщения; медленный popup создаёт backpressure |
| Подписки `events` | Не более одной на процесс хоста |
| Холодный старт | Запрещён для всех команд |
| stderr | Диагностика; в stdout только кадры |

## 9. Установщик

| Цель | Файлы | Native-манифест |
|------|-------|-----------------|
| `chrome` | 6 файлов в `$XDG_DATA_HOME/orbitfm/integrations/chrome/` | в каждом обнаруженном Chromium-каталоге |
| `firefox` | 6 файлов в `$XDG_DATA_HOME/orbitfm/integrations/firefox/` | `~/.mozilla/native-messaging-hosts/` |

Состав: `extension/{manifest.json,popup.html,popup.css,popup.js}`,
`orbitfm-native-host` (0755) и `README.md`. Wrapper содержит
`exec "<путь к orbitfm>" native-host`; для AppImage подставляется путь к
самому образу. Квитанция и `uninstall` — по ADR-0037; `ORBITFM_INTEGRATION_ROOT`
переносит и файлы, и манифесты в тестовый корень.

Версия в `extension/manifest.json` — числовая (`0.13.0`), без `-rc.N`:
Chrome и Firefox не принимают предрелизный суффикс. Полная версия остаётся в
квитанции и `status.version`.

## 10. Пример последовательности

```text
connectNative("org.modernbyte.orbitfm")
→ {"id":1,"command":"events"}
← {"id":1,"ok":true,"response":"events"}
← {"ok":true,"event":{"ev":1,"type":"snapshot","status":{…}}}
→ {"id":2,"command":"playpause"}
← {"id":2,"ok":true,"response":"paused"}
← {"ok":true,"event":{"ev":1,"type":"state","status":{…}}}
→ {"id":3,"command":"volume 0.74"}
← {"id":3,"ok":true,"response":"volume 0.74"}
← {"ok":true,"event":{"ev":1,"type":"state","status":{…}}}
← {"ok":true,"event":null}
```

## 11. Совместимость и безопасность

- Конверт `{"ok":…}` версионируется этим документом; кадр `event` — полем
  `ev` (ADR-0039), снимок — полем `status.schema` (ADR-0037).
- Добавление поля в ответ или `status` не меняет версии; удаление или
  переименование поля `status` требует `schema: 2`, формы кадра — `ev: 2`.
- Браузер проверяет ID расширения по native-манифесту; хост ID не видит и
  полагается на эту проверку.
- Страница расширения не имеет content scripts и `externally_connectable`,
  поэтому веб-страницы не могут открыть порт.
- Локальный процесс пользователя, имеющий доступ к IPC-сокету `0600`, и так
  управляет плеером напрямую (SECURITY.md); native host не даёт ему новых
  прав.
- Сетевого входа нет: хост не принимает соединений и не проксирует трафик.

## 12. Ссылки

- ADR-0038 — браузерное расширение и native messaging host
- ADR-0037 — единая поверхность интеграции
- ADR-0039 — поток событий состояния
- `docs/interfaces/05-ipc.md`, `docs/interfaces/13-events.md`
- `docs/modules/17-browser-companion.md`
