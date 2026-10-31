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
| Popup, поиск и библиотека companion-попапа, скан активной вкладки, манифесты, wrapper, installed-раскладка, native-манифесты | Правка настроек браузера, магазин расширений, подпись XPI |
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
| `integrations/browser/popup.html` | Разметка панели: шапка, карточка станции, транспорт, громкость, мини-визуализатор, вкладки «Поиск/Избранное/История», футер «Play this page» |
| `integrations/browser/popup.css` | Оформление popup токенами панели Noctalia: стекло, рамка, акцент, тёмная и светлая темы |
| `integrations/browser/popup.js` | `connectNative`, `events`, поиск с debounce, библиотека, избранное, скан вкладки, громкость, тема, офлайн-fallback |
| `integrations/browser/orbitfm-native-host` | Wrapper `exec "<orbitfm>" native-host` (0755) |
| `integrations/chrome/manifest.json` | MV3, фиксированный `key`, popup, `nativeMessaging` + `activeTab` + `scripting` |
| `integrations/firefox/manifest.json` | MV3, gecko ID `orbitfm@modernbyte`, popup, те же три разрешения |
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
4. Слайдер шлёт `volume 0.74` при отпускании, кнопки ±5 % — `volume` по шагу,
   кнопка темы — `theme dark|light|system`, сердце — `favorite`.
5. Вкладки «Избранное/История» запрашивают `library`; поиск идёт глаголом
   `search`, результат приходит кадром `search`, запуск — `play-result` или
   `play-library`.
6. «Play this page» сканирует активную вкладку, показывает кандидатов и только
   по клику шлёт `play-url`; автозапуска нет.
7. Закрытие popup закрывает порт; хост завершает подписку и выходит.

Popup не опрашивает `status`: каждый кадр `state` самодостаточен (`status`
схемы 1). Библиотека перечитывается при открытии вкладки и после изменения
избранного; поиск отменяется по поколению (`id` кадра) и таймауту 15 с.

## 4. Хост

- **Фрейминг.** `u32` native-endian + UTF-8 JSON. `0`/EOF — конец, лимит
  1 МиБ в обе стороны. Входящий сверхлимитный кадр не читается: ответ
  `oversized` и выход. Исходящий сверхлимитный заменяется короткой ошибкой.
- **Разбор.** `Request { id: Option<Value>, command: String }` с
  `deny_unknown_fields`; команда разбирается `IpcCommand::parse`.
- **Allowlist.** Разрешены `toggle`, `show`, `hide`, `status`,
  `status --json`, `playpause`, `stop`, `next`, `previous`, `roulette`,
  `volume`, `volume-up`, `volume-down`, `theme`, `settings`, `panel`,
  `overlay`, `search`, `library`, `favorite`, `play-result`, `play-library`,
  `play-url`. Запрещены `quit`, `play`, `mode` (query и set) и `Raw`. Играть
  можно только результат выдачи, элемент библиотеки или явный URL — не
  произвольный UUID.
- **Ответы.** `{"id"?, "ok":true, "response":"…"}` или
  `{"id"?, "ok":false, "code":"…", "error":"…"}`. `id` возвращается как есть.
  Ошибки исполнения (`play-result`, `play-library`, `play-url`, `favorite`,
  `library`) приходят успешным конвертом со строкой `error …`; popup
  распознаёт префикс и показывает сообщение, а не считает команду успешной.
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
  Разрешения — `nativeMessaging`, `activeTab` и `scripting`: последние два дают
  доступ только к активной вкладке и только по клику.
- **Панель.** 360 px, стеклянная карточка с рамкой и радиусом 16, акцент из
  `status.accent`, тёмная/светлая тема (system следует `prefers-color-scheme`),
  системный шрифт, inline SVG и монограмма — без сетевых картинок и шрифтов.
  Секции: шапка, now playing, транспорт (disabled по `status.can`), громкость,
  декоративный мини-визуализатор на 16 полос, вкладки, футер. Hit-target
  ≥ 36 px, `focus-visible`, `aria-live` для станции, визуализатор
  `aria-hidden`, `prefers-reduced-motion` отключает анимацию.
- **Идентичность.** Chrome-ID
  `lfhmfgjblkebbalfdlbmnalghhjplfbg` выводится из публичного `key` в
  манифесте; Firefox-ID — `orbitfm@modernbyte`. Native-манифесты перечисляют
  ровно эти ID.
- **Поиск и библиотека.** Запрос уходит с debounce 300 мс, не длиннее 128
  символов; ответ принимается только для своего поколения (`id` кадра),
  таймаут 15 с, результатов ≤ 20. Избранное и история — по 20 записей из
  `library`; запуск — `play-result`/`play-library`, сердце — `favorite`.
- **Play this page.** По явному клику popup сканирует активную вкладку
  (`og:audio`, `<audio src>`, `<source src>`, URL вкладки), приводит
  относительные адреса через `new URL`, оставляет только `http(s)`, убирает
  дубликаты и показывает ≤ 20 кандидатов. Без `scripting` остаётся только URL
  вкладки. Импорт запускает выбранный кандидат через `play-url`; ошибка
  показывается в панели.
- **Локализация.** Подписи кнопок, вкладок, «ничего не играет», поиска,
  избранного и «показать/скрыть окно» берутся из `status.ui`; при недоступном
  приложении остаётся английский офлайн-текст. Popup не дублирует 16 языков.
- **Громкость.** Слайдер отправляет `volume 0.74` при отпускании, кнопки —
  шаг ±5 %; пока пользователь тянет ползунок, входящие кадры не перебивают его
  значение.
- **Тема.** Кнопка циклически переключает system → dark → light и шлёт
  `theme <режим>`; акцент панели следует `status.accent`.
- **Офлайн.** `not-running`, `event: null`, `stream-closed` и молчание событий
  дольше 20 с переводят панель в offline; переподключение — с backoff
  (1–10 с, шесть попыток) и кнопкой «Повторить». GUI при этом не запускается.
- **Версия.** В манифест подставляется числовая версия без `-rc.N`; иначе
  браузер отказывается загружать расширение.

## 6. Установка

```bash
orbitfm integration install chrome    # Chrome, Chromium, Brave, Edge, Vivaldi
orbitfm integration install firefox   # Firefox
```

Обе цели независимы и ставятся по отдельности; массовой команды `install all`
нет.

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

2026-10-31, Linux x86_64, niri/Wayland, 0.16.0-rc.2, запущенный экземпляр
OrbitFM и локальный `python3 -m http.server` на `127.0.0.1:8765`:

- **Chrome for Testing 153** (CDP на `9224`, `--load-extension` и
  `--disable-features=DisableLoadExtensionCommandLineSwitch`; native-манифест
  в `<user-data-dir>/NativeMessagingHosts`): попап подключился к приложению,
  показал Hit FM, русские подписи, бейдж «в эфире», громкость 75 %;
  поиск `jazz` дал 20 результатов, клик по первому выполнил `play-result` и
  станция зазвучала; сердце переключило `favorite` (список избранного
  обновился), «История» сыграла внешний поток через `play-library`
  (`source: stream`); кнопки громкости 75 → 85 → 80 и стрелка на слайдере
  75 → 76 дошли до приложения, затем громкость восстановлена 75 %;
  тема прошла цикл dark → light → system → dark.
- **Настоящий action-попап**: в тестовом профиле назначен `Ctrl+Shift+Y`,
  открытие через `wtype` выдало `activeTab`. «Играть эту страницу» на
  локальной странице нашла `og:audio` и предложила два кандидата —
  `127.0.0.1/page.html` и `127.0.0.1/tone.wav`; выбор страницы импортировал
  поток `url:a6ea…` (`source: stream`). На странице без кандидатов попал
  локализованный текст «На этой странице не нашлось аудио.».
- **Chrome 156 (пользовательский профиль)**: расширение уже зарегистрировано
  по установленному каталогу, страница попапа отрисовалась с Hit FM и
  русскими подписями. Брендовый Chrome 156 больше не принимает
  `--load-extension`, поэтому интерактивный прогон выполнен на CfT 153.
- **Офлайн/повтор**: после `orbitfm quit` попап перешёл в `offline` с текстом
  «OrbitFM не запущен» и кнопкой «Повторить»; после перезапуска приложения
  повтор подключился, состояние стало «остановлено».
- **Доступность**: у иконок русские `aria-label`, слайдер управляется
  стрелками, станция объявляется через `aria-live="polite"`, визуализатор
  `aria-hidden`; эмуляция `prefers-reduced-motion: reduce` через CDP дала
  `animation-duration: 0s` и статичные полосы.
- **Установщик**: 14 целей 0.16.0-rc.2, Chrome/Firefox — 6 файлов и
  native-манифест (7 записей в квитанции), GNOME/Waybar — 4 файла; статусы
  и квитанции совпадают.

Найденные и исправленные дефекты:

1. версия `0.13.0-rc.2` не проходила валидацию манифеста — добавлена
   подстановка `@BROWSER_VERSION@` и регрессионные проверки (0.15);
2. popup ждал `status` в корне сообщения, а получал конверт
   `{"ev":1,"type":"snapshot","status":…}` — разбор приведён к интерфейсу 13
   (0.15);
3. бейдж состояния и сообщения импорта дублировали английские строки в JS —
   добавлены аддитивные `ui.state_*`, `ui.no_audio`, `ui.importing`,
   `ui.stream_rejected`, `ui.close`, тема в `aria-label` берётся из
   `ui.theme_*` (0.16.0-rc.2).

Ограничение доказательности: Firefox в этом прогоне не перезапускался (API
попапа не менялись с проверки 0.15); Brave, Edge, Vivaldi, Windows и macOS не
запускались; Chromium с нестандартным `--user-data-dir` требует
native-манифест в `<user-data-dir>/NativeMessagingHosts`; брендовый Chrome
запрещает `--load-extension` для распакованных расширений.

## 9. Ссылки

- ADR-0038 — браузерное расширение и native messaging host
- ADR-0037 — единая поверхность интеграции
- ADR-0039 — поток событий состояния
- `docs/interfaces/14-native-host.md`
- `docs/modules/16-integrations.md`
- `docs/wiki/12-Браузерное-расширение.md`
