# OrbitFM

**Плеер интернет-радио с 3D-глобусом**  
Под эгидой [ModernByte](https://github.com/ModernByte) · версия `0.22.0-rc.2`

[![Релиз](https://img.shields.io/github/v/release/ModernByte/orbitfm?include_prereleases)](https://github.com/ModernByte/orbitfm/releases)
[![Статус CI](https://img.shields.io/github/actions/workflow/status/ModernByte/orbitfm/ci.yml?branch=main)](https://github.com/ModernByte/orbitfm/actions/workflows/ci.yml)
[![Rust](https://img.shields.io/badge/rust-1.95%2B-orange?logo=rust&logoColor=white)](https://www.rust-lang.org)
[![Платформы](https://img.shields.io/badge/Linux%20%C2%B7%20Windows%20%C2%B7%20macOS-blueviolet)](#системные-требования)
[![macOS](https://img.shields.io/badge/macOS%2013%2B%20%C2%B7%20x86__64%20%C2%B7%20arm64-blue)](#матрица-поддержки-macos)

## Что это

OrbitFM — плеер интернет-радио, где каталог показан глобусом, а не списком: крутите планету, кликните по стране —
и её станции лежат рядом в списке; так же можно искать по жанру или по названию станции. Закрытие окна музыку не
останавливает: приложение работает одним процессом, живёт в системном трее и принимает команды из терминала и от
медиа-клавиш. Для экрана есть три режима — от большого рабочего стола с глобусом до компактной капсулы в углу.
Приоритетная платформа — Linux (Wayland и X11), сборки есть для Windows и macOS; версия `0.22.0-rc.2` —
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
- **17 языков интерфейса** — «Системный» плюс 16 языков, смена без перезапуска.
- **Настройки.** Отдельное окно, а не модалка поверх стола: три автоматических размера под категорию,
  тема (системная / тёмная / светлая), акцент, язык, громкость, визуализатор, реактивность и стиль
  движения, «Уменьшить движение», компоновка окна, таймер сна, зацикливание очереди, профиль сети,
  ритуал сна и учёт статистики. Сохраняются автоматически.
- **Таймер сна.** `orbitfm sleep 30` (15 / 30 / 45 / 60 / 90 минут в настройках) работает при закрытом
  окне; остаток виден на кнопке часов, в трее и в `orbitfm status --json`, отмена — `sleep clear`.
  Пока играет эфир и тикает таймер, система не уходит в сон (`systemd-inhibit`/`caffeinate`/
  `SetThreadExecutionState`). По истечении — плавное затухание до 300 секунд, за минуту до действия —
  окно предупреждения с обратным отсчётом и кнопкой «Отменить», затем «Ничего» / «Сон» / «Выключить»
  (`orbitfm sleep action suspend`).
- **Очередь без тупиков.** `Next`/`Previous` идут по текущему видимому списку, а настройка «Зациклить очередь»
  (по умолчанию включена) решает, переходить ли с последней станции на первую; доступность кнопок отражается
  в `can` у `status --json` и в транспорте.
- **Профиль сети.** `orbitfm network metered` ограничивает discovery битрейтом 128 kbps, уменьшает мировой
  снимок до 250 станций и число параллельных загрузок иконок; `offline-first` выключает фоновые загрузки и
  живёт офлайн-снимком, пока вы сами не запустите поиск или эфир; `unlimited` возвращает полный каталог.
- **Перенос библиотеки.** `orbitfm library-export <файл>` и `orbitfm library-import <файл>` сохраняют и
  принимают избранное и историю в JSON-схеме 1 с теми же проверками публичных URL, что у каталога: импорт
  сливается по UUID, локальная запись побеждает, приватные адреса отбрасываются.
- **Статистика прослушивания.** `orbitfm stats` говорит, сколько и чего вы слушали: секунды эфира по станциям,
  странам и тегам, итоги за всё время и за 7 и 30 суток; `stats --json` отдаёт ту же сводку схемой 1,
  `stats clear` обнуляет. Засчитываются только секунды играющей станции, раздел «Статистика» в настройках
  показывает топ-10 с долями и позволяет выключить учёт. Счётчики лежат в `stats.json` в папке пользователя,
  никуда не отправляются и браузерным расширением не читаются.
- **Ручная ссылка станции.** `orbitfm alias <uuid> <url>` подставляет адресу мёртвой или устаревшей
  станции её реальный эфир; в списке и карточке станция помечается «ручная ссылка» с хостом адреса.
  Под адрес попадает только `http`/`https`, прошедший ту же проверку публичности, что каталожная ссылка;
  снять подмену — `alias <uuid> remove`. Алиасы живут в локальной библиотеке, экспорт и статус их не
  содержат, браузерное расширение команду ставить не может.
- **Три режима окна** — Focus / Overlay / Listening, переключение из интерфейса и командой `orbitfm mode …`.
- **Виджет для панели Noctalia v5** и готовые правила окон для niri и Hyprland — ставятся той же кнопкой или
  командой `orbitfm integration install …`, что и остальные цели.
- **Машиночитаемый статус для интеграций.** `orbitfm status --json` (схема 1) отдаёт состояние, станцию,
  громкость, окно, тему, язык, профиль сети, доступные действия, таймер сна и локализованные подписи;
  `orbitfm events` отдаёт тот же снимок потоком — на нём строятся плагины оболочек и браузерная панель.
- **Готовые интеграции оболочек — 17 целей.** Каждая ставится отдельной командой
  `orbitfm integration install <цель>` или кнопкой «Установить» в «Настройки → Интеграции»: плагин Plasma 6,
  расширение GNOME 45–51, виджеты Omarchy/Quickshell и Noctalia, правила окон niri и Hyprland и модули Waybar,
  Polybar, i3blocks, swaybar, i3status-rust, а для XFCE, Cinnamon, MATE и LXQt — скрипт и шаги для штатного
  апплета вывода команды. Виджет Plasma 6 дополнительно публикуется как `.plasmoid` в разделе Releases. Конфиги
  оболочек установщик не переписывает: он раскладывает файлы, печатает готовый сниппет, а из чужих конфигов правит
  ровно одну строку — `include` у niri (с резервной копией и снятием по квитанции). Подробные шаги для каждой цели —
  в разделе «Интеграции».
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
| macOS | 13.0 Ventura и новее, любая арка: `arm64` для Apple Silicon и `x86_64` для Intel — два отдельных DMG; `libmpv` бандлится в `.app`. Пол обеспечивается сборкой, а не словами: [матрица поддержки](#матрица-поддержки-macos) и [ADR-0052](docs/adr/0052-матрица-поддержки-macos.md) |

Конкретные версии Windows и Linux-дистрибутивов репозиторий не фиксирует: проверены сборки CI на `windows-latest`
и на обоих macOS-раннерах — `macos-latest` (arm64) и `macos-15-intel` (x86_64). Для красивой картинки дополнительно
нужны **Noto Color Emoji** (цветные флаги стран) и системные шрифты: **CJK** для китайского, корейского и японского,
**деванагари** (например, Noto Sans Devanagari) для хинди и **арабский** (Noto Sans Arabic) для арабского интерфейса.
Арабский текст отображается справа налево на уровне глифов, но egui раскладывает интерфейс слева направо —
зеркалирование RTL пока не реализовано.

### Матрица поддержки macOS

Единственный источник истины про macOS — [ADR-0052](docs/adr/0052-матрица-поддержки-macos.md); здесь только срез.
Колонка «чем обеспечено» — то, что ломает сборку, если перестало быть правдой; «чем подтверждено» — где правду
видели: в прогоне CI, на живом железе или в уже выложенных артефактах.

| Ось | Что обещаем | Чем обеспечено | Чем подтверждено |
|---|---|---|---|
| Пол ОС | macOS 13.0 Ventura, для обеих арок | `LSMinimumSystemVersion` в `Info.plist.in` (единственное место, где число записано), `MACOSX_DEPLOYMENT_TARGET=13.0` при сборке, бутылки `libmpv` под macOS 13, гейт `check-min-os.py` | запуском на `macos-15-intel` и `macos-latest` в CI и осмотром выложенных DMG `v0.21.0-rc.1`: в каждом 102 Mach-O, ни один не требует macOS выше 13.0 |
| Арки | `x86_64` и `arm64` — два отдельных DMG | `build-dmg.sh --arch` + гейт `check-app-arch.sh` (каждый Mach-O бандла обязан иметь ровно объявленную арку) | сборкой и смоком на раннере нужной арки; срез каждого из 102 Mach-O в выложенных DMG сверен отдельно |
| Железо | приёмочная машина — `MacBookAir8,2` (Intel UHD 617, 8 ГБ, Sonoma 14.8.9) | — | **не проверено**: рендер на UHD 617, запас 8 ГБ RAM |
| Подпись | ad-hoc, нотаризации нет | `codesign --sign -` в упаковке | инструкцией Gatekeeper; на чистом скачивании **не проверено** |

`x86_64`-DMG на Apple Silicon запустится через Rosetta, но рекомендуемым путём не является. Как собрать лог рендера
для приёмочной машины — в [вики, «Неполадки»](docs/wiki/09-Неполадки.md); пока такого лога нет, ни одно утверждение
про Intel GPU остаётся в колонке «не проверено».

## Установка

Готовые сборки — на странице релизов: <https://github.com/ModernByte/orbitfm/releases>. В набор входят AppImage,
MSI, два DMG (Apple Silicon и Intel), `.plasmoid` для Plasma 6 и `install-appimage.sh`; рядом лежит файл
`SHA256SUMS`, так что перед первым запуском можно сверить контрольную сумму: `sha256sum -c SHA256SUMS`.

### AppImage (Linux x86_64)

Файл называется `OrbitFM-<версия>-x86_64.AppImage`, например `OrbitFM-0.22.0-rc.2-x86_64.AppImage`. Права root и
установка не нужны — приложение rootless по определению, и `libmpv` с FFmpeg уже внутри:

```bash
chmod +x OrbitFM-0.22.0-rc.2-x86_64.AppImage
./OrbitFM-0.22.0-rc.2-x86_64.AppImage
```

Чтобы добавить приложение в меню и `PATH` без root и без ручного `chmod +x`, положите рядом `install-appimage.sh`
из релиза и запустите:

```bash
bash install-appimage.sh OrbitFM-0.22.0-rc.2-x86_64.AppImage
```

Скрипт копирует образ в `~/.local/bin/orbitfm`, ставит desktop entry с абсолютным `Exec`, иконки и metainfo в
`~/.local/share`, обновляет кэши и предупреждает, если `~/.local/bin` не в `PATH`. Удаление —
`bash install-appimage.sh --uninstall`; скачанный AppImage и пользовательские настройки не затрагиваются.
Для обновления запустите скрипт с новым образом.

Если FUSE недоступен (контейнеры, минимальные образы) или не нужна интеграция через монтирование, запускайте в
режиме распаковки: `APPIMAGE_EXTRACT_AND_RUN=1 ./OrbitFM-0.22.0-rc.2-x86_64.AppImage`. Виджету Noctalia и командам
`orbitfm …` нужен запускаемый `orbitfm` в `PATH` — скрипт выше ставит его, либо сделайте символьную ссылку на
AppImage.

### Установщики Windows и macOS

Оба лежат на странице релизов и **не подписаны** коммерческим сертификатом.

- **Windows:** `OrbitFM-<версия>-windows-x64.msi` — установка для текущего пользователя, файлы в
  `%LOCALAPPDATA%\Programs\OrbitFM`, ярлык в меню «Пуск». Authenticode-подписи нет, поэтому SmartScreen может
  попросить подтверждение.
- **macOS:** `OrbitFM-<версия>-macos-arm64.dmg` для Apple Silicon и `OrbitFM-<версия>-macos-x86_64.dmg` для Intel.
  Откройте образ и перетащите `OrbitFM.app` в `/Applications`. Подпись только ad-hoc (`codesign --sign -`),
  нотаризации нет, поэтому при первом запуске Gatekeeper блокирует приложение: штатный обход — правый клик по
  приложению → «Открыть», а если не помогает, `xattr -d com.apple.quarantine /Applications/OrbitFM.app`.
  Пошагово для каждой архитектуры — в вики: [Apple Silicon](docs/wiki/Установка-macOS-Apple-Silicon.md) и
  [Intel](docs/wiki/Установка-macOS-Intel.md).

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
`flatpak-builder` и runtime 25.08). Собранный AppImage можно сразу поставить в меню и `PATH`:
`./packaging/linux/install-appimage.sh target/appimage/OrbitFM-<версия>-x86_64.AppImage`. Flatpak в релизы не
входит, а его IPC-сокет живёт в sandbox, поэтому виджет Noctalia и команды `orbitfm …` с Flatpak-сборкой не
работают.

Установка самого плеера на этом заканчивается. Виджет для панели Noctalia,
правила окон для niri и Hyprland, панели Plasma, GNOME и Omarchy, модули баров
и расширения браузеров ставятся установщиком интеграций — см. раздел
[«Интеграции»](#интеграции).

## Интеграции

Каждая из **17 целей** ставится **отдельной командой** — массовой установки нет, потому что у оболочек разные
шаги подключения и разные проверки. Цели лежат на странице «Настройки → Интеграции» четырьмя группами: бары,
окружения, правила окон, браузеры. Подробные шаги по каждой группе — в вики:
[Интеграции](docs/wiki/06-Интеграции.md), [Панели и виджеты](docs/wiki/Интеграции-панели.md),
[Браузер](docs/wiki/Интеграции-браузер.md), [Правила окон](docs/wiki/Интеграции-правила-окон.md).

Команды:

```bash
orbitfm integration status                  # что уже установлено во всех целях
orbitfm integration status waybar           # подробнее об одной цели
orbitfm integration install plasma6         # плагин Plasma 6
orbitfm integration install gnome           # расширение GNOME Shell 45–51
orbitfm integration install omarchy         # виджет Omarchy/Quickshell
orbitfm integration install noctalia        # виджет Noctalia: git-источник и enable командами оболочки
orbitfm integration install niri            # ~/.config/niri/cfg/orbitfm.kdl + строка include в config.kdl
orbitfm integration install hyprland        # ~/.config/hypr/orbitfm.conf + правила текущего сеанса через hyprctl
orbitfm integration install waybar          # модуль Waybar
orbitfm integration install polybar         # модуль Polybar
orbitfm integration install i3blocks        # секция i3blocks
orbitfm integration install swaybar         # status_command для swaybar
orbitfm integration install i3status-rust   # блок i3status-rust
orbitfm integration install xfce            # XFCE: скрипт и инструкция
orbitfm integration install cinnamon        # Cinnamon: скрипт и инструкция
orbitfm integration install mate            # MATE: скрипт и инструкция
orbitfm integration install lxqt            # LXQt: скрипт и инструкция
orbitfm integration install chrome          # Chrome/Chromium/Brave/Edge/Vivaldi
orbitfm integration install firefox         # Firefox
```

Цель `hyprland` принимает и алиасы `hypr`, `hyprctl`. Обнаружение не опирается на собственный след установщика:
`noctalia` — бинарник в `PATH` или каталог `~/.local/state/noctalia`, `niri` — `niri` в `PATH` или файл
`~/.config/niri/config.kdl`, `hyprland` — `hyprland`/`hyprctl` в `PATH` или каталог `~/.config/hypr`. У niri
главный конфиг должен существовать: без `config.kdl` установка отказывает, чтобы не создавать конфиг за
пользователя.

Если файл цели изменён вручную, установщик откажется его перезаписывать; повторная установка с `--force`
перезапишет файл. Удаление — `orbitfm integration uninstall <цель>`: оно стирает только файлы из квитанции
`$XDG_DATA_HOME/orbitfm/integrations/<цель>.json`, убирает свою строку `include` из `config.kdl` у niri и снимает
регистрацию источника у `noctalia`; ваших правок в конфигах она не трогает — если `include` был добавлен вручную до
установки, он остаётся, а установка сообщает об этом (без файла правил niri такой `include` не разберёт). Каталог файлов цели —
`$XDG_DATA_HOME/orbitfm/integrations/<цель>/` (обычно `~/.local/share/orbitfm/integrations/<цель>/`); шаги
подключения и готовый сниппет печатаются после установки и лежат рядом со скриптом. Всем скриптам нужен запущенный
OrbitFM; второй экземпляр они не запускают.

Квитанция по-прежнему схемы `1`, а новые поля аддитивны и читаются со значением по умолчанию: `dirs[]` — каталоги,
которые создал установщик (при снятии удаляется только опустевший, непустой остаётся — пустой каталог больше не
держит цель в состоянии «обнаружено»), `lines[]` — пары «файл + точная строка», добавленные в чужой конфиг
(снятие убирает ровно их), и признаки `noctalia`: `source_added` — источник добавила эта установка, `was_enabled` —
виджет был включён до неё. Переустановка цели эти признаки не теряет, поэтому снятие не удалит чужой источник и не
выключит виджет, который пользователь включал сам.

| Цель | Что ставится | Как включить |
|------|--------------|--------------|
| `plasma6` | виджет Plasma 6: компакт-пилюля и полная карточка | `kbuildsycoca6 --noincremental`, затем «Добавить виджеты» → OrbitFM |
| `gnome` | расширение GNOME Shell 45–51 со `stylesheet.css` | `gnome-extensions enable orbitfm@modernbyte` |
| `omarchy` | виджет бара Omarchy/Quickshell | `omarchy-shell shell rescanPlugins`, затем включить в конфиге бара |
| `noctalia` | git-источник `orbitfm` и включённый виджет `modernbyte/orbitfm`; файлов у цели нет | «Add widget» → OrbitFM в настройках бара; обновления — `noctalia msg plugins update orbitfm` |
| `niri` | `~/.config/niri/cfg/orbitfm.kdl` и ровно одна строка `include` в `config.kdl` | ничего — niri 26.04+ перечитывает конфиг сам |
| `hyprland` | `~/.config/hypr/orbitfm.conf` и 18 правил на текущий сеанс (`hyprctl keyword windowrulev2`) | правила после перезапуска: строка `source = ~/.config/hypr/orbitfm.conf` в свой `hyprland.conf` |
| `waybar` | модуль `custom/orbitfm`, скрипт и `style.css` | добавить модуль в `modules-*` и импортировать стиль |
| `polybar` | модуль `[module/orbitfm]` и скрипт | подключить `module/orbitfm` в `modules-*` |
| `i3blocks` | секция `[orbitfm]` и скрипт | добавить секцию в конфиг i3blocks |
| `swaybar` | `status_command` и скрипт | заменить или объединить `status_command` в блоке `bar` |
| `i3status-rust` | блок `[[block]]` и клики | добавить блок в `config.toml` |
| `xfce` | скрипт и инструкция | Genmon в `xfce4-panel` |
| `cinnamon` | скрипт и инструкция | апплет «Вывод команды» |
| `mate` | скрипт и инструкция | апплет «Команда» |
| `lxqt` | скрипт и инструкция | плагин «Вывод команды» / «Пользовательская команда» |
| `chrome` | popup, native host и native-манифест | «Загрузить распакованное расширение» |
| `firefox` | popup, native host и native-манифест | «Загрузить временное дополнение» |

### Plasma 6

```bash
orbitfm integration install plasma6
kbuildsycoca6 --noincremental
```

Затем ПКМ по панели → «Войти в режим редактирования» → «Добавить виджеты» → **OrbitFM**. На Wayland при
необходимости перезапустите Plasma. Установщик подставляет абсолютный путь к бинарнику, поэтому этот способ
предпочтителен для AppImage.

Виджет также публикуется в Releases отдельным архивом `OrbitFM-<версия>-plasma.plasmoid`:

```bash
kpackagetool6 -t Plasma/Applet -i OrbitFM-0.22.0-rc.2-plasma.plasmoid
```

Или в графическом интерфейсе: режим редактирования панели → «Добавить виджеты» → «Установить из файла» → выбрать
`.plasmoid`. Архив вызывает `orbitfm` из `PATH`, поэтому команда должна быть доступна (для AppImage — символическая
ссылка). Обновление — той же командой с `-u`, удаление — `kpackagetool6 -t Plasma/Applet -r org.modernbyte.orbitfm`.

Компактный вид — стеклянная пилюля с иконкой, названием станции и точкой состояния. Полная карточка: станция,
транспорт, слайдер громкости, тема, «О станции» и кнопки панели/окна/настроек. ЛКМ — пуск/пауза, ПКМ — панель,
колесо — громкость.

### GNOME Shell 45–51

```bash
orbitfm integration install gnome
gnome-extensions enable orbitfm@modernbyte
```

Каталог расширения — `~/.local/share/gnome-shell/extensions/orbitfm@modernbyte/`; `stylesheet.css` Shell загружает
автоматически. На Wayland после установки может понадобиться выйти из сеанса и войти снова — GNOME сканирует
каталог расширений при старте. Проверка: `gnome-extensions list | grep orbitfm`. Кнопка в панели открывает меню с
карточкой станции, транспортом, громкостью, темой, избранным и домашней страницей.

### Omarchy и Quickshell

```bash
orbitfm integration install omarchy
omarchy-shell shell rescanPlugins
```

После пересканирования включите виджет OrbitFM в конфигурации бара Omarchy — установщик её не редактирует.
ЛКМ — пуск/пауза, ПКМ — выпадающая панель, глиф показывает состояние, tooltip — страну, битрейт и громкость.

### Noctalia

```bash
orbitfm integration install noctalia
```

Файлов у цели нет: установщик регистрирует git-источник `orbitfm`
(`https://github.com/ModernByte/orbitfm`) и включает виджет `modernbyte/orbitfm` командами `noctalia msg plugins …`,
предварительно прочитав `plugins list` и `source list`, чтобы не сломать то, что включал пользователь. Загрузку
плагина делает сама оболочка — OrbitFM ничего не копирует и не скачивает. Если источник `orbitfm` уже
зарегистрирован, `source add` не выполняется, а снятие убирает только наш источник и возвращает прежнюю
включённость виджета, если он всё ещё доступен из чужого источника. Без `noctalia` в `PATH` или при молчащей
оболочке установка ничего не пишет и возвращает причину.

Ручной путь остаётся запасным. Команды проверены на Noctalia v5.3.0; подкоманды `install` у `noctalia msg plugins`
нет, установка идёт через git-источник:

```bash
noctalia msg plugins source add orbitfm git https://github.com/ModernByte/orbitfm
noctalia msg plugins update orbitfm
noctalia msg plugins enable modernbyte/orbitfm
```

Дальше виджет нужно положить в бар: настройки бара → «Add widget» → **OrbitFM**. `enable` только разрешает
плагин, но не добавляет его. Проверка: `noctalia msg plugins list` и `noctalia msg plugins source list`; в списке
должны быть `modernbyte/orbitfm [orbitfm] 1.1.0 enabled requires orbitfm` и источник
`orbitfm git https://github.com/ModernByte/orbitfm`.

Виджет открывает выпадающую панель 400×640 командой `orbitfm panel`; повторный клик её скрывает. «Содержимое
виджета» переключает «Иконка и текст» / «Только иконку», а цвет иконки показывает состояние: зелёный — играет,
тёмно-серый — нет сети, красный — ошибка, цвет темы — пауза или остановлено. Снятие —
`orbitfm integration uninstall noctalia`. Подробные шаги и разбор ошибок — в вики:
[Панели и виджеты](docs/wiki/Интеграции-панели.md).

### niri и Hyprland

```bash
orbitfm integration install niri      # ~/.config/niri/cfg/orbitfm.kdl + строка include в config.kdl
orbitfm integration install hyprland  # ~/.config/hypr/orbitfm.conf + правила текущего сеанса
orbitfm integration uninstall niri
orbitfm integration uninstall hyprland
```

Правила подбираются по паре «класс окна (`app-id`) + заголовок» и фиксируют размеры поверхностей: Focus — 1100×800, Overlay —
500×620, Listening — 420×152, Panel — 400×640, настройки — 420×560 / 760×544 / 1040×720 по варианту
Compact/Medium/Wide, предупреждение таймера сна — 420×320. Заголовок различает поверхности, `app-id`
(`org.modernbyte.OrbitFM`) — принадлежность приложению: оба поля обязательны, иначе окно не найдёт своё правило. У niri установщик добавляет в `~/.config/niri/config.kdl` ровно одну
строку `include "./cfg/orbitfm.kdl"` (с копией `config.kdl.orbitfm.bak` перед первой правкой, без дублей при
переустановке). Снятие убирает только свою строку и свой файл; строку, добавленную вручную до установки, оно не
трогает, но указывает на неё — висячий `include` без файла правил niri не разберёт.

Hyprland получает `~/.config/hypr/orbitfm.conf`, а 18 правил из него дополнительно ставятся на текущий сеанс
командами `hyprctl keyword windowrulev2 …`. Чужой `hyprland.conf` установщик не правит: строку
`source = ~/.config/hypr/orbitfm.conf` добавляете вы сами, иначе правила пропадут с перезапуском сеанса. Правила,
поставленные на живой сеанс, снимаются вместе с сеансом. Подробности и проверка размеров — в вики:
[Правила окон](docs/wiki/Интеграции-правила-окон.md).

### Waybar

```bash
orbitfm integration install waybar
```

Установлены `config.jsonc`, `style.css` и `orbitfm-bar`. Скопируйте блок `custom/orbitfm` из
`~/.local/share/orbitfm/integrations/waybar/config.jsonc` в `~/.config/waybar/config.jsonc` (в некоторых
дистрибутивах — `~/.config/waybar/config`) и добавьте `custom/orbitfm` в нужную секцию `modules-left`,
`modules-center` или `modules-right`. Правила из `style.css` добавьте в свой `~/.config/waybar/style.css`.
Перезагрузка: `killall -SIGUSR2 waybar`.

ЛКМ — пуск/пауза, СКМ — стоп, ПКМ — панель, колесо — громкость. Поля `text`, `alt`, `tooltip` и `class` приходят
из скрипта, поэтому чип сам окрашивается по состоянию `playing`, `paused`, `loading`, `error`, `stopped`.

### Polybar

```bash
orbitfm integration install polybar
```

Скопируйте `[module/orbitfm]` из `~/.local/share/orbitfm/integrations/polybar/config.ini` в
`~/.config/polybar/config.ini` и добавьте `orbitfm` в `modules-left`/`modules-center`/`modules-right`.
ЛКМ — пуск/пауза, СКМ — стоп, ПКМ — панель, колесо — громкость.

### i3blocks

```bash
orbitfm integration install i3blocks
```

Добавьте секцию `[orbitfm]` из `~/.local/share/orbitfm/integrations/i3blocks/config.conf` в
`~/.config/i3blocks/config` (или подключите файл через `include`). В конфиге i3 должен быть запущен i3blocks:
`bar { status_command i3blocks }`. Клики приходят через `BLOCK_BUTTON`: ЛКМ — пуск/пауза, СКМ — стоп, ПКМ —
панель, колесо — громкость.

### swaybar

```bash
orbitfm integration install swaybar
```

В `~/.config/sway/config` замените `status_command` в блоке `bar` на строку из
`~/.local/share/orbitfm/integrations/swaybar/config.sway` либо объедините её с существующим статусом.
Перезагрузка: `swaymsg reload`. Клики: ЛКМ — пуск/пауза, СКМ — стоп, ПКМ — панель, колесо — громкость.

### i3status-rust

```bash
orbitfm integration install i3status-rust
```

Добавьте `[[block]]` и четыре `[[block.click]]` из
`~/.local/share/orbitfm/integrations/i3status-rust/config.toml` в `~/.config/i3status-rust/config.toml`.
Блок использует `json = true`; подпись, глиф и состояние приходят от `orbitfm-bar`. ЛКМ — пуск/пауза, ПКМ —
панель, колесо — громкость.

### XFCE

```bash
orbitfm integration install xfce
```

Установите пакет `xfce4-genmon-plugin`, затем ПКМ по панели → «Панель» → «Добавить новые элементы» → **Genmon**.
В свойствах плагина укажите команду:

```text
~/.local/share/orbitfm/integrations/xfce/orbitfm-bar genmon
```

Период обновления — 2 секунды. Левый клик по тексту запускает пуск/паузу; правого клика у Genmon нет, поэтому
выпадающую панель открывайте ярлыком или горячей клавишей с командой `orbitfm panel`. Полная инструкция лежит в
`~/.local/share/orbitfm/integrations/xfce/usage.txt`.

### Cinnamon

```bash
orbitfm integration install cinnamon
```

ПКМ по панели → «Добавить апплеты» → **«Вывод команды»**; команда —
`~/.local/share/orbitfm/integrations/cinnamon/orbitfm-bar text`, период 2 секунды. Апплет только показывает текст;
для управления добавьте ярлыки с командами `orbitfm playpause`, `orbitfm next`, `orbitfm panel` и
`orbitfm settings`. Инструкция — в `usage.txt` рядом со скриптом.

### MATE

```bash
orbitfm integration install mate
```

ПКМ по панели → «Добавить на панель» → **«Команда»**; команда —
`~/.local/share/orbitfm/integrations/mate/orbitfm-bar text`. Для управления добавьте отдельные кнопки с
`orbitfm playpause`, `orbitfm next`, `orbitfm panel` и `orbitfm settings`.

### LXQt

```bash
orbitfm integration install lxqt
```

ПКМ по панели → «Добавить плагин» → **«Вывод команды»**; команда —
`~/.local/share/orbitfm/integrations/lxqt/orbitfm-bar text`, период 2 секунды. Управление — плагин
«Пользовательская команда» или ярлыки с `orbitfm playpause`, `orbitfm next`, `orbitfm panel`, `orbitfm settings`.

### Chrome, Chromium, Brave, Edge и Vivaldi

```bash
orbitfm integration install chrome
```

Затем `chrome://extensions` → включите «Режим разработчика» → «Загрузить распакованное расширение» →
`~/.local/share/orbitfm/integrations/chrome/extension`. Native-манифест установщик пишет в каждый найденный
Chromium-каталог; ID расширения фиксирован публичным ключом. Подробности, ограничения и разбор ошибок — в
[вики: Браузер](docs/wiki/Интеграции-браузер.md).

### Firefox

```bash
orbitfm integration install firefox
```

`about:debugging#/runtime/this-firefox` → «Загрузить временное дополнение» →
`~/.local/share/orbitfm/integrations/firefox/extension/manifest.json`. Временное дополнение живёт до перезапуска
Firefox; постоянная установка требует подписи Mozilla — подписанный XPI в репозитории не публикуется.

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
размер именно к капсуле; правила матчат пару «`app-id` + заголовок», и этот же app-id
(`org.modernbyte.OrbitFM`) несут окна настроек и предупреждения таймера сна. Без них тайловый WM растянет
Overlay и Listening на весь тайл, и капсула перестанет быть капсулой. Декорации окна — заголовок и рамка — остаются только у Focus: `Overlay`, `Listening` и `Panel`
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
orbitfm sleep 30            # остановить эфир через 1–720 минут
orbitfm sleep clear         # отменить таймер сна
orbitfm sleep action suspend # nothing | suspend | power-off — действие по истечении
orbitfm sleep fade 300      # 0 | 60 | 120 | 180 | 300 секунд затухания
orbitfm sleep inhibit on    # не давать системе уснуть, пока играет эфир и тикает таймер
orbitfm sleep warning on    # окно предупреждения за минуту до действия
orbitfm network metered     # unlimited | metered | offline-first
orbitfm theme dark          # system | dark | light
orbitfm library-export ~/orbitfm-library.json  # избранное и история, схема 1
orbitfm library-import ~/orbitfm-library.json  # слить с текущей библиотекой по UUID
orbitfm stats               # сводка прослушанного: всего, станции, страны, теги
orbitfm stats --json        # та же сводка JSON-схемой 1 (для скриптов)
orbitfm stats clear         # обнулить счётчики статистики
orbitfm alias <uuid>        # ручная ссылка станции: показать, поставить, снять
orbitfm alias <uuid> <url>  # подставить станции её адрес вручную
orbitfm alias <uuid> remove # снять ручной адрес, играть по каталогу
orbitfm alias list          # все ручные адреса станций
orbitfm settings            # открыть настройки (запускает приложение, если нужно)
orbitfm integration status [цель]            # какие интеграции установлены (целей 17)
orbitfm integration install <цель>           # плагин, бар-модуль, виджет Noctalia, правила окон, расширение браузера
orbitfm integration uninstall <цель>         # удалить свои файлы, свою строку include и регистрацию источника
orbitfm quit                # завершить процесс
orbitfm --help              # -h: список команд; -V / --version — версия сборки
```

Цели — `plasma6, gnome, omarchy, noctalia, niri, hyprland, waybar, polybar, i3blocks, swaybar, i3status-rust,
xfce, cinnamon, mate, lxqt, chrome, firefox`; алиасы `hypr` и `hyprctl` ведут в `hyprland`. После установки
`integration install` печатает шаги подключения, а у `noctalia`, `niri` и `hyprland` те же ручные шаги хранит и
подробность строки цели на странице «Интеграции»: файл правил и готовую строку
`source = ~/.config/hypr/orbitfm.conf` — для Hyprland, добавленную строку `include` и её файл — для niri.

## Язык интерфейса

Язык меняется в **Настройки → Язык**: 17 значений — «Системный» и 16 языков (English, Русский, Українська,
Polski, Français, Deutsch, Español, Italiano, Português, Türkçe, العربية, हिन्दी, Bahasa Indonesia, 中文, 한국어,
日本語). Переведён весь интерфейс: HUD, панель станций, транспорт, обе капсулы, подсказки, настройки, уведомления
и меню трея. Названия станций, стран и жанров приходят из Radio Browser и остаются как есть, как и служебные
строки (CLI, протокол IPC, заголовки окон). «Системный» читает переменные окружения по порядку `LC_ALL` →
`LC_MESSAGES` → `LANG`; теги вида `uk_UA.UTF-8`, `pl_PL` или `pt-BR` распознаются, `C`/`POSIX` и неизвестная
локаль дают английский. Смена языка применяется сразу, перезапуск не нужен.

**Арабский и RTL.** Арабские строки выводятся справа налево на уровне глифов, но egui 0.37 раскладывает элементы
слева направо и не зеркалит интерфейс: порядок колонок, выравнивание и направление списков остаются общими для
всех языков. Это осознанное ограничение пре-релиза, а не ошибка перевода.

**Квадратики вместо азиатских шрифтов.** OrbitFM не вкладывает шрифты: он просит fontconfig найти подходящий
системный. Если 中文, 한국어 или 日本語 рисуются tofu — нужен установленный шрифт с поддержкой CJK (например,
Noto Sans CJK вашего дистрибутива); для хинди — шрифт деванагари (Noto Sans Devanagari), для арабского — Noto
Sans Arabic. Это зависимость от окружения, а не дефект программы. Флаги стран рисуются глифами системного
**Noto Color Emoji**, который ищется по четырём стандартным путям (`/usr/share/fonts/noto/`,
`/usr/share/fonts/truetype/noto/`, `/usr/share/fonts/google-noto-emoji/`, `/usr/share/fonts/TTF/`); без него
интерфейс работает как прежде, только без флагов.

## Неполадки

- **Не запускается, сообщение «не удалось запустить GUI».** Окно рисуется через wgpu: проверьте, что работает
  графический стек и сессия Wayland или X11. Если под Wayland что-то не так, попробуйте запуск под X11 (XWayland).
- **Фон окна чёрный или серый вместо полупрозрачного «стекла».** Прозрачность с wgpu на Linux работает; на Windows и
  macOS у неё есть известные ограничения. На функциональность это не влияет.
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
  тайлового WM: их ставит `orbitfm integration install niri` или `hyprland` (см. раздел про niri и Hyprland;
  у Hyprland правила живут до перезапуска сеанса, дальше — строка `source` в вашем `hyprland.conf`) либо не найден
  системный Noto Color Emoji.
- **Окно настроек или предупреждения таймера сна растянуто на весь экран** — сборки до 0.21.0-rc.4 отдавали этим
  окнам пустой класс (`app-id`), и правило с заголовком не могло сработать: обновите сборку и переустановите цель
  `niri`/`hyprland`. Проверка — `niri msg windows`: у каждого окна OrbitFM должен быть
  `App ID: "org.modernbyte.OrbitFM"`.
- **Иконки трея нет.** На Linux трей работает по StatusNotifierItem и требует хоста SNI: Noctalia, KDE, Hyprland,
  Sway/waybar; в GNOME нужно расширение вроде AppIndicator/StatusNotifier. Без хоста иконка не появится — это
  штатная деградация, управляйте через `orbitfm …`. На macOS значок живёт в строке меню; на Windows трея пока
  нет — модуль там намеренно no-op.
- **В меню трея «Ничего не играет».** Процесс жив, но воспроизведение не запущено или поток не загрузился. Запустите
  станцию из меню («Пуск/Пауза», «Радиорулетка») или из окна.
- **Как выйти полностью.** Закрытие окна — не выход. Выход: «Выход» в меню трея, «Выход» в секции «Окно» настроек
  или `orbitfm quit`.
- **Где настройки и библиотека.** Логика одна для всех ОС: сначала XDG-переменные, иначе домашний каталог;
  IPC-транспорт — `$XDG_RUNTIME_DIR/orbitfm.sock` на Linux, `\\.\pipe\orbitfm-<хеш профиля>` на Windows,
  `$TMPDIR/orbitfm-<uid>/orbitfm.sock` на macOS.

  ```text
  $XDG_CONFIG_HOME/orbitfm/settings.json   (обычно ~/.config/orbitfm/settings.json)
  $XDG_DATA_HOME/orbitfm/library.json      (обычно ~/.local/share/orbitfm/library.json)
  ```

- **Настройки не сохранились или «слетели».** При повреждённом файле программа предупредит и продолжит сессию со
  значениями по умолчанию, не перезаписывая ваш файл. Сброс: `orbitfm quit`, затем переименовать
  `~/.config/orbitfm/settings.json` и `~/.local/share/orbitfm/library.json` в `.bak`. Избранное и история живут в
  `library.json`: их удаление чистит их, но не трогает оформление.
- **Виджет Noctalia не управляет плеером.** Виджет вызывает `orbitfm panel` и читает `orbitfm status` для цвета
  иконки, поэтому нативный бинарник должен быть в `PATH`; с Flatpak-сборкой внешний CLI не работает (сокет в sandbox).

## Безопасность и приватность

Полная модель угроз — в [SECURITY.md](SECURITY.md); здесь то, что полезно знать, не открывая документ.

- **Управлять плеером может любой процесс вашего пользователя.** Команды идут по локальному транспорту: сокет
  `$XDG_RUNTIME_DIR/orbitfm.sock` с правами `0600` на Linux, `$TMPDIR/orbitfm-<uid>/orbitfm.sock` на macOS,
  `\\.\pipe\orbitfm-<хеш профиля>` на Windows. Каталог сокета принадлежит пользователю и закрыт от остальных;
  имя канала Windows своё у каждого профиля. Разграничения прав между процессами одного пользователя нет
  и быть не может — это приложение пользовательской сессии, а не сервис. Никаких сетевых портов OrbitFM не
  слушает вообще.
- **Станции — недоверенные данные.** Имя, страна и теги из общей базы очищаются от управляющих
  последовательностей и двунаправленных символов, а панели и бары получают их без права разметки: имя
  станции не может перекрасить чужой бар или подменить строку терминала.
- **Наружу уходит только необходимое:** запросы к зеркалам Radio Browser (поиск, карточка станции), счётчик кликов
  провайдера при запуске станции, сам поток и адрес обложки; резолв ссылки по явному `play-url` читает плейлист или
  страницу, но не больше 1 МиБ. Телеметрии, аналитики, фоновых опросов и обновлений
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
| **Подтверждено на живом железе macOS** | Артефакт реально запускали на приёмочной машине `MacBookAir8,2` (Intel Core i5-8210Y, macOS 14.8.9): её отчёты вскрыли отсутствие `macos-x86_64.dmg` и бандл `libmpv`, требовавший macOS 26.0 (rc.3), `Library not loaded: /tmp/…/Python.framework` (rc.4) и `Symbol not found: _getVSScriptAPI` (rc.5) — каждое из этих отклонений теперь ловит CI. Успешный старт исправленного бандла с той же машины и первый кадр на Intel UHD 617 **ещё не зафиксированы** |
| **Собрано и протестировано в CI** | Компилируется, линтится и покрыт автотестами на `windows-latest`, `macos-latest` (arm64) и `macos-15-intel` (x86_64); приёмки на живом железе не было: MSI, SMTC на Windows, а по macOS — всё, кроме перечисленного в строке выше |
| **Заявлено планом** | Записано в планах проекта как направление, кода или проверки ещё нет: OrbitFM Web Explorer, Android Auto/companion, UPnP/DLNA |

Полный расклад по macOS — чем обеспечено каждое обещание и что из этого остаётся
непроверенным — в [ADR-0052](docs/adr/0052-матрица-поддержки-macos.md) и в
[матрице поддержки](#матрица-поддержки-macos).

Плагины оболочек пока не в первой категории: ассеты проверены статически
(`qmllint`, `qmlformat`, `node --check`), установщик и бар-скрипт — живым прогоном,
но сами Plasma 6, GNOME, Omarchy и большинство баров на машине приёмки не
запускались. Живьём проверены `noctalia` и `niri`; `hyprland` на машине приёмки
остаётся «не обнаружено» — его правила проверены тестами.

Браузерная панель проверена живым прогоном в Chrome for Testing и Firefox на
Linux: реальный popup через native host, snapshot и изменения через
`orbitfm events`, поиск, избранное, история и «Play this page». Brave, Edge,
Vivaldi и постоянно подписанный Firefox не запускались — для них проверены общий
код, манифесты и установщик; Windows-манифесты через реестр и macOS-раскладка в
пакет не входят. Подробности — в вики: [Браузер](docs/wiki/Интеграции-браузер.md).

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
| [Установка](docs/wiki/01-Установка.md) | AppImage / MSI / два DMG, сборка из исходников, Flatpak, контрольные суммы |
| [macOS: Apple Silicon](docs/wiki/Установка-macOS-Apple-Silicon.md) | DMG `arm64`, Gatekeeper, `PATH`, проверка версии и архитектуры |
| [macOS: Intel](docs/wiki/Установка-macOS-Intel.md) | DMG `x86_64`, Gatekeeper, `PATH`, проверка версии и архитектуры |
| [Первый запуск](docs/wiki/02-Первый-запуск.md) | Две минуты от иконки до звучащей станции |
| [Режимы окна](docs/wiki/03-Режимы-окна.md) | Focus / Overlay / Listening и правила окон для niri и Hyprland |
| [Настройки и язык](docs/wiki/04-Настройки-и-язык.md) | Модальное окно настроек, `settings.json`, 17 языков интерфейса |
| [Управление и горячие клавиши](docs/wiki/05-Управление-и-горячие-клавиши.md) | Мышь, клавиши, медиа-клавиши, команды CLI |
| [Интеграции](docs/wiki/06-Интеграции.md) | Страница «Настройки → Интеграции», установщик, обновление и удаление |
| [Панели и виджеты](docs/wiki/Интеграции-панели.md) | Noctalia, Plasma 6, GNOME, Omarchy, Waybar, Polybar, i3blocks, swaybar, i3status-rust, XFCE, Cinnamon, MATE, LXQt |
| [Браузер](docs/wiki/Интеграции-браузер.md) | Панель OrbitFM в Chrome/Chromium и Firefox, native host |
| [Правила окон](docs/wiki/Интеграции-правила-окон.md) | Готовые правила для niri и Hyprland: размеры, установка, проверка |
| [Трей и MPRIS](docs/wiki/07-Трей-и-MPRIS.md) | Меню трея целиком и управление при скрытом окне |
| [Варианты использования](docs/wiki/08-Варианты-использования.md) | Десять готовых сценариев: фон, язык, второй экран, тишина |
| [Неполадки](docs/wiki/09-Неполадки.md) | Нет звука, квадратики вместо флагов, растянутая капсула, логи |
| [Глоссарий](docs/wiki/10-Глоссарий.md) | Термины проекта человеческим языком |

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
каталогах. Профиль сети меняет объём и частоту фоновых запросов, но не их адресатов и не политику
проверки URL. Появление телеметрии потребует отдельного архитектурного решения и явного включения пользователем.

**(д) Товарные знаки.** Noctalia, niri, Hyprland, mpv, Radio Browser, Rust, Windows, macOS и другие названия и
логотипы принадлежат их владельцам; упоминание не означает аффилиации, одобрения или спонсорства.

**(е) Статус пре-релиза.** До 1.0.0 приложение выходит как `0.x`, и совместимость форматов `settings.json` и
`library.json`, протокола IPC и поведения режимов окна не гарантируется. Правила версионирования — в
[docs/VERSIONMAP.md](docs/VERSIONMAP.md), изменения — в [docs/CHANGELOG.md](docs/CHANGELOG.md).

**(ж) Сообщить о нарушении прав или о проблеме.** Единственный канал проекта — GitHub:
[задачи репозитория](https://github.com/ModernByte/orbitfm/issues) (шаблоны: баг, предложение, ADR).
Об уязвимостях сообщайте приватно, как описано в [SECURITY.md](SECURITY.md), раздел 11; отдельного
контакта для правообладателей нет.

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
[repository issues](https://github.com/ModernByte/orbitfm/issues). Report vulnerabilities privately as
described in [SECURITY.md](SECURITY.md), section 11; there is no dedicated rights-holder contact.

This text is not legal advice; consult your own lawyer if any part is unclear where you live.

## Лицензия

[![Лицензия MIT](https://img.shields.io/badge/license-MIT-green)](LICENSE)

MIT © 2026 ModernByte — полный текст в файле [LICENSE](LICENSE). Именно он задаёт отказ от гарантий и ограничение
ответственности; раздел выше его расширяет, но не отменяет.

---

© ModernByte · [Релизы](https://github.com/ModernByte/orbitfm/releases) ·
[Документация](https://github.com/ModernByte/orbitfm/tree/main/docs)
