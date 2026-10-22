#!/usr/bin/env bash
# Собирает каталог дистрибутива: orbitfm.exe, libmpv-2.dll и транзитивные DLL.
# Запускается в оболочке MSYS2 UCRT64 после cargo build --release.
set -euo pipefail

root=$(cd "$(dirname "$0")/../.." && pwd)
cd "$root"

stage=${1:-dist/OrbitFM}
binary=${2:-target/release/orbitfm.exe}
dll_root=/ucrt64/bin

[[ -f $binary ]] || { echo "нет бинарника: $binary" >&2; exit 1; }
[[ -f $dll_root/libmpv-2.dll ]] || { echo "нет MSYS2 libmpv в $dll_root" >&2; exit 1; }

rm -rf "$stage"
mkdir -p "$stage"
cp "$binary" "$stage/orbitfm.exe"

declare -A queued=([libmpv-2.dll]=1)
queue=("$dll_root/libmpv-2.dll")
while ((${#queue[@]} > 0)); do
    dll=${queue[0]}
    queue=("${queue[@]:1}")
    cp -f "$dll" "$stage/"
    while IFS= read -r dep; do
        [[ -z $dep ]] && continue
        candidate="$dll_root/$dep"
        if [[ -f $candidate && -z ${queued[$dep]:-} ]]; then
            queued[$dep]=1
            queue+=("$candidate")
        fi
    done < <(objdump -p "$dll" | awk '/DLL Name:/ {print $3}')
done

echo "Подготовлено DLL: $(find "$stage" -maxdepth 1 -name '*.dll' | wc -l), каталог: $stage"
