#!/usr/bin/env bash
set -euo pipefail

# Интеграция скачанного AppImage в рабочий стол без root (ADR-0011).
# Копирует образ в XDG_BIN_HOME, ставит desktop entry с абсолютным Exec,
# иконки и metainfo в XDG_DATA_HOME, обновляет кэши и умеет --uninstall.
# Desktop-файл правится построчно по точному префиксу ключа, без регулярных
# выражений; остальные строки переносятся байт в байт.

PROG="$(basename -- "${BASH_SOURCE[0]}")"
SCRIPT_PATH="$(readlink -f -- "${BASH_SOURCE[0]}" 2>/dev/null || printf '%s' "${BASH_SOURCE[0]}")"

APP_ID="org.modernbyte.OrbitFM"
BIN_HOME="${XDG_BIN_HOME:-$HOME/.local/bin}"
DATA_HOME="${XDG_DATA_HOME:-$HOME/.local/share}"
STATE_HOME="${XDG_STATE_HOME:-$HOME/.local/state}"
DEST="$BIN_HOME/orbitfm"
RECEIPT="$STATE_HOME/orbitfm/appimage-install.list"

TMP_DIR=""
STAGED=""

cleanup() {
    if [[ -n "$TMP_DIR" && -d "$TMP_DIR" ]]; then
        rm -rf -- "$TMP_DIR"
    fi
    if [[ -n "$STAGED" && -f "$STAGED" ]]; then
        rm -f -- "$STAGED"
    fi
}
trap cleanup EXIT

usage() {
    cat <<EOF
Использование: bash $PROG [--uninstall] [--help] <OrbitFM-*.AppImage>

Без флагов ставит AppImage в $DEST, добавляет пункт меню
«OrbitFM», иконки и metainfo в $DATA_HOME, затем обновляет кэши.
Права root не нужны; исходный файл AppImage не изменяется.

  --uninstall  удалить только те файлы, которые установил этот скрипт
  --help       показать эту справку
EOF
}

fail() {
    echo "$PROG: $*" >&2
    exit 1
}

# Значение Exec по спецификации Desktop Entry: путь с пробелами и
# зарезервированными символами заключается в двойные кавычки.
quote_exec() {
    local value="$1"
    case "$value" in
        *[[:space:]\"\'\\\`\$\<\>\|\&\;\*\?\#]*)
            value=${value//\\/\\\\}
            value=${value//\"/\\\"}
            value=${value//\`/\\\`}
            value=${value//\$/\\\$}
            printf '"%s"' "$value"
            ;;
        *)
            printf '%s' "$value"
            ;;
    esac
}

# Заменяет программу в Exec/TryExec на абсолютный путь, сохраняя аргументы
# и все прочие строки. Возвращает ошибку, если строки Exec нет.
rewrite_desktop() {
    local source="$1"
    local target="$2"
    local line args program rest exec_path saw_exec=0
    exec_path="$(quote_exec "$DEST")"
    while IFS= read -r line || [[ -n "$line" ]]; do
        case "$line" in
            Exec=*)
                args="${line#Exec=}"
                args="${args#"${args%%[![:space:]]*}"}"
                program="${args%%[[:space:]]*}"
                rest="${args#"$program"}"
                printf 'Exec=%s%s\n' "$exec_path" "$rest"
                saw_exec=1
                ;;
            TryExec=*)
                printf 'TryExec=%s\n' "$DEST"
                ;;
            *)
                printf '%s\n' "$line"
                ;;
        esac
    done <"$source" >"$target"
    [[ "$saw_exec" -eq 1 ]]
}

refresh_caches() {
    local data_home="$1"
    if command -v update-desktop-database >/dev/null 2>&1; then
        update-desktop-database "$data_home/applications" >/dev/null 2>&1 || true
    fi
    if command -v gtk-update-icon-cache >/dev/null 2>&1; then
        gtk-update-icon-cache --force --ignore-theme-index --quiet \
            "$data_home/icons/hicolor" >/dev/null 2>&1 || true
    fi
}

install_appimage() {
    local source="$1"
    [[ -f "$source" ]] || fail "файл не найден: $source"

    local magic
    magic="$(head -c 11 -- "$source" | tail -c 3 | od -An -tx1 | tr -d ' \n')"
    case "$magic" in
        414901 | 414902) ;;
        *) fail "это не AppImage (нет маркера AI после ELF-заголовка): $source" ;;
    esac

    install -d -m755 "$BIN_HOME" "$DATA_HOME/applications" "$DATA_HOME/metainfo" \
        "$(dirname -- "$RECEIPT")"

    local source_real dest_real
    source_real="$(readlink -f -- "$source")"
    dest_real="$(readlink -f -- "$DEST" 2>/dev/null || true)"
    if [[ "$source_real" != "$dest_real" ]]; then
        if [[ -e "$DEST" || -L "$DEST" ]]; then
            echo "заменяю существующий $DEST" >&2
        fi
        # Копия ставится атомарно: rename не падает, если старый AppImage
        # в этот момент запущен.
        STAGED="$BIN_HOME/.orbitfm.$$.tmp"
        install -m755 -- "$source" "$STAGED"
        mv -f -- "$STAGED" "$DEST"
        STAGED=""
    else
        chmod 755 -- "$DEST"
    fi

    TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/orbitfm-appimage.XXXXXX")"
    (cd "$TMP_DIR" && "$DEST" --appimage-extract >/dev/null)
    local root="$TMP_DIR/squashfs-root"
    [[ -d "$root" ]] || fail "AppImage не распаковался: $DEST"

    local desktop_src="" candidate
    for candidate in "$root/usr/share/applications/$APP_ID.desktop" \
        "$root"/usr/share/applications/*.desktop; do
        if [[ -f "$candidate" ]]; then
            desktop_src="$candidate"
            break
        fi
    done
    [[ -n "$desktop_src" ]] || fail "в AppImage нет desktop entry"

    local desktop_name desktop_tmp desktop_dest
    desktop_name="$(basename -- "$desktop_src")"
    desktop_tmp="$TMP_DIR/$desktop_name"
    desktop_dest="$DATA_HOME/applications/$desktop_name"
    rewrite_desktop "$desktop_src" "$desktop_tmp" ||
        fail "в desktop entry нет строки Exec"
    if command -v desktop-file-validate >/dev/null 2>&1; then
        desktop-file-validate "$desktop_tmp" ||
            fail "desktop entry не прошёл desktop-file-validate"
    fi

    # Список установленных файлов пишется последним: --uninstall доверяет
    # только путям внутри сохранённых BIN_HOME/DATA_HOME.
    local manifest="$TMP_DIR/install.list"
    {
        printf '# OrbitFM AppImage install receipt v1\n'
        printf 'BIN_HOME=%s\n' "$BIN_HOME"
        printf 'DATA_HOME=%s\n' "$DATA_HOME"
        printf '%s\n' "$DEST"
    } >"$manifest"

    install -Dm644 -- "$desktop_tmp" "$desktop_dest"
    printf '%s\n' "$desktop_dest" >>"$manifest"

    local icon_root="$root/usr/share/icons"
    if [[ -d "$icon_root" ]]; then
        local icon rel target
        while IFS= read -r -d '' icon; do
            rel="${icon#"$icon_root/"}"
            target="$DATA_HOME/icons/$rel"
            install -Dm644 -- "$icon" "$target"
            printf '%s\n' "$target" >>"$manifest"
        done < <(find "$icon_root" -type f \
            \( -name '*.png' -o -name '*.svg' -o -name '*.xpm' \) -print0)
    fi

    local meta_root="$root/usr/share/metainfo"
    if [[ -d "$meta_root" ]]; then
        local meta name target
        while IFS= read -r -d '' meta; do
            name="$(basename -- "$meta")"
            target="$DATA_HOME/metainfo/$name"
            install -Dm644 -- "$meta" "$target"
            printf '%s\n' "$target" >>"$manifest"
        done < <(find "$meta_root" -maxdepth 1 -type f \
            \( -name '*.metainfo.xml' -o -name '*.appdata.xml' \) -print0)
    fi

    install -Dm644 -- "$manifest" "$RECEIPT"
    refresh_caches "$DATA_HOME"

    echo "OrbitFM установлен:"
    echo "  приложение: $DEST"
    echo "  меню:       $desktop_dest"
    echo "  удаление:   bash \"$SCRIPT_PATH\" --uninstall"
    case ":$PATH:" in
        *":$BIN_HOME:"*) ;;
        *) echo "внимание: $BIN_HOME отсутствует в PATH — добавьте каталог в профиль оболочки" >&2 ;;
    esac
}

uninstall_appimage() {
    [[ -f "$RECEIPT" ]] || fail "нет списка установки: $RECEIPT (нечего удалять)"

    local saved_bin="$BIN_HOME" saved_data="$DATA_HOME"
    local line removed=0
    while IFS= read -r line || [[ -n "$line" ]]; do
        case "$line" in
            '' | '#'*) continue ;;
            BIN_HOME=*) saved_bin="${line#BIN_HOME=}" && continue ;;
            DATA_HOME=*) saved_data="${line#DATA_HOME=}" && continue ;;
        esac
        case "$line" in
            "$saved_data"/* | "$saved_bin"/*) ;;
            *)
                echo "$PROG: пропускаю путь вне каталогов установки: $line" >&2
                continue
                ;;
        esac
        rm -f -- "$line"
        removed=$((removed + 1))
    done <"$RECEIPT"

    rm -f -- "$RECEIPT"
    rmdir --ignore-fail-on-non-empty -- "$(dirname -- "$RECEIPT")" 2>/dev/null || true
    refresh_caches "$saved_data"

    echo "удалено файлов: $removed"
    echo "исходный AppImage не тронут"
}

MODE=install
SOURCE=""
while [[ $# -gt 0 ]]; do
    case "$1" in
        --uninstall) MODE=uninstall ;;
        --help | -h)
            usage
            exit 0
            ;;
        --*) fail "неизвестный флаг: $1 (см. --help)" ;;
        *)
            [[ -z "$SOURCE" ]] || fail "лишний аргумент: $1"
            SOURCE="$1"
            ;;
    esac
    shift
done

[[ "$BIN_HOME" == /* ]] || fail "XDG_BIN_HOME должен быть абсолютным: $BIN_HOME"
[[ "$DATA_HOME" == /* ]] || fail "XDG_DATA_HOME должен быть абсолютным: $DATA_HOME"
[[ "$STATE_HOME" == /* ]] || fail "XDG_STATE_HOME должен быть абсолютным: $STATE_HOME"

if [[ "$MODE" == uninstall ]]; then
    [[ -z "$SOURCE" ]] || fail "--uninstall не принимает путь к AppImage"
    uninstall_appimage
else
    [[ -n "$SOURCE" ]] || {
        usage >&2
        exit 2
    }
    install_appimage "$SOURCE"
fi
