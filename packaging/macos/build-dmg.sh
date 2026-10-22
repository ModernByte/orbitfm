#!/usr/bin/env bash
# Собирает OrbitFM.app из release-бинарника и упаковывает его в DMG.
# Требует macOS, dylibbundler, librsvg (rsvg-convert) и уже собранный
# target/release/orbitfm.
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
command -v rsvg-convert >/dev/null || { echo "нужен rsvg-convert (librsvg)" >&2; exit 1; }

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

rm -rf "$app" "$stage"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources" "$app/Contents/Frameworks"
cp "$binary" "$app/Contents/MacOS/orbitfm"
chmod +x "$app/Contents/MacOS/orbitfm"

sed "s/__VERSION__/$version/g" packaging/macos/Info.plist.in > "$app/Contents/Info.plist"

iconset="$tmp/OrbitFM.iconset"
mkdir -p "$iconset"
rsvg-convert -w 1024 -h 1024 \
    packaging/linux/org.modernbyte.OrbitFM.svg -o "$tmp/icon-1024.png"
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
