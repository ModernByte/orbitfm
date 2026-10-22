#!/usr/bin/env bash
set -euo pipefail

# Собирает и устанавливает Flatpak в пользовательскую установку.
# Требует org.freedesktop.Sdk 25.08, rust-stable и flatpak-builder
# (локальный бинарник или org.flatpak.Builder).

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

BUILD_DIR="${ORBITFM_FLATPAK_BUILD_DIR:-$ROOT/target/flatpak-build}"
REPO_DIR="${ORBITFM_FLATPAK_REPO_DIR:-$ROOT/target/flatpak-repo}"
MANIFEST="packaging/linux/org.modernbyte.OrbitFM.yml"

if command -v flatpak-builder >/dev/null 2>&1; then
    BUILDER=(flatpak-builder)
elif flatpak info org.flatpak.Builder >/dev/null 2>&1; then
    # Внутри sandbox Builder нужен явный путь к пользовательской установке Flatpak.
    BUILDER=(flatpak run \
        --env=FLATPAK_USER_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/flatpak" \
        --command=flatpak-builder org.flatpak.Builder)
else
    echo "нужен flatpak-builder или org.flatpak.Builder" >&2
    exit 1
fi

"${BUILDER[@]}" \
    --user \
    --force-clean \
    --disable-rofiles-fuse \
    --repo="$REPO_DIR" \
    "$BUILD_DIR" \
    "$MANIFEST"

flatpak install --user -y "$REPO_DIR" org.modernbyte.OrbitFM
echo "org.modernbyte.OrbitFM установлен; запуск: flatpak run org.modernbyte.OrbitFM"
