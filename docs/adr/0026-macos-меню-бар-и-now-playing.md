# ADR-0026: macOS — трей в строке меню и Now Playing при скрытом окне

**Статус:** Принято (2026-10-28)  
**Дата:** 2026-10-24  
**Участники решения:** ModernByte Architecture, Юрий Анатольевич  

## Контекст

ADR-0018 задаёт единый механизм скрытия на всех платформах: закрытие окна — это
реальное закрытие root-вьюпорта, после чего `eframe::run_native` возвращает
управление, а главный поток уходит в `Session::park` —
`runtime.block_on(tokio::select!)` по IPC, MPRIS, трею и аудио
(`src/session/park.rs:45-53`, `src/main.rs:303-320`). На Wayland это
единственный честный способ убрать окно из раскладки, и решение подтверждено
live-проверкой (ADR-0018, «Проверка реализации»).

На macOS та же модель ломает две системные интеграции.

1. **Now Playing.** playwire 2.0 регистрирует обработчики
   `MPRemoteCommandCenter` через `addTargetWithHandler`. Команды приходят на
   главный run loop — это документирует сам бэкенд: «Commands arrive on the main
   run loop and are forwarded through the threadsafe function rather than
   mutating this struct» (`playwire-2.0.0/src/platform/macos.rs:54-56`). Пока
   окно видно, run loop крутит winit. Когда окно скрыто, главный поток занят
   `Session::park`, NSRunLoop не выполняется, и медиа-клавиши и кнопки Control
   Center не доходят до приложения. Метаданные при этом публикуются:
   `sync_media()` вызывается после каждого события парковки, а
   `MediaControls::set_state` напрямую обновляет `MPNowPlayingInfoCenter`
   (`macos.rs:204-215`). Теряется именно управление, а не витрина.
2. **Трея нет.** SNI — Linux-протокол; на macOS `src/tray/` собирается в
   no-op (`src/tray/mod.rs:94-102`). Пользователю нужен статус-элемент в строке
   меню с тем же набором действий и строкой «сейчас играет», что и в SNI
   (ADR-0020).

При этом eframe 0.37 официально поддерживает скрытое окно как режим простоя.
Невидимый root-вьюпорт не рисуется, но `App::logic` продолжает выполняться:
`update_logic_only` (`eframe-0.37.0/src/native/wgpu_integration.rs:684-745`,
`epi_integration.rs:384-407`), а `close_requested` и `CancelClose`
обрабатываются и в этом режиме. Комментарий eframe прямо описывает сценарий:
«The app logic keeps ticking, so it can e.g. ask to be shown again»
(`wgpu_integration.rs:720-722`). На macOS `set_visible` — реальная операция:
`orderOut`/`makeKeyAndOrderFront`
(`winit-0.30.13/src/platform_impl/macos/window_delegate.rs:904-909`), а
`request_redraw` будит run loop (`app_state.rs:297-303`). В отличие от Wayland,
где `set_visible` — no-op (ADR-0018, п. 1 контекста), на macOS скрытие работает.

Для строки меню нужен бэкенд. `tray-icon` 0.26.1 (MIT/Apache-2.0,
`rust-version = 1.90`) на macOS требует уже запущенного event loop и создания
на главном потоке: «an event loop must be running on the main thread so you also
need to create the tray icon on the main thread… the earliest you can create
icons is on `StartCause::Init`» (`tray-icon-0.26.1/src/lib.rs:21`). Это
совместимо с созданием в `OrbitApp::new`: eframe вызывает AppCreator из
`resumed` уже после старта цикла
(`eframe-0.37.0/src/native/wgpu_integration.rs:345,459`). Меню строит `muda`
0.21.2 (Apache-2.0/MIT), поддерживаются шаблонная иконка
(`with_icon_templated`, `tray-icon/src/lib.rs:412`) и изменяемые пункты
(`set_text`, `set_enabled`, `set_checked` в `muda/src/items/{normal,check}.rs`).
События меню можно получать глобальным обработчиком и будить GUI
(`tray-icon/src/lib.rs:100-122`).

## Решение

1. **На macOS скрытие — это `orderOut`, а не закрытие root-вьюпорта.** В
   `UiIntent` появляется отдельное намерение `Hide`; `quit` и смена режима
   по-прежнему шлют `Close`. Presentation-слой на macOS переводит `Hide` в
   `CancelClose` + `ViewportCommand::Visible(false)` и не выходит из
   `run_native`; на Linux/Windows `Hide` и `Close` оба означают
   `ViewportCommand::Close`, то есть ADR-0018 не меняется. Запрос закрытия от
   пользователя (кнопка окна, Cmd-W) на macOS перехватывается в `App::logic`
   (`close_requested`), отменяется через `CancelClose` и превращается в тот же
   `Hide`; `quit` и пересоздание окна при смене режима остаются реальным
   закрытием.
2. **`run_native` на macOS живёт весь процесс.** Пока окно скрыто, eframe
   выполняет только `App::logic`, без egui-прохода и без отрисовки
   (`wgpu_integration.rs:684-745`); `Session::tick` продолжает разбирать IPC,
   MPRIS и команды трея. Это второй поддерживаемый eframe режим простоя, а не
   обходной путь. `Session::park` на macOS не вызывается; он остаётся путём для
   Linux/Windows и аварийным путём, если цикл всё же завершился.
3. **Состояние окна без завершения прогона.** `WindowState` получает
   `mark_hidden`/`mark_shown`: скрытие по `Visible(false)` сразу выставляет
   `visible = false` и обновляет `status`/`TraySnapshot`, не дожидаясь возврата
   из `run_native`. `WakeHandle` при этом не очищается — именно он будит
   скрытое окно по событию playwire, трея или IPC. Показ — `Visible(true)` и
   `Focus`; в отличие от ADR-0018, окно и UI-контекст не пересоздаются, поэтому
   показ мгновенный.
4. **Now Playing не меняется в `src/media/`.** Достаточно того, что главный run
   loop больше не останавливается: обработчики `MPRemoteCommandCenter`
   доставляются, колбэк playwire кладёт команду в очередь и зовёт `wake`,
   eframe запускает logic-only проход, `Session::tick` исполняет команду. В
   скрытом состоянии метаданные и управление работают одинаково.
5. **macOS-трей — отдельный бэкенд `src/tray/macos.rs` за тем же контрактом.**
   `tray-icon` 0.26.1 подключается только для `cfg(target_os = "macos")`
   optional-зависимостью с `default-features = false`; фича `tray` включает и
   `dep:ksni`, и `dep:tray-icon` (на каждой платформе собирается только её
   зависимость). Linux-путь на `ksni` не трогаем: отклонение `tray-icon` в
   ADR-0020 было мотивировано GTK/libappindicator, на macOS этого графа нет.
   `TrayCommand`, `TraySnapshot`, `TrayHandle::{publish, commands}` и
   `Session::execute_tray_command` остаются общими; меняется только внутреннее
   устройство handle.
6. **Жизненный цикл macOS-трея.** `tray::spawn` на macOS создаёт канал и
   «неподключённый» handle; сам `TrayIcon` строится в `OrbitApp::new` на главном
   потоке, когда run loop уже запущен (требование `tray-icon`). Handle хранится
   в `Session`, поэтому переживает пересоздание окна при смене режима и
   уничтожается при выходе. Если создать иконку не удалось, пишем `warn!` и
   работаем без трея — трей не является условием запуска.
7. **Меню повторяет SNI.** Неактивный заголовок `Now playing: <станция>` /
   `Paused: <станция>` / `Nothing playing`; Show/Hide window; Play/Pause с
   чекбоксом; Stop; Previous; Next; Radio Roulette; Volume Up/Down; тема
   System/Dark/Light; Quit. Пункты не пересобираются: у них стабильные `MenuId`,
   а подписи и состояния обновляются из `TraySnapshot`. Глобальный обработчик
   `MenuEvent` переводит `MenuId` в `TrayCommand`, кладёт его в канал и зовёт
   `wake`.
8. **Иконка — монохромный template-образ.** Цветной pixmap SNI в строке меню
   macOS выглядит чужеродно и не адаптируется к светлой/тёмной теме. Геометрию
   орбитального знака из `src/tray/icon.rs` выносим в платформенно-нейтральный
   растеризатор RGBA; Linux-адаптер по-прежнему конвертирует RGBA в ARGB32 для
   `ksni`, macOS отдаёт RGBA в `tray_icon::Icon` с
   `with_icon_templated(true)`. Новый бинарный ассет не добавляется.
9. **Вне области решения:** Windows-трей в области уведомлений (SMTC уже
   закрыт ADR-0024; трей — отдельная задача), изменение Dock-политики macOS,
   `NSStatusItem` на чистом `objc2` вместо `tray-icon`.

## Альтернативы

| Вариант | Причина отклонения |
|---------|--------------------|
| Оставить `Session::park` и прокачивать `NSRunLoop` вручную | Требует прямой зависимости `objc2-foundation`, ручного polling в `select!` и не решает создание `tray-icon`, которому нужен именно работающий event loop, а не отдельные прокачки; латентность команд зависит от периода опроса |
| Вынести `park` на фоновый поток, оставив главный поток у NSRunLoop | `Session` не `Send` и связан с главным потоком (audio, UI-контекст, eframe); это перестройка Application Core ради платформенной особенности |
| `tray-icon` и на Linux вместо `ksni` | Отклонено ещё в ADR-0020: default-фичи тянут GTK/libappindicator в AppImage/Flatpak; на macOS этого недостатка нет, поэтому выбор платформенный |
| Нативный `NSStatusItem` на `objc2-app-kit` | Новых crate'ов почти нет, но это сотни строк unsafe-ObjC и собственный менеджер меню; `tray-icon`/`muda` уже решают эту задачу и поддерживаются Tauri |
| Ничего не делать, только рантайм-проверка Now Playing | Не закрывает управление при скрытом окне и не даёт трея на macOS; пользователь явно запросил оба сценария |
| XEmbed/AppIndicator | Не существует на macOS; для macOS это не альтернатива |

## Последствия

**Положительные:**

- Now Playing (медиа-клавиши и Control Center) управляет радио и при скрытом
  окне — исчезает платформенная дыра ADR-0018;
- появляется трей в строке меню с тем же набором команд и той же строкой «сейчас
  играет», что и SNI; контракт `TrayCommand`/`TraySnapshot` не меняется;
- показ окна на macOS становится мгновенным (окно скрыто, а не уничтожено), в
  отличие от ≈1 с пересоздания UI на Wayland;
- скрытое окно не рисуется: eframe выполняет только `logic`, кадров и
  GPU-работы нет; пробуждения идут по `request_repaint` от IPC, playwire и трея;
- Linux/Windows не меняются: `Hide` на этих платформах по-прежнему закрывает
  root-вьюпорт и уходит в `park`.

**Отрицательные / Риски:**

- ADR-0018 п. 1 («никаких платформенных веток в механизме скрытия») получает
  явное исключение для macOS; это должно быть отражено в модульных документах;
- скрытое NSWindow и его поверхность остаются в памяти весь процесс; на macOS
  это нормальная цена за живой run loop, но ресурсы не освобождаются, как в
  `park`;
- `tray-icon` и `muda` — новые прямые зависимости для macOS (objc2-крейты уже в
  графе через playwire/winit; дополнительно приходят `muda`, `dpi`,
  `keyboard-types`, `crossbeam-channel`); нужен отдельный контроль их версий и
  лицензий;
- трей создаётся внутри UI-прогона, поэтому его жизненный цикл асимметричен
  Linux-трею (поток-владелец) и требует аккуратного drop при выходе;
- без Mac в окружении разработки возможна только компиляция/тесты в CI; live-
  проверка медиа-клавиш, меню и показа окна остаётся обязательной;
- Cmd-Q и закрытие последнего окна на macOS имеют собственные пути AppKit;
  поведение `quit` из трея нужно проверить отдельно, чтобы процесс завершался,
  а не оставался без окна и без иконки.

## Реализация

Решено и реализовано в 0.13.0-rc.1 (2026-10-28).

- **Намерения окна.** `UiIntent` получил вариант `Hide`; `WindowState::hide`
  ставит его, `set_mode` при живом вьюпорте — `Close` + `reopen`, `quit` —
  `Close` (`src/session/window.rs`). Повторный `hide` до обработки кадра не
  плодит намерений. Presentation-слой в `OrbitApp::logic`
  (`src/app.rs`) переводит на macOS `Hide` в `CancelClose` +
  `ViewportCommand::Visible(false)` и зовёт `Session::mark_hidden`; `Show` —
  `Visible(true)` + `Focus` + `mark_shown`; `Close` — реальное закрытие.
  На Linux/Windows `Hide` по-прежнему означает `ViewportCommand::Close`,
  то есть ADR-0018 не меняется.
- **Перехват пользовательского закрытия.** На macOS `close_requested` (кнопка
  окна, Cmd-W) отменяется через `CancelClose` и превращается в `hide`, кроме
  `is_quitting()` и `is_reopening()` — quit и смена режима остаются реальным
  закрытием.
- **Состояние без перезапуска прогона.** `WindowState::mark_hidden/mark_shown`
  выставляют видимость сразу; `Session::mark_hidden` закрывает настройки,
  фиксирует незаписанную громкость (`flush_pending_volume`), обновляет анализ,
  MPRIS и `TraySnapshot`; `mark_shown` возвращает анализ и витрины.
  `Session::window_visible()` читают `status`, трей и гейт репейнта спектра:
  скрытое окно не рисуется и не просыпается ради кадров. `WakeHandle` при
  скрытии не очищается.
- **Run loop.** `run_session` на macOS живёт в одном `run_native`: скрытый
  вьюпорт получает только logic-проходы, IPC, playwire и трей тикают в них.
  Возврат из `run_native` — только реальный `Close` (quit или смена режима).
  `Session::park` остаётся путём для Linux/Windows и аварийным путём macOS.
- **Трей.** `src/tray/macos.rs` на `tray-icon` 0.26.1 + `muda` 0.21.2
  (target-specific optional deps, `default-features = false`, фича `tray`
  включает `dep:ksni`, `dep:tray-icon`, `dep:muda`). `tray::spawn` создаёт
  `Slot` с каналом команд и копит снимки в `pending`; `OrbitApp::new` зовёт
  `Session::attach_tray` → `Slot::attach` на главном потоке после старта run
  loop. Меню строится один раз со стабильными `MenuId`, подписи и чекбоксы
  обновляются `set_text`/`set_checked`; глобальный `MenuEvent` переводит ID в
  `TrayCommand` и зовёт `wake`. Ошибка создания — `warn!`, приложение работает
  без трея. Общие подписи вынесены в `src/tray/text.rs`, растеризатор знака —
  платформенно-нейтральный RGBA (`src/tray/icon.rs`), Linux-адаптер
  конвертирует его в ARGB32, macOS отдаёт `tray_icon::Icon` с
  `with_icon_templated(true)`.
- **Тесты.** `WindowState`: `Hide` при скрытии, `Close` при смене режима,
  `mark_hidden/shown`, отсутствие повторного скрытия; маппинг всех `MenuId` в
  `TrayCommand` и их уникальность; весь существующий набор идёт зелёным.
  Компиляцию `tray-icon`/`muda` проверяет macOS-раннер CI; live-приёмка
  (медиа-клавиши, меню, Cmd-W/Cmd-Q) — по плану ниже.

## План проверки

CI (macOS-раннер): `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
`cargo test`, сборка с `--no-default-features`.

Live на macOS:

1. после запуска в строке меню появляется template-иконка, читаемая в светлой и
   тёмной теме;
2. меню показывает `Now playing` / `Paused` / `Nothing playing` и обновляется
   при play/pause/stop и смене станции;
3. скрытие кнопкой окна, Cmd-W, пунктом трея и IPC `hide` убирает окно, эфир
   продолжается, `orbitfm status` отвечает `window hidden`;
4. при скрытом окне медиа-клавиши и Control Center выполняют
   play/pause/next/previous/stop, а `status` отражает изменение;
5. показ из трея и IPC возвращает окно мгновенно, без пересоздания UI; фокус
   проверяется отдельно (Wayland-ограничение из ADR-0018 к macOS не относится);
6. пункты трея (громкость, тема, рулетка, Quit) исполняются при скрытом окне;
7. смена Focus/Overlay/Listening пересоздаёт окно, трей при этом не исчезает;
8. Quit завершает процесс, иконка исчезает, в логе нет паник;
9. регрессия Linux/Wayland: `hide`/`show`, SNI-меню и MPRIS работают как раньше.

## Ссылки

- ADR-0018 — скрытие окна закрытием вьюпорта (механизм для Linux/Windows)
- ADR-0020 — SNI-трей, меню и строка «сейчас играет»
- ADR-0024 — выделенное окно Windows для SMTC
- `docs/modules/06-media-controls.md`, `docs/modules/12-tray.md`
- Исходники на диске: `playwire-2.0.0/src/platform/macos.rs`,
  `eframe-0.37.0/src/native/{wgpu_integration,epi_integration}.rs`,
  `winit-0.30.13/src/platform_impl/macos/{window_delegate,app_state}.rs`,
  `tray-icon-0.26.1/src/lib.rs`, `muda-0.21.2/src/items/`
