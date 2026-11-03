#!/usr/bin/env python3
"""Скачивает libmpv и его dylib-зависимости из бутылок Homebrew под macOS 13.

Зачем не `brew install mpv`:

* с сентября 2026 Homebrew не собирает бутылки под Intel (Tier 3) и не
  поддерживает установку локальных бутылок через `brew install <path>`
  (стабильная ветка 7.x пока такой опции не имеет), поэтому под `macos-15-intel`
  обычный `brew install mpv` ушёл бы в многочасовую сборку из исходников;
* на Apple Silicon Homebrew кладёт libmpv под macOS 26 (раннер macos-latest
  в 2026 году — macOS 26), и опубликованный DMG начинал требовать macOS 26,
  хотя `LSMinimumSystemVersion` обещает 13.0.

Поэтому скрипт:

1. берёт последнюю версию mpv, у которой ещё есть бутылки, собранные под macOS 13
   для обеих архитектур (`0.40.0_4`), и длёт из ghcr.io доты зависимостей
   (бутылки с тегом `sh.brew.tab` — точные версии самих dylib-зависимостей);
2. распаковывает их в собственный префикс `/tmp/orbitfm/{Cellar,opt}`,
   переносит `@@HOMEBREW_PREFIX@@/opt/...` и `@@HOMEBREW_CELLAR@@/...` на реальные
   пути;
3. выводит префикс и путь к `dylibbundler`, который нужен только для упаковки
   в DMG (`build-dmg.sh`).

Сам процесс зависимостей вычисляется по LC_LOAD_DYLIB/LC_ID_DYLIB/LC_RPATH
во всех dylib'ах — это ровно та зависимая сеть, которая реально нужна
`dylibbundler` при бандлинге. Homebrew больше в конвейере не используется,
в т.ч. под Intel.

Скрипт идемпотентен: повторный запуск чистит префикс и перечитывает зависимости.

Слияние с `--ignore-dependencies` не требуется: скрипт сам строит граф и
оперативно ставит каждую бутылку из того же исходника.
"""

from __future__ import annotations

import argparse
import json
import platform
import shutil
import stat
import struct
import sys
import tarfile
import tempfile
import time
import urllib.error
import urllib.request
from pathlib import Path

# Последняя mpv с бутылками под macOS 13 для arm64 и x86_64.
MPV_TAG = "0.40.0_4"
# Инструмент упаковки: его бутылка тоже есть под macOS 13 для обеих архитектур.
TOOL_TAG = "1.0.5"

GHCR = "https://ghcr.io/v2"
USER_AGENT = "OrbitFM-build/1.0 (ModernByte)"

ARCH_TO_PLATFORM = {"arm64": "arm64", "x86_64": "amd64"}
# Порядок предпочтения: чем старше macOS в бутылке, тем шире совместимость.
OS_PREFERENCE = ("macOS 13", "macOS 14", "macOS 15")

DEFAULT_PREFIX = Path("/tmp/orbitfm")
# Длина placeholder ``@@HOMEBREW_PREFIX@@`` / ``@@HOMEBREW_CELLAR@@`` — 19 символов.
PLACEHOLDER_LEN = 19
CELLAR_SUFFIX = "/Cellar"


def log(message: str) -> None:
    print(f"==> {message}", flush=True)


def fail(message: str) -> "NoReturn":  # type: ignore[name-defined]
    print(f"ошибка: {message}", file=sys.stderr, flush=True)
    raise SystemExit(1)


_tokens: dict[str, str] = {}


def token(repo: str) -> str:
    if repo not in _tokens:
        url = f"https://ghcr.io/token?scope=repository:{repo}:pull"
        with urllib.request.urlopen(request(url), timeout=60) as response:
            _tokens[repo] = json.load(response)["token"]
    return _tokens[repo]


def request(url: str) -> urllib.request.Request:
    return urllib.request.Request(url, headers={"User-Agent": USER_AGENT})


def ghcr_json(repo: str, path: str, accept: str) -> dict:
    url = f"{GHCR}/{repo}/{path}"
    req = urllib.request.Request(
        url,
        headers={
            "Authorization": f"Bearer {token(repo)}",
            "Accept": accept,
            "User-Agent": USER_AGENT,
        },
    )
    with urllib.request.urlopen(req, timeout=120) as response:
        return json.load(response)


def image_name(formula: str) -> str:
    # Повторяет GitHubPackages.image_formula_name из Homebrew: ffmpeg@7 → ffmpeg/7.
    return formula.replace("@", "/").replace("+", "x")


def bottle_tag(dependency: dict) -> str:
    tag = dependency["pkg_version"]
    rebuild = dependency.get("bottle_rebuild", 0)
    if rebuild:
        tag += f"-{rebuild}"
    return tag


def download(url: str, destination: Path, repo: str) -> None:
    if destination.exists() and destination.stat().st_size > 0:
        return
    destination.parent.mkdir(parents=True, exist_ok=True)
    for attempt in range(1, 4):
        try:
            req = urllib.request.Request(
                url,
                headers={
                    "Authorization": f"Bearer {token(repo)}",
                    "User-Agent": USER_AGENT,
                },
            )
            with urllib.request.urlopen(req, timeout=300) as response, destination.open(
                "wb"
            ) as target:
                while chunk := response.read(1 << 20):
                    target.write(chunk)
            return
        except (urllib.error.URLError, TimeoutError, OSError) as exc:
            destination.unlink(missing_ok=True)
            if attempt == 3:
                fail(f"не скачалась бутылка {url}: {exc}")
            time.sleep(5 * attempt)


OS_TAGS = ((13, "ventura"), (14, "sonoma"), (15, "sequoia"), (26, "tahoe"))


def platform_tag(os_version: str) -> str:
    try:
        major = int(str(os_version).split(".")[0].removeprefix("macOS ").strip())
    except ValueError:
        return "all"
    for known, tag in OS_TAGS:
        if major == known:
            return tag
    return "all"


def select_variant(index: dict, arch: str) -> dict:
    platform_arch = ARCH_TO_PLATFORM[arch]
    variants = [
        manifest
        for manifest in index.get("manifests", [])
        if manifest.get("platform", {}).get("architecture") == platform_arch
        and manifest.get("platform", {}).get("os") == "darwin"
    ]
    for preferred in OS_PREFERENCE:
        for manifest in variants:
            os_version = str(manifest.get("platform", {}).get("os.version", ""))
            if os_version.startswith(preferred):
                return manifest
    seen = sorted(
        str(manifest.get("platform", {}).get("os.version")) for manifest in variants
    )
    fail(f"у бутылки нет варианта {platform_arch} под macOS 13/14/15: {seen}")
    raise AssertionError("недостижимо")


def fetch_bottle(
    name: str,
    tag: str,
    arch: str,
    cache: Path,
    version: str,
    rebuild: int = 0,
) -> tuple[Path, dict]:
    repo = f"homebrew/core/{image_name(name)}"
    index = ghcr_json(
        repo, f"manifests/{tag}", "application/vnd.oci.image.index.v1+json"
    )
    variant = select_variant(index, arch)
    manifest = ghcr_json(
        repo,
        f"manifests/{variant['digest']}",
        "application/vnd.oci.image.manifest.v1+json",
    )
    layer = manifest["layers"][0]["digest"]
    # Homebrew узнаёт бутылку по имени файла
    # (<формула>--<версия>.<тег>.bottle[.<ревизия>].tar.gz) и только потом
    # читает формулу из самого архива. Без этого суффикса `brew install`
    # принял бы файл за обычный архив и отказался его ставить.
    os_tag = platform_tag(variant["platform"]["os.version"])
    suffix = f".{rebuild}" if rebuild else ""
    filename = cache / (
        f"{name.replace('/', '_')}--{version}.{os_tag}.bottle{suffix}.tar.gz"
    )
    legacy = cache / f"{name.replace('/', '_')}--{tag}.tar.gz"
    if legacy.exists() and not filename.exists():
        legacy.rename(filename)
    download(f"{GHCR}/{repo}/blobs/{layer}", filename, repo)
    # В аннотации лежит tab с точными версиями зависимостей этой сборки.
    tab = json.loads(variant["annotations"]["sh.brew.tab"])
    log(f"бутылка {name} {tag} ({variant['platform']['os.version']})")
    return filename, tab


MACHO_MAGIC = (0xFEEDFACE, 0xFEEDFACF)
LC_LOAD_DYLIB = 0xC
LC_ID_DYLIB = 0xD
LC_LOAD_WEAK_DYLIB = 0x80000018
LC_REEXPORT_DYLIB = 0x8000001F
LC_LAZY_LOAD_DYLIB = 0x20
LC_LOAD_UPWARD_DYLIB = 0x80000023
LC_RPATH = 0x8000001C
DYLIB_LOAD_COMMANDS = {
    LC_LOAD_DYLIB,
    LC_LOAD_WEAK_DYLIB,
    LC_REEXPORT_DYLIB,
    LC_LAZY_LOAD_DYLIB,
    LC_LOAD_UPWARD_DYLIB,
}


def _u32(data: bytes, offset: int) -> int:
    return int.from_bytes(data[offset : offset + 4], "little")


def _slices(data: bytes) -> list[tuple[int, int]]:
    if len(data) < 8:
        return []
    magic = int.from_bytes(data[0:4], "big")
    if magic != 0xCAFEBABE:
        # Тонкий Mach-O: 0xFEEDFACE (32-bit) / 0xFEEDFACF (64-bit), читаются младшим
        # байтом вперёд на этой платформе.
        return [(0, len(data))]
    nfat = int.from_bytes(data[4:8], "big")
    result: list[tuple[int, int]] = []
    if 1 <= nfat <= 16 and 8 + nfat * 20 <= len(data):
        for index in range(nfat):
            base = 8 + index * 20
            offset = int.from_bytes(data[base + 8 : base + 12], "big")
            size = int.from_bytes(data[base + 12 : base + 16], "big")
            if offset + size > len(data):
                fail("машина-процессор слишком мала для fat-бинарника")
            result.append((offset, size))
    else:
        return [(0, len(data))]
    if not result:
        result = [(0, len(data))]
    return result


def is_macho(path: Path) -> bool:
    try:
        with path.open("rb") as handle:
            head = handle.read(8)
    except OSError:
        return False
    if len(head) < 8:
        return False
    little = int.from_bytes(head[0:4], "little")
    big = int.from_bytes(head[0:4], "big")
    if little in MACHO_MAGIC or big in MACHO_MAGIC:
        return True
    if big == 0xCAFEBABE and len(head) >= 8:
        nfat = int.from_bytes(head[4:8], "big")
        if 1 <= nfat <= 16 and 8 + nfat * 20 <= path.stat().st_size:
            return True
    return False


def load_commands(data: bytes) -> list[str]:
    """Пути из LC_LOAD_DYLIB/LC_LAZY_LOAD_DYLIB/LC_RPATH/LC_ID_DYLIB/LC_REEXPORT_DYLIB."""
    result: list[str] = []
    for base, size in _slices(data):
        magic = int.from_bytes(data[base : base + 4], "little")
        if magic not in MACHO_MAGIC:
            continue
        is_64 = magic == 0xFEEDFACF
        ncmds = int.from_bytes(data[base + 16 : base + 20], "little")
        offset = base + (32 if is_64 else 28)
        for _ in range(ncmds):
            if offset + 8 > base + size:
                break
            cmd = int.from_bytes(data[offset : offset + 4], "little")
            cmdsize = int.from_bytes(data[offset + 4 : offset + 8], "little")
            if cmdsize < 8 or offset + cmdsize > base + size:
                break
            if cmd in DYLIB_LOAD_COMMANDS or cmd == LC_RPATH or cmd == LC_ID_DYLIB:
                name_offset = int.from_bytes(data[offset + 8 : offset + 12], "little")
                start = offset + name_offset
                end = data.find(b"\0", start, offset + cmdsize)
                if end < 0:
                    fail("не нашёл завершающий NUL в load-команде")
                result.append(data[start:end].decode("utf-8", "surrogateescape"))
            offset += cmdsize
    return result


def relocate_macho(path: Path, prefix: str) -> bool:
    """Подменяет ``@@HOMEBREW_PREFIX@@/...`` на реальный префикс в Mach-O."""
    data = bytearray(path.read_bytes())
    changed = False
    for base, size in _slices(bytes(data)):
        magic = int.from_bytes(data[base : base + 4], "little")
        if magic not in MACHO_MAGIC:
            continue
        is_64 = magic == 0xFEEDFACF
        ncmds = int.from_bytes(data[base + 16 : base + 20], "little")
        offset = base + (32 if is_64 else 28)
        for _ in range(ncmds):
            if offset + 8 > base + size:
                break
            cmd = int.from_bytes(data[offset : offset + 4], "little")
            cmdsize = int.from_bytes(data[offset + 4 : offset + 8], "little")
            if cmdsize < 8 or offset + cmdsize > base + size:
                break
            if cmd in DYLIB_LOAD_COMMANDS or cmd == LC_RPATH or cmd == LC_ID_DYLIB:
                name_offset = int.from_bytes(data[offset + 8 : offset + 12], "little")
                start = offset + name_offset
                end = bytes(data).find(b"\0", start, offset + cmdsize)
                if end < 0:
                    fail(f"не нашёл NUL в load-команде файла {path}")
                old = bytes(data[start:end]).decode("utf-8", "surrogateescape")
                new = old
                if PLACEHOLDER_PREFIX in new:
                    new = new.replace(PLACEHOLDER_PREFIX, prefix)
                if PLACEHOLDER_CELLAR in new:
                    new = new.replace(PLACEHOLDER_CELLAR, prefix + CELLAR_SUFFIX)
                if new != old:
                    encoded = new.encode("utf-8", "surrogateescape")
                    if len(encoded) > end - start:
                        fail(
                            f"путь {old} в {path} не помещается по месту: {new}"
                        )
                    data[start:end] = encoded + b"\0" * (end - start - len(encoded))
                    changed = True
            offset += cmdsize
    if changed:
        mode = path.stat().st_mode
        path.chmod(mode | stat.S_IWUSR)
        path.write_bytes(bytes(data))
        path.chmod(mode)
    return changed


PLACEHOLDER_PREFIX = "@@HOMEBREW_PREFIX@@"
PLACEHOLDER_CELLAR = "@@HOMEBREW_CELLAR@@"


def bottle_dependencies(path: Path, formula: str) -> set[str]:
    """Формулы, на которые ссылаются dylib'ы внутри бутылки."""
    found: set[str] = set()
    with tarfile.open(path) as archive:
        for member in archive.getmembers():
            if not member.isfile() or not member.name.endswith(".dylib"):
                continue
            extracted = archive.extractfile(member)
            if extracted is None:
                continue
            for reference in load_commands(extracted.read()):
                if reference.startswith(PLACEHOLDER_PREFIX + "/opt/"):
                    dependency = reference[len(PLACEHOLDER_PREFIX) + len("/opt/") :].split("/", 1)[0]
                    if dependency and dependency != formula:
                        found.add(dependency)
                elif reference.startswith(PLACEHOLDER_CELLAR + "/"):
                    dependency = reference[len(PLACEHOLDER_CELLAR) :].split("/", 1)[0]
                    if dependency and dependency != formula:
                        found.add(dependency)
                elif reference.startswith("/opt/") or reference.startswith("/usr/local/"):
                    fail(
                        f"{formula}: зависимость {reference} записана абсолютным "
                        "путём, бутылка не переносимая"
                    )
    return found


def collect_bottles(arch: str, cache: Path) -> tuple[list[Path], Path]:
    mpv_path, tab = fetch_bottle("mpv", MPV_TAG, arch, cache, version=MPV_TAG)
    dependencies = {
        dependency["full_name"]: dependency
        for dependency in tab["runtime_dependencies"]
    }
    log(f"в метаданных mpv {MPV_TAG} зависимостей: {len(dependencies)}")

    paths = [mpv_path]
    queue: list[tuple[str, Path]] = [("mpv", mpv_path)]
    seen = {"mpv"}
    while queue:
        formula, path = queue.pop(0)
        for dependency in sorted(bottle_dependencies(path, formula)):
            if dependency in seen:
                continue
            if dependency not in dependencies:
                fail(
                    f"{formula} ссылается на {dependency}, которой нет в метаданных "
                    f"mpv {MPV_TAG}"
                )
            seen.add(dependency)
            metadata = dependencies[dependency]
            dependency_path, _ = fetch_bottle(
                dependency,
                bottle_tag(metadata),
                arch,
                cache,
                version=metadata["pkg_version"],
                rebuild=metadata.get("bottle_rebuild", 0),
            )
            paths.append(dependency_path)
            queue.append((dependency, dependency_path))

    tool_path, _ = fetch_bottle(
        "dylibbundler", TOOL_TAG, arch, cache, version=TOOL_TAG
    )
    paths.append(tool_path)
    log(f"нужно распаковать бутылок: {len(paths)}")
    return paths, mpv_path


def prepare_prefix(prefix: Path, arch: str) -> None:
    marker = prefix / ".orbitfm-libmpv"
    if prefix.exists():
        if marker.exists():
            if prefix.is_dir():
                shutil.rmtree(prefix)
            else:
                prefix.unlink()
        elif any(prefix.iterdir()):
            fail(
                f"каталог {prefix} не пуст, и не распознаётся как каталог проекта: "
                "укажите другой --prefix"
            )
        else:
            prefix.rmdir()
    prefix.mkdir(parents=True)
    marker.write_text(
        json.dumps({"arch": arch, "mpv": MPV_TAG, "tool": TOOL_TAG}, sort_keys=True)
    )


def extract_bottles(paths: list[Path], prefix: Path) -> None:
    cellar = prefix / "Cellar"
    opt = prefix / "opt"
    cellar.mkdir(parents=True, exist_ok=True)
    for bottle in paths:
        with tarfile.open(bottle) as archive:
            members = archive.getmembers()
            if not members:
                fail(f"пустая бутылка: {bottle}")
            top = members[0].name.strip("/")
            formula, version = top.split("/", 1)
            for member in members:
                if member.name.startswith("/") or ".." in member.name.split("/"):
                    fail(f"некорректное имя в бутылке {bottle}: {member.name}")
            try:
                archive.extractall(cellar, filter="data")
            except TypeError:
                archive.extractall(cellar)
        link = opt / formula
        link.parent.mkdir(parents=True, exist_ok=True)
        if link.is_symlink() or link.exists():
            if link.is_symlink() or link.is_file():
                link.unlink()
            else:
                shutil.rmtree(link)
        link.symlink_to(cellar / formula / version)


def _iter_machos(cellar: Path) -> list[Path]:
    result: list[Path] = []
    for path in sorted(cellar.rglob("*")):
        if path.is_symlink() or not path.is_file():
            continue
        if not is_macho(path):
            continue
        try:
            relative = path.relative_to(cellar)
        except ValueError:
            continue
        # Смотрим только собранные библиотеки и бинарники: `share/`, `doc/`,
        # `etc/` и прочие текстовые ресурсы бутылок отсеиваются автоматически.
        if len(relative.parts) < 3 or relative.parts[2] not in ("lib", "bin"):
            continue
        result.append(path)
    return result


def relocate_tree(prefix: Path) -> int:
    """Переносит Homebrew-плейсхолдеры в префикс, возвращает число затронутых бинарников."""
    cellar = prefix / "Cellar"
    changed = 0
    for path in _iter_machos(cellar=cellar):
        if relocate_macho(path, str(prefix)):
            changed += 1
    return changed


def verify(prefix: Path, arch: str) -> None:
    """Проверяет, что все ссылки dylib'ов нашли свои файлы и не остались «плейсхолдерами»."""
    bad: list[str] = []
    for path in _iter_machos(cellar=prefix / "Cellar"):
        for reference in load_commands(path.read_bytes()):
            if PLACEHOLDER_PREFIX in reference or PLACEHOLDER_CELLAR in reference:
                bad.append(f"{path}: не перенесён {reference}")
                break
            if reference.startswith("/opt/") or reference.startswith("/usr/local/"):
                bad.append(f"{path}: абсолютный путь к Homebrew {reference}")
                break
    if bad:
        fail("; ".join(bad[:10]))

    library = prefix / "opt/mpv/lib/libmpv.2.dylib"
    if not library.exists():
        fail(f"после распаковки нет {library}")
    commands = load_commands(library.read_bytes())
    if not any(
        path.startswith(str(prefix) + "/opt/mpv/lib/")
        for path in commands
        if "/opt/mpv/lib/" in path
    ):
        fail(f"libmpv не ссылается в {prefix}/opt/mpv/lib: {commands[:5]}")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--arch",
        choices=sorted(ARCH_TO_PLATFORM),
        default=platform.machine(),
        help="архитектура сборки (по умолчанию — архитектура машины)",
    )
    parser.add_argument(
        "--cache",
        type=Path,
        default=Path(tempfile.gettempdir()) / "orbitfm-libmpv-bottles",
        help="каталог для скачанных бутылок",
    )
    parser.add_argument(
        "--prefix",
        type=Path,
        default=DEFAULT_PREFIX,
        help=f"каталог для распакованных бутылок (по умолчанию {DEFAULT_PREFIX})",
    )
    args = parser.parse_args()

    if args.arch not in ARCH_TO_PLATFORM:
        fail(f"неизвестная архитектура: {args.arch}")

    args.cache = args.cache.expanduser().resolve()
    args.prefix = args.prefix.expanduser().resolve()
    if len(str(args.prefix)) + len(CELLAR_SUFFIX) > PLACEHOLDER_LEN:
        fail(
            f"префикс {args.prefix} слишком длинный для бутылок Homebrew "
            f"(нужен сегмент не длиннее {PLACEHOLDER_LEN}): "
            f"{len(str(args.prefix)) + len(CELLAR_SUFFIX)}"
        )

    paths, _ = collect_bottles(args.arch, args.cache)
    prepare_prefix(args.prefix, args.arch)
    extract_bottles(paths, args.prefix)
    changed = relocate_tree(args.prefix)
    verify(args.prefix, args.arch)
    print(f"libmpv: {args.prefix}/opt/mpv/lib/libmpv.2.dylib")
    print(f"dylibbundler: {args.prefix}/opt/dylibbundler/bin/dylibbundler")


if __name__ == "__main__":
    main()
