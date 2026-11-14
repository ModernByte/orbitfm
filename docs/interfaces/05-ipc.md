# Интерфейс: IPC Protocol

## 1. Транспорт

| Платформа | Тип                    | Адрес |
|-----------|------------------------|-------|
| Linux     | Unix Domain Socket     | `$XDG_RUNTIME_DIR/orbitfm.sock` |
| Windows   | Named Pipe             | `\\.\pipe\orbitfm-<fnv1a64 хеш профиля>` |
| macOS     | Unix Domain Socket     | `$TMPDIR/orbitfm-<uid>/orbitfm.sock` |

Транспорт выбирается при компиляции: Unix-сокет для Linux/macOS, named pipe для
Windows. Named pipe занимает первый экземпляр как признак single-instance, не
принимает удалённые подключения и защищён стандартным DACL Windows. Хеш профиля
в имени канала разводит сессии разных пользователей одной машины (ADR-0005
ред. 2).

Свойства Unix-сокета:

- права доступа `0600` — управлять экземпляром может только его владелец;
- `XDG_RUNTIME_DIR` должен быть абсолютным каталогом с правами `0700`,
  принадлежащим эффективному UID, иначе `SocketPath`; на прочих Unix каталог
  `$TMPDIR/orbitfm-<uid>` создаётся с `0700` и проверяется теми же правилами;
- рядом с сокетом создаётся lock-файл `orbitfm.lock`, удерживаемый на протяжении жизни
  процесса: он сериализует проверку и очистку сокета между одновременными запусками;
- «зомби»-сокет завершившегося процесса определяется пробным подключением и удаляется;
- если путь существует, но не является Unix-сокетом, он не удаляется (`SocketPath`);
- файл сокета удаляется при штатном завершении сервера, пока lock ещё удерживается.

## 2. Формат сообщений

- Кодировка: UTF-8
- Одна команда = одна строка, заканчивается `\n`
- Клиент пишет команду и читает ответ до EOF; на Unix дополнительно закрывает свою
  половину соединения
- Ответ сервера = одна или несколько строк; завершение ответа — EOF (сервер пишет
  финальный `\n` и закрывает соединение)
- Пустой ответ считается ошибкой протокола
- Максимальная длина команды — 4096 байт без учёта завершающего `\n` или `\r\n`
- Команда обязана быть валидным UTF-8; иначе сервер отвечает `error команда должна быть в UTF-8`
- Максимальный размер ответа — 64 КиБ; превышение считается ошибкой клиента

## 3. Команды (запрос → клиент)

| Команда | Описание | Параметры |
|---------|----------|-----------|
| `toggle` | Показать/скрыть окно | — |
| `show` | Показать окно | — |
| `hide` | Скрыть окно | — |
| `panel` | Показать/скрыть выпадающую панель — её зовёт клик бар-виджета (ADR-0033, ред. 2026-10-28) | — |
| `overlay` | Показать/скрыть капсулу Overlay (CLI, скрипты) | — |
| `play <stationuuid>` | Воспроизвести станцию | UUID |
| `playpause` | Переключить воспроизведение (ADR-0037) | — |
| `next` / `previous` | Следующая/предыдущая станция (ADR-0037) | — |
| `roulette` | Запустить радиорулетку (ADR-0037) | — |
| `stop` | Остановить | — |
| `status` | Текущее состояние | — |
| `status --json` | Снимок состояния в JSON, схема 1 (ADR-0037) | — |
| `events` | Поток снимков состояния NDJSON, конверт `ev: 1` (ADR-0039) | — |
| `events --spectrum` | Поток состояния плюс кадры `spectrum` для визуализатора TUI; планируемый флаг, в 0.18 отклоняется как лишний аргумент (ADR-0047 рев. 2) | — |
| `search <запрос>` | Поиск для companion-попапа; запрос — остаток строки, ≤128 символов; ответ `search <id>`, результаты — кадром `search` в `events` (ред. 0.16) | остаток строки |
| `library` | Избранное и история для попапа: JSON `{"favorites":[…20],"history":[…20]}` (ред. 0.16) | — |
| `favorite [uuid]` | Переключить избранное текущей станции или станции из эфира/выдачи/библиотеки; ответ `favorite true\|false` (ред. 0.16) | UUID, необязательно |
| `play-result <uuid>` | Играть станцию только из последней выдачи companion-поиска (ред. 0.16) | UUID |
| `play-library <uuid>` | Играть станцию только из избранного или истории, без обращения к каталогу (ред. 0.16) | UUID |
| `play-url <url>` | Импортировать внешний поток: `.m3u`, `.pls`, страница или аудио; только `http(s)`, без пробелов, ≤2048 байт (ADR-0040) | URL |
| `sleep <minutes>` | Включить таймер сна: остановить эфир через N минут, `1..=720` (ADR-0046) | целое число |
| `sleep clear` | Выключить таймер сна (ADR-0046) | — |
| `sleep action <действие>` | Что делать после остановки: `nothing`, `suspend` или `power-off` (ADR-0055) | `nothing`, `suspend`, `power-off` |
| `sleep fade <секунды>` | Плавное затухание перед стопом: `0`, `60`, `120`, `180`, `300` (ADR-0056) | целое число из списка |
| `sleep inhibit <on\|off>` | Не давать системе уснуть, пока играет эфир и тикает таймер (ADR-0054) | `on`, `off` |
| `sleep warning <on\|off>` | Окно предупреждения за минуту до действия (ADR-0058) | `on`, `off` |
| `library-export <path>` | Записать избранное и историю в JSON схемы 1 (ADR-0046) | остаток строки-путь |
| `library-import <path>` | Слить JSON схемы 1 с локальной библиотекой (ADR-0046) | остаток строки-путь |
| `stats` | Сводка прослушанного: суммарное время и топ-5 станций, стран и тегов (ADR-0049) | — |
| `stats --json` | Сводка статистики одной строкой JSON, схема 1 (ADR-0049) | — |
| `stats clear` | Сбросить счётчики статистики; избранное и историю не трогает (ADR-0049) | `clear` |
| `alias <stationuuid>` | Показать ручную ссылку станции: `alias <uuid> <url>` или `alias none` (ADR-0050) | UUID |
| `alias <stationuuid> <url>` | Поставить или заменить адрес эфира станции; адрес проходит ярус 1, ответ `ok alias <host>` (ADR-0050) | UUID, URL |
| `alias <stationuuid> remove` | Снять ручную ссылку: станция снова играет по каталожному `url_resolved` (ADR-0050) | `remove` |
| `alias list` | Все алиасы: по строке `alias <uuid> <url>` на запись или `alias none` (ADR-0050) | — |
| `mode` | Текущий режим окна | — |
| `mode <режим>` | Переключить режим окна | `focus`, `overlay`, `listening` или `panel` |
| `theme <режим>` | Установить тему интерфейса (ADR-0037) | `system`, `dark`, `light` |
| `network <профиль>` | Установить профиль сети (ADR-0048) | `unlimited`, `metered`, `offline-first` |
| `volume <0.0-1.0>` | Установить громкость | float |
| `volume-up` / `volume-down` | Шаг громкости ±0.1 (ADR-0037) | — |
| `settings` | Открыть окно настроек (ADR-0037) | — |
| `quit` | Полный выход | — |

Лишние аргументы у команд без параметров, некорректный `volume` (не число, `NaN`,
вне диапазона `0.0..=1.0`), отсутствующий аргумент `play`, неизвестный режим у
`mode <режим>`, неизвестная тема у `theme` и неизвестный профиль у `network`
отклоняются как `error <сообщение>`.
`status` принимает только необязательный `--json`; любой другой аргумент —
`error некорректный аргумент команды: …`. `stats` устроен так же: принимает
только необязательный `--json` или `clear`, прочие аргументы отклоняются тем же
сообщением. `alias` так же различает служебные слова по позиции: второй аргумент
`remove` снимает запись, третий аргумент после `list` отклоняется; сам URL
пробелов не содержит (ADR-0050). `events` в 0.18 не принимает
аргументов; `--spectrum` — планируемое аддитивное расширение, при котором
без флага поток не меняется.

`search` — единственная команда, которая берёт остаток строки целиком, поэтому
запрос может содержать пробелы; пустой запрос и запрос длиннее 128 символов
отклоняются. `favorite` без аргумента работает с текущей станцией; с аргументом
ищет UUID в текущем эфире, последней выдаче попапа, избранном и истории.
`play-result` и `play-library` принимают ровно один UUID и не обращаются к
каталогу Radio Browser: первый играет только строку последней выдачи, второй —
только элемент библиотеки (воспроизведение, разумеется, соединяется с потоком).
`play-url` принимает ровно один `http(s)`-адрес без
пробелов; резолв идёт в tokio-задаче, ответ приходит после `start_station`
(бюджет резолвера 8 с, ответа — 10 с), повторный импорт до завершения
получает `error импорт уже выполняется` (ADR-0040).

`sleep` принимает одно целое число минут (`1..=720`) или `clear`; повторный
`sleep <minutes>` заменяет дедлайн, `clear` без активного таймера — no-op.
Таймер переживает скрытие окна и проверяется в простое, но не переживает
выход процесса; по срабатывании эфир останавливается и показывается
уведомление (ADR-0046).

`sleep action|fade|inhibit|warning` — подкоманды ритуала сна (ADR-0054…0058):
`action` выбирает `nothing`/`suspend`/`power-off`, `fade` — длительность
затухания из `0/60/120/180/300` секунд, `inhibit` и `warning` — тумблеры
`on`/`off`. Значения пишутся в `settings.json` и применяются к активному
таймеру сразу: смена `fade` пересчитывает начатый фейд, смена `inhibit`
берёт или снимает лок, смена `action` влияет на ближайшее срабатывание.
Ответы — `ok sleep action <действие>`, `ok sleep fade <секунды>`,
`ok sleep inhibit <on|off>`, `ok sleep warning <on|off>`; неизвестное
действие, секунды вне списка и `on`/`off` в другом регистре отклоняются
как `error`. `sleep action suspend` и `power-off` не запускают действие
немедленно — только выбирают поведение таймера.

`network` меняет профиль сети и сохраняет его в `settings.json`: `metered`
ограничивает discovery битрейтом 128 кбит/с и уменьшает фоновую выборку мира,
`offline-first` отключает фоновые сетевые загрузки и опирается на офлайн-снимок.
Явные поиск, Roulette и запуск станции по-прежнему ходят в сеть; SSRF-политика
и адаптивное аудио не меняются (ADR-0048). Браузерный native host команду не
получает (ADR-0038).

`library-export` и `library-import` берут путь остатком строки, поэтому пробелы
в имени файла допустимы. Относительный путь разрешается от текущего каталога
процесса. Импорт — слияние по `stationuuid` с приоритетом локальной записи;
формат и правила безопасности — `docs/interfaces/15-library-export.md`.
Браузерный native host обе команды запрещает.

`stats`, `stats --json` и `stats clear` читают и сбрасывают локальную статистику
прослушивания (ADR-0049, `docs/interfaces/17-stats.md`). Все три команды
обслуживает **уже запущенный** экземпляр, как `sleep`: новый процесс они не
запускают, без запущенного — `error OrbitFM не запущен`. `stats` отвечает
одной многострочной сводкой: `stats total <seconds>`, `stats enabled <true|false>`
(значение `stats_enabled`, чтобы нули после выключения учёта не выглядели потерей)
и три списка топ-5 — `stats station <seconds> <имя>`, `stats country <seconds> <КОД>`
и `stats tag <seconds> <тег>`; имена станций схлопываются в одну строку, как в
`status`. `stats clear` идемпотентен и всегда отвечает
`ok stats cleared`, в том числе при нулевых счётчиках. `status --json` эти
команды не меняют — поле статистики в схему 1 не добавляется, интеграторы не
платят парсингом за неиспользуемые данные. Браузерный native host их не
получает: вне белого списка ADR-0038 ответ `forbidden`.

`play <uuid>` ищет станцию сначала локально — в результатах последнего поиска
и офлайн-снимке мира, — и запускает её в том же кадре. Если UUID там нет,
поиск в кэше/каталоге Radio Browser идёт в tokio-задаче (таймаут 8 с), а ответ
клиенту приходит после `start_station`: `ok` означает фактически запущенный
эфир, сетевые ошибки и отсутствующая станция — `error <сообщение>`. Повторный
`play` до завершения поиска получает `error поиск станции уже выполняется`.
Click в Radio Browser уходит асинхронно, как и раньше (ADR-0063).
`stop` и `volume` управляют аудио-ядром напрямую.

Неизвестная команда разбирается как `IpcCommand::Raw`; Application Core отвечает на неё
`error неизвестная команда: <текст>`. Это оставляет возможность расширять протокол без
изменения парсера.

Браузерный native host — ещё один клиент этого протокола: он отправляет команды
через тот же сокет, но с собственным allowlist (`quit`, `play` и `mode`
запрещены) и без холодного старта. Контракт — `docs/interfaces/14-native-host.md`.

`mode` без аргумента только спрашивает режим и ничего не меняет. `mode <режим>`
переключает режим окна и показывает его: в Focus-раскладке капсула бессмысленна,
поэтому команда равносильна «перейти и показать» (ADR-0021 п. 5). Смена режима при
открытом окне закрывает root-вьюпорт и тут же открывает новый с геометрией режима,
поэтому окно меняет размер, а не растягивается живым resize-запросом.

`panel` и `overlay` — переключатели поверхностей одного и того же вида (ADR-0033):
если окно уже видимо в режиме команды, она скрывает его, иначе показывает без рамок и
заголовка в геометрии режима. Оба идут через `WindowState::toggle_dropdown(mode)`,
поэтому состояние «что открыто» живёт только в Core. `mode overlay`/`mode panel` —
явная установка режима без toggle-семантики. Бар-виджет Noctalia вызывает `panel`
(ADR-0033, ред. 2026-10-28), ярлык приложения — `mode focus`: развод точек входа
сделан на командах, а не в плагине. Panel не входит в
`WindowMode::ALL` и не предлагается переключателем раскладок — это поверхность, а не
третья (четвёртая) раскладка стола.

## 4. Ответы

### Успех
```
ok
```
или для `status`:
```
status playing
stationuuid 960cf833-0601-11e8-ae97-52543be04c81
name SomaFM Groove Salad (128k MP3)
volume 0.85
window hidden
mode overlay
```

или для `mode`:
```
mode focus
```

или для companion-команд (ред. 0.16):
```
search 7
favorite true
```
`library` отвечает одной строкой JSON:
`{"favorites":[{"uuid":…,"name":…,"country":…,"countrycode":…,"tags":…,"bitrate":128,"source":"radio"}],"history":[…]}`.

`sleep`, `library-export` и `library-import` отвечают:
```
ok sleep 45
ok exported 12
ok imported 3 favorites, 1 history, 2 skipped
```
`sleep clear` отвечает просто `ok`. Подкоманды ритуала сна отвечают
`ok sleep action suspend`, `ok sleep fade 300`, `ok sleep inhibit on` и
`ok sleep warning off`. `network` отвечает `ok network <профиль>`.

`stats` и `stats clear` отвечают (ADR-0049):
```
stats total 246000
stats enabled true
stats station 5400 Radio Swiss Pop
stats country 9000 CH
stats tag 42000 pop
ok stats cleared
```
Первые пять строк — сводка `stats`: суммарное время, признак учёта и по одной
строке каждого из трёх списков топ-5 (станции, страны, теги) — в примере
списки сокращены. `stats clear`
всегда отдаёт `ok stats cleared`, даже когда счётчики нулевые.

`alias` отвечает (ADR-0050):
```
alias 9604b0f1-7f5c-4a3b-b3c9-7c1f5e7d1234 https://live.example.org/stream
alias none
ok alias live.example.org
ok alias removed
alias 11111111-1111-1111-1111-111111111111 https://a.example/stream
alias 22222222-2222-2222-2222-222222222222 https://b.example/stream
```
Первые две строки — ответы `alias <uuid>` (найденный алиас и `alias none`),
третья — `alias <uuid> <url>` с хостом принятого адреса, четвёртая —
`alias <uuid> remove`. `alias list` печатает по строке `alias <uuid> <url>`
на запись, пустой список отдаёт `alias none`. Отклонённый адрес отвечает
`error <причина яруса 1>` (приватный адрес, схема, длина), как в
`docs/interfaces/18-aliases.md` § 5.

`status` принимает значения `stopped`, `playing`, `paused`, `buffering`, `reconnecting`,
`error`. Текст ошибки в статус не включается. `volume` печатается с двумя знаками после
запятой. Строки `stationuuid` и `name` присутствуют, пока состояние не `stopped`/`error`
и известна текущая станция; многострочное имя схлопывается в одну строку. `window hidden`
возвращается для скрытого окна. Строка `mode` стоит после `window` и принимает значения
`focus`, `overlay`, `listening`, `panel`; скрытое окно тоже имеет режим — он применится
следующим показом.

### Ошибка
```
error <сообщение>
```

Клиент считает ответ ошибочным, если после ведущих пробелов строка начинается с `error`.

### `status --json` (схема 1)

Ответ — одна строка компактного JSON без перевода строки в конце (сервер добавляет
финальный `\n`). Текстовый `status` не меняется: JSON — отдельная команда, чтобы
парсеры оболочек не зависели от формата человекочитаемого вывода (ADR-0037).

```json
{
  "schema": 1,
  "app": "orbitfm",
  "version": "0.23.0-rc.1",
  "state": "playing",
  "station": {
    "uuid": "960cf833-0601-11e8-ae97-52543be04c81",
    "name": "SomaFM Groove Salad (128k MP3)",
    "country": "The United States Of America",
    "countrycode": "US",
    "tags": "ambient,chillout",
    "favicon": "https://example.org/favicon.png",
    "homepage": "https://somafm.com",
    "bitrate": 128,
    "codec": "MP3",
    "source": "radio",
    "favorite": false
  },
  "volume": 0.42,
  "window": { "visible": true, "mode": "focus" },
  "theme": "dark",
  "accent": "blue",
  "language": "en",
  "network": "unlimited",
  "can": { "previous": true, "next": true },
  "sleep": {
    "minutes": 45,
    "remaining_secs": 2700,
    "inhibited": true,
    "action": "suspend",
    "warning": true,
    "fade_seconds": 300
  },
  "settings_open": false,
  "ui": {
    "play_pause": "Play/Pause", "stop": "Stop", "previous": "Previous",
    "next": "Next", "radio_roulette": "Radio Roulette",
    "settings": "Settings…", "show_window": "Show window",
    "hide_window": "Hide window", "nothing_playing": "Nothing playing",
    "volume_up": "Volume Up", "volume_down": "Volume Down",
    "search": "Search", "no_match": "Nothing found", "retry": "Retry",
    "close": "Close",
    "favorites": "Favorites", "history": "History",
    "play_this_page": "Play this page",
    "no_audio": "No audio found on this page.",
    "importing": "Importing stream…",
    "stream_rejected": "The stream was rejected.",
    "add_favorite": "Add to favorites", "remove_favorite": "Remove from favorites",
    "now_playing": "Now playing", "volume": "Volume", "theme": "Theme",
    "theme_system": "System", "theme_dark": "Dark", "theme_light": "Light",
    "about_station": "About station", "open_homepage": "Open homepage",
    "sleep_timer": "Sleep timer", "sleep_off": "Off",
    "sleep_minutes": "{minutes} min",
    "state_playing": "on air", "state_paused": "paused",
    "state_buffering": "buffering", "state_reconnecting": "reconnecting",
    "state_stopped": "stopped", "state_error": "error"
  }
}
```

- `state` использует те же значения, что текстовый `status`; `station` — `null`
  для `stopped`/`error` и для состояния без выбранной станции.
- `window.visible` на macOS отражает скрытое `NSWindow`, а не только закрытие
  root-вьюпорта (ADR-0026): `hide` даёт `false` при живом процессе.
- `volume` округляется до тысячных, чтобы серия `volume-up` не давала
  `0.30000000000000004`.
- `ui` — локализованные подписи из каталога i18n (`TrayText`, `CommonText`,
  `TransportText`, `SettingsText`) для действующего языка: оболочки не
  дублируют каталог на 16 языках. Неизвестный язык клиента не влияет — язык
  берётся из настроек приложения. `ui.state_*` — подписи состояний
  воспроизведения для бейджа попапа, `ui.close` — кнопка закрытия
  выбора страницы. `ui.sleep_timer`, `ui.sleep_off` и шаблон
  `ui.sleep_minutes` с `{minutes}` дают быстрым меню расширений те же подписи,
  что и desktop (ADR-0046).
- `accent` — токен акцента интерфейса (`blue`, `green`, `amber`, `violet`,
  `rose`, `cyan`): попап и виджеты панелей красят акцент, не дублируя палитру.
- `sleep` — активный таймер сна (ADR-0046): `minutes` — выбранная
  длительность, `remaining_secs` — остаток на момент снимка; `null`, когда
  таймер выключен. Аддитивные поля ритуала (ADR-0054…0058): `inhibited` —
  лок сна взят прямо сейчас, `action` — `nothing`/`suspend`/`power-off`,
  `warning` — включено ли окно предупреждения, `fade_seconds` — длительность
  затухания. Все поля аддитивны, схема остаётся 1.
- `settings_open` — открыто ли окно настроек (ADR-0053). Поле аддитивно,
  схема остаётся 1.
- `network` — действующий профиль сети: `unlimited`, `metered` или
  `offline-first` (ADR-0048). Поле аддитивно, схема остаётся 1.
- `can` — доступность шага по видимой очереди с учётом `queue_wrap`:
  при выключенном wrap на краю списка соответствующий флаг `false`.
- `station.source` — `radio` (каталог Radio Browser) или `stream` (внешний
  поток из `play-url`, ADR-0040); `station.favorite` — лежит ли станция в
  избранном прямо сейчас.
- `library` не входит в `status`: это отдельный ответ с избранным и историей,
  по 20 компактных записей на список (`uuid`, `name`, `country`,
  `countrycode`, `tags`, `bitrate`, `source`); длинные поля обрезаны, поэтому
  ответ гарантированно меньше 64 КиБ.
- `station.favicon` и `station.homepage` — **сырые значения каталога Radio
  Browser**, какие в нём и лежат. Политика артворка (ADR-0025, ред. 4) проверяет
  адрес цели загрузки и метаданные MPRIS/SMTC/Now Playing, но не переписывает
  снимок станции: клиент, который собирается что-то по этим ссылкам загружать,
  обязан сам применить ярус 1 и не считать приватный адрес доступным. Поле
  остаётся как есть и в `schema: 1` — оно описывает станцию, а не разрешение
  на загрузку.
- Поля `url` в схеме нет: `url_resolved` — адрес потока, полученный при
  разрешении станции, и наружу он не публикуется. Клиент выбирает воспроизведение
  по `uuid` (`orbitfm play <uuid>`), а не по сырому URL.
- Совместимость: добавление полей не меняет `schema`; удаление или переименование —
  только `schema: 2`. Клиенты обязаны игнорировать неизвестные поля.
- `can.previous`/`can.next` — есть ли в очереди станция для перехода; рулетка
  доступна всегда, поэтому флага нет.

### `stats --json` (схема 1)

Ответ — одна строка компактного JSON: `{ schema, app, version, enabled,
total_seconds, week_seconds, month_seconds, stations[], countries[], tags[],
days[], ui }`. Полная схема, пределы списков (топ-10 станций, стран и тегов, не
больше 30 дат) и правило аддитивности — `docs/interfaces/17-stats.md` § 5
(ADR-0049). `app`/`version` и локализованный `ui` устроены как в `status --json`
(ADR-0037, интерфейс 12). Сводки по странам и тегам считает приложение:
потребитель их не пересобирает. Схема ответа независима от схемы `status` и
остаётся 1 — в `status --json` поле статистики не появляется.

### `events` (поток, ADR-0039)

Долгоживущая read-only подписка на тот же сокет. Клиент пишет `events\n`,
закрывает свою половину и читает NDJSON до EOF:

```json
{"ev":1,"type":"snapshot","status":{ … }}
{"ev":1,"type":"state","status":{ … }}
{"ev":1,"type":"search","id":7,"query":"lofi","results":[…],"error":null}
{"ev":1,"type":"keepalive"}
```

- первая строка — всегда `snapshot` с полным объектом схемы 1;
- `state` — полный кадр после изменения; одинаковые кадры не публикуются,
  промежуточные при медленном чтении схлопываются;
- `search` — ответ на `search <запрос>`: поколение, запрос, до 20 компактных
  станций и ошибка; новому подписчику не переигрывается (ред. 0.16);
- `keepalive` — через 15 секунд тишины;
- `events` не принимает команды и не входит в cold-start allowlist;
- одновременно не более 16 подписок, остальные слоты из 64 остаются one-shot
  командам.

Полный контракт — `docs/interfaces/13-events.md`.

## 5. Таймауты и лимиты

| Параметр | Значение |
|----------|----------|
| Максимальная длина команды | 4096 байт |
| Максимальный размер ответа | 64 КиБ |
| Одновременных IPC-соединений | 64 |
| Одновременных подписок `events` | 16 |
| Keepalive потока на простое | 15 с |
| Подключение клиента к сокету | 1 с |
| Чтение команды сервером | 2 с |
| Постановка запроса в очередь App | 1 с |
| Ожидание ответа Application Core | 10 с |
| Ожидание ответа клиентом | 12 с |
| Дренаж соединений при shutdown | 1 с |
| Таймер сна | 1..=720 минут |
| Импорт библиотеки | 2 МиБ, ≤5000 станций |

При истечении таймаута клиент получает `error таймаут ...` (если соединение ещё открыто)
либо локальную ошибку `IpcError::Timeout`.

## 6. Rust-контракт

```rust
/// Верхняя граница таймера сна: двенадцать часов.
pub const MAX_SLEEP_MINUTES: u16 = 720;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowModeArg {
    Focus,
    Overlay,
    Listening,
    /// Выпадающая панель бар-виджета (ADR-0033).
    Panel,
}

impl WindowModeArg {
    /// Имя режима в протоколе.
    pub fn as_str(self) -> &'static str;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeArg {
    System,
    Dark,
    Light,
}

impl ThemeArg {
    /// Имя темы в протоколе.
    pub fn as_str(self) -> &'static str;
}

/// Действие по истечении таймера сна в протоколе (ADR-0055).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SleepActionArg {
    Nothing,
    Suspend,
    PowerOff,
}

impl SleepActionArg {
    /// Имя действия в протоколе: `nothing`, `suspend`, `power-off`.
    pub fn as_str(self) -> &'static str;
    /// Разбор аргумента; регистр и лишние слова — ошибка.
    pub fn parse(raw: &str) -> Result<Self, IpcError>;
}

#[derive(Debug, Clone, PartialEq)]
pub enum IpcCommand {
    Toggle,
    Show,
    Hide,
    Play { station_uuid: String },
    /// `playpause` — toggle воспроизведения (ADR-0037).
    PlayPause,
    /// `next` — следующая станция очереди (ADR-0037).
    Next,
    /// `previous` — предыдущая станция очереди (ADR-0037).
    Previous,
    /// `roulette` — радиорулетка (ADR-0037).
    Roulette,
    Stop,
    Status,
    /// `status --json` — снимок по схеме 1 (ADR-0037).
    StatusJson,
    /// `events` — поток снимков состояния NDJSON (ADR-0039).
    Events,
    /// `mode` — спросить текущий режим.
    ModeQuery,
    /// `mode <режим>` — переключить.
    SetMode { mode: WindowModeArg },
    /// `panel` — переключатель панели для бар-виджета: повтор при открытой
    /// панели скрывает её (ADR-0033, ред. 2026-10-28).
    Panel,
    /// `overlay` — переключатель капсулы Overlay (CLI и скрипты) (ADR-0033).
    Overlay,
    /// `theme <режим>` — установить тему (ADR-0037).
    Theme { mode: ThemeArg },
    /// `settings` — открыть окно настроек; может запустить экземпляр (ADR-0037).
    Settings,
    /// `volume-up`/`volume-down` — шаг громкости ±0.1 (ADR-0037).
    VolumeUp,
    VolumeDown,
    /// `search <запрос>` — поиск попапа; ответ `search <id>`, кадр `search`
    /// уходит в поток `events` (ред. 0.16).
    Search { query: String },
    /// `library` — избранное и история попапа (JSON, ≤64 КиБ).
    Library,
    /// `favorite [uuid]` — избранное текущей или известной станции.
    Favorite { station_uuid: Option<String> },
    /// `play-result <uuid>` — только из последней выдачи попапа.
    PlayResult { station_uuid: String },
    /// `play-library <uuid>` — только из избранного/истории.
    PlayLibrary { station_uuid: String },
    /// `play-url <url>` — импорт внешнего потока (ADR-0040).
    PlayUrl { url: String },
    /// `sleep <minutes>` — таймер сна, 1..=720 минут (ADR-0046).
    SetSleep { minutes: u16 },
    /// `sleep clear` — выключить таймер сна.
    ClearSleep,
    /// `sleep action <действие>` — что делать после остановки (ADR-0055).
    SetSleepAction { action: SleepActionArg },
    /// `sleep fade <секунды>` — затухание перед стопом (ADR-0056).
    SetSleepFade { seconds: u16 },
    /// `sleep inhibit <on|off>` — не давать системе уснуть (ADR-0054).
    SetSleepInhibit { enabled: bool },
    /// `sleep warning <on|off>` — окно предупреждения (ADR-0058).
    SetSleepWarning { enabled: bool },
    /// `library-export <path>` — JSON схемы 1 (ADR-0046).
    LibraryExport { path: String },
    /// `library-import <path>` — слияние JSON схемы 1 (ADR-0046).
    LibraryImport { path: String },
    /// `stats` — человекочитаемая сводка статистики прослушивания (ADR-0049).
    Stats,
    /// `stats --json` — сводка статистики в JSON схемы 1 (ADR-0049).
    StatsJson,
    /// `stats clear` — сбросить счётчики статистики (ADR-0049).
    ClearStats,
    Quit,
    Volume { level: f64 },
    Raw(String),
}

#[derive(Debug, thiserror::Error)]
pub enum IpcError {
    #[error("экземпляр уже запущен")]
    AlreadyRunning,
    #[error("OrbitFM не запущен")]
    NotRunning,
    #[error("не удалось подключиться к существующему экземпляру: {0}")]
    Connect(String),
    #[error("ошибка сокета: {0}")]
    Io(#[from] std::io::Error),
    #[error("некорректный аргумент команды: {0}")]
    InvalidArgument(String),
    #[error("превышен допустимый размер команды")]
    CommandTooLong,
    #[error("таймаут IPC: {0}")]
    Timeout(String),
    #[error("сервер отклонил подписку: {0}")]
    Subscription(String),
    #[error("не удалось определить путь сокета: {0}")]
    SocketPath(String),
}

/// Публикатор полного кадра состояния для подписок `events` (ADR-0039).
pub struct StatusFeed { /* watch::Sender<Option<String>> */ }

impl StatusFeed {
    pub fn new() -> Self;
    /// Новый получатель видит текущий кадр сразу; `None` — первый ещё не готов.
    pub fn subscribe(&self) -> tokio::sync::watch::Receiver<Option<String>>;
    /// Публикует кадр; повтор того же кадра подписчиков не будит.
    pub fn publish(&self, status: String);
}

/// Публикатор кадров `search`; latest-wins, новому подписчику не повторяется
/// (ADR-0039, ревизия 0.16).
pub struct SearchFeed { /* watch::Sender<Option<String>> */ }

impl SearchFeed {
    pub fn new() -> Self;
    pub fn subscribe(&self) -> tokio::sync::watch::Receiver<Option<String>>;
    /// Публикует готовый кадр `{"ev":1,"type":"search",…}`; каждый `id` меняет
    /// значение канала, даже если запрос и результаты совпали.
    pub fn publish(&self, frame: String);
}

/// Пробуждает GUI-цикл после постановки команды в очередь.
pub type WakeFn = std::sync::Arc<dyn Fn() + Send + Sync>;

/// Запрос, доставленный в Application Layer.
pub struct IpcRequest { /* command, oneshot reply */ }

impl IpcRequest {
    pub fn command(&self) -> &IpcCommand;

    /// Возвращает `false`, если клиент уже отключился.
    pub fn try_respond(self, response: impl Into<String>) -> bool;
}

pub struct IpcServer { /* ... */ }

impl IpcServer {
    /// Попытаться занять сокет. Err(AlreadyRunning), если уже есть экземпляр.
    pub async fn try_bind() -> Result<Self, IpcError>;

    /// Забрать очередь входящих запросов. Вызывается один раз при старте App.
    pub fn commands(&mut self) -> tokio::sync::mpsc::Receiver<IpcRequest>;

    /// Отправить команду уже работающему экземпляру.
    pub async fn send_to_existing(command: &str) -> Result<String, IpcError>;

    /// Подписаться на поток событий уже работающего экземпляра (ADR-0039).
    pub async fn subscribe_to_existing() -> Result<(), IpcError>;

    /// Принимать соединения до сигнала shutdown, затем дождаться активных ответов.
    pub async fn run(
        self,
        wake: WakeFn,
        status: tokio::sync::watch::Receiver<Option<String>>,
        search: tokio::sync::watch::Receiver<Option<String>>,
        shutdown: tokio::sync::oneshot::Receiver<()>,
    );
}
```

`WindowModeArg` — wire-тип: `src/ipc/` не знает доменного `WindowMode`
(`src/session/window.rs`), а `impl From<WindowModeArg> for WindowMode` живёт в
Application Core (`src/session/commands.rs`). Точно так же `ThemeArg` не знает
`ThemeMode`: преобразование объявлено в `src/session/commands.rs`. Направление
зависимости остаётся Core → инфраструктура.

При shutdown сервер прекращает accept, ждёт завершения активных соединений до 1 с и
только затем удаляет сокет. Это гарантирует, что ответ на `quit` успевает уйти клиенту.

`IpcServer` — конкретная структура, а не трейт: подмена реализации в тестах не требуется,
для изоляции используется `try_bind_at()` внутри модуля.

Ответ клиенту отправляется после обработки команды в Application Core: состояние
приложения уже обновлено и `ViewportCommand` отправлены. Применение команды оконным
менеджером асинхронно и в ответе не подтверждается. Если клиент отключился раньше,
`try_respond` возвращает `false`; это не считается ошибкой сервера.

## 7. Семантика CLI

| Ситуация | Поведение |
|----------|-----------|
| Без аргументов | Команда `toggle` |
| Экземпляр запущен | Команда отправляется ему; ответ печатается для явно заданной команды |
| Экземпляр не запущен, команда `toggle`/`show`/`panel`/`overlay` | Запускается новый процесс с видимым окном (для `panel` — в режиме Panel, для `overlay` — в режиме Overlay) |
| Экземпляр не запущен, команда `mode <режим>` | Запускается новый процесс с видимым окном в этом режиме |
| Экземпляр не запущен, команда `settings` | Запускается новый процесс с открытым окном настроек (ADR-0037) |
| `try_bind` вернул `AlreadyRunning` во время запуска | Команда повторно отправляется до 5 раз с интервалом 50 мс |
| Экземпляр не запущен, `playpause`/`next`/`previous`/`roulette`/`theme`/`volume-up`/`volume-down`/`sleep`/`network`/`status`/`status --json`/`events` | Ошибка `OrbitFM не запущен`, код возврата 1: управляющая команда не поднимает GUI |
| Экземпляр не запущен, `search`/`library`/`favorite`/`play-result`/`play-library`/`play-url` | Ошибка `OrbitFM не запущен`, код возврата 1: импорт и поиск попапа не поднимают GUI (ADR-0040) |
| Экземпляр не запущен, `stats`/`stats --json`/`stats clear` | Ошибка `OrbitFM не запущен`, код возврата 1: статистика обслуживает только живой экземпляр и GUI не поднимает (ADR-0049) |
| Экземпляр не запущен, `integration <install\|uninstall\|status>` | Не ошибка: команда перехватывается `parse_invocation` до проверки одиночного экземпляра, работает без запущенного OrbitFM и GUI не поднимает |
| Ответ начинается с `error` | Текст ошибки выводится в stderr, код возврата 1 |
| `status --json` | Строка JSON печатается как есть, без разбора клиентом |
| `stats --json` | Строка JSON печатается как есть, без разбора клиентом (ADR-0049) |
| `events` | Кадры NDJSON печатаются по мере поступления до EOF/Ctrl-C (ADR-0039) |
| `search` | Печатается `search <id>`; результаты приходят кадром `search` в `events` |
| `play` | Локальная станция — ответ сразу; незнакомый UUID — после поиска в каталоге (до 8 с) и запуска; при ошибке — `error …` |
| `play-url` | Ответ приходит после резолва (до 8 с) и запуска станции; при ошибке — `error …` |
| `quit` | Сервер отвечает `ok`, затем окно закрывается и процесс завершается |

Гонка «экземпляр появился между проверкой и `try_bind`» обрабатывается повторной
отправкой исходной команды уже запущенному процессу.

### `integration` — команда CLI, не команда сокета

`integration` в протоколе отсутствует: `parse_invocation` (`src/main.rs`)
перехватывает её раньше решения «отправить запущенному или поднять своё окно» и
отдаёт модулю `src/integration/mod.rs`. Ответов `ok`/`error` она не печатает —
это обычный CLI-вывод, отказ же приходит в stderr с кодом возврата 1.
Грамматика: `orbitfm integration status [цель]`,
`orbitfm integration install <цель> [--force]`,
`orbitfm integration uninstall <цель> [--force]`; без подсказки или с `--help`
печатается справка. `install`/`uninstall` принимают ровно одну цель, `status`
без цели обходит все. Цели задаются id из списка 17 целей —
`plasma6, gnome, omarchy, noctalia, niri, hyprland, waybar, polybar, i3blocks, swaybar, i3status-rust, xfce, cinnamon, mate, lxqt, chrome, firefox`
(константа `TARGET_LIST`, там же алиасы `hypr`/`hyprctl` → `hyprland`,
`kde` → `plasma6`); неизвестная цель и незнакомый флаг отклоняются текстом,
перечисляющим весь список. Установка применима только на Linux, на других ОС
цель отклоняется как неприменимая. Браузерный native host
(`docs/interfaces/14-native-host.md`) команд `integration` не получает: они
разбираются как `IpcCommand::Raw` и попадают под `forbidden` белого списка
ADR-0038.

Вывод `install` — отчёт по цели: `Установлено: <цель> (N файлов)`, затем
`каталог:` (строка печатается только если каталог цели существует — у `noctalia`
файлов нет), `квитанция:` и `внимание:` при наличии заметки; дальше идёт блок
`Подключение <оболочка>:` с шагами, которые остаются за пользователем. Для целей
ADR-0051 шаги такие (полностью — модуль 16 § 4.7):

- `noctalia` — сообщение о зарегистрированном источнике `orbitfm` (наш он этой
  установке или был ранее), URL источника, состояние виджета
  `modernbyte/orbitfm` и команда обновления
  `noctalia msg plugins update orbitfm`;
- `niri` — путь файла правил `~/.config/niri/cfg/orbitfm.kdl`, а про
  `config.kdl` вывод зависит от квитанции: если строку
  `include "./cfg/orbitfm.kdl"` добавил установщик, печатаются эта строка, путь
  резервной копии `config.kdl.orbitfm.bak` и указание, что снимает её
  `integration uninstall niri`; если строка уже была в чужом конфиге, печатается
  «добавили её не мы», копии `config.kdl` установка не делала и снятие оставит
  там нерабочий `include` — строку убирает пользователь. В обоих случаях
  добавлено указание, что niri 26.04+ перечитывает конфиг сам, отдельной команды
  нет;
- `hyprland` — путь `~/.config/hypr/orbitfm.conf` и строку
  `source = ~/.config/hypr/orbitfm.conf`, которую пользователь сам добавляет в
  свой `hyprland.conf` (в чужой конфиг установщик её не пишет), плюс сообщение,
  что 18 правил текущего сеанса применены через
  `hyprctl keyword windowrulev2`.

Отказ `hyprctl` установкой не отменяется: он уходит в строку
`внимание: правила не применены на сеанс: <причина>; файл <путь>, подключение:
<строка source>` — той же подробностью её показывает и строка «Интеграции»
окна настроек (`handoff_note`, модуль 16 § 4.1). У `uninstall` вывод —
`удалено файлов: N` плюс `внимание:` с причиной, если оболочка не сняла
регистрацию, и отдельное `внимание:` для niri — если в `config.kdl` осталась строка
`include "./cfg/orbitfm.kdl"`, которую добавляли не мы: файл правил снят, а
ссылка на него живёт и niri не разберёт конфиг, поэтому строку и её путь
показывают (модуль 16 § 4.5). У `status <цель>` дополнительно печатаются
`каталог:` — тоже только когда каталог цели существует, —
`квитанция:` и `бинарник:`. Схема квитанции при этом остаётся `1`: поля `dirs`, `lines`,
`source_added` и `was_enabled` аддитивны (модуль 16 § 4).

`status --json` (схема 1) и поток `events` от появления целей `noctalia`, `niri`
и `hyprland` не меняются: интеграция оболочки — это раскладка файлов и внешние
команды, а не состояние плеера.
