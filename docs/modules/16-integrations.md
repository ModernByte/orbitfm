# Модуль: Интеграции оболочек и баров (`src/integration/`, `integrations/`)

**Путь в коде:** `src/integration/{mod,assets}.rs`, `integrations/`  
**Ответственность:** CLI `orbitfm integration status|install|uninstall`, встроенные
ассеты оболочек, установка и удаление в пользовательские каталоги, квитанции и
печать шагов подключения. Модуль не запускает GUI, не общается с IPC и не меняет
чужие конфиги.  
**Зависимости:** `anyhow`, `serde`, `serde_json`, `std::fs`, `tempfile` (тесты);
ассеты встроены через `include_str!`.  
**Решение:** ADR-0037 «Единая поверхность интеграции — JSON-статус, CLI-команды
и установщик»

---

## 1. Назначение и границы

Релиз состоит ровно из четырёх артефактов (AppImage, MSI, DMG, `.plasmoid`),
поэтому прочие плагины и сниппеты нельзя приложить отдельным архивом; Plasma 6 —
исключение со штатным форматом KPackage. Установщик живёт внутри бинарника: он раскладывает
текстовые ассеты по XDG-каталогам пользователя, пишет квитанцию и печатает
готовый фрагмент конфигурации. Конфиги Waybar, Polybar, i3, sway, Quickshell,
Plasma и GNOME установщик не редактирует.

| Живёт в модуле | Не живёт в модуле |
|---|---|
| Разбор `integration`, цели, обзор `status` по всем целям, рендер ассетов, проверки путей, квитанции, установка/удаление одной цели, `status` | IPC-протокол (`src/ipc/`), состояние эфира и подписи (`Session`, `i18n`), работа GUI и трея |
| Список целей, пути XDG, шаги подключения | Запуск оболочки, включение виджета, правка конфигураций оболочки |

`parse_invocation` в `src/main.rs` перехватывает `integration` до создания
tokio-runtime и до маршрутизации IPC, поэтому команда работает без запущенного
экземпляра и никогда не открывает второе окно. На не-Linux платформах
`ensure_applicable` завершает установку с понятной ошибкой: цели рассчитаны на
Linux-оболочки.

## 2. Состав пакета

| Файл | Ответственность |
|------|-----------------|
| `src/integration/mod.rs` | `Target`, `Scope`, `Command`, `parse`, `run`; `Layout` XDG и `ORBITFM_INTEGRATION_ROOT`; рендер, квитанции, установка, удаление, инспекция |
| `src/integration/assets.rs` | Таблица встроенных ассетов: путь установки, содержимое, исполняемость, режим подстановки, `asset|generated` |
| `integrations/common/orbitfm-bar.sh` | Мост для баров: чтение `orbitfm status`, форматы text/waybar/polybar/i3blocks/swaybar/i3status-rust/genmon, клики |
| `integrations/common/bar-README.md` | Общая справка по бару |
| `integrations/{plasma6,gnome,omarchy}/` | Метаданные и код виджетов: Plasmoid, GNOME Shell extension с `stylesheet.css`, Quickshell `BarWidget` |
| `integrations/{waybar,polybar,i3blocks,swaybar,i3status-rust}/` | Сниппеты конфигураций; у Waybar — ещё `style.css` с токенами панели |
| `integrations/{xfce,cinnamon,mate,lxqt}/usage.txt` | Инструкции для апплетов вывода команды |
| `integrations/browser/` | Общий popup и wrapper native host (модуль 17) |
| `integrations/{chrome,firefox}/` | Манифесты и README браузерных целей |

Для бар-целей установка — `orbitfm-bar` (0755), файл конфигурации и `README.md`
(0644); Waybar дополнительно получает `style.css`. Для оболочек — метаданные,
код виджета и `README.md`; GNOME дополнительно получает `stylesheet.css`,
который Shell загружает из каталога расширения автоматически. Итого: Plasma 6 и
Omarchy — 3 файла, GNOME и Waybar — 4, Polybar, i3blocks, swaybar,
i3status-rust и апплеты XFCE/Cinnamon/MATE/LXQt — 3. Браузерные цели
(`chrome`, `firefox`) устанавливают шесть файлов расширения и дополнительно
native-манифест в каждый обнаруженный браузер; их раскладка и протокол описаны
в модуле 17.

## 3. Ассеты и подстановки

`assets(target)` возвращает `&'static [Asset]`; пути ассетов проверяются
`ensure_relative` (только обычные компоненты). Подстановки выполняет
`render_content`:

- `@VERSION@` заменяется всегда — версия из `CARGO_PKG_VERSION`;
- `Escape::Shell` подставляет `@ORBITFM@` и `@BAR_SCRIPT@` в одинарных кавычках
  (`'…'` с экранированием внутренних кавычек) — для shell, JSON-строк и TOML;
- `Escape::Quote` экранирует `\` и `"` без добавления кавычек — для QML/JS, где
  подстановка стоит внутри строкового литерала;
- `Escape::None` не подставляет пути — для README и метаданных.

Подписи виджетов не дублируют каталог на 16 языках: Plasma 6, GNOME Shell и
Omarchy читают `ui` из `orbitfm status --json` (ADR-0037). Бар-скрипт для
пустого эфира берёт `ui.nothing_playing` из того же JSON, поэтому язык бара
совпадает с языком приложения.

Оформление виджетов следует токенам выпадающей панели Noctalia: стеклянная
карточка с мягкой рамкой, монограмма станции, точка состояния и акцент из
`status.accent` (ADR-0037, ревизия 0.16). Plasma 6 показывает компакт-пилюлю и
полную карточку с транспортом, слайдером громкости, темой, избранным и
homepage; GNOME — карточку в меню индикатора и собственный `stylesheet.css`;
Omarchy — глиф состояния и tooltip без сторонних API Quickshell. Бар-скрипт
отдаёт Waybar `text`/`alt`/`tooltip`/`class`, swaybar — цвет панели, остальным
— глиф и имя; `Volume · NN%` есть в tooltip всех режимов.

## 4. Квитанция и инспекция

Квитанция схемы 1 лежит в `$XDG_DATA_HOME/orbitfm/integrations/<цель>.json`:

```json
{
  "schema": 1,
  "target": "waybar",
  "version": "0.14.0-rc.1",
  "binary": "/home/user/.local/bin/orbitfm",
  "files": [
    {"path": "…/orbitfm-bar", "kind": "generated", "hash": "…"},
    {"path": "…/config.jsonc", "kind": "generated", "hash": "…"},
    {"path": "…/README.md", "kind": "asset", "hash": "…"}
  ]
}
```

`kind` отличает сгенерированный файл (путь к бинарнику/версия) от статического
ассета. Хеш — FNV-1a 64 бита; он не является криптографической подписью, а
служит только для обнаружения ручной правки. `status` печатает одно из
состояний: `не установлено`, `установлено <версия> (N файлов)`,
`изменено пользователем: …`, `отсутствуют файлы: …`.

При чтении квитанции проверяются схема, имя цели и каждый путь: абсолютный,
без `..` и строго внутри каталога установки. Квитанция пишется во временный
`*.json.tmp` с последующим `rename`, чтобы не оставить نصف записанный файл.

## 5. Безопасность установки и удаления

- Установщик не требует root и пишет только в XDG-каталоги пользователя.
- Символическая ссылка на месте ассета не перезаписывается без `--force`;
  после `--force` ссылка удаляется и создаётся обычный файл, а не запись по
  чужому пути.
- Существующий файл, которого нет в старой квитанции, считается чужим и
  требует `--force`.
- Файл из старой квитанции с другим хешем считается изменённым пользователем;
  перезапись и удаление требуют `--force`.
- Удаление трогает только файлы из квитанции. Пользовательские файлы рядом
  остаются; пустые каталоги установки вычищаются до корня цели, но не выше.
- `ORBITFM_INTEGRATION_ROOT` подменяет XDG-корни для тестов; путь обязан быть
  абсолютным. Раскладка: `<root>/targets/<цель>` и `<root>/receipts/<цель>.json`.

## 6. Цели и форматы

| Цель | Каталог установки | Подключение |
|---|---|---|
| `plasma6` | `$XDG_DATA_HOME/plasma/plasmoids/org.modernbyte.orbitfm` | `kbuildsycoca6 --noincremental`, добавить виджет |
| `gnome` | `$XDG_DATA_HOME/gnome-shell/extensions/orbitfm@modernbyte` | `gnome-extensions enable orbitfm@modernbyte` |
| `omarchy` | `$XDG_CONFIG_HOME/omarchy/plugins/io.github.modernbyte.orbitfm` | `omarchy-shell shell rescanPlugins`, включить виджет |
| `waybar` | `$XDG_DATA_HOME/orbitfm/integrations/waybar` | модуль `custom/orbitfm` |
| `polybar` | `$XDG_DATA_HOME/orbitfm/integrations/polybar` | `[module/orbitfm]` |
| `i3blocks` | `$XDG_DATA_HOME/orbitfm/integrations/i3blocks` | секция `[orbitfm]` |
| `swaybar` | `$XDG_DATA_HOME/orbitfm/integrations/swaybar` | `status_command` |
| `i3status-rust` | `$XDG_DATA_HOME/orbitfm/integrations/i3status-rust` | `[[block]]` + `[[block.click]]` |
| `xfce` | `$XDG_DATA_HOME/orbitfm/integrations/xfce` | Genmon, `orbitfm-bar genmon` |
| `cinnamon` | `$XDG_DATA_HOME/orbitfm/integrations/cinnamon` | апплет вывода команды |
| `mate` | `$XDG_DATA_HOME/orbitfm/integrations/mate` | апплет вывода команды |
| `lxqt` | `$XDG_DATA_HOME/orbitfm/integrations/lxqt` | Command Output / Custom Command |
| `chrome` | `$XDG_DATA_HOME/orbitfm/integrations/chrome` + `$XDG_CONFIG_HOME/<браузер>/NativeMessagingHosts` | загрузить `extension` распакованным; см. модуль 17 |
| `firefox` | `$XDG_DATA_HOME/orbitfm/integrations/firefox` + `~/.mozilla/native-messaging-hosts` | временное дополнение; см. модуль 17 |

Цели ставятся по одной; `status` без цели — обзор всех квитанций. Для Plasma 6
дополнительно собирается `OrbitFM-<версия>-plasma.plasmoid`
(`packaging/linux/build-plasmoid.py`): детерминированный ZIP с `metadata.json`
и `contents/ui/main.qml`, где `@VERSION@` и `@ORBITFM@` подставлены. Архив
устанавливается через `kpackagetool6 -t Plasma/Applet -i` или «Установить из
файла» и вызывает `orbitfm` из `PATH`; прямой `integration install plasma6`
по-прежнему подставляет абсолютный путь.

Бар-скрипт поддерживает режимы `text`, `polybar`, `waybar`, `i3blocks`,
`swaybar`, `i3status-rust`, `genmon`. Интервал опроса — 2 с, переопределяется
`ORBITFM_BAR_INTERVAL`. Клики Waybar/Polybar/i3blocks/swaybar: ЛКМ —
`playpause`, средняя — `stop`, ПКМ — `panel`, колесо — `volume-up`/`volume-down`.
Глифы состояния: ▶ играет, ⏸ пауза, … буферизация/переподключение, ⚠ ошибка,
◍ остановлено или станции нет; классы — `playing`, `paused`, `loading`, `error`,
`stopped`. XFCE genmon поддерживает только клик по тексту; для
Cinnamon/MATE/LXQt управление выносится на отдельные кнопки/ярлыки, потому что
их апплеты вывода команды кликов не обрабатывают.

`swaybar` — поток i3bar: заголовок `{"version":1,"click_events":true}`, затем
бесконечный массив кадров; клики приходят в stdin и складываются фоновым
читателем в файл, чтобы основной цикл не блокировался. По SIGTERM/SIGINT
скрипт удаляет временный каталог и завершается.

## 7. Ошибки и коды возврата

`run` возвращает `anyhow::Result`: `install`/`uninstall` принимают ровно одну
цель, поэтому ошибка цели сразу даёт ненулевой код; `status` без цели
показывает обзор и терпит отсутствие установки. Ошибки установки печатаются в
stderr. `status` возвращает ошибку, только если не удалось прочитать квитанцию
(битая схема, чужой путь), а не из-за отсутствия установки.

## 8. Тесты и доказательность

Автотесты (`cargo test`, модуль `integration::tests`): разбор целей и `--force`,
установка и квитанция, идемпотентность, отказ при изменённом файле и успех с
`--force`, удаление только файлов квитанции, отказ при изменённом файле при
удалении, `status` для отсутствующего файла, отказ при пути вне каталога.

Живая приёмка 2026-10-31 (niri/Wayland, Linux x86_64):

- установка целей по одной во временный корень и в подставной XDG (Plasma,
  GNOME, Omarchy, бары) — по 3–4 файла на цель (GNOME и Waybar — 4), квитанции
  читаются, `status` показывает `установлено`; массовая команда отклоняется;
- повторная установка идемпотентна; ручная правка и симлинк отклоняются,
  `--force` восстанавливает файл, `/etc/hostname` не изменился;
- удаление изменённого файла без `--force` отклоняется, с `--force` проходит;
  после удаления каждой цели по квитанции файлов не осталось;
- вывод `orbitfm-bar` проверен в режимах text/polybar/waybar/i3blocks/
  i3status-rust/genmon; JSON разобран Python, XML — ElementTree; поток swaybar
  принял клики колеса и восстановил громкость, по TERM завершился без
  временных каталогов;
- `qmllint`/`qmlformat` разбирают QML, `node --check` — `extension.js`,
  `sh -n` — бар-скрипт.

Браузерные цели проверены отдельно: установщик в подставном `HOME` ставит
6 файлов расширения и native-манифесты для обнаруженных браузеров, `uninstall`
удаляет их по квитанции; протокол и живая приёмка Chrome/Firefox — в модуле 17.
Локальная установка 0.16.0-rc.1 разложила все 14 целей в реальный `$HOME`
командами по одной: Chrome/Firefox — по 6 файлов и native-манифесту (7 записей
в квитанции), GNOME/Waybar — по 4 файла, `status` подтвердил «установлено».
Сборка `.plasmoid` проверяется `unzip -l` и релизным workflow; сам формат
KPackage на машине приёмки не запускался.

Ограничение доказательности: Plasma 6, GNOME Shell, Omarchy/Quickshell,
Waybar, Polybar, i3blocks, swaybar, i3status-rust, XFCE, Cinnamon, MATE и LXQt
на машине приёмки не установлены. Их UI не запускался; проверены установщик,
форматы вывода, синтаксис ассетов и API по документации, а не живой виджет в
конкретной оболочке. Это тот же уровень, что «собрано и протестировано в CI», а
не «проверено в runtime» для каждой оболочки.

## Ссылки

- ADR-0037 — единая поверхность интеграции
- ADR-0038 — браузерное расширение и native messaging host
- ADR-0039 — поток событий `orbitfm events`
- `docs/interfaces/05-ipc.md` — `status --json` и IPC-глаголы
- `docs/modules/17-browser-companion.md` — браузерные цели
- `docs/wiki/11-Интеграции-оболочек.md` — пользовательская инструкция
