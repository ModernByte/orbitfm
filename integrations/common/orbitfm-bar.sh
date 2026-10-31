#!/bin/sh
# OrbitFM bar bridge (ADR-0037).
# Установлен командой `orbitfm integration install <цель>`; путь к бинарнику
# подставлен установщиком. Режим передаётся первым аргументом:
# text | waybar | polybar | i3blocks | swaybar | i3status-rust | genmon.
#
# Скрипт только читает `orbitfm status` и отправляет команды; пользовательские
# файлы и конфиги оболочек он не трогает.

ORBITFM=@ORBITFM@
MODE=${1:-text}
INTERVAL=${ORBITFM_BAR_INTERVAL:-2}

state=
name=
volume=0.00
window=hidden
window_mode=focus
nothing=

refresh() {
    raw=$("$ORBITFM" status 2>/dev/null) || {
        state=
        name=
        volume=0.00
        window=hidden
        window_mode=focus
        nothing=
        load_nothing
        return 1
    }

    state=
    name=
    volume=0.00
    window=hidden
    window_mode=focus

    old_ifs=$IFS
    IFS='
'
    set -f
    for line in $raw; do
        case $line in
            "status "*) state=${line#status } ;;
            "name "*) name=${line#name } ;;
            "volume "*) volume=${line#volume } ;;
            "window "*) window=${line#window } ;;
            "mode "*) window_mode=${line#mode } ;;
        esac
    done
    set +f
    IFS=$old_ifs

    if [ -z "$name" ]; then
        nothing=
        load_nothing
    fi

    [ -n "$state" ]
}

json_escape() {
    printf '%s' "$1" | sed -e 's/\\/\\\\/g' -e 's/"/\\"/g'
}

xml_escape() {
    printf '%s' "$1" | sed -e 's/&/\&amp;/g' -e 's/</\&lt;/g' -e 's/>/\&gt;/g'
}

load_nothing() {
    [ -n "$nothing" ] && return 0
    json=$("$ORBITFM" status --json 2>/dev/null) || {
        nothing=OrbitFM
        return 0
    }
    nothing=$(printf '%s' "$json" | sed -n 's/.*"nothing_playing":"\([^"]*\)".*/\1/p')
    nothing=$(printf '%s' "$nothing" | sed -e 's/\\"/"/g' -e 's/\\\\/\\/g')
    [ -n "$nothing" ] || nothing=OrbitFM
}

state_glyph() {
    case $state in
        playing) printf '▶' ;;
        paused) printf '⏸' ;;
        buffering | reconnecting) printf '…' ;;
        error) printf '⚠' ;;
        *) printf '◍' ;;
    esac
}

state_color() {
    case $state in
        playing) printf '#68d391' ;;
        paused) printf '#ffbe5c' ;;
        buffering | reconnecting) printf '#b294ff' ;;
        error) printf '#ff8aa5' ;;
        *) printf '#969eb0' ;;
    esac
}

bar_text() {
    if [ -n "$name" ]; then
        printf '%s %s' "$(state_glyph)" "$name"
    else
        printf '%s %s' "$(state_glyph)" "${nothing:-OrbitFM}"
    fi
}

volume_percent() {
    whole=${volume%%.*}
    fraction=${volume#*.}
    [ "$fraction" = "$volume" ] && fraction=0
    case $fraction in
        [0-9][0-9]) fraction=${fraction#0} ;;
        [0-9]) ;;
        *) fraction=0 ;;
    esac
    [ -n "$fraction" ] || fraction=0
    if [ "$whole" = 1 ]; then
        printf '100'
    else
        printf '%s' "$fraction"
    fi
}

state_class() {
    case $state in
        playing) printf 'playing' ;;
        paused) printf 'paused' ;;
        buffering | reconnecting) printf 'loading' ;;
        error) printf 'error' ;;
        *) printf 'stopped' ;;
    esac
}

tooltip_text() {
    if [ -n "$name" ]; then
        printf 'OrbitFM\n%s\nVolume · %s%%' "$name" "$(volume_percent)"
    else
        printf 'OrbitFM\n%s' "${nothing:-OrbitFM}"
    fi
}

tooltip_json() {
    if [ -n "$name" ]; then
        printf 'OrbitFM\\n%s\\nVolume · %s%%' \
            "$(json_escape "$name")" "$(volume_percent)"
    else
        printf 'OrbitFM\\n%s' "$(json_escape "${nothing:-OrbitFM}")"
    fi
}

polybar_escape() {
    printf '%s' "$1" | sed 's/%/%%/g'
}

mode_plain() {
    refresh || true
    bar_text
    printf '\n'
}

mode_polybar() {
    refresh || true
    # Polybar трактует %{...} как управляющие последовательности: имя станции
    # из общей базы не должно менять цвета и отступы чужой панели.
    polybar_escape "$(bar_text)"
    printf '\n'
}

mode_waybar() {
    refresh || true
    printf '{"text":"%s","alt":"%s","tooltip":"%s","class":"%s"}\n' \
        "$(json_escape "$(bar_text)")" "$(state_glyph)" "$(tooltip_json)" "$(state_class)"
}

mode_i3blocks() {
    case ${BLOCK_BUTTON:-} in
        1) "$ORBITFM" playpause >/dev/null 2>&1 ;;
        2) "$ORBITFM" stop >/dev/null 2>&1 ;;
        3) "$ORBITFM" panel >/dev/null 2>&1 ;;
        4) "$ORBITFM" volume-up >/dev/null 2>&1 ;;
        5) "$ORBITFM" volume-down >/dev/null 2>&1 ;;
    esac
    refresh || true
    # i3bar по умолчанию разбирает full_text как Pango-разметку.
    xml_escape "$(bar_text)"
    printf '\n'
}

mode_i3status_rust() {
    refresh || true
    case $state in
        playing) rust_state=Good ;;
        paused | buffering | reconnecting) rust_state=Info ;;
        error) rust_state=Critical ;;
        *) rust_state=Idle ;;
    esac
    printf '{"text":"%s","state":"%s"}\n' \
        "$(json_escape "$(bar_text)")" "$rust_state"
}

mode_genmon() {
    refresh || true
    printf '<txt>%s</txt>\n' "$(xml_escape "$(bar_text)")"
    printf '<tool>%s</tool>\n' "$(xml_escape "$(tooltip_text)")"
    printf '<txtclick>%s playpause</txtclick>\n' "$(xml_escape "$ORBITFM")"
}

mode_swaybar() {
    tmpdir=$(mktemp -d "${TMPDIR:-/tmp}/orbitfm-bar.XXXXXX") || exit 1
    clicks=$tmpdir/clicks
    : >"$clicks"
    reader=

    cleanup() {
        [ -n "$reader" ] && kill "$reader" 2>/dev/null
        rm -rf "$tmpdir"
    }
    stop() {
        cleanup
        trap - 0 1 2 15
        exit 0
    }
    trap cleanup 0
    trap stop 1 2 15

    # i3bar присылает клики в stdin; фоновый читатель складывает их в файл,
    # чтобы основной цикл мог опрашивать состояние без блокировки.
    (
        while IFS= read -r event; do
            printf '%s\n' "$event" >>"$clicks"
        done
    ) &
    reader=$!

    printf '{"version":1,"click_events":true}\n[\n'

    while :; do
        if [ -s "$clicks" ]; then
            while IFS= read -r event; do
                button=$(printf '%s' "$event" |
                    sed -n 's/.*"button"[[:space:]]*:[[:space:]]*\([0-9][0-9]*\).*/\1/p')
                case $button in
                    1) "$ORBITFM" playpause >/dev/null 2>&1 ;;
                    2) "$ORBITFM" stop >/dev/null 2>&1 ;;
                    3) "$ORBITFM" panel >/dev/null 2>&1 ;;
                    4) "$ORBITFM" volume-up >/dev/null 2>&1 ;;
                    5) "$ORBITFM" volume-down >/dev/null 2>&1 ;;
                esac
            done <"$clicks"
            : >"$clicks"
        fi

        refresh || true
        printf '[{"name":"orbitfm","instance":"orbitfm","full_text":"%s","short_text":"%s","color":"%s","markup":"none"}],\n' \
            "$(json_escape "$(bar_text)")" "$(json_escape "$(bar_text)")" "$(state_color)"
        sleep "$INTERVAL"
    done
}

case $MODE in
    text) mode_plain ;;
    polybar) mode_polybar ;;
    waybar) mode_waybar ;;
    i3blocks) mode_i3blocks ;;
    swaybar) mode_swaybar ;;
    i3status-rust | i3status_rust) mode_i3status_rust ;;
    genmon) mode_genmon ;;
    *)
        printf 'orbitfm-bar: неизвестный режим: %s\n' "$MODE" >&2
        exit 2
        ;;
esac
