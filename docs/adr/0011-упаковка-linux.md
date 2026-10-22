# ADR-0011: Поставка Linux и упаковка

**Статус:** Принято  
**Дата:** 2026-10-22  
**Участники решения:** ModernByte Architecture, Юрий Анатольевич  

## Контекст

Этап 6 требует упаковки AppImage / Flatpak / MSI / DMG. Фактическое окружение
разработки — Linux x86_64 (CachyOS, Wayland, mpv 0.41 / FFmpeg 12.1). Для Windows и
macOS отсутствуют нативные раннеры, SDK и сборки libmpv; кросс-проверка `cargo check`
упирается в C-зависимости (`aws-lc-sys` требует mingw-компилятор). Кроме того, в коде
пока нет Windows named pipe и привязки SMTC к HWND, а macOS Now Playing требует
запущенного `NSApplication` в момент регистрации.

Flatpak требует воспроизводимой сборки libmpv и vendored cargo-исходников; в окружении
нет `flatpak-builder`. MSI и DMG нельзя собрать и проверить без Windows/macOS.

При этом метаданные рабочего стола (desktop entry, иконка, AppStream) нужны для любого
формата поставки и не зависят от платформенных бэкендов.

## Решение

1. Единый источник метаданных рабочего стола — `packaging/linux/`:
   `org.modernbyte.OrbitFM.desktop`, `org.modernbyte.OrbitFM.metainfo.xml` и
   масштабируемая иконка `org.modernbyte.OrbitFM.svg`.
2. Основной проверяемый артефакт Linux — **AppImage**, который собирает
   `packaging/linux/build-appimage.sh`: release-сборка, `linuxdeploy` (бандлинг
   динамических библиотек, включая libmpv и FFmpeg) и `appimagetool`.
   Результат — `target/appimage/OrbitFM-<version>-x86_64.AppImage`.
3. Инструменты AppImage не входят в репозиторий: скрипт использует `LINUXDEPLOY` и
   `APPIMAGETOOL` из окружения либо скачивает continuous-сборки в
   `target/appimage-tools`. Для сред без FUSE применяется
   `APPIMAGE_EXTRACT_AND_RUN=1`.
4. CI проверяет только метаданные (`desktop-file-validate`, `appstreamcli validate`,
   `bash -n`); сборка AppImage остаётся локальной/ручной до появления release-workflow.
5. **Отложено до появления нативных раннеров и платформенных бэкендов:**
   - Flatpak — нужны vendored cargo-исходники и модуль libmpv/FFmpeg;
   - MSI — нужны Windows-сборка libmpv, named pipe IPC и SMTC с HWND;
   - DMG — нужна macOS-сборка libmpv и проверка Now Playing;
   - Windows (SMTC) и macOS (Now Playing) как пункты Этапа 6.

   Критерий возврата: зелёная сборка и ручная проверка медиа-интеграции на
   соответствующей ОС. До этого пункты ROADMAP остаются незакрытыми с пометкой
   «заблокировано».

## Альтернативы

| Вариант | Причина отклонения |
|---------|--------------------|
| Только tar.gz | Не соответствует ROADMAP и не даёт интеграции с рабочим столом |
| Flatpak первым | Воспроизводимее, но в текущем окружении не собирается и требует модуля libmpv |
| Статическая сборка musl | libmpv, wgpu и Wayland/EGL делают её непрактичной |
| Пакеты дистрибутивов (deb/rpm/AUR) | Отдельная инфраструктура и поддержка; возможна позже |

## Последствия

**Положительные:**

- Есть проверяемый переносимый артефакт для Linux
- Метаданные рабочего стола переиспользуются Flatpak и дистрибутивными пакетами
- Блокеры кроссплатформенности зафиксированы явно, а не замаскированы «зелёным» чекбоксом

**Отрицательные / Риски:**

- AppImage наследует ограничения хостового libmpv/FFmpeg и драйверов; проверен только
  на x86_64 Linux
- Continuous-инструменты не закреплены по хешу; для релизов потребуется pinning
- Flatpak/MSI/DMG остаются незакрытыми до появления соответствующей инфраструктуры

## Ссылки

- ADR-0002: Выбор GUI-стека
- ADR-0003: Выбор аудио-движка
- ADR-0005: Single-instance и IPC
- ADR-0006: Медиа-контроли
- AppImage documentation, linuxdeploy, appimagetool
- Desktop Entry Specification, AppStream specification
