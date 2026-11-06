#!/usr/bin/env bash
# Проверяет, что каждый Mach-O внутри OrbitFM.app собран ровно с объявленной аркой.
#
# Арка релиза — объявленное значение (`build-dmg.sh --arch`), а не производная от
# машины сборки: `uname -m` описывал хост, поэтому arm64-раннер мог выложить
# x86_64-артефакт под arm64-именем, и ничто этого не ловило (ADR-0052 § 4).
#
# FAIL в трёх случаях:
#   - у Mach-O арка не совпала с объявленной;
#   - бинарник fat (`lipo -archs` вернёт две арки) — universal-бандл не публикуется;
#   - внутри .app не найдено ни одного Mach-O: значит путь в бандле изменился и
#     проверка просто не состоялась (молчаливый PASS хуже FAIL).
#
# Вызывается из build-dmg.sh после сборки бандла и из smoke-теста DMG в CI, где на
# него же проверяют негативный fixture с объявленной аркой, которой в бандле нет.
set -euo pipefail

usage() {
    echo "использование: check-app-arch.sh <OrbitFM.app> --arch x86_64|arm64" >&2
}

app=
arch=
while [[ $# -gt 0 ]]; do
    case $1 in
        --arch)
            if [[ $# -lt 2 ]]; then
                echo "у --arch нет значения" >&2
                usage
                exit 2
            fi
            arch=$2
            shift 2
            ;;
        -h | --help)
            usage
            exit 0
            ;;
        --*)
            echo "неизвестный флаг: $1" >&2
            usage
            exit 2
            ;;
        *)
            if [[ -n $app ]]; then
                echo "лишний аргумент: $1" >&2
                usage
                exit 2
            fi
            app=$1
            shift
            ;;
    esac
done

if [[ -z $app ]]; then
    echo "не указан .app" >&2
    usage
    exit 2
fi
case $arch in
    x86_64 | arm64) ;;
    '')
        echo "не указан --arch" >&2
        usage
        exit 2
        ;;
    *)
        echo "неизвестная архитектура '$arch': ожидается x86_64 или arm64" >&2
        exit 2
        ;;
esac
[[ -d "$app/Contents/MacOS" ]] || { echo "нет $app/Contents/MacOS" >&2; exit 1; }

checked=0
for dir in "$app/Contents/MacOS" "$app/Contents/Frameworks"; do
    [[ -d $dir ]] || continue
    while IFS= read -r bin; do
        # Проверяем именно Mach-O: в Frameworks встречаются ресурсы без кода,
        # и `lipo` на них падает по другой причине.
        [[ $(file -b "$bin") == *Mach-O* ]] || continue
        actual=$(lipo -archs "$bin")
        if [[ $actual != $arch ]]; then
            echo "arch FAIL: $actual вместо объявленной $arch: $bin" >&2
            exit 1
        fi
        checked=$((checked + 1))
    done < <(find "$dir" -type f | sort)
done

if [[ $checked -lt 1 ]]; then
    echo "arch FAIL: в $app нет ни одного Mach-O — проверка арки не состоялась" >&2
    exit 1
fi
echo "arch ok: $checked Mach-O, все $arch"
