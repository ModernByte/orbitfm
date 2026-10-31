#!/usr/bin/env python3
"""Собирает Plasma 6 .plasmoid из integrations/plasma6.

Архив детерминирован (фиксированная дата, порядок файлов), поэтому один и
тот же исходник даёт одинаковый SHA256. Путь к orbitfm берётся из
ORBITFM_BIN (по умолчанию «orbitfm»): виджет из архива вызывает команду из
PATH, а установщик `orbitfm integration install plasma6` подставляет
абсолютный путь сам.
"""

import os
import re
import sys
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
VERSION = re.search(
    r'^version\s*=\s*"([^"]+)"',
    (ROOT / "Cargo.toml").read_text(encoding="utf-8"),
    re.MULTILINE,
).group(1)
BINARY = os.environ.get("ORBITFM_BIN", "orbitfm")


def add(archive: zipfile.ZipFile, name: str, data: str) -> None:
    info = zipfile.ZipInfo(name, date_time=(1980, 1, 1, 0, 0, 0))
    info.compress_type = zipfile.ZIP_DEFLATED
    info.external_attr = 0o644 << 16
    archive.writestr(info, data)


def main() -> int:
    if len(sys.argv) > 2:
        print("использование: build-plasmoid.py [выходной .plasmoid]", file=sys.stderr)
        return 2
    output = (
        Path(sys.argv[1])
        if len(sys.argv) == 2
        else ROOT / "target" / "plasmoid" / f"OrbitFM-{VERSION}-plasma.plasmoid"
    )
    output.parent.mkdir(parents=True, exist_ok=True)

    metadata = (ROOT / "integrations" / "plasma6" / "metadata.json").read_text(
        encoding="utf-8"
    )
    main_qml = (ROOT / "integrations" / "plasma6" / "main.qml").read_text(
        encoding="utf-8"
    )
    metadata = metadata.replace("@VERSION@", VERSION)
    main_qml = main_qml.replace("@ORBITFM@", BINARY)
    if "@VERSION@" in metadata or "@ORBITFM@" in main_qml:
        print("не подставлены все плейсхолдеры", file=sys.stderr)
        return 1

    with zipfile.ZipFile(output, "w") as archive:
        add(archive, "metadata.json", metadata)
        add(archive, "contents/ui/main.qml", main_qml)

    print(output)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
