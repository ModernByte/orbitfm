# OrbitFM — План реализации UX «Orbital Desk»

**Организация:** ModernByte  
**Версия:** 1.2.0  
**Дата:** 2026-10-24  
**Статус:** Принят вместе с ADR-0015; фазы 1–7 выполнены (фаза 7 — в составе 0.8.0-rc.1)  
**Связанные документы:** [design-vision.md](design-vision.md), [ADR-0015](../adr/0015-ux-редизайн-orbital-desk.md)  
**Нумерация фаз:** 1–7 — синхронна с §12 `design-vision.md` и Этапом 8 `ROADMAP.md` (Фаза 6 = полировка UI по эскизу, Фаза 7 = WindowMode Overlay/Listening и доступность)

---

## 1. Цель

Довести текущий интерфейс до Focus-раскладки из референс-макета: глобус — главный
элемент, HUD и правая панель — компактные инструменты. Добавить два явных режима
поиска, hover/контур страны, Roulette с перелётом камеры и визуализацию эфира, не
ломая существующие темы, избранное, историю, IPC, MPRIS и Noctalia.

Границы:

- GUI-стек eframe/egui/egui-wgpu не меняется;
- новых зависимостей в MVP нет;
- `library.json` и `settings.json` остаются совместимыми;
- публичные контракты меняются только вместе с `docs/interfaces/` и модулями;
- N ≫ 500 маркеров, SQLite, фотореалистичная Земля и смена фреймворка — вне скоупа.

---

## 2. Исходное состояние (аудит 2026-10-23)

| Область | Факт | Следствие |
|---------|------|-----------|
| `app.rs` | 1638 строк: состояние, поиск, весь draw, палитра, тесты | Нужен вынос UI до начала роста |
| Камера | Orbit + damping + zoom limits + `project_on_sphere` | Переиспользуется, добавляется fly-to |
| Клик | Маркер (10 px) → иначе страна | Нужен `SearchMode` и правило «маркер всегда играет» |
| Поиск | Свободный текст или countrycode, limit 30; гео-маркеры limit 60 | Нет жанрового режима, фильтров, сортировки |
| `Station` | Нет `votes`/`country`/`state` | Нельзя показать популярность и страну в карточке |
| `GeoService` | Только `country_at` и `country_at_sphere_point` | Нет контура, hover-подсветки и поиска страны |
| Рендер | Текстура + маркеры + FFT-полоска | Нет атмосферы, pulse, контуров, heatmap |
| MPRIS | title/artist(теги)/artUrl/volume | Нужны страна/штат в artist |
| Окно | Show/Hide, одна карточка | Overlay/Listening — отдельная фаза |
| Library | JSON, favorites/history | Не меняем формат |

---

## 3. Архитектура изменений

### 3.1. Модули UI

```
src/
├── app.rs                 # состояние, эффекты, маршрутизация UiAction
├── ui/
│   ├── mod.rs             # UiAction, общие типы, экспорт
│   ├── palette.rs         # Palette, configure_style (перенос из app.rs)
│   ├── hud.rs             # top HUD: режим, поиск, now-playing, volume
│   ├── mode_switch.rs     # [Страна] [Жанр] + genre chips
│   ├── station_panel.rs   # drawer: вкладки, фильтры, сортировка, карточки
│   ├── transport.rs       # play/stop, favorite, Roulette, codec/bitrate
│   ├── globe_view.rs      # allocate, GlobeInput, маркеры, контур, spectrum
│   ├── capsule.rs         # Overlay top/мини-глобус/избранное и Listening (фаза 7)
│   └── toast.rs           # краткие сообщения, таймер
├── geo/                   # + поиск страны и кольца контура
├── render/
│   ├── countries.rs       # экранные контуры/заливка без доменных типов
│   ├── camera.rs          # + fly_to
│   ├── markers.rs         # + paint_markers / MarkerLabel
│   └── globe.rs/wgsl      # + позиция камеры и pulse
└── services/radio_browser.rs  # + votes/country/state, random_station
```

Правила:

- UI-модули получают только нужные срезы состояния и возвращают `UiAction`;
- никаких вызовов `AudioEngine`, `RadioBrowserClient`, `StationLibrary` из UI;
- `render` не импортирует `Station`, `Country`, `AudioEngine`;
- `geo` не импортирует egui, render и домен станций;
- `app.rs` остаётся единственной точкой побочных эффектов.

`UiAction` (проект):

```rust
enum UiAction {
    Play(Station),
    ToggleFavorite(Station),
    SelectCountry(Country),
    SwitchMode(SearchMode),
    GenreSelected(String),
    RunSearch,
    SetSort(StationSort),
    SetFilter(StationFilter),
    ClearHistory,
    SetTheme(ThemeMode),
    SetAccent(AccentColor),
    ToggleSpectrum(bool),
    SetVolume(f64),
    Roulette,
    Hide,
    Quit,
    TogglePanel,
}
```

### 3.2. Состояние

```rust
enum SearchMode { Country, GlobalGenre }
enum StationSort { Popularity, Bitrate, Name }
struct StationFilter { text: String, tag: Option<String> }

struct SearchState {
    mode: SearchMode,
    selected_country: Option<Country>,
    selected_country_iso: Option<String>,
    selected_genre: Option<String>,
    filter: StationFilter,
    sort: StationSort,
    generation: u64,
    pending: bool,
    results: Vec<Station>,
    markers: Vec<StationMarker>,
    error: Option<String>,
    empty: bool,
}
```

`search_rx` дополняется номером поколения: результат старого запроса после смены
режима/страны/жанра отбрасывается. Это заменяет неявное «последний ответ wins».

### 3.3. Поток данных

```
egui input → ui/* → UiAction
                 ↓
            OrbitApp::apply_action
                 ↓
     audio / radio / geo / library / settings
                 ↓
        состояние → следующий кадр ui/*
```

Дорогие операции (сеть, старт станции) остаются в App и выполняются через
существующие каналы/`runtime`; UI никогда не блокируется.

---

## 4. Фазы

### Фаза 1. UI-каркас Orbital Desk

**Статус:** выполнена (2026-10-23). Roulette из п. 3 перенесён в Фазу 4 и появляется
вместе с `random_station` и `fly_to`; остальные пункты закрыты.

**Задачи**

1. Вынести `Palette`, `configure_style`, `theme_*`, `accent_*` в `ui/palette.rs` без
   изменения поведения.
2. Вынести `draw` на `hud`, `station_panel`, `transport`, `globe_view`.
3. Собрать Focus-раскладку:
   - top HUD: логотип, переключатель режима, поиск, now-playing, volume, настройки;
   - центр: глобус на всю доступную высоту (не менее 60% контента);
   - правый drawer 300–320 px, сворачиваемый; на узком окне (< 900 px) —
     overlay-панель поверх глобуса с кнопкой закрытия;
   - нижний transport: play/stop, favorite, codec/bitrate, FFT (Roulette — Фаза 4).
4. Убрать постоянный футер с двумя строками подсказок; заменить `on_hover_text` и
   короткой подсказкой при первом запуске (без сохранения флага в MVP).
5. Перенести существующие вкладки, звезду, избранное/историю, темы и FFT в новую
   раскладку без изменения поведения.
6. Добавить `Toast` (одно сообщение, 3 с) для ошибок и Roulette.

**Файлы:** `src/app.rs`, `src/ui/*`, без изменений домена.

**Приёмка**

- `cargo fmt --check`, `clippy -D warnings`, `cargo test` зелёные;
- глобус визуально доминирует; список не занимает половину высоты;
- все прежние функции доступны: поиск, вкладки, звезда, темы, FFT, hide/quit;
- `app.rs` заметно меньше, UI-код не превышает мягкий лимит.

### Фаза 2. Режим «Страна»

**Статус:** выполнена (2026-10-23). Отклонения: `SearchMode` не вводился — режим
страны активен, пока выбрана страна (`search_country`), переключатель режимов
появится в Фазе 3; hover определяется raycast по текущей позиции курсора без кэша
последней точки.

**Задачи**

1. `Station`: `votes`, `country`, `state` с `serde(default)`; тесты десериализации
   старого JSON и API-ответа.
2. `SearchMode::Country`, `StationFilter`, `StationSort`; локальная фильтрация по
   `name/tags/country/state` и сортировка.
3. Панель страны:
   - заголовок страны и количество станций;
   - поле текста, чипы All/Rock/Pop/Jazz/Electronic/News/Metal;
   - сортировка Popularity/Bitrate/Name;
   - карточки: имя, state/country, теги, kbps, codec, звезда; favicon — позже.
4. `GeoService`: `search_countries(query, limit)`, `country_shape(iso_a2)`;
   тесты на нормализацию, пустой запрос, антимеридиан.
5. `render/countries.rs`: проекция колец, разрыв на задней полусфере, контур
   hover/selected и мягкая заливка; без доменных типов.
6. Hover страны: raycast по `hover_pos` с кэшем последней точки; курсор
   `PointingHand` только над страной/маркером; `Grabbing` при ПКМ-drag.
7. Upper HUD country search: подсказки из `GeoService`, Enter выбирает первую
   страну; клик по подсказке — выбрать и мягко довернуть камеру.
8. Клик по стране запускает `countrycode`-поиск (limit 100); маркер по-прежнему
   приоритетен.

**Приёмка**

- клик по стране → контур/заливка → список с фильтрами и сортировкой;
- hover показывает контур без клика;
- текст/чипы/сортировка не делают сетевых запросов;
- пустой результат — «В этом секторе пока тихо» + сброс фильтров;
- старый `library.json` читается.

### Фаза 3. Режим «Жанр»

**Статус:** выполнена (2026-10-23). Отклонения и уточнения: предустановлено восемь
тегов (Rock, Pop, Jazz, Electronic, News, Metal, Classical, Synthwave) плюс поле
своего тега; `SearchMode` и `SearchContext` живут в `ui/mod.rs` и `app.rs`;
поколение поиска — единый счётчик `search_generation` с `SearchResponse` в
`mpsc`-канале; лимиты свободного поиска подняты до 100/100 (страна — 100/100,
жанр — 200/200); в активном контексте (страна или жанр) поле HUD работает как
локальный фильтр, кнопка подписана «Фильтр». На Этапе 9 (ADR-0012, обновление)
динамический размер по голосам и цвет по битрейту отменены: все маркеры одного
размера (3 px) и цвета, качество и рейтинг — в подписи при наведении. Проверено
вживую на Wayland/niri при ширине окна 952 и 820 px.

**Задачи**

1. `SearchMode::GlobalGenre`; переключатель сбрасывает страну и контур.
2. Список жанров: предустановленные чипы + поле своего тега; выбор запускает
   поиск.
3. Два параллельных запроса:
   - список: `tag`, `limit=200`, `hide_broken=true`;
   - маркеры: `tag`, `has_geo_info=true`, `limit=200`.
   Ошибка гео-запроса не ломает список; старые маркеры очищаются сразу.
4. ~~`MarkerVisual.weight: f32`; App вычисляет вес из `votes` и `bitrate`;
   `paint_markers` масштабирует радиус/свечение в заданных пределах.~~
   Отменено на Этапе 9: `MarkerVisual` несёт только `pos` и `playing`,
   `paint_markers` рисует единые маркеры и подпись `MarkerLabel` (ADR-0012).
5. Клик по маркеру запускает станцию; клик по стране в этом режиме — no-op.
6. Список «Результаты» показывает выборку жанра; Stations без координат остаются
   в списке и отсутствуют на карте (это ожидаемо).
7. Поколение поиска защищает от ответов прошлого жанра.

**Приёмка**

- выбор жанра → маркеры по миру → клик стартует эфир;
- во время загрузки нет маркеров прошлого жанра;
- подпись маркера `MarkerLabel` собирается детерминированно и покрыта unit-тестами;
- 200 маркеров не вызывают заметных подвисаний на Linux.

### Фаза 4. Roulette и MPRIS

**Статус:** выполнена (2026-10-23). Отклонения и уточнения: кнопка Roulette живёт
в transport; `fly_to` уже существовал с Фазы 2 и переиспользован с длительностью
900 мс; `random_station` дополнительно отбрасывает координаты `(0, 0)` и делает
до трёх попыток; MPRIS artist — «штат, страна», иначе одно поле, иначе теги.
IPC-команды нет. Проверено вживую на Wayland/niri: бельгийская станция, перелёт
камеры, тост с названием и страной, `xesam:artist = Brussels-Capital Region,
Belgium`.

**Задачи**

1. `RadioBrowserClient::random_station()`:
   `order=random&limit=1&has_geo_info=true&hidebroken=true`; до 3 попыток,
   отбрасывание пустого `url_resolved`/координат; `NotFound` после повторов.
2. `GlobeCamera::fly_to(lat, lon, duration)`:
   - кратчайший путь yaw с учётом wrap;
   - ease-out, 0.8–1.0 с;
   - отмена при drag/новом fly-to; `is_animating` учитывает полёт.
3. Кнопка Roulette в transport/HUD: загрузка → старт станции → fly-to → toast с
   названием и страной; ошибка — toast, глобус не блокируется.
4. MPRIS: artist = `country/state`, иначе теги; title и artUrl без изменений;
   тесты маппинга.
5. Не добавлять IPC-команду Roulette: действие доступно из GUI.

**Приёмка**

- Roulette запускает случайную станцию с координатами и анимирует камеру;
- drag во время полёта отменяет анимацию;
- MPRIS показывает страну/штат и favicon;
- ошибка сети показывается тостом и не ломает текущий эфир.

### Фаза 5. Живой глобус: pulse и heatmap

**Статус:** выполнена (2026-10-23). Отклонения и уточнения: `pulse()` добавлен в
`SpectrumOverlay` как `0.75·level + 0.25·flux` (0 при невидимом оверлее) — отдельный
сглаживатель в App не понадобился. Uniform камеры расширен позицией `eye` и `pulse`,
Fresnel-обод считается в WGSL. Heatmap — только выбранная и наведённая страна:
количество берётся из загруженных маркеров (`GlobeMarker.count`), поэтому
агрегата по всем странам нет (отдельное решение). Заливка считается в `globe_view`
по лог-шкале, альфа 26..96; hover получает заливку только при `count > 0`.
Проверено вживую на Wayland/niri: pulse 0.83 при Playing (бирюзовый обод),
затухание после выключения FFT, Австралия — 100 станций и максимальная заливка.

**Задачи**

1. Расширить uniform глобуса: позиция камеры, `pulse` 0..1; обновить WGSL.
2. Fresnel-обод атмосферы; яркость/ширина зависит от сглаженных `rms_db`/`flux`.
3. App передаёт `pulse` в `paint_globe`; при выключенном FFT/тишине — 0.
4. `SpectrumOverlay` остаётся опцией; добавить `pulse()`/`energy()` или отдельный
   сглаживатель в App без изменения аудио-контракта.
5. Heatmap выбранной/наведённой страны: заливка с интенсивностью от количества
   загруженных станций; глобальный aggregate по всем странам — отдельное решение.
6. Тесты шейдерной математики, где возможно; визуальная проверка на Linux.

**Приёмка**

- при Playing атмосфера пульсирует, при Stop затухает;
- FFT не блокирует GUI и не обязателен для базового глобуса;
- выбранная страна имеет читаемую заливку в тёмной и светлой теме.

### Фаза 6. Полировка UI по эскизу

**Статус:** выполнена (2026-10-23, в составе 0.7.0-rc.1). Отклонения и уточнения:
плавающая раскладка — full-bleed глобус со звёздным небом, поверх плавающие
стеклянные зоны; геометрию зон считает `DeskLayout::compute`, а клик/колесо
глобуса внутри зон отсекает `blocks_point` (`src/ui/layout.rs`). Кастомные
векторные контролы (`src/ui/widgets.rs`): pill-переключатель, жанровые чипы,
pill-поле поиска с лупой и popup-подсказками, вкладки с accent-подчёркиванием,
LIVE-бейдж, мини-эквалайзер, volume-слайдер, круглая play/pause, круглый
favicon-аватар, знак OrbitFM и флаг-эмодзи. Favicon — асинхронный кэш
`FaviconCache` со состояниями Queued/Loading/Ready/Failed, до 8 запросов
одновременно (`MAX_IN_FLIGHT`) и LRU-эвикцией до 256 (`MAX_CACHED`), декод
`image` (features `jpeg`/`png`/`ico`) к 64×64, спавнит `OrbitApp` — отдельного
`egui_extras` не понадобилось (ADR-0016, `src/ui/images.rs`). Строка списка
(`src/ui/station_row.rs`) и переключатель плотности `Compact`
(`compact_rows`/`filtered_results` в `src/ui/station_filter.rs`, чип в
`src/ui/station_panel.rs`). prev/next идут по текущему видимому списку с циклом
через `play_relative` (`src/session/playback.rs`) — кнопки transport и MPRIS
`Next`/`Previous`. Тултип страны с цветным emoji-флагом по ADR-0017. Все надписи
интерфейса переведены на английский. WindowMode и доступность перенесены в
Фазу 7. Проверено вживую на Wayland/niri.

**Задачи**

1. Плавающая раскладка по эскизу: full-bleed глобус, поверх — HUD, панель
   станций, transport, help-карточка и легенда плотности; геометрия зон и
   отсечение ввода глобуса внутри них.
2. Кастомные векторные контролы вместо стандартных egui-виджетов.
3. Асинхронная загрузка и кэш favicon для карточек и transport; ADR по зависимости.
4. Строка списка: аватар, `state, country`, теги-чипы, битрейт/кодек, звезда;
   переключатель плотности `Compact`.
5. prev/next по текущему видимому списку с циклом (кнопки transport и MPRIS).
6. Тултип страны с флагом-эмодзи; перевод всех надписей интерфейса на английский.

**Файлы:** `src/ui/layout.rs`, `src/ui/widgets.rs`, `src/ui/images.rs`,
`src/ui/station_row.rs`, `src/ui/station_filter.rs`, `src/ui/station_panel.rs`,
`src/app.rs`.

**Приёмка**

- глобус full-bleed, зоны плавают поверх, клик/колесо внутри зон не вращает глобус;
- favicon грузятся асинхронно без блокировки GUI, при сбое — круг с инициалом;
- prev/next и MPRIS `Next`/`Previous` листают видимый список по кругу;
- плотность `Compact` меняет высоту строк; интерфейс — на английском, читается в
  обеих темах.

### Фаза 7. WindowMode Overlay/Listening и доступность

**Статус:** выполнена (2026-10-24, в составе 0.8.0-rc.1). Отклонения и уточнения:
`LayoutDensity::{Full, Compact, Minimal}` выводится из `WindowMode` в
`src/ui/layout.rs`, `DeskLayout::compute` принимает режим; Focus сохраняет прежние
прямоугольники и full-bleed глобус. Раскладки капсул живут в `src/ui/capsule.rs`:
Overlay — HUD-строка (станция, play/stop, громкость, expand/close), мини-глобус без
звёздного фона и нижняя панель избранного (до 8 строк `station_row`, пустое
состояние); Listening — одна строка now-playing с пульсирующим кольцом аватара при
Playing. Сглаженный сигнал эфира считается один раз в `app.rs`
(`SpectrumOverlay::pulse`) и передаётся и глобусу, и капсуле. Возврат в Focus —
кнопка expand в капсулах и раздел «Window» в попапе шестерёнки; смена режима идёт
через IPC `mode` и перезапуск UI-цикла ADR-0018/ADR-0021. Доступность — штатный
фокус egui 0.37: focus-ring у кастомных контролов, `WidgetInfo` с ролями и
подписями (Button/RadioButton/CheckBox/Tab/ListItem/slider), клавиатурный шаг
громкости (стрелки/Home/End), навигация ↑/↓ по строкам списка с `scroll_to_me`,
Space/Enter запускают станцию. Проверено живьём на Wayland/niri: Focus 920×660 не
изменился, Overlay 420×620 и Listening 420×152 (niri по умолчанию тайлит окно;
размеры проверены плавающим окном, без изменения конфигурации WM) в тёмной и
светлой теме, пульс при Playing. Клавиатурная навигация покрыта headless-тестами
egui (стрелки и Space); физический ввод на живой сессии не автоматизировался.

**Задачи**

1. WindowMode (`Focus`/`Overlay`/`Listening`) и `LayoutDensity` как параметры
   одного набора виджетов (design-vision §2); поведение существующего Focus не
   ломать. Отдельное решение по IPC-команде смены режима и обновление
   `docs/interfaces/05-ipc.md`.
2. Overlay-капсула (design-vision §6): top — станция + play/stop + volume,
   center — мини-глобус или выбранный регион, bottom — 5–8 избранных; без блока
   «Оформление» и длинных подсказок; компактность и управляемость с панели
   Noctalia (критерий приёмки design-vision §10 п. 6).
3. Listening: минимальный now-playing + пульс при низкой активности UI.
4. Скрытие/показ окна под Overlay/Listening опирается на принятый и реализованный
   механизм ADR-0018 (закрытие root-вьюпорта + `Session::park`); новых оконных
   механизмов не вводить.
5. Доступность: фокус и клавиатурная навигация по списку станций, контраст в
   обеих темах, читаемые подписи контролов.

**Приёмка**

- Focus остаётся прежним; Overlay — компактная капсула, вызывается из Noctalia/`toggle`;
- Listening показывает минимальный now-playing с пульсом при неактивном UI;
- смена режима работает через принятый ADR-0018 механизм hide/show и IPC-команду
  (либо явное решение её не добавлять);
- список полностью управляется с клавиатуры, контраст проходит обе темы;
- `cargo fmt --check`, `clippy -D warnings`, `cargo test` зелёные.

---

## 5. Поисковый конвейер (детали)

```
apply_action(SwitchMode/SelectCountry/GenreSelected/RunSearch)
  → generation += 1
  → pending = true, error = None, empty = false
  → очистить markers/results (для смены контекста)
  → spawn primary + optional geo
  → результаты несут generation
poll_search:
  → если generation != current → drop
  → primary: results, empty
  → geo: markers (ошибка только warn)
  → pending = false
```

Ограничения:

| Контекст | Основной запрос | Гео-запрос |
|----------|-----------------|------------|
| Страна | `countrycode`, limit 100 | `countrycode` + `has_geo_info`, limit 100 |
| Жанр | `tag`, limit 200 | `tag` + `has_geo_info`, limit 200 |
| Свободный текст | `name`, limit 100 | `name` + `has_geo_info`, limit 100 |
| Roulette | `order=random`, limit 1 | — (в том же запросе `has_geo_info`) |

Глобальный поиск станции по имени сохраняется как fallback: он доступен из панели,
когда страна/жанр ещё не выбраны (например, Enter в поле при пустом контексте).
После выбора страны или жанра то же поле работает как локальный фильтр и сетевых
запросов не делает.

Локальные фильтры применяются к `results` после ответа; чипы проверяют
`station.tags` по списку через запятую, без учёта регистра. Сортировка:
`Popularity` — `votes desc`, `Bitrate` — `bitrate desc`, `Name` — `name` без учёта
регистра. При равных значениях — `name`.

Ошибки:

- основной запрос — баннер/тост + пустой список, маркеры очищены;
- гео-запрос — лог, список остаётся;
- пустой успешный ответ — состояние empty с CTA сброса фильтров;
- устаревший ответ — тихо отбрасывается.

---

## 6. Планируемые изменения контрактов

Все изменения фиксируются в `docs/interfaces/` и модулях в той же фазе, где
появляются в коде.

### Radio (`docs/interfaces/02-radio.md`)

```rust
pub struct Station {
    // существующие поля...
    #[serde(default)] pub votes: u32,
    #[serde(default)] pub country: String,
    #[serde(default)] pub state: String,
}

impl RadioBrowserClient {
    /// Случайная станция с координатами; до 3 попыток.
    pub async fn random_station(&self) -> Result<Station, RadioError>;
}
```

`SearchParams` не расширяется: сортировка остаётся локальной, API — по votes.
`search_query` продолжает отдавать `order=votes&reverse=true`; `random_station`
строит отдельный запрос.

### Geo (`docs/interfaces/03-geo.md`)

```rust
pub struct CountryShape { /* кольца lon/lat */ }

impl GeoService {
    pub fn countries(&self) -> impl Iterator<Item = &Country>;
    pub fn search_countries(&self, query: &str, limit: usize) -> Vec<&Country>;
    pub fn country_shape(&self, iso_a2: &str) -> Option<CountryShape>;
}
```

Возвращаемая геометрия — только координаты; проекция и отрисовка — в App/render.

### Render (`docs/interfaces/04-render.md`)

```rust
impl GlobeCamera {
    pub fn fly_to(&mut self, lat: f64, lon: f64, duration: Duration);
    pub fn cancel_fly(&mut self);
}

pub struct MarkerVisual {
    pub pos: Pos2,
    pub playing: bool,
}

pub struct MarkerLabel {
    pub title: String,
    pub details: Option<String>,
}

pub fn paint_globe(
    painter: &egui::Painter,
    rect: egui::Rect,
    camera: &GlobeCamera,
    pulse: f32,
);
```

`CameraUniform` получает позицию камеры и signal; `render` не импортирует
`Station`/`AudioEngine`.

### UI

`UiAction` и `SearchMode` — внутренние типы App; в `docs/interfaces/07-app.md`
фиксируется поведение, а не публичный Rust API.

---

## 7. Тесты

| Уровень | Что проверяем |
|---------|----------------|
| Radio unit | `Station` с новыми полями и старым JSON; `random_station` query; отбрасывание пустого URL; повторы |
| Geo unit | поиск страны по имени; `country_shape` для ISO; антимеридиан; пустой запрос |
| Render unit | fly-to: кратчайший yaw, завершение, отмена; weight → радиус; pulse → uniform |
| App unit | фильтрация по тексту/тегу; сортировка; смена режима сбрасывает состояние; generation отбрасывает старый ответ |
| MPRIS unit | artist из страны/штата, fallback на теги; artUrl |
| Ручной Linux | клик/ hover/орбита/зум; страна и жанр; Roulette; темы; overlay hide/toggle; отсутствие wgpu |
| CI | `fmt`, `clippy -D warnings`, `cargo test`; live-тесты только `--ignored` |

Сетевые live-тесты не входят в обязательный CI. Для UI-проверок используется
`cargo run` на Linux (Niri/Wayland) и, при необходимости, скриншот окна.

---

## 8. Производительность и память

- максимум 200 маркеров на кадр, проекция O(N) в App; без запросов на кадр;
- контур страны кэшируется по ISO; проекция выполняется только для
  выбранной/наведённой страны;
- hover-страна: raycast не чаще движения указателя/камеры, кэш последней точки;
- фильтрация/сортировка по выборке ≤ 200 станций — на UI-потоке;
- `Vec` → `Box<[T]>` для готовых срезов (результаты, контуры), если коллекция больше
  не растёт;
- FFT pulse читает уже сглаженные метрики, без новых потоков и блокировок;
- при N ≫ 500 потребуется LOD/кластеризация и отдельный perf-ADR.

---

## 9. Документация по фазам

| Фаза | Обновить |
|------|----------|
| 1 | `docs/modules/07-app-core.md`, `docs/interfaces/07-app.md`, CHANGELOG |
| 2 | `docs/interfaces/02-radio.md`, `03-geo.md`, `04-render.md`; modules 02/03/04/07 |
| 3 | radio/render interfaces и modules; CHANGELOG |
| 4 | `docs/interfaces/04-render.md`, `06-media.md`; modules 04/06/07 |
| 5 | `docs/interfaces/04-render.md`, `docs/modules/04-render-globe.md`; CHANGELOG |
| 6 | ADR-0016 (favicon-зависимость) и ADR-0017 (emoji-флаги), `docs/interfaces/06-media.md`, `docs/interfaces/07-app.md`, modules 06/07; CHANGELOG |
| 7 | ADR по WindowMode, `docs/interfaces/05-ipc.md`, modules 05/07 |

Правило: контракт меняется в том же PR, что и код; `docs/ui/design-vision.md`
обновляется только при изменении целевого UX, а не при каждой реализации.

---

## 10. Definition of Done (MVP, фазы 1–4)

1. Focus-раскладка из макета: глобус доминирует, HUD тонкий, drawer справа.
2. Country: клик/hover страны, контур, список с текстом/чипами/сортировкой,
   country search в HUD.
3. Genre: переключатель, tag-поиск, маркеры с весом, клик → play, сброс страны.
4. Камера: ПКМ-орбита с инерцией, zoom limits, fly-to для Roulette.
5. Roulette: случайная станция, автостарт, toast; MPRIS обогащён.
6. Старые возможности сохранены: избранное/история, темы, FFT, IPC, Noctalia.
7. `fmt`, `clippy`, `cargo test` зелёные; CI PR зелёный; приложение переустановлено
   после мёржа.

---

## 11. Отложенные решения

- WindowMode и IPC-команда смены режима (фаза 7);
- источник глобального heatmap по всем странам (aggregate/кэш);
- LOD/кластеризация при N ≫ 500;
- системные уведомления вне MPRIS;
- SMTC (Windows) и Now Playing (macOS) — по существующему ROADMAP.
