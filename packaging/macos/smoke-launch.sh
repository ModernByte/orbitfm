#!/usr/bin/env bash
# Проверка запуска macOS-артефакта: dyld обязан разрешить весь граф зависимостей.
#
# audit-app-dylibs.sh сверяет только load-команды, а Mach-O с двухуровневым пространством
# имён требует ещё и наличия связанных символов: 0.20.0-rc.5 прошёл аудит путей и
# abort'ился на «Symbol not found: _getVSScriptAPI». Такой дефект виден только фактическим
# запуском, причём dyld показывает его до main() — достаточно `--version`, который печатает
# версию и выходит, не поднимая окно, IPC и libmpv.
#
# FAIL (exit 1): процесс не запустился, упал по сигналу либо вывел версию, не совпадающую
# с CFBundleShortVersionString.
set -euo pipefail

if [[ $# -ne 1 ]]; then
    echo "usage: smoke-launch.sh <OrbitFM.app>" >&2
    exit 2
fi
app=$1
exe="$app/Contents/MacOS/orbitfm"
if [[ ! -x "$exe" ]]; then
    echo "smoke: нет исполняемого $exe" >&2
    exit 1
fi

tmp=$(mktemp)
trap 'rm -f "$tmp"' EXIT

want=$(plutil -extract CFBundleShortVersionString raw -o - "$app/Contents/Info.plist")
if [[ -z "$want" ]]; then
    echo "smoke: пустой CFBundleShortVersionString в $app/Contents/Info.plist" >&2
    exit 1
fi

out=$(LC_ALL=C "$exe" --version 2>"$tmp") && code=0 || code=$?
if [[ $code -ne 0 ]]; then
    reason="код $code"
    # 128+N — убит сигналом; SIGABRT от dyld именно так и выглядит.
    if [[ $code -gt 128 ]]; then
        reason="$reason, сигнал $((code - 128))"
    fi
    echo "smoke FAIL: $exe --version не запустился ($reason)" >&2
    cat "$tmp" >&2
    exit 1
fi
if [[ $out != "orbitfm $want" ]]; then
    echo "smoke FAIL: $exe --version вывел '$out', ожидаем 'orbitfm $want'" >&2
    cat "$tmp" >&2
    exit 1
fi

echo "smoke ok: $app запускается, --version вывел '$out' — dyld разрешил зависимости"
