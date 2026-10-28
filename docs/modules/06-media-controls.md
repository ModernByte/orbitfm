# Модуль: Media Controls

**Путь в коде:** `src/media/`  
**Ответственность:** Интеграция с системными медиа-клавишами и виджетами (MPRIS v2 на Linux; SMTC/Now Playing — по мере платформенной проверки).
**Зависимости:** playwire 2.0 (Linux-бэкенд — MPRIS через zbus), tokio; на Windows — `windows` (Win32) для скрытого окна SMTC

---

## 1. Назначение

Позволить пользователю управлять воспроизведением через:

- Медиа-клавиши
- Системные UI (GNOME, KDE, Noctalia, `playerctl` и т.д.)
- ОС-специфичные контролы (в перспективе)

Модуль не содержит бизнес-логики: он публикует метаданные в систему и передаёт команды
Application Core.

## 2. Публичный интерфейс

Полное описание — `docs/interfaces/06-media.md`. Кратко:

```rust
pub type WakeFn = Arc<dyn Fn() + Send + Sync>;

pub struct MediaControls { /* ... */ }

impl MediaControls {
    pub fn new(wake: WakeFn) -> Result<Self, MediaError>;
    pub fn set_playback(&mut self, info: &PlaybackInfo) -> Result<(), MediaError>;
    pub fn events(&mut self) -> &mut mpsc::UnboundedReceiver<MediaCommand>;
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlaybackInfo {
    pub title: String,
    pub artist: Option<String>,
    pub art_url: Option<String>,
    pub url: String,
    pub playing: bool,
    pub volume: f64,
    pub can_go_next: bool,
    pub can_go_previous: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MediaCommand {
    Play,
    Pause,
    PlayPause,
    Stop,
    Next,
    Previous,
    SetVolume(f64),
}
```

## 3. Платформенные особенности

- **Linux:** MPRIS v2 через playwire (zbus). Имя — `org.mpris.MediaPlayer2.OrbitFM`.
- **Windows:** System Media Transport Controls. playwire 2.0 требует HWND при
  создании (`PlayerConfig::hwnd`). `src/media/host_window.rs` создаёт на выделенном
  потоке скрытое top-level окно (`WS_POPUP` + `WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE`,
  без `WS_VISIBLE`) и передаёт его дескриптор в конфиг (ADR-0024). Окно живёт весь
  процесс, поэтому SMTC не зависит от показа/скрытия окна eframe (ADR-0018). Ошибка
  создания окна не фатальна — приложение работает без медиа-контролов. Рантайм-проверка
  на Windows ожидается.
- **macOS:** MPNowPlayingInfoCenter + remote commands через playwire; код
  платформенно-нейтрален, рантайм-проверка на macOS ожидается. Обработчики
  `MPRemoteCommandCenter` приходят на главный run loop
  (`playwire-2.0.0/src/platform/macos.rs:54-56`). С 0.13.0-rc.1 главный поток не
  паркуется: на macOS скрытие — это `Visible(false)` без остановки run loop
  (ADR-0026), поэтому remote-команды доставляются и при скрытом окне, а
  метаданные продолжают публиковаться `set_state`.

## 4. Конфигурация MPRIS

| Параметр | Значение |
|----------|----------|
| Identity | `OrbitFM` |
| DesktopEntry | `org.modernbyte.OrbitFM` |
| Track ID prefix | `/org/modernbyte/OrbitFM/track` |
| Volume | из `PlaybackInfo`, `0.0..=1.0` |
| Capabilities | `CanPlay`, `CanPause`, `CanControl`; `CanGoNext`/`CanGoPrevious` — по непустой видимой очереди; `CanSeek` выключен |

## 5. Потоки и пробуждение GUI

playwire вызывает колбэк в рабочем потоке D-Bus. Колбэк:

1. преобразует событие в `MediaCommand` (неизвестные и seek игнорируются);
2. кладёт команду в `mpsc::UnboundedSender`;
3. вызывает `WakeFn`, который делает `egui::Context::request_repaint()`.

Без шага 3 команда осталась бы в очереди до следующего пользовательского ввода или
анимации. Application Core разбирает очередь в `logic()` и выполняет аудио-команду.

`set_playback` дедуплицирует состояние: если `PlaybackInfo` не изменился, D-Bus не
трогается. Это важно, потому что `sync_media()` вызывается каждый кадр.

## 6. Соответствие состояний

| `AudioEngine` | `playing` в MPRIS | `PlaybackStatus` |
|---------------|-------------------|------------------|
| `Stopped`, `Error` | `false`, трек отсутствует | `Stopped` |
| `Paused` | `false` | `Paused` |
| `Playing`, `Buffering`, `Reconnecting` | `true` | `Playing` |

Метаданные берутся из текущей станции: `name` → title, `favicon` → artUrl,
`url_resolved` → `xesam:url`. Artist формирует Application Core: «штат, страна»,
иначе страна/штат по отдельности, иначе теги станции (Этап 8, фаза 4). Если станция
неизвестна, используется URL потока.

## 7. Ограничения

- `Next` / `Previous` ходят по видимой очереди с циклом; при пустом списке это no-op,
  и способности выключены. Отдельного плейлиста нет (ADR-0013).
- Перемотка (seek) не поддерживается для живого радио.
- Windows-бэкенд SMTC и macOS Now Playing не проверялись в рантайме (Linux-окружение
  разработки); Windows-код проверяется компиляцией и clippy в CI.
- На macOS remote-команды при скрытом окне доставляются, потому что run loop не
  останавливается (ADR-0026); live-проверка медиа-клавиш и Control Center
  выполняется на живом Mac по матрице ADR-0026.

## 8. Тестирование

- Unit-тесты маппинга событий playwire, включая клампинг громкости и отказ `NaN`, и
  переноса `can_go_next`/`can_go_previous` в `Capabilities`.
- Живая проверка на Linux: `playerctl -p OrbitFM status`, `volume`, `play-pause`,
  `stop`; IPC `status` должен отражать изменения в течение одного кадра.

## 9. Связанные ADR

- ADR-0006: Медиа-контроли
- ADR-0018: Скрытие окна закрытием вьюпорта
- ADR-0024: Выделенное окно для Windows SMTC
- ADR-0026: macOS — строка меню и Now Playing при скрытом окне
