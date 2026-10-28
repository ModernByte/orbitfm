#!/usr/bin/env python3
"""Проверяет, что Flatpak-вендинг Cargo-зависимостей честен и не устарел.

Вендинговый список `flatpak-cargo-sources.json` порождается
`flatpak-cargo-generator` по `Cargo.lock` (ADR-0011) и платформенно
независим: в нём лежат крейты всех таргетов, включая macOS и Windows.
Именно поэтому проверка идёт по всему лок-файлу, а не по подмножеству одного
таргета — иначе 136 непроверяемых записей стали бы нормой, а рассинхрон, из-за
которого Flatpak-сборка падает на `--offline`, осталось бы незамеченным.

Инварианты:

A. Множество registry-пакетов `Cargo.lock` равно множеству вендоринг-записей —
   в обе стороны (ловит и пропущенный новый крейт, и осиротевший старый).
B. `sha256` archive-записи совпадает с `checksum` того же пакета в
   `Cargo.lock`. Для registry-пакетов Cargo пишет в лок-файл именно sha256
   тарбола, так что проверка работает офлайн и не требует скачивания.
C. У каждой archive-записи есть парная inline-запись `.cargo-checksum.json` с
   тем же дайджестом, а `url` собран из имени и версии пакета, а не из
   произвольной строки (защита от ручной правки источника).
D. Количество записей печатается числом, чтобы рост вендинга был ревью-абельным.

Выход ненулевой при любом нарушении. Зависимости: только stdlib и PyYAML.
"""

from __future__ import annotations

import json
import sys
import tomllib
from pathlib import Path

import yaml

REPO_ROOT = Path(__file__).resolve().parent.parent.parent
MANIFEST = REPO_ROOT / "packaging/linux/org.modernbyte.OrbitFM.yml"
SOURCES = REPO_ROOT / "packaging/linux/flatpak-cargo-sources.json"
LOCK = REPO_ROOT / "Cargo.lock"
VENDOR_PREFIX = "cargo/vendor/"


def registry_packages() -> dict[str, tuple[str, str, str]]:
    """`name-version` -> (name, version, sha256) для пакетов из реестра.

    Имя и версию берём из лок-файла, а не вырезаем из ключа: у версий бывает
    build-metadata (`wasi-0.11.1+wasi-snapshot-preview1`), и splitting по
    последнему дефису тогда ломается.
    """
    lock = tomllib.loads(LOCK.read_text(encoding="utf-8"))
    return {
        f"{package['name']}-{package['version']}": (
            package["name"],
            package["version"],
            package["checksum"],
        )
        for package in lock["package"]
        if package.get("source", "").startswith("registry+")
        and "checksum" in package
    }


def fail(problems: list[str], message: str) -> None:
    problems.append(message)


def main() -> int:
    problems: list[str] = []
    expected = registry_packages()
    entries = json.loads(SOURCES.read_text(encoding="utf-8"))

    archives: dict[str, dict] = {}
    checksums: dict[str, str] = {}
    for entry in entries:
        dest = entry.get("dest", "")
        if not dest.startswith(VENDOR_PREFIX):
            continue
        key = dest.removeprefix(VENDOR_PREFIX)
        if entry.get("type") == "archive":
            if key in archives:
                fail(problems, f"двойная archive-запись для {key}")
            archives[key] = entry
        elif entry.get("dest-filename") == ".cargo-checksum.json":
            if "contents" not in entry:
                fail(problems, f"пустая inline-запись для {key}")
            else:
                checksums[key] = entry["contents"]

    # A: полнота и отсутствие мусора в обе стороны.
    missing = sorted(set(expected) - set(archives))
    stale = sorted(set(archives) - set(expected))
    if missing:
        fail(problems, f"cargo-sources.json не содержит: {missing}")
    if stale:
        fail(problems, f"cargo-sources.json содержит лишнее: {stale}")

    orphans = sorted(set(checksums) - set(archives))
    if orphans:
        fail(problems, f"inline-записи без archive: {orphans}")

    for key, (name, version, digest) in sorted(expected.items()):
        archive = archives.get(key)
        if archive is None:
            continue  # уже сообщено в missing

        # B: дайджест взят из лок-файла, а не наугад.
        if archive.get("sha256") != digest:
            fail(problems, f"{key}: sha256 не совпадает с checksum из Cargo.lock")

        # C: источник выводим из имени и версии пакета.
        wanted_url = f"https://static.crates.io/crates/{name}/{name}-{version}.crate"
        if archive.get("url") != wanted_url:
            fail(problems, f"{key}: url {archive.get('url')!r}, ожидают {wanted_url!r}")
        if archive.get("archive-type") != "tar-gzip":
            fail(problems, f"{key}: archive-type {archive.get('archive-type')!r}")

        contents = checksums.get(key)
        if contents is None:
            fail(problems, f"{key}: нет inline-записи .cargo-checksum.json")
            continue
        try:
            payload = json.loads(contents)
        except json.JSONDecodeError:
            fail(problems, f"{key}: .cargo-checksum.json не разбирается как JSON")
            continue
        if payload.get("package") != digest:
            fail(problems, f"{key}: дайджест в .cargo-checksum.json не совпадает с Cargo.lock")

    # Манифест и команда: без них вендинг сам по себе бесполезен.
    manifest = yaml.safe_load(MANIFEST.read_text(encoding="utf-8"))
    for field, want in (
        ("id", "org.modernbyte.OrbitFM"),
        ("runtime", "org.freedesktop.Platform"),
        ("runtime-version", "25.08"),
        ("command", "orbitfm"),
    ):
        if manifest.get(field) != want:
            fail(problems, f"{MANIFEST.name}: {field} = {manifest.get(field)!r}, ожидают {want!r}")
    if not any(module.get("name") == "orbitfm" for module in manifest.get("modules", [])):
        fail(problems, f"{MANIFEST.name}: нет модуля orbitfm")

    if problems:
        print("Flatpak-вендинг не согласован с Cargo.lock:", file=sys.stderr)
        for problem in problems:
            print(f"  - {problem}", file=sys.stderr)
        print(
            "Починка: перегенерировать packaging/linux/flatpak-cargo-sources.json\n"
            "по актуальному Cargo.lock (flatpak-cargo-generator, ADR-0011).",
            file=sys.stderr,
        )
        return 1

    print(
        f"manifest ok; вендоринг согласован: пакетов в Cargo.lock {len(expected)}, "
        f"archive-записей {len(archives)}, inline-записей {len(checksums)}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
