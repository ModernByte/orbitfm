#!/usr/bin/env bash
# Собирает OrbitFM.app из release-бинарника и упаковывает его в DMG.
# Требует macOS, dylibbundler и уже собранный target/release/orbitfm.
# Иконка берётся готовым PNG (packaging/macos/OrbitFM-1024.png), чтобы не
# тащить librsvg: под Intel у Homebrew нет готовых бутылок, и rsvg пришлось бы
# собирать из исходников.
set -euo pipefail

root=$(cd "$(dirname "$0")/../.." && pwd)
cd "$root"

version=${VERSION:-$(python3 -c 'import tomllib; print(tomllib.load(open("Cargo.toml", "rb"))["package"]["version"])')}
arch=$(uname -m)
binary=target/release/orbitfm
out_dir=dist
app="$out_dir/OrbitFM.app"
stage="$out_dir/dmg"

[[ -f $binary ]] || { echo "нет бинарника $binary: сначала cargo build --release" >&2; exit 1; }
command -v dylibbundler >/dev/null || { echo "нужен dylibbundler" >&2; exit 1; }
icon=packaging/macos/OrbitFM-1024.png
[[ -f $icon ]] || { echo "нет иконки $icon" >&2; exit 1; }

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

rm -rf "$app" "$stage"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources" "$app/Contents/Frameworks"
cp "$binary" "$app/Contents/MacOS/orbitfm"
chmod +x "$app/Contents/MacOS/orbitfm"

sed "s/__VERSION__/$version/g" packaging/macos/Info.plist.in > "$app/Contents/Info.plist"

iconset="$tmp/OrbitFM.iconset"
mkdir -p "$iconset"
cp "$icon" "$tmp/icon-1024.png"
for size in 16 32 128 256 512; do
    sips -z "$size" "$size" "$tmp/icon-1024.png" \
        --out "$iconset/icon_${size}x${size}.png" >/dev/null
    double=$((size * 2))
    sips -z "$double" "$double" "$tmp/icon-1024.png" \
        --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$iconset" -o "$app/Contents/Resources/OrbitFM.icns"

dylibbundler -b \
    -x "$app/Contents/MacOS/orbitfm" \
    -d "$app/Contents/Frameworks" \
    -p "@executable_path/../Frameworks" \
    -of -od

# VapourSynth: libmpv из бутылок Homebrew собирается с VapourSynth (meson feature
# `vapoursynth`), и он жёстко тянет libvapoursynth-script → Python.framework. OrbitFM
# (только аудио-радио; mpv `load-scripts=no`) VapourSynth не использует, поэтому ссылку
# libmpv → VapourSynth выгружаем в локальную no-op-заглушку и убираем реальный
# libvapoursynth* из бандла. Иначе dyld abort'ил до main() на машине без Python 3.13
# («Library not loaded: /tmp/*/Python.framework/Versions/3.13/Python»).
#
# Заглушка обязана экспортировать ровно те имена, которые libmpv импортирует у
# VapourSynth: Mach-O связывает импорт с конкретной библиотекой (two-level namespace),
# поэтому «рядом лежит какой-то dylib» недостаточно. В 0.20.0-rc.5 заглушка экспортировала
# выдуманную orbitfm_vsscript_stub_init, libmpv продолжал просить _getVSScriptAPI, и dyld
# оборвал запуск заново — уже с «Namespace DYLD, Code 4 Symbol missing». Имена берутся из
# самих бинарников (nm), а не из догадок; сборка падает, если заглушка их не покрывает.
command -v clang >/dev/null || { echo "нужен clang (Apple CLT)" >&2; exit 1; }
stub="$app/Contents/Frameworks/liborbitfm-vsscript-stub.0.dylib"
stub_ref="@executable_path/../Frameworks/liborbitfm-vsscript-stub.0.dylib"
libmpv="$app/Contents/Frameworks/libmpv.2.dylib"
vsscript_libs=()
if otool -L "$libmpv" | grep -qi 'vapoursynth'; then
    while IFS= read -r vs; do
        vsscript_libs+=("$vs")
    done < <(find "$app/Contents/Frameworks" -type f -name 'libvapoursynth*' | sort)
    if [[ ${#vsscript_libs[@]} -eq 0 ]]; then
        echo "libmpv ссылается на VapourSynth, но libvapoursynth* в бандле нет: выводить имена не из чего" >&2
        exit 1
    fi
fi

# Экспорт Mach-O как пары «имя тип». Тип читаем из колонки nm, а не по ключу -U:
# реализации nm трактуют -U по-разному, а нам важны и имя, и KIND символа. Библиотека
# без симтаба (strip) просто не добавляет имён — launch-гейт ниже всё равно не даст
# выложить бандл, в котором dyld что-то не связал.
exports_of() {
    nm -g "$1" 2>/dev/null | awk 'NF >= 3 && $2 !~ /^[Uu]/ { print $NF, $2 }' || true
}

if [[ ${#vsscript_libs[@]} -gt 0 ]]; then
    # Импорты всех Mach-O бандла, которые до выгрузки экспортировали удаляемые
    # libvapoursynth*. Двухуровневое пространство имён привязывает импорт к конкретной
    # библиотеке, поэтому заглушка обязана закрыть весь набор целиком — даже если то же
    # имя определено где-то ещё, dyld будет искать его именно в ней.
    imports=$(
        for bin in "$app/Contents/MacOS/orbitfm" "$app"/Contents/Frameworks/*.dylib; do
            nm -u "$bin" 2>/dev/null || true
        done | awk 'NF >= 1 { print $NF }' | sed '/^$/d' | sort -u
    )
    vsscript_symbols=$(
        for vs in "${vsscript_libs[@]}"; do exports_of "$vs"; done | sort -u | sed '/^$/d'
    )
    needed=$(
        comm -12 <(awk '{ print $1 }' <<<"$vsscript_symbols" | sort -u) \
                 <(printf '%s\n' "$imports") |
            sed '/^$/d'
    )

    stub_c="$tmp/vsscript-stub.c"
    : > "$stub_c"
    if [[ -z $needed ]]; then
        # Ни одного импорта: выгружать нечего, но dylib с пустым __SYMTAB clang не
        # соберёт — оставляем явный placeholder.
        echo "VapourSynth: бандл не импортирует ни одного символа — заглушка на placeholder"
        printf 'void orbitfm_vsscript_stub_placeholder(void) {}\n' > "$stub_c"
    fi
    while IFS= read -r symbol; do
        [[ -n $symbol ]] || continue
        name=${symbol#_}
        if [[ $symbol != _* || ! $name =~ ^[A-Za-z_][A-Za-z0-9_]*$ ]]; then
            echo "VapourSynth: символ '$symbol' нельзя определить из C — это не идентификатор" >&2
            exit 1
        fi
        kind=$(awk -v s="$symbol" '$1 == s { print $2 }' <<<"$vsscript_symbols" | head -n 1)
        if [[ $kind != T ]]; then
            echo "VapourSynth: '$symbol' выгружается как '$kind', а заглушка умеет только функции (T) — расширять осознанно" >&2
            exit 1
        fi
        printf 'void *%s(void) { return 0; }\n' "$name" >> "$stub_c"
    done <<< "$needed"

    clang -shared -fPIC -mmacosx-version-min=13.0 -o "$stub" "$stub_c"
    install_name_tool -id "$stub_ref" "$stub"

    uncovered=$(comm -23 <(printf '%s\n' "$needed") \
        <(exports_of "$stub" | awk '{ print $1 }' | sort -u))
    if [[ -n $uncovered ]]; then
        echo "заглушка $stub не экспортирует: $(tr '\n' ' ' <<<"$uncovered")" >&2
        exit 1
    fi
    if [[ -n $needed ]]; then
        echo "VapourSynth: заглушка закрывает $(tr '\n' ' ' <<<"$needed")"
    fi

    refs=$(awk 'NR > 1 { print $1 }' <<<"$(otool -L "$libmpv")" | grep -i vapoursynth || true)
    if [[ $(grep -c . <<<"$refs") -gt 1 ]]; then
        echo "libmpv ссылается на несколько VapourSynth-библиотек: $(tr '\n' ' ' <<<"$refs")" >&2
        exit 1
    fi
    if [[ -n $refs ]]; then
        install_name_tool -change "$refs" "$stub_ref" "$libmpv"
    fi
    rm -f "$app/Contents/Frameworks"/libvapoursynth*.dylib
    if otool -L "$libmpv" | grep -qi 'vapoursynth'; then
        echo "VapourSynth осталась в $libmpv после выгрузки" >&2
        exit 1
    fi
fi
bash packaging/macos/audit-app-dylibs.sh "$app"

find "$app/Contents/Frameworks" -type f -name '*.dylib' \
    -exec codesign --force --sign - --timestamp=none {} \;
codesign --force --sign - --timestamp=none "$app"
codesign --verify --deep --strict "$app"

# Проверка разрешения зависимостей запуском: аудит путей не видит случай «библиотека
# на месте, а символа нет». Неразрешимый импорт dyld показывает до main(), поэтому
# хватает уже `--version`, чтобы отличить рабочий бандл от аборта.
bash packaging/macos/smoke-launch.sh "$app"

mkdir -p "$stage"
ditto "$app" "$stage/OrbitFM.app"
ln -s /Applications "$stage/Applications"

dmg="$out_dir/OrbitFM-${version}-macos-${arch}.dmg"
rm -f "$dmg"
hdiutil create -volname OrbitFM -srcfolder "$stage" -fs HFS+ -format UDZO -ov "$dmg"

echo "DMG: $dmg"
