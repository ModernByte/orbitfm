# Модуль: Браузерный компаньон (`src/native_host.rs`, `integrations/browser/`)

**Путь в коде:** `src/native_host.rs`, `src/integration/{mod,assets}.rs`,
`integrations/{browser,chrome,firefox}/`  
**Ответственность:** stdio-хост native messaging, общий popup расширения,
манифесты Chromium и Firefox, установка и удаление через
`orbitfm integration install chrome|firefox`.  
**Зависимости:** `tokio` (current-thread), `serde`, `serde_json`, `anyhow`,
`include_str!`; общий IPC-клиент и `events` (ADR-0039).  
**Решение:** ADR-0038 «Браузерное расширение и native messaging host»

---

## 1. Назначение и границы

Браузерная панель показывает «сейчас играет» и управляет уже запущенным
OrbitFM. Native messaging — единственный штатный канал браузера к настольному
приложению; собственного сетевого сервера и второго GUI-процесса нет.

| Живёт в модуле | Не живёт в модуле |
|---|---|
| Фрейминг stdio, allowlist, конверт `{"ok":…}`, пересылка `events`, лимиты | Состояние эфира (`Session`), IPC-сервер, локализованные подписи (`i18n`) |
| Popup, манифесты, wrapper, installed-раскладка, native-манифесты | Правка настроек браузера, магазин расширений, подпись XPI |
| Тесты протокола и установки | Автозапуск OrbitFM из браузера |

Границы с соседями:

- `src/ipc` — единственный путь команд и подписки; хост не дублирует логику
  `Session`;
- `src/integration` — общие квитанции, `--force`, проверки путей и
  `ORBITFM_INTEGRATION_ROOT`; браузерные цели не заводят второй установщик;
- `src/i18n` — popup не содержит каталога строк, подписи приходят из
  `status.ui` (ADR-0037).

## 2. Состав пакета

| Файл | Ответственность |
|------|-----------------|
| `src/native_host.rs` | `run`, `run_loop`, `read_frame`/`write_frame`, allowlist, ответы, пересылка `events`, лимиты |
| `src/main.rs` | Служебный маршрут `orbitfm native-host` без аргументов |
| `integrations/browser/popup.html` | Разметка панели: станция, состояние, транспорт, громкость, тема, окно |
| `integrations/browser/popup.css` | Оформление popup |
| `integrations/browser/popup.js` | `connectNative`, подписка `events`, кнопки, громкость, тема, офлайн-fallback |
| `integrations/browser/orbitfm-native-host` | Wrapper `exec "<orbitfm>" native-host` (0755) |
| `integrations/chrome/manifest.json` | MV3, фиксированный `key`, popup, `nativeMessaging` |
| `integrations/firefox/manifest.json` | MV3, gecko ID `orbitfm@modernbyte`, popup, `nativeMessaging` |
| `integrations/chrome/README.md` | Инструкция Chrome и Chromium-семейства |
| `integrations/firefox/README.md` | Инструкция Firefox |

Встроенные ассеты: `CHROME` и `FIREFOX` в `src/integration/assets.rs` — по
шесть файлов. Native-манифест не является ассетом каталога: он генерируется
установщиком, потому что содержит абсолютный путь к wrapper и зависит от
обнаруженных браузеров.

## 3. Поток данных

```text
popup.js
  │  connectNative("org.modernbyte.orbitfm")
  ▼
браузер ──проверка native-манифеста по ID──► orbitfm native-host
                                                │
                              ┌─────────────────┴─────────────────┐
                              │ команда: send_to_existing         │
                              │ события: subscribe_frames         │
                              ▼                                   ▼
                        IPC-сокет 0600 ─────────────────────► Session
```

1. Popup открывается и сразу отправляет `{"id":1,"command":"events"}`.
2. Хост подтверждает подписку и пересылает кадры без изменений.
3. Кнопки шлют `playpause`, `next`, `previous`, `stop`, `roulette`,
   `settings`, `panel`, `overlay`, `show`/`hide`.
4. Слайдер шлёт `volume 0.74` при отпускании, список темы — `theme dark`.
5. Закрытие popup закрывает порт; хост завершает подписку и выходит.

Popup не делает фонового опроса и не хранит состояние: каждый кадр `state`
самодостаточен (`status` схемы 1).

## 4. Хост

- **Фрейминг.** `u32` native-endian + UTF-8 JSON. `0`/EOF — конец, лимит
  1 МиБ в обе стороны. Входящий сверхлимитный кадр не читается: ответ
  `oversized` и выход. Исходящий сверхлимитный заменяется короткой ошибкой.
- **Разбор.** `Request { id: Option<Value>, command: String }` с
  `deny_unknown_fields`; команда разбирается `IpcCommand::parse`.
- **Allowlist.** Разрешены `toggle`, `show`, `hide`, `status`,
  `status --json`, `playpause`, `stop`, `next`, `previous`, `roulette`,
  `volume`, `volume-up`, `volume-down`, `theme`, `settings`, `panel`,
  `overlay`. Запрещены `quit`, `play`, `mode` (query и set) и `Raw`.
- **Ответы.** `{"id"?, "ok":true, "response":"…"}` или
  `{"id"?, "ok":false, "code":"…", "error":"…"}`. `id` возвращается как есть.
- **События.** Одна подписка на процесс; ack `response: "events"`, кадры
  `{"ok":true,"event":<frame>}`, конец `event: null`, ошибки с `stream`.
- **Жизненный цикл.** Отдельный current-thread runtime, без GUI и без
  cold-start. `send_to_existing` возвращает `not-running`, если приложение не
  запущено. Очередь вывода — 64 сообщения. `native-host` не принимает
  аргументов: браузерный origin отбрасывается wrapper'ом, а не используется
  для авторизации.
- **AppImage.** Установщик подставляет путь к образу (`$APPIMAGE`), а не к
  временному mount, который исчезает после выхода.

## 5. Расширение

- **Popup-only.** Нет content scripts, background/service worker,
  `host_permissions`, `externally_connectable` и `web_accessible_resources`.
  Единственное разрешение — `nativeMessaging`.
- **Идентичность.** Chrome-ID
  `lfhmfgjblkebbalfdlbmnalghhjplfbg` выводится из публичного `key` в
  манифесте; Firefox-ID — `orbitfm@modernbyte`. Native-манифесты перечисляют
  ровно эти ID.
- **Локализация.** Подписи кнопок, «ничего не играет», «показать/скрыть окно»
  берутся из `status.ui`; при недоступном приложении остаётся английский
  офлайн-текст. Popup не дублирует 11 языков.
- **Громкость.** Значение слайдера переводится в `volume 0.74`; пока
  пользователь тянет ползунок, входящие кадры не перебивают его значение.
- **Офлайн.** `not-running`, `event: null` и `stream-closed` переводят панель в
  состояние offline с кнопкой «Повторить»; GUI при этом не запускается.
- **Версия.** В манифест подставляется числовая версия без `-rc.N`; иначе
  браузер отказывается загружать расширение.

## 6. Установка

```bash
orbitfm integration install chrome    # Chrome, Chromium, Brave, Edge, Vivaldi
orbitfm integration install firefox   # Firefox
orbitfm integration install all       # включает обе браузерные цели
```

- Файлы расширения и wrapper — в
  `$XDG_DATA_HOME/orbitfm/integrations/<цель>/`.
- Native-манифест — только для обнаруженных браузеров; если браузера нет,
  установка файлов проходит, манифест пропускается с подсказкой.
- Квитанция хранит и файлы расширения, и native-манифесты; `uninstall`
  удаляет только их.
- `ORBITFM_INTEGRATION_ROOT` переносит и ассеты, и манифесты в тестовый
  корень.
- На не-Linux платформах установщик отказывает: это общая граница целей
  ADR-0037. Windows-реестр и macOS-раскладка не реализованы.
- Пользователь загружает расширение сам: Chrome — «Загрузить распакованное
  расширение» в `chrome://extensions`; Firefox — «Загрузить временное
  дополнение» в `about:debugging`. Постоянная установка Firefox требует
  подписи Mozilla.

## 7. Тесты

Автотесты (`cargo test`, 2026-10-31 — 488 passed, 5 ignored):

- `native_host::tests` — 7 тестов: allowlist блокирует и пропускает,
  строгий разбор запроса, чтение кадра и EOF, нулевая длина как EOF,
  сверхлимитный кадр без чтения payload, запись native-endian;
- `integration::tests::installs_chrome_extension_and_native_manifests` —
  6 ассетов + 5 манифестов в тестовом корне, версия числовая, wrapper
  экранирован, `allowed_origins` точный, квитанция на 11 файлов;
- `integration::tests::installs_firefox_native_manifest` — 6 + 1 файл,
  gecko ID и числовая версия;
- `integration::tests::uninstall_browser_removes_extension_and_manifests` —
  удаление 11 файлов, манифеста и квитанции без остатка;
- `node --check integrations/browser/popup.js` — синтаксис popup.

## 8. Живая приёмка

2026-10-31, Linux x86_64, niri/Wayland, запущенный экземпляр OrbitFM:

- **Chrome for Testing 153** (CDP `Extensions.loadUnpacked` с
  `--enable-unsafe-extension-debugging`): popup → wrapper → native host →
  `events`; панель показала `stopped`, «Ничего не играет», громкость 75 %,
  русские подписи; `volume 0.74` дошла до приложения и вернулась в popup,
  затем громкость восстановлена 75 %;
- **Firefox 159** (WebDriver BiDi `webExtension.install`, временное
  дополнение): тот же сценарий, тот же snapshot и round-trip громкости;
- **Установщик в подставном `HOME`:** Chrome — 6 файлов + 2 манифеста
  (google-chrome, chromium), Firefox — 6 + 1; JSON точный, ID фиксированные,
  wrapper исполняемый; `uninstall` чистый;
- **Протокол хоста** отдельным клиентом: `status --json`, `events`,
  `forbidden` (`quit`, `play`, `mode`), `not-running`, oversized, битый кадр.

Найденные и исправленные дефекты:

1. версия `0.13.0-rc.2` не проходила валидацию манифеста — добавлена
   подстановка `@BROWSER_VERSION@` и регрессионные проверки;
2. popup ждал `status` в корне сообщения, а получал конверт
   `{"ev":1,"type":"snapshot","status":…}` — разбор приведён к интерфейсу 13.

Ограничение доказательности: Brave, Edge, Vivaldi и Firefox с постоянной
подписью не запускались; Windows/macOS-манифесты не реализованы; Chromium с
нестандартным `--user-data-dir` требует ручного native-манифеста в
`<user-data-dir>/NativeMessagingHosts`.

## 9. Ссылки

- ADR-0038 — браузерное расширение и native messaging host
- ADR-0037 — единая поверхность интеграции
- ADR-0039 — поток событий состояния
- `docs/interfaces/14-native-host.md`
- `docs/modules/16-integrations.md`
- `docs/wiki/12-Браузерное-расширение.md`
