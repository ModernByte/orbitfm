#!/usr/bin/env bash
set -euo pipefail

# Собирает AppImage в target/appimage. Требует Linux x86_64 и системный libmpv.
# LINUXDEPLOY и APPIMAGETOOL можно задать путями к готовым AppImage-инструментам;
# иначе continuous-сборки скачиваются в target/appimage-tools.

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

TOOLS_DIR="${ORBITFM_APPIMAGE_TOOLS:-$ROOT/target/appimage-tools}"
WORK_DIR="$ROOT/target/appimage"
APPDIR="$WORK_DIR/AppDir"
ARCH="${ARCH:-x86_64}"

if [[ "$ARCH" != "x86_64" ]]; then
    echo "поддерживается только x86_64, получено: $ARCH" >&2
    exit 1
fi

# FUSE может отсутствовать в CI/контейнерах.
export APPIMAGE_EXTRACT_AND_RUN=1

LINUXDEPLOY="${LINUXDEPLOY:-$TOOLS_DIR/linuxdeploy.AppImage}"
APPIMAGETOOL="${APPIMAGETOOL:-$TOOLS_DIR/appimagetool.AppImage}"

# continuous-ассеты обновляются без смены URL, поэтому скачанный инструмент
# сверяется с зафиксированным дайджестом. Обновление хеша — осознанная правка
# вместе с проверкой нового AppImage; свои сборки задаются через env.
LINUXDEPLOY_SHA256="${LINUXDEPLOY_SHA256:-d900b99faf8656aa613038dafbd2cc23ca7f90794a9a00c9546f0ab572af94dd}"
APPIMAGETOOL_SHA256="${APPIMAGETOOL_SHA256:-ac7fd56feade6002c2f57db39587113275d351fce911a5fbb29ea84cd0666cab}"

ensure_tool() {
    local path="$1"
    local url="$2"
    local expected="$3"
    if [[ ! -x "$path" ]]; then
        mkdir -p "$(dirname "$path")"
        echo "скачивание $(basename "$path")" >&2
        curl -fL --retry 3 --connect-timeout 20 -o "$path" "$url"
        chmod +x "$path"
    fi
    # Проверяется и закэшированный инструмент: иначе подмена файла в каталоге
    # tools/ обходила бы пиннинг.
    if ! echo "$expected  $path" | sha256sum --check --quiet; then
        echo "$(basename "$path"): SHA256 не совпал с зафиксированным" >&2
        echo "  обновите дайджест осознанно или задайте LINUXDEPLOY_SHA256/APPIMAGETOOL_SHA256" >&2
        exit 1
    fi
}

ensure_tool "$LINUXDEPLOY" \
    "https://github.com/linuxdeploy/linuxdeploy/releases/download/continuous/linuxdeploy-x86_64.AppImage" \
    "$LINUXDEPLOY_SHA256"
ensure_tool "$APPIMAGETOOL" \
    "https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage" \
    "$APPIMAGETOOL_SHA256"

cargo build --release --locked

VERSION="$(awk -F'"' '/^version = / { print $2; exit }' Cargo.toml)"
OUTPUT="$WORK_DIR/OrbitFM-$VERSION-$ARCH.AppImage"

rm -rf "$APPDIR"
install -Dm755 target/release/orbitfm "$APPDIR/usr/bin/orbitfm"
install -Dm644 packaging/linux/org.modernbyte.OrbitFM.desktop \
    "$APPDIR/usr/share/applications/org.modernbyte.OrbitFM.desktop"
install -Dm644 packaging/linux/org.modernbyte.OrbitFM.metainfo.xml \
    "$APPDIR/usr/share/metainfo/org.modernbyte.OrbitFM.metainfo.xml"
install -Dm644 packaging/linux/org.modernbyte.OrbitFM.svg \
    "$APPDIR/usr/share/icons/hicolor/scalable/apps/org.modernbyte.OrbitFM.svg"
install -Dm644 packaging/linux/org.modernbyte.OrbitFM.png \
    "$APPDIR/usr/share/icons/hicolor/256x256/apps/org.modernbyte.OrbitFM.png"

ARCH="$ARCH" "$LINUXDEPLOY" \
    --appdir "$APPDIR" \
    --executable "$APPDIR/usr/bin/orbitfm" \
    --desktop-file "$APPDIR/usr/share/applications/org.modernbyte.OrbitFM.desktop" \
    --icon-file "$APPDIR/usr/share/icons/hicolor/scalable/apps/org.modernbyte.OrbitFM.svg"

# appimagetool ищет устаревшее имя *.appdata.xml, хотя AppStream использует *.metainfo.xml.
ln -sf org.modernbyte.OrbitFM.metainfo.xml \
    "$APPDIR/usr/share/metainfo/org.modernbyte.OrbitFM.appdata.xml"

# С 31.10.2026 continuous-appimagetool требует PNG в корне AppDir, чтобы
# создать .DirIcon; SVG в hicolor/scalable он больше не принимает.
install -Dm644 packaging/linux/org.modernbyte.OrbitFM.png \
    "$APPDIR/org.modernbyte.OrbitFM.png"

# Метаданные проверяются отдельно (appstreamcli --no-net); appimagetool без сети
# считает URL недоступными и прерывает сборку.
ARCH="$ARCH" "$APPIMAGETOOL" --no-appstream "$APPDIR" "$OUTPUT"
echo "$OUTPUT"
