# Настройки как отдельное окно + полноценный ритуал сна (ингибирование ОС, отсчёт, действие, предупреждение, фейд)

## Context

Владелец поставил две задачи.

**(1) Настройки — настоящее окно.** Сейчас это самописная модалка внутри окна стола:
скрим + `egui::Area` с `Order::Foreground` (`src/app.rs:1391–1453`), геометрия
`SETTINGS_MODAL = (760.0, 544.0)` с clamp по вьюпорту (`src/ui/layout.rs:21, 280–292`),
и блокирование глобуса через `blocks_point` (`src/ui/layout.rs:252–263`). Мотивация
владельца — реальный сценарий: кнопка «Settings» в расширении браузера показывает
панель/стол, а сверху ложится модалка; «это полная фигня». Плюс окно обязано
закрываться вместе с завершением сервиса. Требуется также определить варианты размеров
окна настроек и прописать правила окон в интеграциях.

**(2) Таймер сна.** Пять требований, где первое помечено «Это ВАЖНО»:
не давать ОС уйти в сон, пока играет радио и таймер считает; иконка часов → оставшееся
время (1/мин до последней минуты, затем 1/с); действие по истечении (Ничего / Сон /
Выключить); красивое центрированное окно предупреждения за минуту до действия с обратным
отсчётом, убывающим круговым прогресс-баром и рампой зелёный → жёлтый → красный; очень
плавное затухание громкости.

Сегодня таймер — `SleepTimer { deadline, minutes }` (`src/session/sleep.rs:16–19`),
срабатывание = `stop_playback` + тост (`sleep.rs:86–96`), единственный one-shot репейн
на дедлайн (`sleep.rs:99–101`, `park.rs:123–160`), никакого ингибирования ОС, действия
и предупреждения.

**Решения владельца этой сессии:** ингибитор — дочерний `systemd-inhibit` (без новой
зависимости); «Отменить» в предупреждении отменяет только действие; фейд 300 с
экспонентой в дБ; размер окна настроек выбирается только автоматически, без настройки.

Ожидаемый результат: шесть принятых ADR, окно настроек реальной оконной поверхностью с
правилами WM в трёх синхронных местах, и таймер сна, который будит машину только когда
пользователь сам этого хочет.

## Доказательная база (проверено на этой машине и в источниках)

| Факт | Источник |
|---|---|
| `mode=delay` бесполезен: `InhibitDelayMaxSec=` по умолчанию **5 с**, потом операция выполняется всё равно | `man 5 logind.conf` |
| `mode=block` на `sleep` **не** блокирует закрытие крышки: `LidSwitchIgnoreInhibited=` по умолчанию **yes** | `man 5 logind.conf` |
| `delay` доступен только для `sleep` и `shutdown`; `idle`+`delay` — недопустимая комбинация | `man 1 systemd-inhibit` |
| `systemd-inhibit --list`: 7 локов, ни одного `sleep/block`; noctalia держит `idle/block` + `sleep/delay`, niri — `handle-power-key/block` | замер этой сессии |
| fd-лок logind живёт ровно пока открыт fd ⇒ `busctl` не годится (fd умирает вместе с ним) | `login1.Manager.Inhibit` возвращает `h` |
| `egui::Context::show_viewport_immediate(id, builder, FnMut(&mut Ui, ViewportClass) -> T)` существует | `egui-0.37.0/src/context.rs:4662` |
| На desktop `embed_viewports = false` ⇒ потомок = настоящее окно; при отсутствии поддержки — `ViewportClass::EmbeddedWindow` прямо в колбэке | `eframe-0.37.0/src/native/winit_integration.rs:59` |
| wgpu-бэкенд регистрирует immediate-рендер **на каждый** `run_native` ⇒ окно настроек может открыть предупреждение как своего потомка | `eframe-0.37.0/src/native/wgpu_integration.rs:383` |
| `open_settings()` при скрытом окне вызывает `show()` (`src/session/mod.rs:354–361`) — источник жалобы владельца | код |
|Portal-путь на этой машине даёт только `Idle` (flags=8 → granted; 4/1/2/12 → denied), метода Suspend/PowerOff в portal нет | замер предыдущей сессии |
| `Language::ALL` = 17 (16 локалей + `System`), гейт `assert_catalog_complete` | `src/settings/language.rs:44`, `src/i18n/mod.rs:118` |

## Волна 1 — ADR (блокер остальных, кода не касается)

Новые: **0053** «Окно настроек как отдельное окно» (immediate-вьюпорт + второй носитель
для припаркованного процесса, инвариант `settings_open`, три размера и схема заголовков,
отказ от scrim/`blocks_point`); **0054** «Запрет сна на время таймера» (почему
`systemd-inhibit`, а не `busctl`/`zbus`, `what=sleep`/`mode=block`, строка про крышку и
про `InhibitDelayMaxSec`, `PR_SET_PDEATHSIG`, владение в `Session`, macOS
`caffeinate -i -w <pid> -t <secs>`, Windows `SetThreadExecutionState` + новые features
`windows`); **0055** «Действие по истечении таймера сна» (`SleepAction`, порядок шагов,
точные argv по ОС, привилегии, политика отказа, `ORBITFM_POWER_DRY_RUN` как
read-only-диагностика); **0056** «Затухание звука» (фейд в аудио-воркере, линейно в дБ,
D=45, дефолт 300 с, конфликт с ручной громкостью); **0057** «Отсчёт оставшегося времени»
(каденция 1/мин → 1/с, `next_event` вместо `sleep_repaint_after`/`sleep_deadline`,
поверхности); **0058** «Экран предупреждения» (гибрид потомок/отдельный прогон, кольцо,
рампа, Cancel = отмена действия + восстановление громкости).

`## Уточнение (…)`: 0031 §4/§6, 0032 §7, 0021 п. 4, 0046, 0051 §2/§5, 0018, 0026, 0037,
0039, 0042, 0052. Скелеты правок `docs/interfaces/`: 05-ipc, 09-settings, 10-session,
07-app, 11-tray, 12-i18n, 13-events, 14-native-host.

## Волна 2 — ядро сна и настроек (чистое, без ОС и без UI)

`src/session/sleep.rs`: поля `fade_start`, `warning_shown`; методы
`next_display_change(now)` (остаток > 60 с → до целой минуты; ≤ 60 с → до целой секунды),
`display_seconds(now)`, `next_event(now)` = min(deadline, fade_start, warning_start,
display-tick). `poll_sleep` → `poll_sleep_events` с порядком fade → warning → expiry;
`sleep_deadline` удаляется, единственный потребитель — `park.rs:61`.
**Инвариант «интервальных таймеров нет» сохраняется**: ветка `sleep_until` в
`wait_on` (`park.rs:115–160`) переиспользуется под `next_event`, нового потока нет.

`src/settings/mod.rs`: enum `SleepAction { Nothing, Suspend, PowerOff }`
(kebab-case `as_str`/`parse` по образцу `NetworkProfile`, `settings/mod.rs:237–243`) и
четыре ключа с `#[serde(default)]`: `sleep_action` = `Nothing`, `sleep_fade_seconds` =
`300` (ряд `SLEEP_FADE_CHOICES = [0, 60, 120, 180, 300]` + sanitize по образцу
`nearest_viz_bars`, `settings/mod.rs:371–372`), `sleep_prevent_sleep` = `true`,
`sleep_warning` = `true`. Неизвестное значение — ошибка разбора и карантин файла по
ADR-0062 (на момент плана — `SettingsError::Parse` и фолбэк на `ephemeral()`).

`src/i18n/mod.rs`: `sleep_remaining_label(language, secs)` рядом с `sleep_option_label`
(:70–77) — `M:SS` / `H:MM:SS` (`MAX_SLEEP_MINUTES = 720` ⇒ потолок `12:00:00`), новых
строк не требует. Требуют: ~15 строк каталога (4 подписи настроек + 3 значения действия
+ окно предупреждения + `SleepExpired`-соседи `InhibitionLost`, `SleepActionFailed` +
трафарет метки трея «осталось {time}») **во всех 16 файлах локалей сразу** — иначе гейт
`assert_catalog_complete` валит волну 5.

IPC: расширяем глагол `sleep`, не плодим top-level команд (`src/ipc/protocol.rs:192–197,
361–381`): `sleep action <nothing|suspend|power-off>`, `sleep fade <0|60|120|180|300>`,
`sleep inhibit <on|off>`, `sleep warning <on|off>`; ответы `ok sleep action suspend`.
Точки: `protocol.rs:162–163`, `310–313`, `517–530`, таблицы round-trip `:665`, `:951`;
исполнение — `src/session/commands.rs:178–185`; allowlist native host'а по префиксам уже
покрывает (`src/native_host.rs:385–414`).

`status --json` — только добавления (схема 1, ADR-0037): в `sleep`
(`src/session/mod.rs:574–587, 644–673`) → `inhibited: bool`, `action: string`,
`warning: bool`, `fade_seconds: u16`; плюс `remaining_secs` получает документированную
гранулярность (≤ 1 мин, ≤ 1 с в последнюю минуту) и top-level `settings_open: bool`.
`docs/interfaces/13-events.md` §5 — до +60 кадров в последнюю минуту.

## Волна 3 — уровень питания: новый `src/power/`

`mod.rs` + `linux.rs` + `macos.rs` + `windows.rs` + `fake.rs`:
`InhibitBackend::{LogindInhibit, Caffeinate, SetThreadExecutionState, Unavailable}`,
`InhibitionGuard` (Drop ⇒ release), `PowerInhibit::acquire(why)`.

Linux — дочерний процесс (решение владельца):
`systemd-inhibit --what=sleep --who=OrbitFM --why="Radio sleep timer" --mode=block sleep infinity`,
argv фиксирован, shell не запускается — precedent `apply_hyprland_rules()`
(`src/integration/wm.rs:233–258`). Носитель лока (`sleep infinity`) убивается при
release; от осиротевшего лока — `CommandExt::pre_exec` + `libc::prctl(PR_SET_PDEATHSIG,
SIGKILL)` (`libc` уже в зависимостях). Проверка живости — `child.try_wait()` в
`poll_sleep_events`, без нового таймера: носитель умер ⇒ `NoticeKey::InhibitionLost`,
не тикаем заново. `busctl` для `Inhibit` неприменим (fd живёт в fd-таблице busctl),
`zbus` — отклонено владельцем (новая прямая зависимость ради одного вызова).

macOS: `caffeinate -i -w <our_pid> -t <secs>` — новых crate нет. Windows:
`SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED)` из главного потока,
release — `ES_CONTINUOUS`; добавляются **features**, а не crate: `Win32_System_Power`
(`Cargo.toml:60–79`). `ES_DISPLAY_REQUIRED` не ставим.

Владение — в `Session` (`sleep_inhibition: Option<InhibitionGuard>`), **не** в event loop
окна: таймер живёт и в `tick()` (`session/mod.rs:297–315`), и в парковке
(`park.rs:59–111`). Идемпотентная `Session::sync_inhibition()` вызывается из `tick()`
после `poll_sleep_events()`, из `park()` рядом с `sync_tray`/`publish_status` и из
`quit()`. Состояние acquire: `timer_active && audio.state() != Stopped` — ровно формулировка
владельца «пока играет радио и таймер считает». Отказ polkit ⇒ таймер продолжает работать,
в разделе «Окно» показывается `Unavailable` + detail ошибки (`notice_text(..., Some(detail))`,
образец `src/app.rs:1244–1252`).

## Волна 4 — фейд в аудио-воркере

`src/audio/worker.rs`: состояние `Fade { started_at, duration, start_pct }`, команды
`AudioCommand::StartFade { duration_ms }` / `CancelFade { restore_volume }`, шаг в том же
drain-цикле на ~20 Hz (`mpv.wait_event(0.05)`). Считать в `tick()` нельзя — в парковке UI-
пассов нет вообще.

Кривая: mpv-свойство `volume` = линейный процент усиления (`apply_volume`,
`worker.rs:289–295`), поэтому «очень плавно» для слуха = линейно в дБ:
`pct(t) = pct0 · 10^(−(45/20)·(t/T))`, при `t ≥ T` один шаг в 0. При T=300 с это −9 дБ/мин
(ощутимая громкость падает вдвое каждые ~66 с); шаг на 20 Hz = 0,0075 дБ, zipper-noise нет.
Отвергнуто с числами: линейно в % (к половине окна всего −6 дБ, последние 30 с дают −20 дБ —
«рывком»), квадратичная амплитуда (−30 дБ к 63 % окна — звук «пропал», а таймер ещё считает).

Конфликты: явный `SetVolume` отменяет фейд без восстановления (воля пользователя выше
автоматики); сброс/перестановка таймера ⇒ `CancelFade { restore_volume: fade_pre_volume }`;
`Settings.volume` не пишется (ADR-0031 commit-on-release); `fade_seconds = 0` ⇒ сегодняшнее
поведение, стоп сразу.

## Волна 5 — поверхности отсчёта

`icons::sleep_button` (`src/ui/icons.rs:306–329`) → `sleep_status_button(ui, palette, hint,
remaining: Option<&str>, active)`: при активном таймере вместо `Icon::Clock` — моноширинный
текст фиксированной ширины (под максимум `12:00:00`), чтобы цифры не дёргали соседние кнопки
каждый тик; при неактивном glyph остаётся. Правки вызовов: `ui/sleep_picker.rs:17–46`,
`ui/hud.rs:308, 521`, `ui/capsule.rs:301–303`, панель.
Трей: `TraySnapshot.sleep_remaining_secs` (`src/session/tray.rs:137–166`) +
`tray::text::sleep_label` (`src/tray/text.rs:125–129`) → «Sleep timer · 12:34»; пункты
подменю (`src/tray/macos.rs:186–189, 265–313`) остаются длительностями — это выбор, не статус.
Родительский репейн: `src/app.rs:1580–1583` с `sleep_repaint_after` на `next_display_change`.

## Волна 6 — окно настроек (самая большая; монопольно владеет `src/app.rs` и `src/ui/layout.rs`)

**6a — носитель «стол виден».** `draw_settings` (`src/app.rs:1362–1456`) остаётся единственной
функций вёрстки; вызов оборачивается в `ctx.show_viewport_immediate(SETTINGS_VIEWPORT_ID,
builder, |ui, class| …)` прямо в `draw()`. Внутри колбэка:
`ViewportClass::EmbeddedWindow` ⇒ прежняя модалка в этом же окне + `warn!` (бэкенд без
мультивьюпортности не ломается). Из потомка наружу — `Vec<UiAction>` через `&mut`-перехват
(именно поэтому immediate, а не deferred: `&mut Session` в `FnOnce + Send + Sync + 'static`
не помещается). Удаляются: scrim-`Area` (`app.rs:1391–1415`), `settings_rect` и его clamp
(`layout.rs:280–292`), позиция `settings` из `blocks_point` (`layout.rs:252–263`), крестик в
шапке панели, Esc-в-потомке; Esc в столе больше не закрывает настройки.

**6b — носитель «стол скрыт».** Без UI-пасса immediate-потомок родиться не может
(`run_session` `main.rs:419–443`: hide ⇒ реальный close ⇒ `park()`), а сценарий владельца —
именно этот. Поэтому: `ParkOutcome::Settings` (новый вариант, `park.rs:27–32`; матчится
одним местом в `main.rs`), в `session/window.rs` — `enum RootSurface { Desk(WindowMode),
Settings(SettingsSize) }` (не новый `WindowMode`: `mode` — публичное поле статуса), и
`run_window` строит для него обычное декорированное окно с заголовком
`OrbitFM · Settings · <вариант>`, а `OrbitApp::new` в этом surface не создаёт `GlobeRenderer`
(строки 165–187) и не печатает `AdapterReport`. `open_settings(origin)` перестаёт дёргать
`show()`: если есть UI-пасс — потомок, если процесс припаркован — `UiIntent::Settings`.
Закрытие корня-настроек ⇒ `end_window()` ⇒ `park()`, стол не показывается; при
`session.quit()` настройки закрываются вместе с процессом — прямое требование владельца.

**Размеры (только авто, решение владельца).** `SettingsSize::{Compact, Medium, Wide}` в
`session/window.rs`: Compact 420×560 (min 360×480), Medium 760×544 (min 560×420, сегодняшняя
модалка), Wide 1040×720 (min 760×480). Чистое `for_open(category, mode)` — категория ⇒
вариант: `Integrations | Stats` → Wide; `Language | About` → Compact; остальное → Medium.
Заголовок несёт вариант, потому что правила WM матчатся по `title` (прецедент —
`OVERLAY_TITLE`/`PANEL_TITLE`, `window.rs:29–31`, ADR-0021 п. 4). Геометрия не персистится:
на Wayland размер задаёт правило WM, иначе два источника правды.

## Волна 7 — финальная интеграция (две независимые половины)

**(а) Правила WM.** `HYPRLAND_RULES` 14 → 18: три `size` под заголовки настроек +
`size 420 320,title:^OrbitFM · Sleep Warning$`. `float`/`center`/`stayfocused` остаются
классовыми (`src/integration/wm.rs:215–221`) — перенос `stayfocused` на заголовки сознательно
**не** делаем: правил больше, а выигрыша нет. Зеркало в
`wm-configs/orbitfm-hyprland.conf:6–22` держим обязательным тестом
`hyprland_array_mirrors_the_conf_snippet`; в
`wm-configs/orbitfm-niri.kdl` — 4 существующих блока + 4 новых по тому же шаблону
(`open-floating true` + фиксированные размеры, без `default-floating-position`: настройки и
предупреждение центрируются композитором) и расширение теста
`niri_include_points_at_the_installed_rule_file` требованием «все заголовки настроек
присутствуют»; дрейф сниппета ⇒ `InstallState::UpdateAvailable`
(`src/integration/status.rs`, `inspect()`).

**(б) Окно предупреждения.** `src/ui/sleep_warning.rs::show(...) -> Vec<WarningAction>` —
одна функция на оба носителя; пресет 420×320, `decorations(false)` + `transparent(true)` +
glass по всему rect (helper `widgets::alert_surface`, куда переезжает цикл теней
`app.rs:1424–1430`). Кольцо — сегментная полилиния (arc-API в egui 0.37 нет; precedent
`orbit_ring.rs:383–398` и `:405–451`), `SEGMENTS = 96`, отсчёт ~48 px по центру, ниже —
«Отменить» и строка действия. Рампа `warning_color(p)` на существующем `mix_colors`
(`src/ui/palette.rs`): p ≥ 0,5 → жёлтый→зелёный, иначе красный→жёлтый. Носители:
immediate-потомок (от стола **или** от окна настроек — TLS-рендер регистрируется на каждый
прогон) либо, когда процесс припаркован, `ParkOutcome::Warn` + отдельный `run_native`
той же `Session` (ADR-0018/0021 легализуют несколько прогонов; правило «не более одного
GUI-процесса» соблюдено). Показ ровно один на таймер (`warning_shown`).
Cancel ⇒ `sleep_action_once = Nothing` + `warning_shown = true`: **действие** отменяется,
таймер досчитывает, громкость возвращается через `CancelFade { restore_volume }` — решение
владельца. Esc и `close_requested` потомка = Cancel (безопасная трактовка), отдельной кнопки
«выключить сейчас» нет.

**Порядок при истечении** (`Session::sleep_expiry`, тело уходит из `sleep.rs:86–96`):
`clear_sleep` → **release inhibitor** (иначе собственный `Suspend` упрётся в наш `block`-лок)
→ снять фейд и `stop_playback` → `sync_tray` + `publish_status` (все поверхности снимают
отсчёт до того, как окно может исчезнуть) → закрыть предупреждение → действие.
Linux: `busctl --user call org.freedesktop.login1 /org/freedesktop/login1
org.freedesktop.login1.Manager Suspend|PowerOff b true` (здесь `busctl` применим: возврата
нет, fd не передаётся; `interactive=true` — чтобы polkit мог спросить; на этой машине
`allow_active=yes` для обоих), фолбэк `loginctl suspend|poweroff`. macOS: `pmset sleepnow` /
`osascript … shut down` — UNVERIFIED. Windows: `SetSuspendState` / `InitiateSystemShutdownExW`
(+`Win32_Security`). Ошибка действия не фатальна: `NoticeKey::SleepActionFailed` с detail из
stderr, без ретраев. `ORBITFM_POWER_DRY_RUN=1` — лог вместо spawn (иначе ветка неизвлекаемо
не тестируется).

## Критические файлы

`docs/adr/0053…0058` (новые) · `src/session/sleep.rs` · `src/session/mod.rs` ·
`src/session/park.rs` · `src/session/window.rs` · `src/session/commands.rs` ·
`src/session/tray.rs` · `src/settings/mod.rs` · `src/power/*` (новый модуль) ·
`src/audio/worker.rs` · `src/audio/mod.rs` · `src/app.rs` · `src/ui/layout.rs` ·
`src/ui/sleep_warning.rs` (новый) · `src/ui/sleep_picker.rs` · `src/ui/icons.rs` ·
`src/ui/settings_panel.rs` · `src/tray/text.rs` · `src/tray/macos.rs` · `src/i18n/*` (16 локалей) ·
`src/ipc/protocol.rs` · `src/integration/wm.rs` · `src/integration/status.rs` ·
`wm-configs/orbitfm-{hyprland.conf,niri.kdl}` · `src/main.rs` · `Cargo.toml`/`Cargo.lock` ·
`docs/interfaces/{05,07,09,10,11,12,13,14}*.md` · `docs/modules/*` · `README.md` ·
`docs/{CHANGELOG,ROADMAP,VERSIONMAP}.md`

## Verification

Каждая волна — отдельно: `cargo fmt --all --check`; `cargo clippy --all-targets --
-D warnings`; `cargo clippy --all-targets --no-default-features -- -D warnings`;
`cargo test --all-targets`; `bash packaging/cargo-deny-check.sh`;
`cargo check --all-targets --locked`.

Юниты (без egui и без ОС): таблица `SettingsSize::for_open` и «заголовок ↔ вариант»;
`sleep_remaining_label` (59→`0:59`, 3600→`1:00:00`, 43200→`12:00:00`);
`fade_gain` — монотонность, `gain(0)=pct0`, `gain(T)=0`, таблица −9 дБ/мин и **отдельный**
тест «не линейна в процентах»; `warning_color` на концах и середине; `next_event` — порядок
fade→warning→expiry→display-tick и отсутствие пробуждений «в никуда»; `warning_shown` — один
раз на таймер, сброс в `set`/`clear`; serde-совместимость старого `settings.json` и отказ на
`sleep_action: "reboot"`; round-trip новых IPC-команд + ошибка на `sleep fade 75`;
`status_json` — новые поля, `STATUS_SCHEMA` по-прежнему 1; `blocks_point` больше не блокирует
центр; `hyprland_array_mirrors_the_conf_snippet`.
Негативные фикстуры: `PATH` без `systemd-inhibit` ⇒ `Unavailable`, `inhibited=false`, таймер
работает; контейнер без logind ⇒ `power::linux::detect()` не зависает и не спавнит (assert,
что spawn не вызывался); «носитель лока умер» (fake `try_wait() == Some(exit)`) ⇒ сброс +
notice без ретраев; `sleep_fade_seconds: 9999` ⇒ sanitize; `embed_viewports=true` ⇒ настройки
рисуются embedded и не паникуют.

Live (в план-протокол, в CI не проверяется): `loginctl list-inhibitors` видит наш лок и теряет
его после `sleep clear` **и** после `kill -9` нашего процесса (доказательство
`PR_SET_PDEATHSIG`); `systemctl suspend` при активном блоке ⇒ отказ; закрытие крышки ⇒ сон
всё равно происходит (доказательство `LidSwitchIgnoreInhibited=yes`); реальное
`Suspend`/`PowerOff` — только в VM или под `ORBITFM_POWER_DRY_RUN=1`; `hyprctl clients` и
`niri msg windows` — размеры/float/центрирование настроек и предупреждения;
`grep -c window-rule ~/.config/niri/...` 4 → 8 и `UpdateAvailable` на старом конфиге;
сценарий владельца — «Settings» из расширения при спрятанном столе: окно одно, стола нет;
смена режима при открытых настройках; субъективная проверка фейда 300 с владельцем.
Ручная сессия: `cargo run -- --command "sleep 30"` → `loginctl list-inhibitors` →
`status --json | jq .sleep` → `sleep action power-off` → `settings`.

## Чего сознательно не делаем

`zbus` как прямая зависимость и `IOPMAssertionCreateWithName`/`core-foundation` (отклонено
владельцем — дочерний процесс покрывает задачу без новых crate); `mode=delay`
(`InhibitDelayMaxSec=5` ⇒ не гарантия); `handle-lid-switch` и `ES_DISPLAY_REQUIRED`
(намерение пользователя и погасание экрана — не наша задача); deferred-вьюпорт с
`Arc<RwLock<…>>` (второй источник истины мимо `Session`); новую настройку размера окна;
`Hibernate`/`Restart`/`Logout` как действия (разные привилегии на трёх ОС, «и т.д.» из
запроса не обязывает); перенос геометрии настроек в `Settings`; запись `Settings.volume`
фейдом; ретраи действия по истечении; изменение `status --json` иначе как добавление полей;
универсальный `doctor`; интервальные таймеры.

## Открытые допущения (закрываются live-протоколом, не обещаниями в коде)

Законность `show_viewport_immediate` в первом пассе свежего `run_native` (mode-switch);
`close_requested()` именно у потомка; отсутствие `GlobeRenderer` в settings-root прогоне не
ломает стиль/иконки/шрифты (все `install` — на `Context`, `app.rs:153–157`); поведение
`RequestUserAttention` на niri; вид прозрачного окна предупреждения с SSD-скруглениями;
`pmset`/`osascript` на macOS без root; каденция 1/мин в парковке не будит лишнего.
Ни один текст в README/вики не должен утверждать работоспособность того, для чего нет лога
или замера, — правило честности из ADR-0052 применяется и здесь.
