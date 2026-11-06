#!/usr/bin/env python3
"""Проверяет, что .app действительно запускается на заявленной macOS и архитектуре.

`LSMinimumSystemVersion` в Info.plist — только обещание оболочки: dyld смотрит
на `LC_BUILD_VERSION`/`LC_VERSION_MIN_MACOSX` каждого бинарника и каждой
библиотеки в бандле. Если Homebrew положил dylib, собранный под macOS 26,
приложение на macOS 13–15 не стартует, хотя Info.plist разрешает запуск.
Скрипт ловит это до публикации релиза.

Предел по умолчанию читается из того же Info.plist, поэтому сверка сравнивает
обещанный пол с фактическим, а не с числом, продублированным в CI.
"""

from __future__ import annotations

import argparse
import plistlib
import struct
import sys
from pathlib import Path

CPU_TYPE_X86_64 = 0x01000007
CPU_TYPE_ARM64 = 0x0100000C
ARCH_CPU = {"x86_64": CPU_TYPE_X86_64, "arm64": CPU_TYPE_ARM64}

MACHO_MAGIC = (0xFEEDFACE, 0xFEEDFACF)
FAT_MAGIC = (0xCAFEBABE, 0xCAFEBABF)

LC_VERSION_MIN_MACOSX = 0x24
LC_BUILD_VERSION = 0x32
PLATFORM_MACOS = 1


def decode_version(value: int) -> tuple[int, int, int]:
    return value >> 16, (value >> 8) & 0xFF, value & 0xFF


def version_string(version: tuple[int, int, int]) -> str:
    return ".".join(str(part) for part in version)


def parse_version(text: str) -> tuple[int, int, int]:
    parts = tuple(int(part) for part in text.split("."))
    if not 1 <= len(parts) <= 3 or any(part < 0 for part in parts):
        raise SystemExit(f"ошибка: версия macOS '{text}': ожидается major[.minor[.patch]]")
    return (*parts, *([0] * (3 - len(parts))))


def promised_floor(app: Path) -> tuple[int, int, int]:
    """Пол, который бандл сам себе обещает, — из LSMinimumSystemVersion Info.plist.

    Число 13.0 жило в трёх местах (plist, `--max` в CI, дефолт этого скрипта) и
    разъезжалось молча; теперь оно объявлено один раз, а сверка односторонняя:
    если plist обещает 13.0, а dylib требует 14 — сборка падает (ADR-0052 § 1).
    """
    plist = app / "Contents" / "Info.plist"
    try:
        with plist.open("rb") as handle:
            value = plistlib.load(handle).get("LSMinimumSystemVersion")
    except FileNotFoundError:
        raise SystemExit(f"ошибка: нет {plist} — сверять пол не с чем") from None
    except plistlib.InvalidFileException:
        raise SystemExit(f"ошибка: {plist} не разбирается как plist") from None
    if not value:
        raise SystemExit(f"ошибка: в {plist} нет LSMinimumSystemVersion")
    return parse_version(str(value))


def thin_info(data: bytes) -> tuple[int, tuple[int, int, int]] | None:
    if len(data) < 32:
        return None
    magic = struct.unpack_from("<I", data, 0)[0]
    if magic not in MACHO_MAGIC:
        return None
    is_64 = magic == 0xFEEDFACF
    cputype = struct.unpack_from("<I", data, 4)[0]
    ncmds = struct.unpack_from("<I", data, 16)[0]
    offset = 32 if is_64 else 28
    minimum = None
    for _ in range(ncmds):
        if offset + 8 > len(data):
            break
        cmd, size = struct.unpack_from("<II", data, offset)
        if size < 8:
            break
        if cmd == LC_BUILD_VERSION and offset + 16 <= len(data):
            platform = struct.unpack_from("<I", data, offset + 8)[0]
            value = struct.unpack_from("<I", data, offset + 12)[0]
            if platform == PLATFORM_MACOS:
                minimum = decode_version(value)
        elif cmd == LC_VERSION_MIN_MACOSX and offset + 12 <= len(data):
            value = struct.unpack_from("<I", data, offset + 8)[0]
            minimum = decode_version(value)
        offset += size
    if minimum is None:
        return None
    return cputype, minimum


def slices(data: bytes) -> list[bytes]:
    if len(data) < 8:
        return []
    magic = struct.unpack_from(">I", data, 0)[0]
    if magic not in FAT_MAGIC:
        return [data]
    count = struct.unpack_from(">I", data, 4)[0]
    result = []
    for index in range(count):
        base = 8 + index * 20
        if base + 20 > len(data):
            break
        offset, size = struct.unpack_from(">II", data, base + 8)
        result.append(data[offset : offset + size])
    return result


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("app", type=Path, help="путь к OrbitFM.app")
    parser.add_argument("--arch", required=True, choices=sorted(ARCH_CPU))
    parser.add_argument(
        "--max",
        help="явно задать максимально допустимую минимальную macOS; без флага предел "
        "берётся из LSMinimumSystemVersion собранного Info.plist",
    )
    args = parser.parse_args()

    contents = args.app / "Contents"
    if args.max:
        limit, floor_source = parse_version(args.max), "--max"
    else:
        limit, floor_source = promised_floor(args.app), "Info.plist"
    expected_cpu = ARCH_CPU[args.arch]

    binaries = [
        path
        for path in [contents / "MacOS" / "orbitfm", *(contents / "Frameworks").rglob("*.dylib")]
        if path.is_file()
    ]
    if not binaries:
        print(f"ошибка: в {args.app} нет бинарников", file=sys.stderr)
        raise SystemExit(1)

    problems: list[str] = []
    checked = 0
    highest = (0, 0, 0)
    highest_path = ""
    for path in sorted(binaries):
        data = path.read_bytes()
        infos = [info for info in map(thin_info, slices(data)) if info]
        if not infos:
            problems.append(f"{path}: не Mach-O")
            continue
        for cputype, minimum in infos:
            checked += 1
            if cputype != expected_cpu:
                problems.append(
                    f"{path}: архитектура 0x{cputype:08x}, ожидалась 0x{expected_cpu:08x}"
                )
            if minimum > highest:
                highest = minimum
                highest_path = str(path)
            if minimum > limit:
                problems.append(
                    f"{path}: требует macOS {version_string(minimum)} "
                    f"(предел {version_string(limit)})"
                )

    relative_highest = highest_path.removeprefix(str(contents.parent) + "/")
    print(
        f"проверено бинарников: {len(binaries)} ({checked} срезов); "
        f"предел {version_string(limit)} ({floor_source}); "
        f"самая новая минимальная macOS: {version_string(highest)} "
        f"({relative_highest or '—'})"
    )
    if problems:
        for problem in problems:
            print(f"ошибка: {problem}", file=sys.stderr)
        raise SystemExit(1)
    print("совместимость подтверждена")


if __name__ == "__main__":
    main()
