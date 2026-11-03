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
command -v clang >/dev/null || { echo "нужен clang (Apple CLT)" >&2; exit 1; }
stub="$app/Contents/Frameworks/liborbitfm-vsscript-stub.0.dylib"
stub_ref="@executable_path/../Frameworks/liborbitfm-vsscript-stub.0.dylib"
cat > "$tmp/vsscript-stub.c" <<'EOF'
void orbitfm_vsscript_stub_init(void) {}
EOF
clang -shared -fPIC -o "$stub" "$tmp/vsscript-stub.c"
install_name_tool -id "$stub_ref" "$stub"
libmpv="$app/Contents/Frameworks/libmpv.2.dylib"
vsscript_ref="@executable_path/../Frameworks/libvapoursynth-script.0.dylib"
if otool -L "$libmpv" | grep -q 'vapoursynth'; then
    install_name_tool -change "$vsscript_ref" "$stub_ref" "$libmpv"
fi
rm -f "$app/Contents/Frameworks"/libvapoursynth*.dylib
if otool -L "$libmpv" | grep -qi 'vapoursynth'; then
    echo "VapourSynth осталась в $libmpv после выгрузки" >&2
    exit 1
fi
bash packaging/macos/audit-app-dylibs.sh "$app"

find "$app/Contents/Frameworks" -type f -name '*.dylib' \
    -exec codesign --force --sign - --timestamp=none {} \;
codesign --force --sign - --timestamp=none "$app"
codesign --verify --deep --strict "$app"

mkdir -p "$stage"
ditto "$app" "$stage/OrbitFM.app"
ln -s /Applications "$stage/Applications"

dmg="$out_dir/OrbitFM-${version}-macos-${arch}.dmg"
rm -f "$dmg"
hdiutil create -volname OrbitFM -srcfolder "$stage" -fs HFS+ -format UDZO -ov "$dmg"

echo "DMG: $dmg"
