# Модуль: Media Controls

**Путь в коде:** `src/media/`  
**Ответственность:** Интеграция с системными медиа-клавишами и виджетами (MPRIS / SMTC / Now Playing).  
**Зависимости:** playwire (предпочтительно) или souvlaki

---

## 1. Назначение

Позволить пользователю управлять воспроизведением через:

- Медиа-клавиши
- Системные UI (GNOME, KDE, Noctalia, playerctl и т.д.)
- ОС-специфичные контролы

## 2. Публичный интерфейс

```rust
pub struct MediaControls { /* ... */ }

impl MediaControls {
    pub fn new(config: PlatformConfig) -> Result<Self, MediaError>;

    /// Обновить отображаемые метаданные и состояние
    pub fn set_playback(&mut self, state: &PlaybackInfo);

    /// Подписка на команды от системы (Play/Pause/Stop/Next...)
    pub fn events(&self) -> mpsc::Receiver<MediaCommand>;
}

#[derive(Debug, Clone)]
pub struct PlaybackInfo {
    pub title: String,               // название станции
    pub artist: Option<String>,      // или country / tags
    pub art_url: Option<String>,     // favicon
    pub playing: bool,
    pub volume: f64,
}

#[derive(Debug, Clone)]
pub enum MediaCommand {
    Play,
    Pause,
    PlayPause,
    Stop,
    Next,                            // пока может быть no-op
    Previous,
    SetVolume(f64),
}
```

## 3. Платформенные особенности

- **Linux:** MPRIS v2 через D-Bus (zbus)
- **Windows:** System Media Transport Controls
- **macOS:** MPNowPlayingInfoCenter + remote commands

## 4. Минимальный набор MPRIS-свойств

- `Identity` / `DesktopEntry`
- `PlaybackStatus`
- `Metadata` (xesam:title, mpris:artUrl и т.д.)
- `Volume`
- Capabilities: CanPlay, CanPause, CanControl

## 5. Связанные ADR

- ADR-0006: Медиа-контроли
