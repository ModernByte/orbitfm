# Changelog

Все значимые изменения проекта OrbitFM документируются в этом файле.

Формат основан на [Keep a Changelog](https://keepachangelog.com/ru/1.1.0/),  
проект следует [Semantic Versioning](https://semver.org/lang/ru/).

## [Unreleased]

### Добавлено
- Трей: первая строка меню показывает, что играет (`Now playing: <станция>`,
  `Paused: <станция>` или `Nothing playing`); строка неактивна и не является
  командой
- ADR-0024 (принято) — выделенное скрытое top-level окно Windows как HWND для
  playwire/SMTC: окно на выделенном потоке с циклом сообщений, `WS_POPUP` +
  `WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE`, без `WS_VISIBLE`; дескриптор живёт весь
  процесс, поэтому медиа-контролы переживают скрытие окна (ADR-0018). Рантайм-проверка
  SMTC на Windows остаётся
- ADR-0025 (принято) — безопасная обработка favicon: allowlist PNG/JPEG/WebP по
  сигнатуре, лимиты 512 КиБ / 512 px / 8 МиБ, потоковый лимит ответа, MIME- и
  `http(s)`-allowlist, лимит редиректов и перехват паники декодера
- ADR-0026 (предложено) — macOS: трей в строке меню на `tray-icon` и Now
  Playing при скрытом окне; скрытие через `Visible(false)` сохраняет NSRunLoop,
  Linux/Windows остаются на закрытии вьюпорта и `Session::park` (ADR-0018)
- ADR-0027 (принято) — трёхполосный анализ эфира без PCM-tap: `@level`
  сохраняет шесть агрегатов, `@spectrum` через `acrossover`/`amerge`/`pan`
  отдаёт RMS Bass/Mid/High; побитовое совпадение звука проверено на stereo и mono
- ADR-0028 (принято) — аудио-реактивная деформация глобуса: spring-squash,
  шесть стилей, onset/beat, приоритет камеры, перевороты и Reduce motion
- ADR-0029 (принято) — орбитальное аудиокольцо за глобусом: экранный слой
  painter'а до wgpu-callback'а, 120-столбцовая корона, аврора, комета и искры
  на onset; без новых зависимостей
- Focus: орбитальное аудиокольцо `src/ui/orbit_ring.rs` вокруг планеты —
  haze, орбитальные риски и дуги, волна по centroid, корона из 120 столбцов,
  комета с хвостом и искры; появляется и гаснет вместе с эфиром, следует чипу
  `Visualizer`, в Overlay/Listening не рисуется
- Этап 9, фаза 0: `SpectrumFrame.bands` с RMS Bass/Mid/High из второго
  `@spectrum`-фильтра (`acrossover`/`amerge`/`pan`); stereo/mono-графы,
  отложенное чтение раскладки после `FileLoaded` и fallback на `@level`
  без остановки эфира; App Core передаёт `BandTargets` в `SpectrumTarget`
- Этап 9, фаза 1: ядро аудио-движения — `src/render/motion.rs` с `MotionMixer`,
  spring-squash с сохранением объёма, idle-дыханием 0,12 Гц и возвратом после
  Stop за 300–500 мс; `GlobeDeform`/`GlobeMotion` и uniform `CameraUniform.motion`;
  анизотропный масштаб и трёхлепестковая рябь в WGSL; маркеры, страны, callout и
  hit-test используют ту же деформацию; настройки `motion`, `motion_style`,
  `allow_flips`, `reduce_motion`; секция Motion в меню шестерёнки; аудио-анализ
  включается по требованию (`analysis_needed`); кадры движения 30/15 fps
- Этап 9, фаза 2: onset/beat-детектор в `src/render/motion.rs` — шаг 20 Гц,
  медианная базовая линия без последних 200 мс, рефрактерность 150 мс, период
  60–180 BPM по медиане четырёх интервалов и гейт уверенности; шесть стилей
  движения (Soft Pulse, Groove Sway, Bass Drop, Ambient Drift, Sparkle Twitch,
  Beat March); Auto выбирает стиль по EMA-признакам сигнала с удержанием 3 с и
  кроссфейдом 1 с, без полос ограничен тремя стилями; uniform
  `CameraUniform.motion2` (`flash`, `style_id`, `beat_phase`, `time`) и
  Fresnel-вспышка на onset; `MotionStyle` с `ALL`/`label`/`id`/`requires_bands`
  в настройках и выбор стиля в секции Motion HUD
- `docs/ui/audio-reactive-globe.md` — зафиксированы концепция и план Фаз 0–4
  аудио-реактивной планеты; фазы 0–2 реализованы, остальные впереди
- SRS §3.6 актуализирован под фактически реализованный Этап 8 (0.8.0-rc.1);
  добавлен §3.7 о безопасности favicon

### Изменено
- Чип `Visualizer` теперь управляет только визуализатором и кольцом: если
  включено движение глобуса, аудио-анализ продолжает работать в фоне и
  отключается лишь при `Motion: Off` или `Reduce motion`
- Добавлен repaint-такт движения: ~33 мс при активном эфире и ~66 мс для
  idle-дыхания; при `Off`/`Reduce motion` лишних кадров нет
- Дистанция камеры по умолчанию увеличена с 2.5 до 3.2: планета по-прежнему
  доминирует, но вокруг неё помещается орбитальное аудиокольцо; зум `[1.4, 6.0]`
  не менялся
- Глобус больше не пульсирует от эфира: аудио-модуляция Fresnel-обода и
  геометрический пульс `1 + 0.005·pulse` удалены, обод остаётся статичным до
  Этапа 9; `SpectrumOverlay::pulse()` сохраняется для кольца Listening
- social preview репозитория добавлен в `assets/social-preview.jpeg`
- `image`: фича `ico` заменена на `webp`; favicon в ICO/GIF/SVG больше не
  декодируются и заменяются инициалом станции

## [0.8.0-rc.1] - 2026-10-24

Второй публичный пре-релиз: Application Core, честное скрытие окна, системный
трей, режимы окна Overlay/Listening и доступность (Этап 8, фаза 7). Пакеты:
AppImage для Linux x86_64, MSI для Windows x64 и DMG для macOS — все без подписи
и нотаризации. Flatpak в этот пре-релиз не включён: манифест и vendored-источники
проверены CI, но для сборки нужны `flatpak-builder` и runtime 25.08.

### Добавлено
- ADR-0018 (принято, редакция 2) — честное «close = hide» на целевом Wayland/niri:
  скрытие окна означает закрытие root-вьюпорта и остановку UI-цикла, процесс, эфир,
  IPC и MPRIS продолжают жить; показ — повторный `eframe::run_native` из главного
  потока с восстановленным размером окна (замер на niri 26.04: ≈1.02 с от IPC-`show`
  до окна в раскладке)
- Новый слой Application Core `src/session/`: `Session` (подсистемы + состояние,
  живущее между показами окна), `AppServices`, `WindowState`, `Notice`, `UiIntent`,
  `WakeHandle`, `ParkOutcome` и подмодули `commands`, `playback`, `search`, `markers`,
  `park`, `tray`
- Припаркованный цикл `Session::park()` — `select!` по IPC, MPRIS, трею, аудио-событиям
  и ответу рулетки без окна: `play`, `stop`, `volume` и `status` исполняются при
  закрытом окне, как и все команды MPRIS и трея (включая смену темы и рулетку);
  `show`/`toggle`/`quit` выводят цикл
- Контракты нового слоя: `docs/interfaces/10-session.md`, `docs/modules/11-session.md`;
  состав Application Layer зафиксирован в `docs/architecture/00-overview.md` (§4.1)
- ADR-0020 (принято, реализован) — иконка в системном трее по SNI: `src/tray/` на
  `ksni` 0.3.7 (`blocking`) — процедурная иконка-орбита, `Activate` как toggle,
  `SecondaryActivate`, колесо громкости ±0.10 и меню (play/pause, stop, prev/next,
  roulette, тема, show/hide, quit) с живым состоянием
- Трей — третий источник команд наравне с IPC и MPRIS в том же dispatch: `TrayCommand`
  исполняется `Session::execute_tray_command`, состояние приложения публикуется одним
  `TraySnapshot` только при изменении; отдельного протокола и бизнес-логики у трея нет
- Поток трея — обычный `std::thread` (`orbitfm-tray`): снимки сворачиваются до
  последнего, прежде чем поток вызовет блокирующий `Handle::update`; без SNI-хоста
  (`watcher_offline`) подключение сохраняется, а `Session` отключает ветку трея в
  `select!`, так что деградация не превращается в busy-loop
- Контракты трея: `docs/interfaces/11-tray.md`, `docs/modules/12-tray.md`; проверка
  реализации на живом хосте Noctalia зафиксирована в ADR-0020 (регистрация, свойства
  через `busctl`, команды при закрытом окне, показ окна ≈986 мс, чистый quit)
- ADR-0022 (принято, реализован) — визуализатор в стиле cava на шести агрегатах
  эфира: поле 156×46 px, столбцы с зазором 2 px и скруглением 1 px, сглаженная
  атака 28 мс, линейная гравитация спада 1,8 ед/с, удержанные пики с падением
  0,42 ед/с и отдельной шапкой 2 px, градиент по абсолютной высоте поля; мелкая
  фактура столбцов зависит от `flux` (не выдаётся за FFT)
- ADR-0023 (принято, реализован) — геометрический пульс глобуса в такт эфиру
  (`1 + 0.005 · pulse` в вершинном шейдере) и авто-вращение после 4 с простоя:
  разгон 2 с, 0,006 рад/с, континенты движутся на восток; простой считается по
  реальному интервалу кадра, в простое достаточно 30 fps
- Маркеры глобуса по обновлению ADR-0012: единый радиус 3 px и один цвет,
  пульсирующее кольцо у играющей станции, при наведении — белое кольцо и
  подпись с качеством (битрейт, кодек) и рейтингом (голоса)
- ADR-0021 (принято, реализовано в Core, IPC и UI) — режимы окна
  `WindowMode::{Focus, Overlay, Listening}`: состояние `WindowState` в
  `src/session/window.rs` с размерами и заголовком режима; глагол IPC `mode`
  (запрос и `mode focus|overlay|listening`), строка `mode <…>` в `status`;
  заголовки режимов разведены в правилах Niri/Hyprland, поэтому каждый режим
  открывается со своим размером
- Этап 8, фаза 7 — капсульные раскладки `LayoutDensity`: `src/ui/layout.rs`
  выводит плотность из режима окна (Focus — полный Orbital Desk, Overlay —
  Compact, Listening — Minimal); `src/ui/capsule.rs` даёт Overlay верхнюю
  HUD-строку (станция, play/stop, громкость, expand/close), мини-глобус и капсулу
  избранного до 8 строк, Listening — одну строку now-playing с пульсирующим
  кольцом аватара при Playing; сглаженный `pulse` считается один раз в `app.rs` и
  общий для глобуса и капсулы; возврат в Focus — expand в капсуле или раздел
  «Window» в попапе шестерёнки. Проверено живьём на Wayland/niri в тёмной и
  светлой теме: Focus 920×660, Overlay 420×620, Listening 420×152 (размер капсул —
  плавающим окном без изменения WM-конфига)
- Этап 8, фаза 7 — доступность: focus-ring и `WidgetInfo` с ролями/подписями у
  кастомных контролов (`Role::Button`/`RadioButton`/`CheckBox`/`Tab`, `ListItem`
  у строки, `slider` у громкости), клавиатурный шаг громкости стрелками/Home/End,
  навигация ↑/↓ по видимому списку станций со `scroll_to_me`; Space/Enter
  запускают станцию штатным egui-фокусом; навигация покрыта headless-тестами

### Изменено
- `src/app.rs` из оркестратора (1506 строк) стал чистым presentation-слоем (553
  строки): `OrbitApp<'a>` заимствует `Session` и живёт ровно один прогон UI-цикла;
  побочные эффекты `apply_action` маршрутизируются в методы `Session`
- Читаемое состояние кадра собрано в снапшот `Frame::capture`, зоны раскладки
  возвращают `Vec<UiAction>` вместо накопления общего вектора
- `SearchMode` переехал в `src/session/search.rs` (UI делает только ре-экспорт),
  `LibraryTab` — в `src/session/mod.rs`; тесты маркеров — в `src/session/markers.rs`
- Удалены механизмы отклонённой редакции 1 ADR-0018: сжатие поверхности до 1×1,
  `ViewportCommand::Visible`, `ViewportCommand::CancelClose`, `AppState`
- Документация `docs/interfaces/07-app.md` и `docs/modules/07-app-core.md`
  переписана под presentation-модель (жизненный цикл окна, `logic`/`ui`, инициализация)
- `Session::tick()` и `Session::end_window()` исполняют команды трея и публикуют
  `TraySnapshot`; порядок шагов кадра зафиксирован в `docs/modules/11-session.md` (§ 4.10)
- Разбор ответа рулетки вынесен из `poll_roulette` в `Session::apply_roulette`: один и
  тот же путь для кадра UI и припаркованного цикла, поэтому рулетка из меню трея
  работает и с закрытым окном
- Сборка без фичи `tray` (`cargo build --no-default-features`) остаётся рабочей:
  `tray::spawn` возвращает `None`, SNI-код и `icon.rs` скрыты под `cfg`
- Визуализатор работает без ручного включения: воркер подключает `af`-фильтр
  спектра при старте, `Session` инициализирует зеркало из движка; чип
  «Visualizer» остаётся выключателем до конца сессии
- Правая группа transport расширена до 410 px под поле 156×46: на узких окнах
  название станции раньше обрезается, зато cava-столбцы читаются
- Сглаженный сигнал эфира считается один раз за кадр в `draw_layers` и передаётся
  в `paint_globe` и капсулы; `globe_view` больше не обновляет `SpectrumOverlay`
  сам, поэтому мини-глобус и Listening получают тот же `pulse`

### Исправлено
- MPRIS больше не занижает способности: `CanGoNext`/`CanGoPrevious` рекламируются,
  когда видимая очередь непуста, и выключаются вместе с no-op на пустом списке;
  раньше `Next`/`Previous` исполнялись, хотя клиенту не обещались
- Стеклянные панели больше не распадаются на полосы и не теряют скруглённые
  углы: градиент рисуется одним мешем по точному контуру `rounded_rectangle`,
  сверху добавлен тонкий rim; прежние семь плоских срезов удалены
- `тулtip` → `tooltip` в записи CHANGELOG о подсказке страны на глобусе

## [0.7.0-rc.1] - 2026-10-23

Первый публичный пре-релиз: UX «Orbital Desk» (Этап 8, фазы 1–6) и визуальная
доводка по эскизу (Этап 9). Пакеты: AppImage и Flatpak для Linux x86_64, MSI для
Windows x64, DMG для macOS — все без подписи и нотаризации.

### Добавлено
- Этап 8, фаза 6 — полировка UI по эскизу «Orbital Desk»:
  - плавающая раскладка: full-bleed глобус со звёздным небом, поверх — HUD,
    панель станций, transport, help-карточка и легенда `Station density`;
    геометрия зон в `ui/layout.rs` (`DeskLayout`), клики глобуса внутри зон
    отсекаются `blocks_point`
  - кастомные контролы в `ui/widgets.rs`: pill-переключатель, жанровые чипы,
    pill-поле поиска с лупой и popup-подсказками, вкладки с иконками и
    accent-подчёркиванием, LIVE-бейдж с пульсом, мини-эквалайзер, volume-слайдер,
    круглая play/pause, круглый favicon-аватар, знак OrbitFM, флаг-эмодзи ISO→emoji
  - асинхронный favicon-загрузчик (ADR-0016): `ui/images.rs` (`FaviconCache`,
    состояния Queued/Loading/Ready/Failed, лимит 8 запросов, LRU-эвикция до 256),
    декод `image` (features `png`, `ico`) к 64×64, запросы спавнит `OrbitApp`
  - строка списка `ui/station_row.rs`: аватар, имя, `state, country`, теги-чипы
    (клик — глобальный поиск по тегу), битрейт/кодек, звезда; играющая строка —
    accent-рамка с эквалайзером; переключатель плотности `Compact`
  - transport по эскизу: now-playing с favicon и `Now Playing: <теги>`, LIVE+eq,
    `⏮ play ⏭`, stop, heart, кнопка `Radio Roulette` с кубиком, `CODEC kbps`
    и мини-спектр из FFT-метрик (статичные полоски при выключенном FFT)
  - prev/next идут по текущему видимому списку с циклом (`play_relative`):
    кнопки transport и MPRIS `Next`/`Previous` (ранее — no-op)
  - tooltip страны на глобусе с флагом-эмодзи и числом станций; постоянная
    help-карточка вместо авто-скрываемой подсказки первого запуска
  - цветной emoji-шрифт для флагов (ADR-0017): `epaint` подключён прямой
    зависимостью с feature `color_fonts` (новых crate'ов нет), `OrbitApp::new`
    ставит системный `NotoColorEmoji.ttf` в запас `Proportional`; флаг остаётся
    только в тултипе глобуса, контекстная строка панели — текстовая `Country: …`
  - все надписи интерфейса переведены на английский (Country/Genre, Search…,
    Results/Favorites/History, Sort by, Popularity/Bitrate/Name, LIVE,
    Now Playing, Station density, тосты и сообщения ошибок)
- Этап 8, фаза 5 — живой глобус:
  - uniform камеры расширен позицией `eye` и `pulse` 0..1; `CameraUniform::uniform`
    ограничивает сигнал и заменяет нечисловое значение нулём
  - Fresnel-обод атмосферы в WGSL: яркость `0.14 + 0.55·pulse`, ширина от
    `mix(3.2, 1.8, pulse)`, цвет сдвигается от голубого к бирюзовому
  - `SpectrumOverlay::pulse()` — сглаженный сигнал `0.75·level + 0.25·flux`,
    равный нулю без данных; `globe_view` передаёт его в `paint_globe`, поэтому
    при Playing атмосфера пульсирует, при Stop и выключенном FFT затухает
  - heatmap выбранной страны: заливка с альфой 26..96 по логарифму количества
    загруженных станций (`GlobeMarker.count`), hover-заливка только при наличии
    станций; контур наведения без изменений
  - новые тесты: pulse без данных и по level/flux, uniform камеры, лог-шкала
    heatmap и суммирование группированных маркеров
- Этап 8, фаза 4 — Radio Roulette и MPRIS:
  - `RadioBrowserClient::random_station()`: `order=random&limit=1&has_geo_info=true&
    hidebroken=true`, до трёх попыток, отбрасывание станций без потока, координат
    и с координатами `(0, 0)`; после повторов — `NotFound`
  - кнопка «Рулетка» в transport: случайная станция запускается, камера перелетает
    к её координатам за 900 мс, тост показывает название и страну; повторный
    запрос блокируется до ответа, ошибка не прерывает текущий эфир
  - перелёт отменяется перетаскиванием, как и доворот к выбранной стране
  - MPRIS artist обогащён: «штат, страна» (или одно поле), иначе теги станции;
    artUrl остаётся favicon, title/url без изменений
  - новых IPC-команд нет: Roulette доступна только из GUI
- Этап 8, фаза 3 — режим «Жанр»:
  - `SearchMode { Country, GlobalGenre }` и переключатель в HUD; смена режима
    сбрасывает контекст, результаты, маркеры и локальные фильтры
  - popup выбора жанра: восемь пресетов (Rock, Pop, Jazz, Electronic, News,
    Metal, Classical, Synthwave) и поле своего тега; выбор запускает tag-поиск
    `limit=200` с гео-запросом `has_geo_info=true`, `limit=200`
  - `SearchContext` (страна или жанр); при активном контексте поле HUD работает
    как локальный фильтр (кнопка «Фильтр»), в свободном поиске — «Найти»
  - поколение поиска: `SearchResponse` несёт номер, ответы прошлого
    режима/жанра отбрасываются и не возвращают устаревшие маркеры
  - `MarkerVisual.weight` и веса из голосов/битрейта (sqrt к 5000 и линейно
    к 320, смесь 70/30); радиус `3.5 + 3.5·weight`, активная станция +2 px,
    свечение растёт с весом
  - панель в режиме жанра показывает `Жанр: …` и локальные фильтры; станции без
    координат остаются в списке, клик по стране в этом режиме — no-op
  - лимиты свободного поиска подняты до 100/100; маркеры жанра — до 200
- Этап 8, фаза 2 — режим «Страна»:
  - `Station` дополнен `votes`, `country`, `state` с `serde(default)`; старые
    `library.json` читаются без миграции
  - клик по стране и поиск в HUD запускают `countrycode`-поиск с лимитом 100;
    гео-запрос для маркеров использует тот же лимит
  - HUD «Страна»: подсказки по названию или ISO-коду (до 6), Enter выбирает
    первую страну, выбор мягко доворачивает камеру за 650 мс
  - hover страны рисует контур без заливки, выбранная страна — акцентный контур
    с полупрозрачной заливкой; маркер станции по-прежнему приоритетнее
  - панель страны: заголовок и число станций, текстовый фильтр по
    `name/tags/country/state`, чипы All/Rock/Pop/Jazz/Electronic/News/Metal,
    сортировка «Популярность/Битрат/Имя», карточки с локацией, kbps, codec,
    тегами, голосами и звездой
  - пустой результат фильтров — «В этом секторе пока тихо» со сбросом фильтров;
    фильтры локальные и не выполняют сетевых запросов
  - `GeoService`: `CountryShape`, `search_countries`, `country_shape`,
    `CountryShape::focus_point` с корректным центром на антимеридиане
  - `render/countries.rs`: проекция колец с разрывом на горизонте и заливка
    методом уха; без доменных типов и GPU-ресурсов
  - `GlobeCamera::fly_to` — плавный доворот по кратчайшей дуге, отменяется
    перетаскиванием
- Этап 8, фаза 1 — UI-каркас «Orbital Desk»:
  - UI вынесен из `app.rs` в `src/ui/` (`hud`, `station_panel`, `transport`,
    `globe_view`, `toast`, `palette`, `icons`); модули возвращают `UiAction`,
    побочные эффекты выполняет `OrbitApp`
  - Focus-раскладка: HUD 58 px, глобус в центре, drawer 310 px справа, transport
    48 px; при ширине окна < 900 px drawer становится overlay с кнопкой закрытия
  - векторные иконки play/pause, stop и звезды вместо Unicode-глифов, которых нет
    в шрифте egui; длинные названия станций обрезаются с многоточием
  - тосты (3 с) для ошибок и подтверждений, подсказка первого запуска (10 с)
  - темы и акцент перенесены в меню «Настройки», FFT — в нижний transport
- UX-редизайн «Orbital Desk» зафиксирован документально (Этап 8):
  - `docs/ui/design-vision.md` — целевой UX, референс-макет Focus, режимы
    Country/GlobalGenre, состояния и критерии приёмки
  - `docs/ui/implementation-plan.md` — шесть фаз, изменения модулей `ui`/`geo`/
    `render`/`radio`, планируемые контракты, тесты и Definition of Done
  - ADR-0015 (запланировано): UI выносится в `src/ui/`, вводятся `SearchMode`,
    локальные фильтры, контур страны, `fly_to`, `random_station` и Fresnel pulse;
    формат library/settings не меняется
  - ROADMAP Этап 8 и VERSIONMAP 0.7.0/0.8.0; внешняя публикация плагина и
    установщиков по-прежнему не выполняется
- Нативные CI-джобы:
  - macOS: libmpv из Homebrew, `fmt`, `clippy`, тесты и release-сборка
  - Windows: `fmt` и `clippy`
- Сборка нативных установщиков в CI:
  - Windows MSI (джоба `msi`): MSYS2 UCRT64 с libmpv 0.41.0, импорт `mpv.lib`
    через `dumpbin`/`lib.exe`, сбор DLL-замыкания, WiX 5, per-user установка в
    `%LOCALAPPDATA%\Programs\OrbitFM` с ярлыком в меню «Пуск» и smoke-тестом
    `msiexec`
  - macOS DMG (джоба `dmg`): `.app` с `Info.plist` и `.icns`, `dylibbundler`,
    ad-hoc подпись, UDZO DMG со ссылкой на `/Applications` и проверкой образа
  - артефакты `OrbitFM-windows-x64-msi` и `OrbitFM-macos-dmg`; без подписи
    сертификатами и без нотаризации
- Windows IPC: `src/ipc/pipe.rs` — named pipe `\\.\pipe\orbitfm`, single-instance
  через первый экземпляр; Unix-транспорт выделен в `socket.rs`, сервер и клиент общие
- Этап 6 — Flatpak для Linux:
  - манифест `packaging/linux/org.modernbyte.OrbitFM.yml`: runtime
    `org.freedesktop.Platform` 25.08, Rust 1.98 из SDK-расширения
  - libmpv 0.41.0, libass 0.17.5 и libplacebo 7.360.1 собираются из исходников;
    FFmpeg 7.1, GnuTLS, PulseAudio и PipeWire берутся из runtime/SDK
  - `packaging/linux/flatpak-cargo-sources.json` с vendored crate-исходниками;
    сборка идёт офлайн
  - `packaging/linux/build-flatpak.sh` собирает и ставит приложение в
    пользовательскую установку Flatpak
  - проверено: запуск окна, воспроизведение станции по UUID, MPRIS и IPC;
    публикация в Flathub не выполняется
- Этап 7 — плагин Noctalia как источник каталога:
  - каталог плагина переименован `noctalia-plugin/` → `orbitfm/`: имя подкаталога
    должно совпадать с частью id после `/`
  - `plugin.toml`: допустимые теги `bar`/`audio`/`music`, зависимость
    `orbitfm`; переводы переведены на вложенные ключи `widget.*`
  - `catalog.toml` в корне индексирует плагин для git-источника
    `ModernByte/orbitfm`
  - `orbitfm/README.md` с требованиями и использованием, карточка
    `orbitfm/thumbnail.webp` 960×540 WebP
  - подключение через `noctalia msg plugins source add orbitfm git
    https://github.com/ModernByte/orbitfm`
  - манифест проверен `noctalia plugins lint` и официальным
    community-валидатором: 0 ошибок
  - внешняя публикация в `noctalia-dev/community-plugins` не выполняется:
    плагин распространяется как git-источник `ModernByte/orbitfm`
- Этап 7 — темы и кастомизация интерфейса:
  - модуль `src/settings/`: `SettingsStore`, `ThemeMode`, `AccentColor`,
    `SettingsError`, `default_path`; настройки не зависят от egui
  - хранение в `$XDG_CONFIG_HOME/orbitfm/settings.json` (fallback
    `$HOME/.config/orbitfm/settings.json`), атомарная запись через
    `settings.json.tmp` + `rename`
  - тема `system`/`dark`/`light` через `egui::ThemePreference`; прозрачные заливки
    панелей и окна сохранены для overlay-режима
  - шесть пресетов акцента (Орбита, Изумруд, Янтарь, Фиалка, Роза, Океан);
    выделение, гиперссылки и палитра карточки следуют активной теме
  - `MarkerColors` в рендере: обычные маркеры используют акцент, активный маркер
    остаётся тёплым янтарным
  - блок «Оформление» в карточке; ошибки чтения/записи показываются в статусе и не
    прерывают воспроизведение, повреждённый файл не перезаписывается
  - unit-тесты XDG-путей, roundtrip, значений по умолчанию, повреждённого файла и
    эфемерного хранилища
- ADR-0014: Темы и кастомизация
- Этап 7 — избранное и история станций:
  - модуль `src/library/`: `StationLibrary`, `LibraryError`, `default_path`;
    снимки `Station` для офлайн-запуска, уникальность по `stationuuid`
  - хранение в `$XDG_DATA_HOME/orbitfm/library.json` (fallback
    `$HOME/.local/share/orbitfm/library.json`), атомарная запись через
    `library.json.tmp` + `rename`
  - история ограничена 20 станциями, повторный запуск перемещает её в начало;
    при загрузке дубликаты удаляются, история обрезается до лимита
  - повреждённый файл не перезаписывается: приложение переходит на эфемерную
    библиотеку и показывает предупреждение
  - `serde_json` переведён в обычные зависимости
  - вкладки «Результаты / Избранное (N) / История (N)», звезда у текущей станции и
    строк списка, кнопка «Очистить историю»; новый поиск возвращает вкладку
    «Результаты»
  - успешный запуск станции (результат, сохранённая станция, маркер, IPC `play`)
    пополняет историю; возобновление из MPRIS историю не меняет
  - unit-тесты XDG-путей, roundtrip, переключения избранного, дедупликации,
    лимита истории, повреждённого файла и эфемерной библиотеки
- ADR-0013: Избранное и история станций
- Этап 7 — маркеры станций на глобусе:
  - `Station` дополнен `geo_lat`/`geo_long`, `SearchParams` — `has_geo_info`;
    гео-фильтр `has_geo_info=true` в `/json/stations/search`
  - каждый поиск параллельно запрашивает до 60 станций с координатами; ошибка
    гео-запроса не влияет на список результатов
  - `GlobeCamera::project_on_sphere` — проекция точки сферы в экранные координаты
    с отсечением задней полусферы и viewport
  - `src/render/markers.rs`: `MarkerVisual`, `sphere_point`, `paint_markers` —
    egui-оверлей поверх глобуса без GPU-ресурсов и доменных типов
  - группировка станций с координатами, округлёнными до 2 знаков, дедупликация по
    UUID; на маркере показывается количество
  - наведение в радиусе 10 px показывает подпись, клик по маркеру приоритетнее
    выбора страны, текущая станция подсвечена
  - строка «Маркеры: N станций с координатами» в карточке
  - unit-тесты разбора координат, query, проекции и группировки маркеров
- ADR-0012: Маркеры станций на глобусе
- Этап 6 — упаковка Linux:
  - `packaging/linux/` — desktop entry, AppStream metainfo и масштабируемая иконка
  - `build-appimage.sh`: release-сборка, бандлинг libmpv/FFmpeg через linuxdeploy,
    сборка `OrbitFM-<version>-x86_64.AppImage` в `target/appimage`
  - AppImage проверен запуском, `--version` и воспроизведением станции через IPC
  - CI-джоба валидации desktop entry, AppStream и синтаксиса скрипта
- ADR-0011: поставка Linux через AppImage; Flatpak, MSI, DMG и кроссплатформенные
  медиа-интеграции отложены до появления нативных раннеров
- Этап 6 — экспериментальный FFT-визуализатор:
  - `SpectrumFrame` в аудио-ядре: centroid/spread/rolloff/flatness/flux и RMS
    из `af-metadata` фильтров `aspectralstats` + `astats`, опрос раз в 50 мс
  - `AudioEngine::set_spectrum_enabled`, `spectrum_enabled`, `spectrum`; при
    недоступности фильтра — `AudioError::Spectrum` без прерывания воспроизведения
  - `src/render/spectrum.rs`: сглаженный оверлей в лог-шкале 30 Гц–20 кГц,
    градиентная огибающая, затухание и подпись `FFT · <частота> · <RMS> дБ`
  - чекбокс «FFT» в заголовке (выключен по умолчанию), ошибки в карточке
  - живой тест `produces_fft_metrics_for_live_stream` (вне CI)
- ADR-0010: FFT-визуализатор
- ADR-0011: Упаковка Linux на агрегированных метриках без PCM-tap
- Этап 5 — интеграции Noctalia и оконных менеджеров:
  - `noctalia-plugin/`: манифест `plugin.toml` (id `modernbyte/orbitfm`,
    `plugin_api = 3`), виджет `plugin.luau` с глифом/подсказкой и отправкой
    `orbitfm toggle` через `noctalia.runAsync`
  - переводы `translations/en.json` и `translations/ru.json`
  - манифест проверен `noctalia plugins lint` (0 ошибок, 0 предупреждений)
  - `wm-configs/orbitfm-niri.kdl` — плавающее окно 920×660, проверено
    `niri validate`
  - `wm-configs/orbitfm-hyprland.conf` — float/size/center/stayfocused
    через `windowrulev2`
- Этап 4 — 3D-глобус и выбор страны:
  - `assets/geojson/world-110m.json` — Natural Earth 110m Admin 0 Countries
    (public domain), встроен через `include_str!`
  - `assets/textures/earth.jpg` — текстура Земли 2048×1024 (public domain),
    встроена через `include_bytes!`
  - `src/geo/`: `GeoService` с R-Tree по bounding box и Point-in-Polygon
    (`geo::Contains`), преобразование точки сферы в широту/долготу,
    fallback `ISO_A2_EH` для стран без `ISO_A2`
  - `src/render/`: `GlobeRenderer` на wgpu (UV-сфера 64×32, текстура sRGB,
    ambient + directional light, backface culling без depth buffer)
  - `GlobeCamera`: орбитальное вращение ПКМ + drag с инерцией, зум колесом
    с ограничением дистанции, `screen_to_sphere` через inverse view-projection
  - интеграция через `egui_wgpu::Callback`: GPU-ресурсы в `CallbackResources`,
    камера и ввод — в `OrbitApp`
  - клик по стране запускает поиск `countrycode=<ISO alpha-2>` (до 30 станций,
    `hide_broken=true`), заголовок `Страна: …` и состояние «Станции не найдены»
  - заглушка «3D-глобус недоступен», если wgpu-состояние отсутствует
  - unit-тесты geo (страны, антимеридиан, ISO fallback, сфера) и render
    (геометрия сферы, камера, raycast, инерция, зум)
- Этап 3 — Radio Browser и запуск станций:
  - `src/services/`: `RadioBrowserClient` с ленивым DNS-discovery
    `all.api.radio-browser.info`, дедупликацией IP и случайной ротацией пула
  - отдельный `reqwest::Client` на каждый IP с `.resolve` (TLS/SNI остаются на имени
    хоста), User-Agent `OrbitFM/1.0 (ModernByte; Linux; Rust)`, таймаут 5 с
  - failover по пулу для сетевых ошибок/таймаутов/`5xx`/`429`/`408`, повторный
    discovery и один повтор; остальные `4xx` возвращаются сразу
  - поиск `/json/stations/search` (`hidebroken`, сортировка по голосам), станция по
    `/json/stations/byuuid`, click `/json/url`; in-memory кэш до 512 станций
  - `play <uuid>`: станция из результатов поиска или каталога (таймаут 8 с),
    воспроизведение `url_resolved`, асинхронный click
  - поиск станций в окне (поле + Enter/«Найти», до 30 результатов) и запуск по клику
  - MPRIS-метаданные из станции: название, теги, favicon, `url_resolved`
  - `status` дополнен строками `stationuuid` и `name` при активной станции
  - фоновый прогрев пула серверов при старте (ошибка не фатальна)
  - unit-тесты клиента и живой тест `searches_and_fetches_live_station` (вне CI)
- Этап 2 — аудио-ядро и базовый MPRIS:
  - `src/audio/`: безопасная обёртка `libmpv2` в выделенном потоке `orbitfm-audio`,
    команды через каналы tokio с таймаутом 3 с
  - состояния `Stopped`, `Playing`, `Paused`, `Buffering`, `Reconnecting`, `Error`;
    события `Started`, `Stopped`, `Buffering`, `Reconnecting`, `Error`, `EndOfFile`
  - play / pause / resume / stop / volume; громкость по умолчанию 0.7
  - реконнект: до 5 попыток с задержками 1, 2, 4, 8, 16 с
  - `LC_NUMERIC=C` и опции mpv `config=no`, `terminal=no`, `idle=yes`, `vid=no`,
    `network-timeout=10`; отслеживание `paused-for-cache`
  - `src/media/`: MPRIS v2 через playwire (`org.mpris.MediaPlayer2.OrbitFM`),
    метаданные, громкость, Play/Pause/PlayPause/Stop; пробуждение GUI через `WakeFn`
  - IPC `stop` и `volume` управляют аудио-ядром; `status` возвращает
    `status <state>`, `volume <0.00>`, `window visible|hidden`
  - живой тест `plays_and_stops_live_stream` (по запросу, вне CI)
- Этап 1 (MVP-0) — каркас приложения:
  - crate `orbitfm` (Rust 2024, MSRV 1.95), зависимости eframe/egui 0.37, wgpu 30, tokio 1.54
  - single-instance IPC через Unix Domain Socket (`$XDG_RUNTIME_DIR/orbitfm.sock`)
  - CLI: `toggle`, `show`, `hide`, `status`, `play`, `stop`, `volume`, `quit`, `-h`, `-V`
  - eframe/egui-скелет с прозрачным окном overlay и app-id `org.modernbyte.OrbitFM`
  - закрытие окна = Hide (`ViewportCommand::CancelClose` + `Visible(false)`), процесс продолжает работу
  - `status` на Этапе 1: `status stopped` + `window visible|hidden`
  - unit- и интеграционные тесты IPC (13 тестов)
- SRS v3.0
- Обзор архитектуры (`docs/architecture/00-overview.md`)
- ADR-0001: Фиксировать архитектурные решения
- ADR-0002: Выбор GUI-стека (eframe + egui + egui-wgpu)
- ADR-0003: Выбор аудио-движка (libmpv)
- ADR-0004: Выбор клиента Radio Browser
- ADR-0005: Single-instance и IPC
- ADR-0006: Медиа-контроли (MPRIS / системные кнопки)
- ADR-0007: Интеграция с Noctalia v5
- ADR-0008: Геопространственный поиск (Raycasting + Point-in-Polygon)
- Модульные спецификации (`docs/modules/`):
  - 01-audio-engine
  - 02-radio-browser
  - 03-geo
  - 04-render-globe
  - 05-ipc
  - 06-media-controls
  - 07-app-core
  - 08-noctalia-wm
- Интерфейсы (`docs/interfaces/`):
  - 00-overview
  - 01-audio
  - 02-radio
  - 03-geo
  - 04-render
  - 05-ipc
  - 06-media
  - 07-app
- ROADMAP
- VERSIONMAP
- CONTRIBUTING
- AGENTS
- README
- AI-скиллы (`.agents/skills/`):
  - orbitfm-core, orbitfm-architect, orbitfm-rust, orbitfm-patterns
  - orbitfm-gui, orbitfm-audio, orbitfm-radio
  - orbitfm-geo, orbitfm-ipc, orbitfm-integration, orbitfm-docs
- `docs/patterns/00-overview.md` — каталог паттернов
- ADR-0009: каталог паттернов
- ADR-0010: FFT-визуализатор
- ADR-0011: Упаковка Linux
- Шаблоны GitHub: PR, issue (bug/feature/adr), COMMIT_CONVENTION
- `docs/templates/MERGE_CHECKLIST.md`, `PROJECT_STATUS.md`

### Изменено
- Этап 9, визуальная доводка по `docs/ui/visual-spec.md` (в работе):
  - аудиовизуализатор вместо FFT-strip (ADR-0019): `render/visualizer.rs`
    синтезирует 16/24/32 столбца из шести агрегатов mpv (centroid, spread,
    rolloff, flatness, flux, RMS) с attack/decay и peak-hold; окраска
    зелёный → жёлтый → красный семью горизонтальными срезами на столбец.
    Полосы — оценка огибающей, не бинный FFT; `SpectrumOverlay` оставлен только
    как источник пульса Fresnel-обода, его отрисовка удалена
  - громкость переехала из HUD в правую часть транспортной панели рядом с
    визуализатором; `HudState.volume` убрано
  - маркеры станций: радиус — из популярности (`smoothstep(log1p(votes))`,
    3.5…13 px), цвет — из тира битрейта (ТЗ §2.3), станция в эфире получает
    `palette.live`, внешнее кольцо и пульс `1.0 ↔ 1.15` с периодом 1.4 с;
    наведение осветляет цвет на 30 %. `MarkerVisual.weight` и веса из
    голосов/битрейта заменены на `popularity` + `bitrate` + `playing`,
    `paint_markers` принимает время интерфейса; `GlobeMarker` хранит UUID
    участников группы и максимальный битрейт, поэтому маркер городка пульсирует,
    даже если играет не станция-представитель; hover-подпись маркера стала
    двухстрочной по ТЗ §2.4 — имя и `место · kbps · кодек`
  - country callout вместо тултипа под курсором (ТЗ §1): стеклянная карточка с
    флагом-эмодзи, названием и «{N} stations», leader line 1 px к якорю
    `country_shape(iso).focus_point()` и точка-пин; hover α 0.85, select α 1.0,
    кроссфэйд 150 мс через `CalloutState` (уходящая карточка догорает на своих
    кадрах), flip над/под якорем и clamp между HUD, transport и панелью станций;
    в режиме «Жанр» карточка не показывается, `paint_country_tooltip` удалён
  - градиент столбцов визуализатора привязан к высоте области, а не к высоте
    столбца: короткий столбец остаётся зелёным, жёлтый и красный появляются
    только на подъёме (ТЗ §3.3)
  - help-карточка больше не висит постоянно: поле `Settings::help_seen`,
    первый запуск — видно, дальше только по `?` (закрытие — `Esc` или `×`)
  - `Settings::viz_bars` (16/24/32, по умолчанию 24) с выбором в попапе
    шестерёнки; значение вне списка приводится к ближайшему допустимому
- plugin ID Noctalia приведён к обязательному формату `<author>/<plugin>`:
  `modernbyte/orbitfm` вместо `org.modernbyte.orbitfm`; app-id окна
  `org.modernbyte.OrbitFM` не изменился
- пример плагина и документация переведены с несуществующего `exec_async` на
  `noctalia.runAsync`; SRS и модуль 08 дополнены проверенными манифестом и правилами
- зависимости: добавлены `egui-wgpu` 0.37, `wgpu` 30, `glam` 0.34, `bytemuck` 1.25,
  `geo` 0.33, `geojson` 1.0, `rstar` 0.13, `image` 0.25 (jpeg)
- поиск станций разделён на поиск по названию и по стране; общий канал результатов
  и `WakeFn`
- `stop`/`error` больше не сбрасывают последний URL: MPRIS `Play` перезапускает поток
- зависимости: добавлены `reqwest` 0.13 (rustls, json, query), `serde`, `thiserror`
- аудио-стек уточнён под ADR-0003: безопасная обёртка `libmpv2` вместо прямого
  `libmpv-sys`; MPRIS реализован через `playwire` (ADR-0006)
- CI устанавливает `libmpv-dev` для сборки и тестов
- IPC-контракт уточнён под реализацию: `IpcRequest` с oneshot-ответом, `commands(&mut self)`,
  `run(WakeFn, shutdown)`, завершение ответа по EOF, лимит команды 4096 байт, таймауты,
  права сокета `0600`
- `IpcServer` описан как конкретная структура (трейт не используется)
- MSRV в SRS: Rust 1.85+ → 1.95+ (требование eframe 0.37)
- app-id в примерах приведён к `org.modernbyte.OrbitFM`, plugin ID — к `org.modernbyte.orbitfm`
- Именование приведено к стандартам ModernByte
- AGENTS.md — команда субагентов, делегирование, git/docs, модульность, perf, patterns
- CONTRIBUTING — ссылки на шаблоны и patterns

### Исправлено
- Flatpak: `packaging/linux/flatpak-cargo-sources.json` дополнен крейтами `arc-swap`,
  `crossbeam-queue`, `ksni` и `pastey` — CI-проверка сверяет vendored-список с `Cargo.lock`
- Паника на старте: `ksni` через feature-unification включает tokio-бэкенд у `zbus`,
  а инициализация MPRIS в `playwire` при регистрации object server вызывает
  `tokio::task::spawn`, которому нужен контекст рантайма на текущем потоке.
  `MediaControls::new` выполняется внутри `runtime.enter()`, поэтому приложение
  стартует и регистрирует `org.mpris.MediaPlayer2.OrbitFM`
- IPC: чтение команды ограничено 4096 байтами без неограниченной аллокации; невалидный
  UTF-8 отклоняется; ответ клиента ограничен 64 КиБ; не более 64 соединений
- Single-instance: lock-файл `orbitfm.lock` устраняет гонку удаления живого сокета;
  проверяются абсолютность и права `0700` у `XDG_RUNTIME_DIR`; чужой не-сокет не удаляется
- `quit`: graceful shutdown с дренажом активных соединений гарантирует доставку `ok`
- CLI: не-UTF-8 аргументы дают ошибку вместо паники
- `toggle` восстанавливает свёрнутое окно, а не скрывает его

## [0.1.0] - не выпускался

Версии 0.1.0–0.6.0 были реализованы без отдельного релиза (см. `VERSIONMAP.md`);
первый публичный пре-релиз — [0.7.0-rc.1](#070-rc1---2026-10-23), он включает все
возможности этих этапов.
