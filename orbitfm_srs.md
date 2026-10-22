# OrbitFM — Technical Requirements Document & Engineering Specification (SRS v3.0)

**Проект:** OrbitFM (Desktop App & Noctalia v5 Plugin)  
**Инженерная эгида:** ModernByte  
**Автор / Архитектор:** Юрий Анатольевич (`code_warlord`)  
**Язык разработки:** Pure Rust (Edition 2024) + Luau / TOML (Noctalia v5 Wrapper)  
**Целевые платформы:** Linux (Niri 25.01+, Hyprland, KDE, GNOME), Windows, macOS  

---

## 1. Архитектура системы и жизненный цикл (Single-Daemon IPC)

OrbitFM проектируется по архитектуре **«Единый фоновый сервис + Легковесный графический GUI-клиент»**.

```text
┌────────────────────────────────────────────────────────┐
│                   Noctalia Bar / Shell                 │
│  [ 📻 OrbitFM Icon ] ── (по клику) ─────────────────┐  │
└─────────────────────────────────────────────────────┼──┘
                                                      │
                                          IPC (Unix Domain Socket)
                                                      │
┌─────────────────────────────────────────────────────▼──┐
│              OrbitFM App (Single Instance)             │
│ ┌────────────────────────────────────────────────────┐ │
│ │  Core Services (same process)                      │ │
│ │  - MPV Engine Session Owner                        │ │
│ │  - MPRIS v2 Interface                              │ │
│ │  - Radio Browser API Client & Cache                │ │
│ └─────────────────────────┬──────────────────────────┘ │
│                           │                            │
│ ┌─────────────────────────▼──────────────────────────┐ │
│ │  GUI Layer (egui + wgpu Custom Render Pass)        │ │
│ │  - 3D Globe Scene & R-Tree Spatial Raycaster       │ │
│ │  - Station List & Player Controls                  │ │
│ └────────────────────────────────────────────────────┘ │
└────────────────────────────────────────────────────────┘
```

### 1.1. Модель жизненного цикла процесса
1. **Единый процесс (`orbitfm`):** Владеет аудиосессией (`libmpv`), IPC-сервером и GUI. Отдельного демона `orbitfmd` нет (ADR-0005).
2. **Графическое окно (GUI):** Создается в рамках того же процесса. При закрытии окна (нажатие `[X]`, а в overlay-режиме — клик мимо) окно **скрывается** (Hide to Tray/Background), а аудиосессия продолжает играть.
3. **Повторный запуск (`orbitfm` CLI):** При вызове бинарника проверяется наличие активного сокета. Если процесс уже запущен, команда отправляется существующему экземпляру, а текущий бинарник завершается. Без аргументов отправляется `toggle`; при отсутствии экземпляра запускается GUI.

---

## 2. Утвержденный технологический стек (Tech Stack)

| Модуль | Технология / Crate | Назначение и обоснование |
| :--- | :--- | :--- |
| **Core & Async Engine** | Rust 1.95+ + `tokio` | Асинхронная обработка IPC, HTTP-запросов и событий |
| **GUI Framework** | `eframe` / `egui` | Реактивный кроссплатформенный UI с поддержкой прозрачности |
| **3D Rendering Pass** | `egui-wgpu` (`PaintCallback`) | Чистый нативный рендер 3D-шара через `wgpu` прямо в кастомный пасс `egui` (без несовместимого `three-d`) |
| **Spatial / Geo Index** | `geo` + `geojson` + `rstar` | Парсинг GeoJSON, R-Tree индекс стран для мгновенного `Point-in-Polygon` Raycasting |
| **Audio Core** | `libmpv-sys` (`libmpv` C-API) | Отказоустойчивое декодирование сетевых стримов (HLS, AAC, MP3) с поддержкой реконнекта |
| **Radio API Client** | `reqwest` + Server Discovery | Динамический поиск живых серверов Radio Browser и авто-переключение зеркала |
| **Desktop Integrations**| `mpris-player` + `zbus` | Нативная интеграция с системными медиа-клавишами и виджетами Linux |

---

## 3. Детальные спецификации подсистем

### 3.1. 3D-Рендеринг и Spatial Raycasting Engine

#### Рендер-пайплайн (`egui-wgpu` Integration)
* **Сцена:** 3D-сфера с UV-разверткой, матовой PBR-текстурой континентов и процедурным Fresnel-шейдером свечения атмосферы.
* **Рендеринг:** Осуществляется через `egui_wgpu::PaintCallback`. `wgpu` создаёт изолированный Pipeline State и рендерит 3D-сферу непосредственно в целевой `TextureView` UI-контекста.

#### Математика Raycasting & R-Tree Поиска
1. **Пересечение луча со сферой:** При клике ЛКМ вычисляется луч $R(t) = O + tD$ из координат курсора через обратную матрицу видовой проецирующей камеры $(M_{proj} \times M_{view})^{-1}$.
2. **Координатное преобразование:** Точка пересечения сферы $(x, y, z)$ переводится в географические координаты:
   $$\phi = \arcsin\left(\frac{y}{R}\right), \quad \lambda = \arctan2(z, x)$$
3. **R-Tree spatial lookup:** Координата $(\phi, \lambda)$ передается в `rstar::RTree` индекс прямоугольных рамок (Bounding Boxes) стран.
4. **Точная проверка (Point-in-Polygon):** Для отфильтрованных кандидатов из R-Tree запускается проверка `geo::contains` по точным `MultiPolygon` границам из датасета *Natural Earth 110m*.
5. **Обработка антимеридиана:** Полигоны, пересекающие $+180^\circ/-180^\circ$, нормализуются до единого координатного пространства.

---

### 3.2. Подсистема работы с Radio Browser API

1. **DNS Server Discovery:**
   * Перед первым запросом выполняется DNS-разрешение `all.api.radio-browser.info`.
   * Из полученного списка IP-адресов случайным образом выбирается активный сервер с минимальной задержкой (ping).
2. **HTTP Контракт:**
   * Каждый HTTP-запрос содержит обязательный заголовок:  
     `User-Agent: OrbitFM/1.0 (ModernByte; Linux; Rust)`
3. **Fallback & Retry Policy:**
   * При получении HTTP `5xx` или таймауте соединение автоматически переключается на следующее доступное зеркало из пула.

---

### 3.3. Аудиобэкенд и экспериментальный FFT

* **Основное воспроизведение:** Реализовано через `libmpv` C-API (`mpv_create`, `mpv_initialize`). Позволяет из коробки обрабатывать сетевые лаги, динамический битрейт и кодеки.
* **Изолированный FFT-трек (Экспериментальный):**
  Доступ к PCM-данным для спектроанализатора реализуется через подключение `audio-filter` в mpv с выводом в IPC-stream (pcm pipe). Подсистема визуализатора изолирована и не блокирует основной цикл рендеринга GUI.

---

## 4. Контракты интеграций и правила окон

### 4.1. Манифест плагина Noctalia v5 (`noctalia-plugin/plugin.toml`)

```toml
[plugin]
id = "org.modernbyte.orbitfm"
name = "OrbitFM"
version = "1.0.0"
description = "3D Interactive Global Radio Player"
author = "code_warlord (ModernByte)"
icon = "radio-symbolic"

[entry]
script = "plugin.luau"
```

### 4.2. Luau Адаптер для Noctalia v5 (`noctalia-plugin/plugin.luau`)

```luau
local noctalia = require("noctalia")

noctalia.register_widget({
    id = "orbitfm_launcher",
    icon = "radio",
    tooltip = "OrbitFM Player",
    on_click = function()
        -- Отправка асинхронной IPC-команды переключения окна
        noctalia.exec_async("orbitfm toggle")
    end
})
```

### 4.3. Оконные правила Niri 25.01+ (`wm-configs/orbitfm-niri.kdl`)

Соответствует официальной спецификации Niri (v25.01+):

```kdl
window-rule {
    match app-id="org.modernbyte.OrbitFM"

    open-floating true
    default-column-width { fixed 920; }
    default-window-height { fixed 660; }
    draw-border-with-background false
    
    // В Niri 25.01+ центрирование плавающих окон задается по умолчанию 
    // или через явную позиционирующую директиву:
    default-floating-position x=0 y=0 relative-to="center"
}
```

### 4.4. Оконные правила Hyprland (`wm-configs/orbitfm-hyprland.conf`)

```ini
windowrulev2 = float, class:^(org\.modernbyte\.OrbitFM)$
windowrulev2 = size 920 660, class:^(org\.modernbyte\.OrbitFM)$
windowrulev2 = center, class:^(org\.modernbyte\.OrbitFM)$
windowrulev2 = stayfocused, class:^(org\.modernbyte\.OrbitFM)$
```

---

## 5. Итоговая структура репозитория

```text
orbitfm/
├── assets/
│   ├── geojson/world-110m.json      <-- Offline векторные границы стран (Natural Earth)
│   └── icons/                       <-- SVG/PNG иконки интерфейса
├── noctalia-plugin/
│   ├── plugin.toml                  <-- Официальный валидный манифест Noctalia v5
│   └── plugin.luau                  <-- Luau-скрипт адаптера панели
├── wm-configs/
│   ├── orbitfm-niri.kdl             <-- Валидный конфигурационный файл Niri 25.01+
│   └── orbitfm-hyprland.conf        <-- Правила окон Hyprland
├── src/
│   ├── main.rs                      <-- CLI, Single-Instance & IPC Server (Unix Socket)
│   ├── app.rs                       <-- eframe/egui UI Loop & Layout
│   ├── audio/                       <-- libmpv Core Engine + MPRIS Interface
│   ├── geo/                         <-- GeoJSON R-Tree Index & Spatial Raycaster
│   ├── render/                      <-- egui-wgpu Custom Render Pass (3D Globe Pipeline)
│   ├── services/                    <-- Radio Browser Client & DNS Server Discovery
│   └── wm/                          <-- Platform WM Detector & Config Generator
├── Cargo.toml
└── README.md
```