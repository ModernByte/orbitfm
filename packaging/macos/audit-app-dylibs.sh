#!/usr/bin/env bash
# Аудит macOS-артефакта OrbitFM.app перед публикацией.
#
# Ловит «сырые» пути динамических зависимостей, которые гарантированно сломают
# запуск на чистой машине пользователя:
#   - абсолютные пути /tmp/, /Users/, /opt/homebrew/, /usr/local/ (build-machine paths);
#   - ссылки на Python.framework (цепочка libvapoursynth-script → Python.framework,
#     из-за которой dyld abort'ил до main() на машине без Python 3.13);
#   - @rpath-ссылки без реализации внутри .app;
#   - наличие libvapoursynth* в Contents/Frameworks.
#
# Используется в CI/release smoke-тесте DMG и внутри build-dmg.sh перед codesign.
# FAIL (exit 1) при любом совпадении; на чистом артефакте — PASS.
set -euo pipefail

if [[ $# -ne 1 ]]; then
    echo "usage: audit-app-dylibs.sh <OrbitFM.app>" >&2
    exit 2
fi
app=$1
contents="$app/Contents"
exe="$contents/MacOS/orbitfm"
if [[ ! -x "$exe" ]]; then
    echo "audit: нет исполняемого $exe" >&2
    exit 1
fi

fail() {
    echo "audit FAIL: $*" >&2
    exit 1
}

bins=("$exe")
while IFS= read -r dylib; do
    bins+=("$dylib")
done < <(find "$contents/Frameworks" -type f -name '*.dylib' | sort)

for bin in "${bins[@]}"; do
    bad=$(otool -L "$bin" | awk '
        NR > 1 &&
        ( /\/tmp\// || /\/Users\// || /\/opt\/homebrew\// || /\/usr\/local\// || /Python\.framework/ ) {
            print
            found = 1
        }
        END { exit found ? 1 : 0 }
    ' || true)
    if [[ -n "$bad" ]]; then
        fail "$bin: неразрешённые ссылки: $bad"
    fi
    if otool -L "$bin" | grep -qi 'vapoursynth'; then
        fail "$bin: осталась ссылка на VapourSynth"
    fi
    if [[ "$bin" == *Frameworks* ]] && [[ "$(basename "$bin")" == libvapoursynth* ]]; then
        fail "оставлен VapourSynth dylib: $bin"
    fi
    if otool -L "$bin" | grep -q '@rpath/'; then
        # @rpath-ссылка допустима только с rpath, указывающим внутрь .app (Frameworks).
        if ! otool -l "$bin" | grep -q 'path @executable_path/../Frameworks'; then
            fail "$bin: @rpath-ссылка без реализации в .app"
        fi
    fi
done

echo "audit ok: $app — чистый dylib-контур (без /tmp, /Users, Homebrew-refs, Python.framework, VapourSynth)"
