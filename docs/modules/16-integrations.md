# Модуль: Интеграции оболочек и баров (`src/integration/`, `integrations/`)

**Путь в коде:** `src/integration/{mod,assets,status,reload,waybar,noctalia,wm}.rs`,
`integrations/`  
**Ответственность:** CLI `orbitfm integration status|install|uninstall` по 17 целям,
встроенные ассеты оболочек, установка и удаление в пользовательские каталоги,
квитанции, печать шагов подключения, снимок состояния для окна «Интеграции»,
CST-правка Waybar, перезагрузка баров, регистрация git-источника Noctalia и
правила окон niri и Hyprland. Модуль не запускает GUI и не общается с IPC. Из
чужих конфигов правится ровно одна строка — `include` у niri, с копией
`config.kdl.orbitfm.bak` и снятием по квитанции; конфиг Waybar меняется только
явной кнопкой «Включить в конфиге» в GUI (ADR-0042, ADR-0051 § 5). Внешним
оболочкам командуют только фиксированным argv без shell.  
**Зависимости:** `anyhow`, `serde`, `serde_json`, `jsonc-parser`, `libc`,
`tokio` (фоновая операция), `std::fs`, `tempfile` (тесты); ассеты встроены через
`include_str!`.  
**Решение:** ADR-0037 «Единая поверхность интеграции — JSON-статус, CLI-команды
и установщик», ADR-0042 «Управление интеграциями из GUI», ADR-0051 «Noctalia и
правила окон как цели интеграции»

---

## 1. Назначение и границы

Релиз состоит ровно из четырёх артефактов (AppImage, MSI, DMG, `.plasmoid`),
поэтому прочие плагины и сниппеты нельзя приложить отдельным архивом; Plasma 6 —
исключение со штатным форматом KPackage. Установщик живёт внутри бинарника: он раскладывает
текстовые ассеты по XDG-каталогам пользователя, пишет квитанцию и печатает
готовый фрагмент конфигурации. CLI-установщик конфиги Waybar, Polybar, i3, sway,
Quickshell, Plasma, GNOME и Hyprland не редактирует. Правится ровно одна чужая
строка — `include` в `~/.config/niri/config.kdl` (цель `niri`, ADR-0051 § 2);
группа «Правила окон» ограничена ею одной. Второе исключение — GUI-кнопка «Включить
в конфиге» для Waybar: она через CST добавляет `custom/orbitfm` в `modules-right`,
сохраняя комментарии и форматирование (ADR-0042 § 2). Команды оболочкам
(`noctalia msg plugins …`, `hyprctl keyword windowrulev2 …`) собираются только
кодом, без подстановки пользовательских строк и без shell (ADR-0051 § 5).

| Живёт в модуле | Не живёт в модуле |
|---|---|
| Разбор `integration`, 17 целей и четыре группы, обзор `status` по всем целям, рендер ассетов, проверки путей, квитанции, установка/удаление одной цели, `status` | IPC-протокол (`src/ipc/`), состояние эфира и подписи (`Session`, `i18n`), отрисовка окна и трея |
| Снимок `TargetStatus` для GUI, фоновая операция установки/удаления, CST-правка Waybar, перезагрузка баров, обнаружение композитора и runtime-правила Hyprland, регистрация git-источника Noctalia, строка `include` у niri | Запуск самой оболочки и загрузка плагина (её делает Noctalia), произвольная правка конфигураций и произвольные argv; страница рисуется в `src/ui/settings_panel.rs` (модуль 14) |

`parse_invocation` в `src/main.rs` перехватывает `integration` до создания
tokio-runtime и до маршрутизации IPC, поэтому команда работает без запущенного
экземпляра и никогда не открывает второе окно. На не-Linux платформах
`ensure_applicable` завершает установку с понятной ошибкой: цели рассчитаны на
Linux-оболочки.

## 2. Состав пакета

| Файл | Ответственность |
|------|-----------------|
| `src/integration/mod.rs` | `Target`, `TargetGroup`, `Scope`, `Command`, `parse`, `run`; `Layout` XDG и `ORBITFM_INTEGRATION_ROOT`; рендер, квитанции, установка, удаление, инспекция, обнаружение целей |
| `src/integration/assets.rs` | Таблица встроенных ассетов: путь установки, содержимое, исполняемость, режим подстановки, `asset|generated` |
| `src/integration/status.rs` | `IntegrationPanel`: снимок `TargetStatus`, состояния `InstallState`, фоновая операция `PanelAction`, события `PanelEvent` |
| `src/integration/waybar.rs` | Поиск `config.jsonc`/`config`/`config.json`, чтение состояния, CST-вставка `custom/orbitfm`, бэкап `.orbitfm.bak` |
| `src/integration/reload.rs` | `waybar-msg` → адресный `SIGUSR2` процессам `waybar` → просьба перезапустить вручную; `polybar-msg` |
| `src/integration/noctalia.rs` | Построение фиксированного argv `noctalia msg plugins {list, source list, source add, source remove, enable, disable}`, разбор ответов оболочки, `Flags { source_added, was_enabled }`, регистрация и снятие источника (ADR-0051 § 2) |
| `src/integration/wm.rs` | `Desktop::detect`, `RuntimeRules`, `hyprctl keyword` с фиксированными аргументами; пути и тексты правил целей (`NIRI_RULE_FILE`, `HYPRLAND_RULE_FILE`, `NIRI_INCLUDE`, `HYPRLAND_SOURCE`), `add_include`/`remove_include` для конфига niri |
| `integrations/common/orbitfm-bar.sh` | Мост для баров: чтение `orbitfm status`, форматы text/waybar/polybar/i3blocks/swaybar/i3status-rust/genmon, клики |
| `integrations/common/bar-README.md` | Общая справка по бару |
| `integrations/{plasma6,gnome,omarchy}/` | Метаданные и код виджетов: Plasmoid, GNOME Shell extension с `stylesheet.css`, Quickshell `BarWidget` |
| `integrations/{waybar,polybar,i3blocks,swaybar,i3status-rust}/` | Сниппеты конфигураций; у Waybar — ещё `style.css` с токенами панели |
| `integrations/{xfce,cinnamon,mate,lxqt}/usage.txt` | Инструкции для апплетов вывода команды |
| `integrations/browser/` | Общий popup и wrapper native host (модуль 17) |
| `integrations/{chrome,firefox}/` | Манифесты и README браузерных целей |
| `wm-configs/orbitfm-niri.kdl`, `wm-configs/orbitfm-hyprland.conf` | Тексты правил окон целей `niri` и `hyprland`; встроены в бинарник через `include_str!` и показываются мастером первого запуска (ADR-0041) |

Для бар-целей установка — `orbitfm-bar` (0755), файл конфигурации и `README.md`
(0644); Waybar дополнительно получает `style.css`. Для оболочек — метаданные,
код виджета и `README.md`; GNOME дополнительно получает `stylesheet.css`,
который Shell загружает из каталога расширения автоматически. Итого: Plasma 6 и
Omarchy — 3 файла, GNOME и Waybar — 4, Polybar, i3blocks, swaybar,
i3status-rust и апплеты XFCE/Cinnamon/MATE/LXQt — 3. У целей группы «Правила
окон» один файл правил: `cfg/orbitfm.kdl` у niri и `orbitfm.conf` у Hyprland;
у цели `noctalia` файлов ноль — её след живёт в состоянии оболочки (§ 4.5).
Браузерные цели
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
homepage; GNOME — карточку в меню индикатора, подменю таймера сна (пункты
`ui.sleep_off` и шаблон `ui.sleep_minutes` с `{minutes}`, отметка активного
варианта по `status.sleep`, ADR-0046) и собственный `stylesheet.css`;
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
без `..` и строго внутри каталога установки; для `dirs` дополнительно разрешён
родитель корня цели — `ensure_dir` помечает уровни, которых до установки не было
(например, каталог целей при первой установке). Квитанция пишется во временный
`*.json.tmp` с последующим `rename`, чтобы не оставить половину записанного файла.

Схема остаётся `1`, поля цели `niri` и `noctalia` аддитивны и читаются под
`#[serde(default)]` (ADR-0051 § 4):

- `dirs[]` — каталоги, которые создал установщик. При снятии удаляется только
  опустевший каталог, deepest-first; непустой остаётся как есть;
- `lines[]` — пары «файл + точная строка», добавленные в чужой конфиг. Снятие
  убирает ровно эти строки и больше ничего;
- `source_added` и `was_enabled` — признаки `noctalia`: добавляли ли источник
  `orbitfm` мы и был ли виджет включён до установки. На них опирается снятие,
  чтобы не удалить чужой источник и не выключить виджет, который пользователь
  включал сам.

Квитанция цели `niri` выглядит так:

```json
{
  "schema": 1,
  "target": "niri",
  "version": "0.20.0-rc.1",
  "binary": "/home/user/.local/bin/orbitfm",
  "files": [{"path": "…/.config/niri/cfg/orbitfm.kdl", "kind": "asset", "hash": "…"}],
  "dirs": ["…/.config/niri/cfg"],
  "lines": [
    {"path": "…/.config/niri/config.kdl", "line": "include \"./cfg/orbitfm.kdl\""}
  ]
}
```

Файл `config.kdl` лежит вне каталога установки, поэтому путь строки проверяется
на принадлежность цели так же, как пути файлов, а правка допускается только в
обычный файл: символическая ссылка отклоняется до первой записи.

### 4.1. Снимок для страницы «Интеграции»

`IntegrationPanel` собирает `Vec<TargetStatus>` один раз при открытии категории,
а не на каждый кадр. Для каждой цели хранятся доступность (`Detected`/
`NotDetected`), состояние квитанции (`NotInstalled`, `Installed`,
`UpdateAvailable`, `Problem`), техническая подробность для hover, найденные
браузеры и `WaybarStatus` (путь, включён ли модуль, ошибка разбора). Страница
только читает этот снимок и возвращает `UiAction` (ADR-0042 § 1).

Для целей «Правил окон» и `noctalia` пустая подробность доказывается заметкой
о шаге, который остался за человеком (`handoff_note`): путь файла правил и строка
`source = ~/.config/hypr/orbitfm.conf` у Hyprland, путь файла правил, строка
`include` и её конфиг у niri — без утверждения, кто именно эту строку добавил, —
зарегистрированный источник и `modernbyte/orbitfm` у
Noctalia. Заметка появляется только при `Installed` — снятая цель её не
дублирует (ADR-0051 § 2).

Первичная подпись строки — не прямое отражение `InstallState`: для `NotDetected`
presentation-слой показывает `не обнаружено` приглушённым цветом независимо от
квитанции, для обнаруженной цели — подпись и тон берутся из состояния квитанции.
Правило выбирает чистая `state_caption` в `src/ui/settings_panel.rs` (ADR-0042 § 1,
ред. 2). Данные снимка при этом не меняются: наличие квитанции остаётся основанием
для установки и удаления, а заметка «не обнаружено» под строкой больше не дублирует
первичную подпись.

Установка, удаление, включение Waybar и перезагрузка выполняются в tokio-фоне
по одной операции за раз; завершение будит GUI через `WakeFn`, после чего снимок
обновляется. Установка из GUI идёт через тот же `install_one`, что и CLI, и
подставляет абсолютный путь текущего бинарника (`current_binary`): native-манифест
Chrome/Firefox получает путь к реальному файлу, а не имя из `PATH`. Пока
операция идёт, все строки страницы недоступны.

### 4.2. Правка Waybar через CST

Кнопка «Включить в конфиге» работает только с Waybar и только с массивом
`modules-right`. Регулярные выражения и `serde_json` не применяются: файл
разбирается `jsonc-parser` в CST, узел вставляется в дерево, результат перед
записью разбирается заново и только затем атомарно заменяет файл. Комментарии,
порядок ключей и форматирование сохраняются.

Гарантии:

- ищется первый существующий из `config.jsonc`, `config`, `config.json` — тот
  же порядок, что у Waybar;
- нет корневого объекта или массива `modules-right` — ошибка, файл не меняется;
- существующее определение `custom/orbitfm` пользователя не перезаписывается,
  повторное включение идемпотентно;
- символическая ссылка на конфиге отклоняется, запись по чужому пути не идёт;
- первая резервная копия `<имя>.orbitfm.bak` создаётся один раз и никогда не
  обновляется; при повторных правках она остаётся исходной точкой отката;
- ошибка разбора показывается в строке цели, а не роняет страницу.

CLI-команда `integration install waybar` по-прежнему только раскладывает ассеты
и печатает фрагмент конфигурации; она не трогает `config.jsonc`.

### 4.3. Перезагрузка баров без `killall`

Waybar сначала получает документированную команду `waybar-msg cmd reload`. Если
её нет, `reload.rs` читает `/proc/*/comm` и посылает `SIGUSR2` только процессам
с именем `waybar`; сигнал перезагружает конфиг, а не завершает процесс. Polybar
перезагружается `polybar-msg cmd restart`. Если оба пути недоступны, кнопка
возвращает `Manual` и UI просит перезапустить бар вручную. `killall` и `pkill`
не используются ни в одном пути (ADR-0042 § 3).

### 4.4. Окружение и правила окна

`wm.rs` определяет композитор по `HYPRLAND_INSTANCE_SIGNATURE`, `NIRI_SOCKET`,
`XDG_CURRENT_DESKTOP` и `DESKTOP_SESSION` (переменные композитора приоритетнее
имён рабочего стола). Для Hyprland мастер вызывает `hyprctl keyword` с
фиксированным набором из 14 правил, без shell и без правки конфига. Для niri
показывает KDL-блок из `wm-configs/orbitfm-niri.kdl` с кнопкой «Копировать»:
runtime-интерфейса для window-rule у niri нет. KDE, GNOME и прочие окружения
получают `Unsupported` и никаких действий (ADR-0041 § 2). Мастер первого запуска
конфиги пользователя по-прежнему не редактирует: точечная правка одной строки
появилась только у цели `niri` установщика (§ 4.5, ADR-0051).

### 4.5. Цели `noctalia`, `niri` и `hyprland`

Три цели группы «Правила окон» и цели `noctalia` из «Окружений» ставят то, что
раньше делалось двумя командами и `cp` из README (ADR-0051 § 1–2). Шаг цели
идёт после раскладки файлов (`apply_target_step`); у остальных целей записи
остаются чисто файловыми.

**`noctalia`** файлов не раскладывает вообще: цель регистрирует официальный
git-источник проекта средствами самой оболочки и включает виджет —
`noctalia msg plugins source add orbitfm git https://github.com/ModernByte/orbitfm`
и `noctalia msg plugins enable modernbyte/orbitfm`. Загрузку плагина делает
оболочка, установщик не качает ничего. До включения читаются `plugins list` и
`source list`: по ним решаются `was_enabled` и `source_added`, а `source add`
пропускается, если источник `orbitfm` уже зарегистрирован — переустановка на
машине владельца просто включает виджет. Если `noctalia` нет в `PATH` или
оболочка не ответила, установка возвращает причину в `detail` и ничего не
пишет: без оболочки регистрировать нечего.

Снятие — `disable modernbyte/orbitfm`, затем `source remove orbitfm` только за
наш источник (`source_added`), после чего прежняя включённость возвращается
(`enable`), если виджет всё ещё доступен из чужого источника. Отказ оболочки
снятие не отменяет: причина уходит в заметку строки `внимание: …`.

**`niri`** пишет `~/.config/niri/cfg/orbitfm.kdl` (текст —
`wm-configs/orbitfm-niri.kdl`) и добавляет в `~/.config/niri/config.kdl` ровно
одну строку `include "./cfg/orbitfm.kdl"`, если такой там ещё нет. Перед первой
правкой создаётся копия `config.kdl.orbitfm.bak` — тот же приём, что у правки
Waybar (§ 4.2). Отсутствие `config.kdl` — ошибка установки с указанием пути,
символическая ссылка тоже отклоняется; запись атомарна и проверяется
перечитыванием (строка обязана появиться ровно одна). Снятие убирает ровно эту
целую строку (сравнение по всей строке, не по подстроке), затем файл правил и
свой каталог `cfg`. Строку, добавленную пользователем до установки, снятие не
трогает — но и молчать не вправе: файл правил исчезает, а `include` на него
остаётся и niri не разберёт конфиг, поэтому `uninstall_one` перечитывает
`config.kdl` и добавляет в заметку удаления путь конфига и саму оставшуюся
строку. Отдельной
команды перезагрузки нет: niri 26.04+ перечитывает конфиг сам.

**`hyprland`** пишет `~/.config/hypr/orbitfm.conf` (текст —
`wm-configs/orbitfm-hyprland.conf`) и применяет те же 14 правил на текущий сеанс
через `hyprctl keyword windowrulev2 <правило из кода>` — по одному вызову на
правило, без shell. `hyprland.conf` установщик **не** правит: ветка Hyprland
переводит конфиг на Lua, и верифицированного синтаксиса include на момент
решения нет (ADR-0051, «Альтернативы»). Поэтому строку подключения
`source = ~/.config/hypr/orbitfm.conf` показывают в заметке строки страницы
«Интеграции» и в выводе CLI после установки. Невозможность применить правила на
сеанс целью не считается: файл остаётся установленным, а причина уходит в
заметку.

### 4.6. Обнаружение целей

Обнаружение обязано смотреть на оболочку, а не на собственную запись
установщика (ADR-0051 § 3):

| Цель | Считается обнаруженной, если |
|---|---|
| `noctalia` | `noctalia` в `PATH` или `~/.local/state/noctalia` — каталог |
| `niri` | `niri` в `PATH` или `~/.config/niri/config.kdl` — файл |
| `hyprland` | `hyprland` либо `hyprctl` в `PATH` или `~/.config/hypr` — каталог |

Файлы правил пишутся только в каталоги, принадлежащие оболочке
(`~/.config/niri`, `~/.config/hypr`), а `noctalia` файлов не создаёт вовсе.
Каталоги, которые создал установщик, помечаются в `dirs` и снимаются при
`uninstall`, если к снятию опустели: тот же механизм освобождает от обманки и
старые цели `omarchy`, `gnome`, `plasma6`.

### 4.7. Вывод установки: шаги подключения

`install` печатает заголовок `Установлено: <цель> (N файлов)`, затем каталог (если
он есть), путь квитанции, заметку `внимание: …` при незавершённом действии
оболочки и блок шагов конкретной цели. Для целей `noctalia`, `niri` и `hyprland`
файловых сниппетов больше нет, поэтому блок печатает суть:

- `Noctalia` — имя источника, добавлен ли он этой установкой, его URL, состояние
  виджета `modernbyte/orbitfm` (включён; был включён и до установки) и команда
  обновления `noctalia msg plugins update orbitfm`;
- `niri` — путь файла правил, а дальше вывод смотрит в квитанцию: если строку
  `include "./cfg/orbitfm.kdl"` добавил установщик, печатаются сама строка, путь
  конфига, путь копии `config.kdl.orbitfm.bak` и указание, что строку и файл
  снимает `orbitfm integration uninstall niri`; если подключение было ручным,
  установки строки в квитанции нет, и печатается «добавили её не мы, копии
  `config.kdl` установка не делала» плюс предупреждение, что снятие уберёт файл
  правил и оставит в конфиге нерабочий `include` — строку убирает пользователь.
  В обоих случаях добавляется заметка про автоперечитывание niri 26.04+;
- `Hyprland` — путь файла правил, готовая строка `source = ~/.config/hypr/orbitfm.conf`,
  которую пользователь добавляет сам, объяснение, почему установщик не пишет её в
  чужой конфиг, и отметка, что правила текущего сеанса применены через
  `hyprctl keyword windowrulev2`.

У бар-целей вывод остался прежним: сниппет целиком печатается на stdout,
конфиги оболочки не трогаются. Строка `каталог:` печатается только когда каталог
цели действительно существует — у `noctalia` файлов нет, поэтому вместо него
видно только квитанцию. Тот же guard держится и в `status <цель>`: он печатает
каталог, путь квитанции и бинарник, но каталог — только если он есть.

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
- Из чужого конфига правится ровно одна строка (`include` у niri), только в
  обычный файл, только с одноразовой копией `<имя>.orbitfm.bak` и только с
  удалением по квитанции. Это распространение ADR-0042 § 2 на новый формат, а не
  разрешение редактора конфигов (ADR-0051 § 5).
- Каталоги, созданные установщиком, снимаются при `uninstall`, если опустели;
  непустой каталог не трогается (§ 4.6).
- Внешние команды — только фиксированный argv из кода (`noctalia msg plugins …`,
  `hyprctl keyword windowrulev2 …`) без shell и без подстановки пользовательских
  строк; вывод оболочки только читается и в argv не попадает.
- `ORBITFM_INTEGRATION_ROOT` подменяет XDG-корни для тестов; путь обязан быть
  абсолютным. Раскладка: `<root>/targets/<цель>` и `<root>/receipts/<цель>.json`.
  В песочнице живой оболочке не командуют: `hyprctl` и `noctalia` не запускаются,
  владение источником фиксируется как «наш».
- Операции GUI вызывают те же `install_one`/`uninstall_one`, поэтому проверки
  путей и квитанций не дублируются и не ослабляются.

## 6. Цели и форматы

| Цель | Каталог установки | Подключение |
|---|---|---|
| `plasma6` | `$XDG_DATA_HOME/plasma/plasmoids/org.modernbyte.orbitfm` | `kbuildsycoca6 --noincremental`, добавить виджет |
| `gnome` | `$XDG_DATA_HOME/gnome-shell/extensions/orbitfm@modernbyte` | `gnome-extensions enable orbitfm@modernbyte` |
| `omarchy` | `$XDG_CONFIG_HOME/omarchy/plugins/io.github.modernbyte.orbitfm` | `omarchy-shell shell rescanPlugins`, включить виджет |
| `noctalia` | файлов нет; квитанция в `$XDG_DATA_HOME/orbitfm/integrations/noctalia.json` | `noctalia msg plugins enable modernbyte/orbitfm` делает сама цель; виджет положить в бар |
| `waybar` | `$XDG_DATA_HOME/orbitfm/integrations/waybar` | модуль `custom/orbitfm` |
| `polybar` | `$XDG_DATA_HOME/orbitfm/integrations/polybar` | `[module/orbitfm]` |
| `i3blocks` | `$XDG_DATA_HOME/orbitfm/integrations/i3blocks` | секция `[orbitfm]` |
| `swaybar` | `$XDG_DATA_HOME/orbitfm/integrations/swaybar` | `status_command` |
| `i3status-rust` | `$XDG_DATA_HOME/orbitfm/integrations/i3status-rust` | `[[block]]` + `[[block.click]]` |
| `xfce` | `$XDG_DATA_HOME/orbitfm/integrations/xfce` | Genmon, `orbitfm-bar genmon` |
| `cinnamon` | `$XDG_DATA_HOME/orbitfm/integrations/cinnamon` | апплет вывода команды |
| `mate` | `$XDG_DATA_HOME/orbitfm/integrations/mate` | апплет вывода команды |
| `lxqt` | `$XDG_DATA_HOME/orbitfm/integrations/lxqt` | Command Output / Custom Command |
| `niri` | `$XDG_CONFIG_HOME/niri` (`cfg/orbitfm.kdl` + строка `include` в `config.kdl`) | строку подключения добавляет установка; niri 26.04+ перечитывает конфиг сам |
| `hyprland` | `$XDG_CONFIG_HOME/hypr/orbitfm.conf` | правила текущего сеанса ставит `hyprctl keyword`; строку `source = ~/.config/hypr/orbitfm.conf` добавляет пользователь |
| `chrome` | `$XDG_DATA_HOME/orbitfm/integrations/chrome` + `$XDG_CONFIG_HOME/<браузер>/NativeMessagingHosts` | загрузить `extension` распакованным; см. модуль 17 |
| `firefox` | `$XDG_DATA_HOME/orbitfm/integrations/firefox` + `~/.mozilla/native-messaging-hosts` | временное дополнение; см. модуль 17 |

Целей 17, они разложены по четырём группам `TargetGroup` — бары, окружения,
правила окон, браузеры (порядок страницы «Интеграции», не CLI). Собственные
имена оболочек в подписях не переводятся: `Noctalia`, `niri`, `Hyprland`;
подпись группы «Правила окон» — ключ `group_windows` во всех 16 каталогах i18n
(ADR-0051 § 1). В CLI цель задаётся именем из `TARGET_LIST`, у части целей есть
алиасы: `hypr` и `hyprctl` ведут на `hyprland`, `plasma`/`kde` — на `plasma6`,
`quickshell` — на `omarchy`. Цели ставятся по одной; `status` без цели — обзор
всех квитанций. Для Plasma 6
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

Отказ записи — ошибка с ненулевым кодом: `noctalia` без команды в `PATH`
(`нет команды noctalia в PATH`, ничего не написано), `niri` без `config.kdl`
(`конфиг niri не найден: <путь> — создайте его и повторите установку`),
симлинк на месте конфига niri. Невыполненное действие оболочки ошибкой не
считается: Hyprland без отвечающего `hyprctl` остаётся установленным и
печатает `внимание: правила не применены на сеанс: …`, отказ `noctalia` при
снятии — заметку той же формы.

## 8. Тесты и доказательность

Автотесты (`cargo test`, модуль `integration::tests`): разбор 17 целей, их
алиасов и `--force`,
установка и квитанция, идемпотентность, отказ при изменённом файле и успех с
`--force`, удаление только файлов квитанции, отказ при изменённом файле при
удалении, `status` для отсутствующего файла, отказ при пути вне каталога.

Отдельно покрыты новые поверхности ADR-0041/0042/0051:

- `integration::waybar`: `injects_module_and_preserves_comments_and_layout`
  (комментарии и отступы на месте), `second_enable_is_idempotent_and_keeps_first_backup`,
  `malformed_config_is_left_untouched`, `missing_modules_array_is_an_error`,
  `root_array_is_rejected`, `symlinked_config_is_not_replaced`,
  `finds_config_without_extension`;
- `integration::wm`: `compositor_variables_win_over_desktop_names`,
  `desktop_names_are_matched_case_insensitively`,
  `hyprland_rules_are_single_line_fixed_arguments`,
  `niri_include_points_at_the_installed_rule_file` (строка `include` ссылается на
  записанный файл), `add_include_is_idempotent_and_backs_up_once`,
  `add_include_appends_to_text_without_trailing_newline`,
  `remove_include_drops_only_its_whole_line` (ни комментарий, ни чужое `include`
  не задеты), `include_edit_refuses_a_symlinked_config`;
- `integration::noctalia`: `argv_is_the_fixed_command_vector` (argv совпадает с
  ADR-0051 § 5, shell-символов нет), `plugin_parsers_read_the_shell_listing`,
  `source_parser_reads_the_source_listing` (источники различаются по имени, не по
  подстроке адреса), `sandbox_never_commands_the_shell`;
- обнаружение и раскладка целей: `new_targets_are_detected_by_binary_or_config`,
  `installer_dirs_are_pruned_so_detection_stays_honest` (каталог-обманка не
  переживает снятие), `niri_install_edits_include_once_and_uninstall_restores`,
  `noctalia_round_trip_without_files` (нул файлов, квитанция и бейдж есть);
- `integration::reload`: `unsupported_targets_ask_for_a_manual_restart` —
  неподдерживаемая цель не получает сигнал;
- браузерные установки: `installs_chrome_extension_and_native_manifests`,
  `installs_firefox_native_manifest`,
  `uninstall_browser_removes_extension_and_manifests`; в манифесте проверяется
  абсолютный путь wrapper'а и `exec` на текущий бинарник.

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
Локальная установка 0.16.0-rc.2 разложила все цели того релиза (14) в реальный
`$HOME` командами по одной: Chrome/Firefox — по 6 файлов и native-манифесту
(7 записей в квитанции), GNOME/Waybar — по 4 файла, `status` подтвердил
«установлено». Сборка `.plasmoid` проверяется `unzip -l` и релизным workflow; сам
формат KPackage на машине приёмки не запускался.

Формат команд Noctalia и поведение `source remove` сверены с живой оболочкой
5.4.0 на этапе принятия ADR-0051 § 2 (оттуда же вывод: чужой `path`-источник
перекрывает виджет, а снятие источника оставляет его выключенным). Ритуал
локальной выкатки ставит обнаруженные цели; на машине приёмки это `chrome`,
`firefox`, `noctalia`, `niri`, а `hyprland` остаётся «не обнаружено»
(ADR-0051, «Последствия»).

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
- ADR-0041 — мастер первого запуска и runtime-правила Hyprland
- ADR-0042 — управление интеграциями из GUI, CST-правка Waybar, перезагрузка
- ADR-0051 — Noctalia, niri и Hyprland как цели установщика
- `docs/interfaces/05-ipc.md` — `status --json`, IPC-глаголы и `integration`
- `docs/modules/14-ui-settings.md` — страница «Интеграции»
- `docs/modules/17-browser-companion.md` — браузерные цели
- `docs/modules/08-noctalia-wm.md` — плагин Noctalia и тексты правил окон
- `docs/wiki/06-Интеграции.md` — пользовательская инструкция
