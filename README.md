# OrbitFM

**Плеер интернет-радио с 3D-глобусом**  
Под эгидой [ModernByte](https://github.com/ModernByte) · версия `0.16.0-rc.1`

[![Релиз](https://img.shields.io/github/v/release/ModernByte/orbitfm?include_prereleases)](https://github.com/ModernByte/orbitfm/releases)
[![Статус CI](https://img.shields.io/github/actions/workflow/status/ModernByte/orbitfm/ci.yml?branch=main)](https://github.com/ModernByte/orbitfm/actions/workflows/ci.yml)
[![Rust](https://img.shields.io/badge/rust-1.95%2B-orange?logo=rust&logoColor=white)](https://www.rust-lang.org)
[![Платформы](https://img.shields.io/badge/Linux%20%C2%B7%20Windows%20%C2%B7%20macOS-blueviolet)](#системные-требования)

## Что это

OrbitFM — плеер интернет-радио, где каталог показан глобусом, а не списком: крутите планету, кликните по стране —
и её станции лежат рядом в списке; так же можно искать по жанру или по названию станции. Закрытие окна музыку не
останавливает: приложение работает одним процессом, живёт в системном трее и принимает команды из терминала и от
медиа-клавиш. Для экрана есть три режима — от большого рабочего стола с глобусом до компактной капсулы в углу.
Приоритетная платформа — Linux (Wayland и X11), сборки есть для Windows и macOS; версия `0.16.0-rc.1` —
пре-релиз, отдельные детали ещё могут меняться.

## Что умеет

- **3D-глобус.** Клик по стране выбирает её, наведение рисует контур, выбранная страна подсвечивается
  «heatmap»-заливкой: чем больше станций загружено, тем ярче. Границы вшиты в бинарник, глобус работает без сети.
- **Поиск.** По стране (страницы по 100 станций с догрузкой — «100 из 1244»), по жанру во всём мире
  (до 200, с маркерами на глобусе) и по названию станции. Есть локальные фильтры с мультивыбором
  жанров и сортировка по популярности, битрейту, имени.
- **Избранное и история.** Звезда у каждой станции, вкладки «Результаты», «Избранное», «История». Избранное
  открывается и без сети; история держит последние 20 станций.
- **Радиорулетка.** Запускает случайную станцию по всему миру, камера перелетает к ней, результат показывается
  уведомлением.
- **Живая картинка.** Визуализатор на 16, 24 или 32 полосы, орбитальное аудиокольцо в Focus и аудио-реактивная
  планета: глобус дышит и качается в такт эфиру, шесть стилей движения, перевороты камеры. Всё выключается одной
  галкой «Уменьшить движение».
- **Системный трей.** На Linux — StatusNotifierItem, на macOS — элемент строки меню. Иконка живёт, пока жив
  процесс, в том числе после остановки музыки при закрытом окне. Меню сгруппировано: карточка станции, показ
  окна, транспорт, громкость с процентом, подменю темы, «О станции» (избранное и домашняя страница), настройки
  и выход; колесо мыши по иконке меняет громкость.
- **Внешние потоки.** `orbitfm play-url` принимает `.m3u`/`.pls`, прямую аудиоссылку и HTML-страницу с аудио
  (`og:audio`, `<audio>`, `<source>`). Импорт запускается только вручную, HLS отклоняется, локальная сеть
  разрешена лишь при явной команде; поток становится станцией с историей, избранным и MPRIS.
- **Медиа-клавиши и системные виджеты.** MPRIS на Linux, System Media Transport Controls на Windows,
  Now Playing на macOS — в том числе при скрытом окне.
- **11 языков интерфейса** — «Системный» плюс 10 языков, смена без перезапуска.
- **Настройки.** Тема (системная / тёмная / светлая), акцент, язык, громкость, визуализатор, реактивность и стиль
  движения, «Уменьшить движение», компоновка окна. Сохраняются автоматически.
- **Три режима окна** — Focus / Overlay / Listening, переключение из интерфейса и командой `orbitfm mode …`.
- **Виджет для панели Noctalia v5** и готовые правила окон для niri и Hyprland.
- **Машиночитаемый статус для интеграций.** `orbitfm status --json` (схема 1) отдаёт состояние, станцию,
  громкость, окно, тему, язык, доступные действия и локализованные подписи; `orbitfm events` отдаёт тот же
  снимок потоком — на нём строятся плагины оболочек и браузерная панель.
- **Готовые интеграции оболочек.** `orbitfm integration install all` раскладывает по каталогам пользователя
  плагин Plasma 6, расширение GNOME 45–51, виджет Omarchy/Quickshell и модули Waybar, Polybar, i3blocks,
  swaybar, i3status-rust; для XFCE, Cinnamon, MATE и LXQt печатаются шаги для штатного апплета вывода команды.
  Конфиги оболочек не изменяются, квитанции позволяют обновлять и удалять только свои файлы.
- **Панель в браузере.** `orbitfm integration install chrome|firefox` раскладывает popup и native host для
  Chrome, Chromium, Brave, Edge, Vivaldi и Firefox. Панель повторяет выпадающую панель Noctalia: карточка
  станции, транспорт, громкость, тема, избранное, поиск, история и «Play this page» для аудио на открытой
  вкладке. Скан вкладки выполняется только по клику (`activeTab` + `scripting`); content scripts и фоновых
  страниц нет, поэтому веб-сайты не получают доступа к плееру. Приложение должно быть уже запущено.

## Системные требования

| Платформа | Что нужно |
|-----------|-----------|
| Linux | Wayland или X11; видеосистема, с которой работает wgpu-рендерер; `libmpv` (в AppImage и Flatpak уже внутри) |
| Windows | x64 (установщик собирается с `-arch x64`); отдельная `libmpv` не нужна — `libmpv-2.dll` кладётся в MSI |
| macOS | 13.0 и новее — это `LSMinimumSystemVersion` собранного `.app`; DMG собирается под архитектуру машины-сборщика, в CI это Apple Silicon (arm64); `libmpv` бандлится в `.app` |

Конкретные версии Windows и Linux-дистрибутивов репозиторий не фиксирует: проверены сборки CI на `windows-latest`
и `macos-latest`. Для красивой картинки дополнительно нужны **Noto Color Emoji** (цветные флаги стран) и системный
**CJK-шрифт** для китайского, корейского или японского интерфейса.

## Установка

Готовые сборки — на странице релизов: <https://github.com/ModernByte/orbitfm/releases>. Рядом с пакетами лежит
файл `SHA256SUMS`, так что перед первым запуском можно сверить контрольную сумму: `sha256sum -c SHA256SUMS`.

### AppImage (Linux x86_64)

Файл называется `OrbitFM-<версия>-x86_64.AppImage`, например `OrbitFM-0.16.0-rc.1-x86_64.AppImage`. Права root и
установка не нужны — приложение rootless по определению, и `libmpv` с FFmpeg уже внутри:

```bash
chmod +x OrbitFM-0.16.0-rc.1-x86_64.AppImage
./OrbitFM-0.16.0-rc.1-x86_64.AppImage
```

Если FUSE недоступен (контейнеры, минимальные образы) или не нужна интеграция через монтирование, запускайте в
режиме распаковки: `APPIMAGE_EXTRACT_AND_RUN=1 ./OrbitFM-0.16.0-rc.1-x86_64.AppImage`. Виджету Noctalia и командам
`orbitfm …` нужен запускаемый `orbitfm` в `PATH` — удобно сделать символьную ссылку на AppImage.

### Установщики Windows и macOS

Оба лежат на странице релизов и **не подписаны** коммерческим сертификатом.

- **Windows:** `OrbitFM-<версия>-windows-x64.msi` — установка для текущего пользователя, файлы в
  `%LOCALAPPDATA%\Programs\OrbitFM`, ярлык в меню «Пуск». Authenticode-подписи нет, поэтому SmartScreen может
  попросить подтверждение.
- **macOS:** `OrbitFM-<версия>-macos-<arch>.dmg` (в CI — `arm64`). Откройте образ и перетащите `OrbitFM.app` в
  `/Applications`. Подпись только ad-hoc (`codesign --sign -`), нотаризации нет, поэтому при первом запуске
  Gatekeeper блокирует приложение: штатный обход — правый клик по приложению → «Открыть», а если не помогает,
  `xattr -d com.apple.quarantine /Applications/OrbitFM.app`

### Сборка из исходников

Нужен Rust 1.95 или новее (Edition 2024). Системные пакеты — те же, что ставит CI (`.github/workflows/ci.yml`,
шаг Install system dependencies; Debian/Ubuntu; для других дистрибутивов: Arch — `mpv`, Fedora —
`mpv-libs-devel`):

```bash
sudo apt-get install -y --no-install-recommends \
  libgl1-mesa-dev libmpv-dev libwayland-dev libx11-dev \
  libxcursor-dev libxi-dev libxkbcommon-dev libxrandr-dev
git clone https://github.com/ModernByte/orbitfm
cd orbitfm
cargo build --release
./target/release/orbitfm
```

Пакеты собираются и локально: `./packaging/linux/build-appimage.sh` (результат —
`target/appimage/OrbitFM-<версия>-x86_64.AppImage`) и `./packaging/linux/build-flatpak.sh` (нужен
`flatpak-builder` и runtime 25.08). Flatpak в релизы не входит, а его IPC-сокет живёт в sandbox, поэтому виджет
Noctalia и команды `orbitfm …` с Flatpak-сборкой не работают.

### Виджет для панели Noctalia v5

OrbitFM поставляет виджет панели как плагин `modernbyte/orbitfm`: кнопка открывает выпадающую панель
без рамок и заголовка (`orbitfm panel`), повторное нажатие скрывает её. Ярлык приложения в доке или на
рабочем столе всегда ведёт в полный стол Focus (`orbitfm mode focus`). Нужен Noctalia v5 с Plugin API 3 и `orbitfm`
в `PATH`. Команды `install` у `noctalia msg plugins` нет — установка идёт через источник плагинов:

```bash
noctalia msg plugins source add orbitfm git https://github.com/ModernByte/orbitfm
noctalia msg plugins update orbitfm
noctalia msg plugins enable modernbyte/orbitfm
```

Дальше виджет добавляется в интерфейсе: настройки бара → «Add widget» → **OrbitFM**. Проверить, что плагин на
месте, можно командой `noctalia msg plugins list`.

### Правила окон для niri и Hyprland

В тайловых окружениях компактные капсулы без правил растянутся на всю высоту тайла. Готовые файлы лежат в
`wm-configs/` и фиксируют размеры всех трёх режимов по заголовку окна:

```kdl
// ~/.config/niri/config.kdl
include "./cfg/orbitfm.kdl"   // скопированный wm-configs/orbitfm-niri.kdl
```

Для Hyprland подключите `wm-configs/orbitfm-hyprland.conf` (`windowrulev2`: `float`, `center`, `stayfocused` и
размеры); этот синтаксис работает в Hyprland 0.52 и в совместимом режиме 0.53+, где помечен как устаревший, — при
обновлении проверяйте документацию Hyprland. Правила niri дополнительно ставят окно под панель Noctalia (якорь к
правому верхнему углу) вместо центра экрана; отступ `x` при другой раскладке бара подбирается вручную.

## Быстрый старт

1. Запустите OrbitFM. Если плеер уже работает, новый запуск просто покажет существующее окно — процесс один.
2. При первом запуске поверх глобуса появится карточка с жестами мыши; закройте её `Esc`.
3. Кликните левой кнопкой по стране — камера мягко довернётся, страна подсветится, справа откроется список станций.
4. Вкладка **«Результаты»** — найденные станции; при желании отфильтруйте по имени или жанру либо отсортируйте по
   битрейту.
5. Нажмите **Play** в строке станции — или выделите её стрелками и нажмите `Space`.
6. Крутите глобус перетаскиванием правой кнопкой, приближайте колесом; левый клик по маркеру запускает станцию.
7. Нужен другой запрос — введите текст в строку HUD («Поиск станций…» или «Поиск страны…») и нажмите `Enter`, либо
   переключитесь на **«Жанр»** и выберите тег.
8. Понравившуюся станцию отметьте **звездой** (избранное), всё прослушанное складывается в «Историю» (20 последних,
   чистятся кнопкой во вкладке).
9. Не знаете, что включить, — **«Радиорулетка»**: случайная станция со всего мира.
10. Шестерёнка (или `Ctrl+,`) открывает настройки; «Скрыть окно» прячет окно, а музыка, трей и команды продолжают
    работать. Выйти полностью — «Выход» в трее или `orbitfm quit`.

## Режимы окна

| Режим | Размер (по умолчанию / минимум) | Для чего | Как переключить |
|-------|--------------------------------|----------|-----------------|
| **Focus** | 1100×800 / 720×480 | Полный рабочий стол: большой глобус, панель станций, транспорт, визуализатор и аудиокольцо. Единственный режим с рамками окна | «Настройки» → «Окно» → «Компоновка»; `orbitfm mode focus`; ярлык приложения |
| **Overlay** | 500×620 / 360×480 | Компактная капсула: строка HUD с 16-полосным визуализатором, мини-глобус, избранное (до 8 станций), громкость и Play/Stop | То же меню; кнопка «Открыть Focus» для возврата; `orbitfm overlay` (из CLI) или `orbitfm mode overlay` |
| **Listening** | 420×152 / 360×120 | Одна строка «сейчас играет» с пульсом эфира | То же меню; `orbitfm mode listening` |
| **Panel** | 400×640 / 360×480 | Выпадающая панель бар-виджета без рамок и заголовка: шапка с поиском, режимами, громкостью, 16-полосной лентой и кнопкой «Открыть Focus», список станций, подвал транспорта | Клик бар-виджета Noctalia (`orbitfm panel`); `orbitfm mode panel` |

`orbitfm mode` без аргумента отвечает, какой режим активен, а `orbitfm mode <режим>` переключает его даже при
скрытом окне; при незапущенном плеере команда стартует сразу в этом режиме. Настройки из Listening открываются в
компоновке Overlay — в одну строку панель не помещается. Режимы различаются заголовком окна (`OrbitFM`,
`OrbitFM · Overlay`, `OrbitFM · Listening`, `OrbitFM · Panel`), поэтому правила окон из `wm-configs/` подбирают
размер именно к капсуле: без них тайловый WM растянет Overlay и Listening на весь тайл, и капсула перестанет быть
капсулой. Декорации окна — заголовок и рамка — остаются только у Focus: `Overlay`, `Listening` и `Panel`
открываются без них, потому что CSD-полоса рисовалась бы поверх стеклянной поверхности. Panel — поверхность,
а не раскладка стола: в переключателе «Компоновка» её нет, а повторный `orbitfm panel` скрывает окно.

## Управление: мышь, клавиатура и медиа-клавиши

**Глобус мышью:** левый клик — выбрать страну или запустить станцию по маркеру; перетаскивание правой кнопкой —
орбитальное вращение с инерцией; колесо — зум; наведение на страну — контур и подсказка с флагом, названием и
числом станций. Колесо над любым регулятором громкости (стол, капсула Overlay, шапка панели, настройки) — шаг ±5 % сразу
с записью в настройки; вне слайдера колесо остаётся зумом глобуса и скроллом списков. Вернуть карточку жестов:
«Настройки» → «Показать управление мышью».

**Клавиатура в окне:** `Tab` / `Shift+Tab` — переход по элементам (фокус виден); `↑` / `↓` — по списку станций,
`Space` или `Enter` — запустить выделенную; стрелки и `Home` / `End` — переключатели, чипы жанров, дискретный шаг
громкости; `Enter` в строке поиска — выполнить поиск, в подсказках — выбрать первый вариант; `/` или `Ctrl+F`
(macOS — `Cmd+F`) — перейти в поиск; `Space` без фокуса — play/pause; `Esc` — закрыть панель настроек, затем
карточку управления; `Ctrl+,` (macOS — `Cmd+,`) — открыть или закрыть настройки. Глобальных горячих клавиш на всю
систему в текущей версии нет.

**Медиа-клавиши и виджеты.** Play/Pause, Stop, следующая и предыдущая станция, громкость приходят из MPRIS (Linux),
SMTC (Windows) и Now Playing (macOS); на Linux они работают и при скрытом окне, на macOS — с 0.13.0-rc.1, потому
что скрытое окно больше не останавливает главный цикл. На Linux плеер виден как
`org.mpris.MediaPlayer2.OrbitFM`, поэтому им управляет и `playerctl`. Рантайм-проверки SMTC и Now Playing на
Windows/macOS ещё не выполнены — там интеграции собраны, но проверялись только сборкой и тестами.

**Команды из терминала:**

```bash
orbitfm toggle              # показать или скрыть окно (команда по умолчанию)
orbitfm show | orbitfm hide
orbitfm overlay             # капсула Overlay: показать или скрыть (из CLI)
orbitfm panel               # выпадающая панель: показать или скрыть (её зовёт бар-виджет)
orbitfm status              # состояние, станция, громкость, окно, режим
orbitfm status --json       # то же в JSON схемы 1 (для плагинов и скриптов)
orbitfm events              # поток состояния NDJSON: snapshot, state, keepalive
orbitfm mode overlay        # focus | overlay | listening | panel
orbitfm play <stationuuid>  # запустить станцию по UUID из Radio Browser
orbitfm play-url <url>      # импортировать .m3u, .pls, аудио или страницу с аудио
orbitfm playpause | orbitfm next | orbitfm previous
orbitfm roulette            # случайная станция
orbitfm stop
orbitfm volume 0.4          # 0.0–1.0
orbitfm volume-up | orbitfm volume-down
orbitfm theme dark          # system | dark | light
orbitfm settings            # открыть настройки (запускает приложение, если нужно)
orbitfm integration status [цель]            # какие интеграции установлены
orbitfm integration install <цель|all>       # плагин, бар-модуль или расширение браузера
orbitfm integration uninstall <цель|all>     # удалить только свои файлы
orbitfm quit                # завершить процесс
orbitfm --help              # -h: список команд; -V / --version — версия сборки
```

## Язык интерфейса

Язык меняется в **Настройки → Язык**: 11 значений — «Системный» и 10 языков (English, Русский, Français, Deutsch,
Español, Italiano, Português, 中文, 한국어, 日本語). Переведён весь интерфейс: HUD, панель станций, транспорт, обе
капсулы, подсказки, настройки, уведомления и меню трея. Названия станций, стран и жанров приходят из Radio Browser
и остаются как есть, как и служебные строки (CLI, протокол IPC, заголовки окон). «Системный» читает переменные
окружения по порядку `LC_ALL` → `LC_MESSAGES` → `LANG`; теги вида `ru_RU.UTF-8` или `pt-BR` распознаются,
`C`/`POSIX` и неизвестная локаль дают английский. Смена языка применяется сразу, перезапуск не нужен.

**Квадратики вместо азиатских шрифтов.** OrbitFM не вкладывает CJK-шрифты: он просит fontconfig найти подходящий
системный. Если 中文, 한국어 или 日本語 рисуются tofu — нужен установленный шрифт с поддержкой CJK (например,
Noto Sans CJK вашего дистрибутива); это зависимость от окружения, а не дефект программы. Флаги стран рисуются
глифами системного **Noto Color Emoji**, который ищется по четырём стандартным путям (`/usr/share/fonts/noto/`,
`/usr/share/fonts/truetype/noto/`, `/usr/share/fonts/google-noto-emoji/`, `/usr/share/fonts/TTF/`); без него
интерфейс работает как прежде, только без флагов.

## Неполадки

- **Не запускается, сообщение «не удалось запустить GUI».** Окно рисуется через wgpu: проверьте, что работает
  графический стек и сессия Wayland или X11. Если под Wayland что-то не так, попробуйте запуск под X11 (XWayland).
- **Фон окна чёрный или серый вместо полупрозрачного «стекла».** Прозрачность с wgpu на Linux работает; на Windows и
  macOS у неё есть известные ограничения (задокументированы в ADR-0002). На функциональность это не влияет.
- **Нет звука или старт падает с «не удалось инициализировать аудио-ядро».** Нативной сборке нужна системная
  `libmpv`: Debian/Ubuntu — `libmpv-dev`, Fedora — `mpv-libs-devel`, Arch — `mpv`. AppImage, MSI и DMG носят
  `libmpv` с собой.
- **Список пустой, «Станции не найдены».** Каталог — внешний Radio Browser: серверы находятся через DNS
  (`all.api.radio-browser.info`), затем зеркала перебираются при сетевых ошибках и `5xx`/`429`/`408`. Проверьте сеть
  и DNS, попробуйте позже или измените запрос.
- **Станция не играет, «Playback failed».** Обычно это сам вещатель: выключенный или битый сервер, гео-блокировка,
  переезд потока. OrbitFM переподключается при обрыве и останавливается, когда попытки исчерпаны; `orbitfm status`
  покажет `playing`, `buffering`, `reconnecting` или `error`.
- **Вместо обложки — инициалы названия.** Так выглядит сработавшая политика артворка: адрес `favicon` ведёт в
  приватную или служебную сеть, не резолвится во внешний адрес, отдаёт не-изображение по `Content-Type`, весит
  больше 512 КиБ либо доступен только по `http` — такой адрес поднимается до `https` и остаётся недоступным, если
  на `https` сервер не отвечает. Если станция раздаёт иконку только по HTTP, её можно включить тумблером
  «Разрешить HTTP-обложки» в настройках («Внешний вид»): plain HTTP позволяет по дороге подменить байты, уходящие в
  декодер, поэтому по умолчанию он выключен. Станция при этом играет нормально — отказ касается только картинки.
  Причина пишется в лог: `RUST_LOG=debug orbitfm show`.
- **Нужен лог для задачи на GitHub.** Логи пишет в stderr тот процесс, который поднял GUI: завершите плеер
  (`orbitfm quit`), затем запустите с подробным фильтром — `RUST_LOG=debug orbitfm show 2> ~/orbitfm.log`.
- **Overlay и Listening растянулись на весь экран** или **вместо флагов квадратики** — не добавлены правила окон
  тайлового WM (см. раздел про niri и Hyprland) либо не найден системный Noto Color Emoji.
- **Иконки трея нет.** На Linux трей работает по StatusNotifierItem и требует хоста SNI: Noctalia, KDE, Hyprland,
  Sway/waybar; в GNOME нужно расширение вроде AppIndicator/StatusNotifier. Без хоста иконка не появится — это
  штатная деградация, управляйте через `orbitfm …`. На macOS значок живёт в строке меню; на Windows трея пока
  нет — модуль там намеренно no-op.
- **В меню трея «Ничего не играет».** Процесс жив, но воспроизведение не запущено или поток не загрузился. Запустите
  станцию из меню («Пуск/Пауза», «Радиорулетка») или из окна.
- **Как выйти полностью.** Закрытие окна — не выход. Выход: «Выход» в меню трея, «Выход» в секции «Окно» настроек
  или `orbitfm quit`.
- **Где настройки и библиотека.** Логика одна для всех ОС: сначала XDG-переменные, иначе домашний каталог;
  IPC-транспорт — `$XDG_RUNTIME_DIR/orbitfm.sock` на Linux, `\\.\pipe\orbitfm` на Windows, сокет в `$TMPDIR` на
  macOS.

  ```text
  $XDG_CONFIG_HOME/orbitfm/settings.json   (обычно ~/.config/orbitfm/settings.json)
  $XDG_DATA_HOME/orbitfm/library.json      (обычно ~/.local/share/orbitfm/library.json)
  ```

- **Настройки не сохранились или «слетели».** При повреждённом файле программа предупредит и продолжит сессию со
  значениями по умолчанию, не перезаписывая ваш файл. Сброс: `orbitfm quit`, затем переименовать
  `~/.config/orbitfm/settings.json` и `~/.local/share/orbitfm/library.json` в `.bak`. Избранное и история живут в
  `library.json`: их удаление чистит их, но не трогает оформление.
- **Виджет Noctalia не управляет плеером.** Виджет вызывает `orbitfm panel`, поэтому нативный бинарник должен быть в
  `PATH`; с Flatpak-сборкой внешний CLI не работает (сокет в sandbox).

## Безопасность и приватность

Полная модель угроз — в [SECURITY.md](SECURITY.md); здесь то, что полезно знать, не открывая документ.

- **Управлять плеером может любой процесс вашего пользователя.** Команды идут по локальному транспорту: сокет
  `$XDG_RUNTIME_DIR/orbitfm.sock` с правами `0600` на Linux, `\\.\pipe\orbitfm` на Windows. Разграничения прав нет
  и быть не может — это приложение пользовательской сессии, а не сервис. Никаких сетевых портов OrbitFM не
  слушает вообще.
- **Наружу уходит только необходимое:** запросы к зеркалам Radio Browser (поиск, карточка станции), счётчик кликов
  провайдера при запуске станции, сам поток и адрес обложки. Телеметрии, аналитики, фоновых опросов и обновлений
  «с собой» нет; User-Agent — `OrbitFM/1.0 (ModernByte; Linux; Rust)`.
- **Обложки грузятся по политике, а не как попало.** Ссылка `favicon` приходит из чужого каталога, поэтому адрес
  проходит два яруса: синтаксический (приватные и служебные диапазоны, `localhost`, `*.local`, `*.internal`,
  одноярлыковые хосты вроде `http://router/`, облачные метаданные) и по адресам DNS — все они обязаны быть
  публичными, а каждый редирект проверяется заново. По умолчанию работает только `https`: адрес `http` поднимается
  до `https`, а явный тумблер «Разрешить HTTP-обложки» — осознанный выбор владельца. Тот же вердикт по DNS решает,
  попадёт ли адрес в системные метаданные MPRIS/SMTC/Now Playing: он считается фоном и переиспользуется 60 секунд,
  поэтому домен, резолвящийся во внутреннюю сеть, не попадает в `artUrl`.
- **Длинная сессия не растёт бесконечно.** Кэш обложек держит не больше 256 записей и не больше восьми загрузок в
  полёте, тело ответа — не больше 512 КиБ, сторона картинки — не больше 512 px.
- **Артефакты релиза не подписаны.** `SHA256SUMS` — контрольные суммы, а не подпись: сертификатов у проекта нет,
  поэтому проверяйте скачанное по суммам и берите сборки только из раздела Releases.

### Что значит «поддерживается»

Утверждения о платформах и интеграциях доказаны по-разному, и мы не делаем из них одинаковых обещаний:

| Уровень | Что за ним стоит |
|---|---|
| **Проверено в runtime** | Прогонено вручную на живом окружении: Linux (Wayland и X11, niri, Noctalia), трей, MPRIS, все режимы окна; панель браузера — Chrome for Testing и Firefox |
| **Собрано и протестировано в CI** | Компилируется, линтится и покрыт автотестами на `windows-latest` и `macos-latest`, но приёмки на живом железе не было: MSI, DMG, Now Playing на macOS, SMTC на Windows |
| **Заявлено планом** | Записано в ROADMAP и ADR как направление, кода или проверки ещё нет: OrbitFM Web Explorer, Android Auto/companion, UPnP/DLNA |

Плагины оболочек 0.16 пока не в первой категории: ассеты проверены статически
(`qmllint`, `qmlformat`, `node --check`), установщик и бар-скрипт — живым прогоном,
но сами Plasma 6, GNOME, Omarchy и бары на машине приёмки не запускались;
подробности — в `docs/modules/16-integrations.md`.

Браузерная панель проверена живым прогоном в Chrome for Testing и Firefox на
Linux: реальный popup через native host, snapshot и изменения через
`orbitfm events`, поиск, избранное, история и «Play this page». Brave, Edge,
Vivaldi и постоянно подписанный Firefox не запускались — для них проверены общий
код, манифесты и установщик; Windows-манифесты через реестр и macOS-раскладка в
пакет не входят. Подробности — в `docs/modules/17-browser-companion.md`.

## Для разработчиков

Архитектура, решения и контракты лежат рядом с кодом: [обзор архитектуры](docs/architecture/00-overview.md),
[ADR](docs/adr/), [модули](docs/modules/), [интерфейсы](docs/interfaces/). Правила участия —
[CONTRIBUTING](docs/CONTRIBUTING.md), план и история изменений — [ROADMAP](docs/ROADMAP.md) и
[CHANGELOG](docs/CHANGELOG.md). Целевой UX — [docs/ui/design-vision.md](docs/ui/design-vision.md), технические
требования — [orbitfm_srs.md](orbitfm_srs.md).

## Вики

Пользовательская документация живёт в [вики репозитория](https://github.com/ModernByte/orbitfm/wiki) и в каталоге
[docs/wiki/](docs/wiki/) — это одни и те же страницы. Начните с [карты разделов](docs/wiki/Home.md):

| Страница | О чём |
|---|---|
| [Установка](docs/wiki/01-Установка.md) | AppImage / MSI / DMG, сборка из исходников, Flatpak, контрольные суммы |
| [Первый запуск](docs/wiki/02-Первый-запуск.md) | Две минуты от иконки до звучащей станции |
| [Режимы окна](docs/wiki/03-Режимы-окна.md) | Focus / Overlay / Listening и правила окон для niri и Hyprland |
| [Настройки и язык](docs/wiki/04-Настройки-и-язык.md) | Модальное окно настроек, `settings.json`, 11 языков интерфейса |
| [Управление и горячие клавиши](docs/wiki/05-Управление-и-горячие-клавиши.md) | Мышь, клавиши, медиа-клавиши, команды CLI |
| [Noctalia](docs/wiki/06-Noctalia.md) | Виджет в баре, git-источник плагина, позиция окна |
| [Трей и MPRIS](docs/wiki/07-Трей-и-MPRIS.md) | Меню трея целиком и управление при скрытом окне |
| [Варианты использования](docs/wiki/08-Варианты-использования.md) | Семь готовых сценариев: фон, язык, второй экран, тишина |
| [Неполадки](docs/wiki/09-Неполадки.md) | Нет звука, квадратики вместо флагов, растянутая капсула, логи |
| [Глоссарий](docs/wiki/10-Глоссарий.md) | Термины проекта человеческим языком |
| [Интеграции оболочек](docs/wiki/11-Интеграции-оболочек.md) | Плагины Plasma 6, GNOME, Omarchy, бары и установщик |
| [Браузерное расширение](docs/wiki/12-Браузерное-расширение.md) | Панель OrbitFM в Chrome/Chromium и Firefox, native host |

## Благодарности

ModernByte собирал OrbitFM из чужой хорошей работы и не считает это зазорным: почти каждая приятная деталь
интерфейса — чья-то многолетняя поддержка. Спасибо сообществу egui и `eframe` за то, что UI и 3D-глобус помещаются
в одном окне, а `wgpu` — за то, что это работает на обычной видеосистеме. Спасибо проекту mpv: `libmpv` уже годы
честно тянет воспроизведение, включая самые каприжные потоки. Спасибо сообществу Radio Browser за бесплатный
каталог станций и зеркала — без него клик по стране ничего бы не значил. Спасибо Natural Earth за границы стран и
тем, кто делает шрифты Noto, — от них зависят контуры и флаги. И отдельно спасибо noctalia.dev, niri и Hyprland:
это окружение, ради которого OrbitFM задумывался как Linux First.

**Зависимости Rust** (названия и версии — из `Cargo.toml` и `Cargo.lock`).

| Crate | Версия | Зачем в OrbitFM |
|-------|--------|-----------------|
| [eframe](https://docs.rs/eframe) | 0.37 | Окно и цикл приложения: `wayland`, `x11`, `wgpu`, `accesskit`, `system_fonts`, `default_fonts` |
| [egui-wgpu](https://docs.rs/egui-wgpu) | 0.37 | Полотно egui поверх wgpu-сцены глобуса |
| [epaint](https://docs.rs/epaint) | 0.37 | Кастомная отрисовка UI; фича `color_fonts` даёт цветные флаги |
| [wgpu](https://docs.rs/wgpu) | 30 | Рендер сферы глобуса, маркеров, света и атмосферы |
| [geo](https://docs.rs/geo) | 0.33 | Point-in-Polygon: страна по точке на сфере |
| [geojson](https://docs.rs/geojson) | 1.0 | Чтение датасета границ `assets/geojson/world-110m.json` |
| [rstar](https://docs.rs/rstar) | 0.13 | R-Tree по bounding box стран — быстрый отбор кандидатов |
| [glam](https://docs.rs/glam) | 0.34 | Матрицы и векторы камеры для униформ |
| [bytemuck](https://docs.rs/bytemuck) | 1.25 | Безопасное приведение вершин и униформ к байтам для буферов |
| [libmpv2](https://docs.rs/libmpv2) | 6.0 | Обёртка C API libmpv: воспроизведение, громкость, события, реконнект |
| [playwire](https://crates.io/crates/playwire) | 2.0 | MPRIS на Linux, SMTC на Windows, Now Playing на macOS |
| [reqwest](https://docs.rs/reqwest) | 0.13 | HTTP к серверам Radio Browser и загрузка фавиконов (rustls, json) |
| [tokio](https://docs.rs/tokio) | 1.54 | Async runtime, каналы, таймауты, DNS-discovery зеркал |
| [serde](https://docs.rs/serde) | 1.0 | Сериализация настроек, библиотеки и ответов API |
| [serde_json](https://docs.rs/serde_json) | 1.0 | `settings.json`, `library.json`, разбор JSON Radio Browser и ответ `status --json` |
| [thiserror](https://docs.rs/thiserror) | 2.0 | Типизированные ошибки внутри модулей |
| [anyhow](https://docs.rs/anyhow) | 1.0 | Ошибки и контекст на границах и при запуске процесса |
| [tracing](https://docs.rs/tracing) | 0.1 | Логи подсистем |
| [tracing-subscriber](https://docs.rs/tracing-subscriber) | 0.3 | Консольный вывод логов с `env-filter` |
| [image](https://docs.rs/image) | 0.25 | Декодирование текстуры Земли и фавиконов (JPEG/PNG/WebP) |
| [url](https://docs.rs/url) | 2.5 | Разбор и проверка адресов потоков и фавиконов |
| [libc](https://docs.rs/libc) | 0.2 | Установка C-локали для корректной работы libmpv |
| [ksni](https://crates.io/crates/ksni) | 0.3.7 | StatusNotifierItem для трея (только Linux, фича `tray`; лицензия Unlicense отмечена в ADR-0020) |
| [tray-icon](https://crates.io/crates/tray-icon) | 0.26.1 | Иконка в строке меню macOS (только macOS, фича `tray`, ADR-0026) |
| [muda](https://crates.io/crates/muda) | 0.21.2 | Меню строки меню macOS и его обновление (только macOS, фича `tray`) |
| [windows](https://docs.rs/windows) | 0.62 | Win32-окно-владелец для SMTC (только Windows) |
| [tempfile](https://docs.rs/tempfile) | 3 | Временные каталоги в тестах (dev-зависимость) |

**Проекты и данные вокруг:** [Radio Browser](https://www.radio-browser.info) и
[его API-документация](https://docs.radio-browser.info/) — каталог станций, зеркала, click-tracking;
[Natural Earth](https://www.naturalearthdata.com) — границы `ne_110m_admin_0_countries` в
`assets/geojson/world-110m.json`, по отметке проектной документации public domain
([ADR-0008](docs/adr/0008-геопространственный-поиск.md), [docs/modules/03-geo.md](docs/modules/03-geo.md));
текстура Земли `assets/textures/earth.jpg` (2048×1024) и её детальный ярус
`assets/textures/earth@2x.jpg` (5400×2700) встроены в бинарник — NASA Blue Marble Next Generation,
август, без облаков, public domain ([docs/modules/04-render-globe.md](docs/modules/04-render-globe.md));
[mpv](https://mpv.io) и его
[встраиваемый libmpv](https://mpv.io/manual/master/#embedding-libmpv);
[Noto Color Emoji](https://fonts.google.com/noto/specimen/Noto+Color+Emoji) для флагов
([ADR-0017](docs/adr/0017-цветной-emoji-шрифт-для-флагов.md)), CJK — из системы
([ADR-0030](docs/adr/0030-локализация-интерфейса.md)); [Noctalia](https://noctalia.dev) и
[её плагины](https://docs.noctalia.dev/noctalia/plugins/development/) — панель, для которой сделан виджет;
[niri](https://niri-wm.github.io/niri/) и [Hyprland](https://hyprland.org) — окружения для правил окон из
`wm-configs/`; [Rust](https://www.rust-lang.org) (Edition 2024, `rust-version = 1.95`);
[eframe / egui](https://github.com/emilk/egui); [cava](https://github.com/karlstav/cava) как визуальный ориентир
визуализатора; `linuxdeploy` и `appimagetool` для AppImage, WiX для MSI.

## Правовая информация и отказ от ответственности

Это не юридическая консультация: если пункты ниже вызывают сомнения в вашей юрисдикции, обратитесь к своему юристу.

**(а) «Как есть».** OrbitFM распространяется по лицензии MIT, и её раздел «DISCLAIMER OF WARRANTY» действует
полностью: программа поставляется «КАК ЕСТЬ», БЕЗ КАКИХ-ЛИБО ГАРАНТИЙ, явных или подразумеваемых, включая гарантии
товарной пригодности, пригодности для конкретной цели и ненарушения прав. Мы не обещаем, что приложение работает без
ошибок, что выбранная станция заиграет и что поведение версии `0.x` совпадёт с поведением следующей.

**(б) Ответственность.** В пределах, допускаемых применимым правом, авторы, контрибьюторы и ModernByte не отвечают за
прямые, косвенные, случайные, особые и штрафные убытки, упущенную выгоду, утрату данных, прерванную работу или ущерб
оборудованию, связанные с использованием программы или невозможностью её использовать. В некоторых юрисдикциях такие
ограничения недопустимы, поэтому часть из них может к вам не применяться (в США дисклеймер должен быть заметным — см.
[рекомендации FTC](https://www.ftc.gov/business-guidance/resources/using-conspicuous-language-make-effective-online-redisclaimers)).

**(в) Станции и потоки.** Приложение ничего не транслирует, не записывает и не хранит аудио. Списки станций приходят
из стороннего каталога Radio Browser, который поддерживает его сообщество; мы его не модерируем и не отвечаем за
содержимое. Права на контент конкретной станции и легальность её прослушивания в вашей стране вы проверяете сами:
ответственность за нарушение авторских прав и за соблюдение условий использования сервисов лежит на пользователе.

**(г) Сеть и данные.** Программа обращается только к серверам Radio Browser (зеркала, разрешённые через
`all.api.radio-browser.info`, включая запись о прослушивании `/json/url/<uuid>`) и к адресам, которые выбрали вы:
потоку станции и её фавикону. Ваш IP-адрес виден вещателю — как при любом сетевом запросе. Аккаунтов, облачной
синхронизации и телеметрии нет: в коде нет адресов аналитики, настройки, избранное и история лежат в ваших
каталогах. Появление телеметрии потребует отдельного архитектурного решения и явного включения пользователем.

**(д) Товарные знаки.** Noctalia, niri, Hyprland, mpv, Radio Browser, Rust, Windows, macOS и другие названия и
логотипы принадлежат их владельцам; упоминание не означает аффилиации, одобрения или спонсорства.

**(е) Статус пре-релиза.** До 1.0.0 приложение выходит как `0.x`, и совместимость форматов `settings.json` и
`library.json`, протокола IPC и поведения режимов окна не гарантируется. Правила версионирования — в
[docs/VERSIONMAP.md](docs/VERSIONMAP.md), изменения — в [docs/CHANGELOG.md](docs/CHANGELOG.md).

**(ж) Сообщить о нарушении прав или о проблеме.** Единственный канал проекта — GitHub:
[задачи репозитория](https://github.com/ModernByte/orbitfm/issues) (шаблоны: баг, предложение, ADR). Отдельного
контакта для правообладателей и SECURITY-адреса в репозитории нет.

### Disclaimer (English)

**(a) AS IS.** OrbitFM is provided under the MIT License, and its “DISCLAIMER OF WARRANTY” section applies in full:
the software is provided “AS IS”, WITHOUT WARRANTY OF ANY KIND, express or implied, including the warranties of
merchantability, fitness for a particular purpose and noninfringement. We do not warrant that the program is
error-free, that a chosen station will play, or that a `0.x` release will behave like a later one.

**(b) Liability.** To the extent permitted by applicable law, the authors, contributors and ModernByte shall not be
liable for any direct, indirect, incidental, special, exemplary or consequential damages, loss of profits, loss of
data, interruption of operation or damage to equipment arising from use of or inability to use the software. Some
jurisdictions do not allow such limitations, so parts of it may not apply to you; in the United States a disclaimer
must be conspicuous (see
[FTC guidance](https://www.ftc.gov/business-guidance/resources/using-conspicuous-language-make-effective-online-redisclaimers)).

**(c) Stations and streams.** The application does not broadcast, record or store audio. Station lists come from the
third-party [Radio Browser](https://www.radio-browser.info) directory, maintained by its own community; we neither
moderate it nor vouch for its contents. You are responsible for the rights in a station's content, for the legality
of listening to it in your jurisdiction and for complying with any service's terms of use.

**(d) Network and personal data.** The application contacts only Radio Browser servers (mirrors resolved via
`all.api.radio-browser.info`, including the listen notification endpoint `/json/url/<uuid>`) and the addresses you
choose: a station stream and its favicon. Your IP address is visible to the broadcaster, as with any network
request. There are no accounts, no cloud sync and no telemetry, and settings, favorites and history stay in your own
user directories.

**(e) Trademarks.** Noctalia, niri, Hyprland, mpv, Radio Browser, Rust, Windows, macOS and other names and logos
belong to their owners; mentioning them implies no affiliation, endorsement or sponsorship.

**(f) Pre-release status.** Until 1.0.0 the application ships as a `0.x` pre-release, and compatibility of
`settings.json`, `library.json`, the IPC protocol and window-mode behaviour is not guaranteed.

**(g) Reporting infringement or problems.** The project's only channel is GitHub:
[repository issues](https://github.com/ModernByte/orbitfm/issues). There is no dedicated rights-holder contact and
no SECURITY contact.

This text is not legal advice; consult your own lawyer if any part is unclear where you live.

## Лицензия

[![Лицензия MIT](https://img.shields.io/badge/license-MIT-green)](LICENSE)

MIT © 2026 ModernByte — полный текст в файле [LICENSE](LICENSE). Именно он задаёт отказ от гарантий и ограничение
ответственности; раздел выше его расширяет, но не отменяет.

---

© ModernByte · [Релизы](https://github.com/ModernByte/orbitfm/releases) ·
[Документация](https://github.com/ModernByte/orbitfm/tree/main/docs)
