# Модуль: Audio Engine

**Путь в коде:** `src/audio/`  
**Ответственность:** Воспроизведение интернет-радиостанций, управление сессией libmpv, реконнект, получение PCM (для FFT).  
**Зависимости:** libmpv (C API), tokio (для асинхронной обёртки событий)

---

## 1. Назначение

Единственный владелец аудиосессии в процессе OrbitFM.  
Все команды play/stop/volume/reconnect проходят только через этот модуль.

## 2. Публичный интерфейс (контракт)

```rust
/// Основной движок воспроизведения
pub struct AudioEngine { /* ... */ }

impl AudioEngine {
    /// Создаёт и инициализирует libmpv-контекст
    pub fn new() -> Result<Self, AudioError>;

    /// Начать воспроизведение URL (предпочтительно url_resolved)
    pub async fn play(&mut self, url: &str) -> Result<(), AudioError>;

    /// Остановить воспроизведение
    pub async fn stop(&mut self) -> Result<(), AudioError>;

    /// Установить громкость (0.0 ..= 1.0)
    pub async fn set_volume(&mut self, volume: f64) -> Result<(), AudioError>;

    /// Текущее состояние
    pub fn state(&self) -> PlaybackState;

    /// Подписка на события (ended, error, reconnecting и т.д.)
    pub fn subscribe(&self) -> broadcast::Receiver<AudioEvent>;
}

#[derive(Debug, Clone, PartialEq)]
pub enum PlaybackState {
    Stopped,
    Playing,
    Buffering,
    Reconnecting,
    Error(String),
}

#[derive(Debug, Clone)]
pub enum AudioEvent {
    Started { url: String },
    Stopped,
    Buffering,
    Reconnecting { attempt: u32 },
    Error { message: String },
    EndOfFile, // для обычных файлов, для радио почти не бывает
}
```

## 3. Внутреннее устройство

- Обёртка над `mpv_handle*` (FFI)
- Все вызовы libmpv выполняются в одном выделенном потоке / через канал команд (чтобы не нарушать требования thread-safety libmpv)
- Автоматический реконнект при обрыве HTTP-стрима (политика: экспоненциальный backoff, максимум N попыток)
- Опциональный audio-filter для вывода PCM (экспериментальный FFT)

## 4. Требования из официальной документации libmpv

- `LC_NUMERIC` должен быть `"C"`
- Сначала `mpv_create()`, затем `mpv_initialize()`
- Команды через `mpv_command` / `mpv_command_string` / `mpv_command_node`
- Свойства через `mpv_get_property` / `mpv_set_property`
- События через `mpv_wait_event`

## 5. Ошибки

Все ошибки мапятся в `AudioError` (thiserror).  
Критические ошибки (не удалось создать контекст) приводят к невозможности работы модуля.

## 6. Тестирование

- Unit-тесты на логику реконнекта (без реального libmpv)
- Integration-тесты с реальным libmpv и тестовым потоком (по возможности)
- Проверка, что при `stop` ресурсы корректно освобождаются

## 7. Связанные ADR

- ADR-0003: Выбор аудио-движка
