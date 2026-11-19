#!/bin/sh
# Установка OrbitFM одной командой (Linux x86_64, без root).
#
#   curl -fsSL https://raw.githubusercontent.com/ModernByte/orbitfm/main/install.sh | sh
#   curl -fsSL https://raw.githubusercontent.com/ModernByte/orbitfm/main/install.sh | sh -s -- --uninstall
#
# Скрипт только скачивает опубликованный релиз: AppImage, SHA256SUMS и
# install-appimage.sh. Суммы сверяются до запуска, интеграция в меню и PATH
# выполняется опубликованным помощником из того же релиза (ADR-0011), поэтому
# установка и удаление всегда соответствуют конкретной версии.

set -eu

REPO="${ORBITFM_REPO:-ModernByte/orbitfm}"
API="https://api.github.com/repos/$REPO/releases"
DOWNLOAD="https://github.com/$REPO/releases/download"
RAW_URL="https://raw.githubusercontent.com/$REPO/main/install.sh"

STATE_HOME="${XDG_STATE_HOME:-$HOME/.local/state}"
HELPER_SAVED="$STATE_HOME/orbitfm/install-appimage.sh"

PROG="install.sh"
MODE=install
RELEASE="${ORBITFM_RELEASE:-}"
DRY_RUN=0
TMP_DIR=""

usage() {
    cat <<EOF
Использование: curl -fsSL $RAW_URL | sh [-s -- ФЛАГИ]

Без флагов ставит последний опубликованный релиз OrbitFM: AppImage,
пункт меню с иконкой и команду orbitfm в ~/.local/bin. Root не нужен,
SHA256SUMS проверяется до установки.

  --release <версия>   поставить конкретную версию (например, 0.23.0-rc.3)
  --uninstall          удалить установленное этим скриптом
  --dry-run            скачать и проверить суммы, ничего не устанавливать
  -h, --help           показать эту справку

Переменные: ORBITFM_RELEASE (то же, что --release), ORBITFM_REPO,
XDG_BIN_HOME, XDG_DATA_HOME, XDG_STATE_HOME.
Windows и macOS ставятся установщиками MSI и DMG: см.
https://github.com/$REPO/blob/main/docs/user/en/install.md
EOF
}

say() {
    printf '%s\n' "$*"
}

warn() {
    printf '%s: %s\n' "$PROG" "$*" >&2
}

die() {
    printf '%s: %s\n' "$PROG" "$*" >&2
    exit 1
}

cleanup() {
    if [ -n "$TMP_DIR" ] && [ -d "$TMP_DIR" ]; then
        rm -rf -- "$TMP_DIR"
    fi
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
trap 'exit 129' HUP

# Единственная внешняя зависимость скачивания: curl, при его отсутствии wget.
fetch() {
    url="$1"
    out="$2"
    if command -v curl >/dev/null 2>&1; then
        curl -fsSL --proto '=https' --tlsv1.2 --connect-timeout 20 --retry 3 -o "$out" "$url"
    elif command -v wget >/dev/null 2>&1; then
        wget -q --https-only --timeout=20 --tries=3 -O "$out" "$url"
    else
        die "нужен curl или wget"
    fi
}

fetch_text() {
    if command -v curl >/dev/null 2>&1; then
        curl -fsSL --proto '=https' --tlsv1.2 --connect-timeout 20 --retry 3 "$1"
    elif command -v wget >/dev/null 2>&1; then
        wget -q --https-only --timeout=20 --tries=3 -O - "$1"
    else
        die "нужен curl или wget"
    fi
}

resolve_tag() {
    if [ -n "$RELEASE" ]; then
        case "$RELEASE" in
            v*) printf '%s' "$RELEASE" ;;
            *) printf 'v%s' "$RELEASE" ;;
        esac
        return 0
    fi

    json="$(fetch_text "$API?per_page=1")" ||
        die "не удалось получить список релизов; укажите версию: --release <версия>"
    tag="$(printf '%s\n' "$json" |
        sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' |
        head -n 1)"
    [ -n "$tag" ] || die "в ответе GitHub нет tag_name; укажите версию: --release <версия>"
    printf '%s' "$tag"
}

asset_sha256() {
    dir="$1"
    file="$2"
    awk -v f="$file" '$2 == f { print $1; exit }' "$dir/SHA256SUMS"
}

verify_asset() {
    dir="$1"
    file="$2"
    expected="$(asset_sha256 "$dir" "$file")"
    [ -n "$expected" ] || die "в SHA256SUMS нет записи для $file"
    actual="$(sha256sum "$dir/$file" | awk '{ print $1 }')"
    if [ "$actual" != "$expected" ]; then
        die "SHA256 не совпал для $file: ожидалось $expected, получено $actual"
    fi
}

# AppImage монтируется через libfuse2; в Ubuntu 24.04+ её нет по умолчанию.
# Без библиотеки помощник ставит Exec с APPIMAGE_EXTRACT_AND_RUN=1, иначе
# значок в меню молча не запускался бы.
have_fuse2() {
    if command -v ldconfig >/dev/null 2>&1 &&
        ldconfig -p 2>/dev/null | grep -q 'libfuse\.so\.2'; then
        return 0
    fi
    for dir in /usr/lib /usr/lib64 /lib /lib64 \
        /usr/lib/x86_64-linux-gnu /lib/x86_64-linux-gnu; do
        if [ -e "$dir/libfuse.so.2" ]; then
            return 0
        fi
    done
    return 1
}

while [ $# -gt 0 ]; do
    case "$1" in
        --release)
            [ $# -ge 2 ] || die "--release требует версию"
            RELEASE="$2"
            shift 2
            ;;
        --release=*)
            RELEASE="${1#--release=}"
            shift
            ;;
        --uninstall)
            MODE=uninstall
            shift
            ;;
        --dry-run)
            DRY_RUN=1
            shift
            ;;
        -h | --help)
            usage
            exit 0
            ;;
        --)
            shift
            ;;
        *)
            die "неизвестный флаг: $1 (см. --help)"
            ;;
    esac
done

if [ "$MODE" = uninstall ]; then
    [ "$DRY_RUN" -eq 0 ] || die "--dry-run не сочетается с --uninstall"
    if [ -x "$HELPER_SAVED" ]; then
        bash "$HELPER_SAVED" --uninstall
        rm -f -- "$HELPER_SAVED"
        rmdir -- "$(dirname -- "$HELPER_SAVED")" 2>/dev/null || true
    else
        die "не найдена установка, сделанная этим скриптом ($HELPER_SAVED); если ставили install-appimage.sh, запустите его с --uninstall"
    fi
    exit 0
fi

case "$(uname -s)" in
    Linux) ;;
    Darwin)
        die "macOS ставится DMG: https://github.com/$REPO/releases (см. docs/user/en/install.md)"
        ;;
    *)
        die "поддерживается Linux; Windows и macOS — через MSI и DMG из релиза"
        ;;
esac

arch="$(uname -m)"
if [ "$arch" != "x86_64" ]; then
    die "готовый AppImage собран только для x86_64 (у вас $arch); сборка из исходников — docs/user/en/install.md"
fi

TAG="$(resolve_tag)"
case "$TAG" in
    v[0-9]*.[0-9]*.[0-9]*) ;;
    *) die "неожиданный тег релиза: $TAG" ;;
esac
case "$TAG" in
    *[!A-Za-z0-9._-]*) die "в теге недопустимые символы: $TAG" ;;
esac
VERSION="${TAG#v}"
ASSET="OrbitFM-$VERSION-x86_64.AppImage"

TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/orbitfm-install.XXXXXX")"

say "OrbitFM $VERSION: скачиваю $ASSET"
fetch "$DOWNLOAD/$TAG/$ASSET" "$TMP_DIR/$ASSET" ||
    die "не удалось скачать $ASSET (релиз $TAG существует?)"
fetch "$DOWNLOAD/$TAG/SHA256SUMS" "$TMP_DIR/SHA256SUMS" ||
    die "не удалось скачать SHA256SUMS релиза $TAG"
fetch "$DOWNLOAD/$TAG/install-appimage.sh" "$TMP_DIR/install-appimage.sh" ||
    die "не удалось скачать install-appimage.sh релиза $TAG"

verify_asset "$TMP_DIR" "$ASSET"
verify_asset "$TMP_DIR" "install-appimage.sh"
say "SHA256SUMS: суммы AppImage и install-appimage.sh совпали"

if [ "$DRY_RUN" -eq 1 ]; then
    say "проверка пройдена; --dry-run ничего не устанавливает"
    exit 0
fi

install -d -m755 "$(dirname -- "$HELPER_SAVED")"
install -m755 -- "$TMP_DIR/install-appimage.sh" "$HELPER_SAVED"

if have_fuse2; then
    bash "$HELPER_SAVED" "$TMP_DIR/$ASSET"
elif grep -q -- '--extract-and-run' "$HELPER_SAVED"; then
    warn "libfuse2 не найден: значок меню будет запускать образ через APPIMAGE_EXTRACT_AND_RUN=1"
    warn "для обычного запуска поставьте libfuse2 (Debian/Ubuntu: libfuse2, Fedora: fuse-libs, Arch: fuse2)"
    bash "$HELPER_SAVED" --extract-and-run "$TMP_DIR/$ASSET"
else
    warn "libfuse2 не найден, а помощник этой версии не умеет --extract-and-run:"
    warn "поставьте libfuse2 (Debian/Ubuntu: libfuse2, Fedora: fuse-libs, Arch: fuse2) или версию 0.23.0-rc.2 и новее"
    bash "$HELPER_SAVED" "$TMP_DIR/$ASSET"
fi

cat <<EOF

OrbitFM $VERSION установлен.
  запуск:     orbitfm  или значок OrbitFM в меню приложений
  обновление: тот же curl — поставит последнюю версию
  удаление:   curl -fsSL $RAW_URL | sh -s -- --uninstall
EOF
