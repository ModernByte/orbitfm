# Интерфейс: Media Controls

Реализация — конкретная структура `MediaControls` на базе playwire (MPRIS v2 через
D-Bus на Linux). Публичный трейт не используется. Колбэк playwire выполняется в рабочем
потоке D-Bus и не должен блокироваться: он только кладёт команду в неограниченную очередь
и вызывает `wake`, чтобы GUI-цикл обработал её на ближайшем кадре.

```rust
/// Пробуждение GUI-цикла после команды от системного медиа-виджета.
pub type WakeFn = std::sync::Arc<dyn Fn() + Send + Sync>;

/// Информация, отображаемая системой.
#[derive(Debug, Clone, PartialEq)]
pub struct PlaybackInfo {
    pub title: String,
    pub artist: Option<String>,
    pub art_url: Option<String>,
    pub url: String,
    pub playing: bool,
    pub volume: f64,
}

/// Команды, приходящие от системы.
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

/// Ошибки медиа-контролов.
#[derive(Debug, thiserror::Error)]
pub enum MediaError {
    #[error("не удалось инициализировать медиа-контролы: {0}")]
    Init(String),
    #[error("не удалось обновить метаданные: {0}")]
    Publish(String),
    #[error("платформа не поддерживается")]
    UnsupportedPlatform,
}

/// Обёртка над системными медиа-контролами.
pub struct MediaControls { /* ... */ }

impl MediaControls {
    /// Регистрирует MPRIS-имя и начинает принимать команды системы.
    pub fn new(wake: WakeFn) -> Result<Self, MediaError>;

    /// Обновляет состояние, отображаемое системой.
    /// Повторный вызов с идентичными данными не обращается к D-Bus.
    pub fn set_playback(&mut self, info: &PlaybackInfo) -> Result<(), MediaError>;

    /// Очередь команд от системы.
    pub fn events(&mut self) -> &mut tokio::sync::mpsc::UnboundedReceiver<MediaCommand>;
}
```

## Семантика

| Поле / команда | Поведение |
|----------------|-----------|
| `title`, `artist`, `art_url` | метаданные `Track`; пустой `title` означает отсутствие трека |
| `url` | идентификатор трека и `xesam:url`; на Этапе 2 — URL потока |
| `playing` | `true` → `PlaybackStatus = Playing`, `false` → `Paused` при наличии трека и `Stopped` без него |
| `volume` | `0.0..=1.0`, передаётся в MPRIS `Volume` |
| `Play` | возобновление или повторный запуск текущего URL |
| `Pause` | пауза активного потока |
| `PlayPause` | переключение в зависимости от состояния |
| `Stop` | остановка воспроизведения |
| `SetVolume` | конечное значение приводится к `0.0..=1.0` |
| `Next` / `Previous` | принимаются, но пока не действуют (Этап 3) |

`set_playback` возвращает `Publish`, если playwire не смог обновить состояние. Application
Core логирует ошибку и продолжает работу: отсутствие MPRIS не фатально.

Capabilities на Этапе 2: `CanPlay`, `CanPause`, `CanControl`; `Next`, `Previous` и seek
выключены, так как переключение станций появится вместе с Radio Browser.
