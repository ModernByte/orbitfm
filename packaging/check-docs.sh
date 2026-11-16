#!/usr/bin/env bash
# Проверка пользовательской документации: двуязычность, отсутствие ADR
# на пользовательских страницах и живые относительные ссылки.
set -euo pipefail

cd "$(dirname "$0")/.."

python3 - <<'PY'
import pathlib
import re
import sys
import urllib.parse

failures: list[str] = []
user = pathlib.Path("docs/user")
ru = sorted(p for p in (user / "ru").glob("*.md") if not p.name.startswith("_"))
en = sorted(p for p in (user / "en").glob("*.md") if not p.name.startswith("_"))

ru_names = {p.name for p in ru}
en_names = {p.name for p in en}
for name in sorted(ru_names - en_names):
    failures.append(f"docs/user/ru/{name}: нет английской версии docs/user/en/{name}")
for name in sorted(en_names - ru_names):
    failures.append(f"docs/user/en/{name}: нет русской версии docs/user/ru/{name}")

# Пользовательские страницы не упоминают ADR и не ссылаются в разделы разработчика.
user_surfaces = list(user.rglob("*.md")) + [
    pathlib.Path("README.md"),
    pathlib.Path("README.ru.md"),
    pathlib.Path("LEGAL.md"),
]
for page in user_surfaces:
    text = page.read_text(encoding="utf-8")
    for pattern, label in (
        (r"ADR-\d", "упоминание ADR"),
        (r"docs/adr", "ссылка в docs/adr"),
        (r"docs/modules", "ссылка в docs/modules"),
        (r"docs/interfaces", "ссылка в docs/interfaces"),
    ):
        if re.search(pattern, text):
            failures.append(f"{page}: {label} на пользовательской странице")

# Переключатель языка на каждой странице руководства.
for page in ru:
    if f"**Русский** · [English](../en/{page.name})" not in page.read_text(encoding="utf-8"):
        failures.append(f"{page}: нет переключателя на English")
for page in en:
    if f"**English** · [Русский](../ru/{page.name})" not in page.read_text(encoding="utf-8"):
        failures.append(f"{page}: нет переключателя на русский")

# Живые относительные ссылки во всей пользовательской документации и на хабах.
link_surfaces = list(user.rglob("*.md")) + list(pathlib.Path("docs/dev").rglob("*.md")) + [
    pathlib.Path("README.md"),
    pathlib.Path("README.ru.md"),
    pathlib.Path("LEGAL.md"),
    pathlib.Path("docs/README.md"),
    pathlib.Path("docs/README.ru.md"),
]
for page in link_surfaces:
    text = page.read_text(encoding="utf-8")
    for match in re.finditer(r"\]\(([^)\s]+)", text):
        target = match.group(1)
        if target.startswith(("http://", "https://", "mailto:", "#")):
            continue
        path = urllib.parse.unquote(target.split("#", 1)[0])
        if not path:
            continue
        if not (page.parent / path).resolve().exists():
            failures.append(f"{page}: битая ссылка {target}")

# Старые пути вики не должны возвращаться.
for page in link_surfaces:
    if "docs/wiki" in page.read_text(encoding="utf-8"):
        failures.append(f"{page}: устаревший путь docs/wiki")

# Описание релиза двуязычное: русский текст — в CHANGELOG, английский —
# в docs/releases/en/<версия>.md, где версия совпадает с Cargo.toml.
cargo = pathlib.Path("Cargo.toml").read_text(encoding="utf-8")
match = re.search(r'^version = "([^"]+)"', cargo, re.MULTILINE)
if not match:
    failures.append("Cargo.toml: не найдена version")
else:
    version = match.group(1)
    notes = pathlib.Path("docs/releases/en") / f"{version}.md"
    if not notes.is_file():
        failures.append(f"{notes}: нет английского текста релиза {version}")
    elif not notes.read_text(encoding="utf-8").strip():
        failures.append(f"{notes}: файл пустой")

changelog = pathlib.Path("docs/CHANGELOG.md").read_text(encoding="utf-8")
for notes in sorted(pathlib.Path("docs/releases/en").glob("*.md")):
    if f"## [{notes.stem}]" not in changelog:
        failures.append(f"{notes}: в CHANGELOG нет секции [{notes.stem}]")

if failures:
    print("Проверка документации не прошла:", file=sys.stderr)
    for failure in failures:
        print(f"  {failure}", file=sys.stderr)
    sys.exit(1)

print(
    f"Документация в порядке: {len(ru)} страниц на русском, "
    f"{len(en)} на английском, ADR на пользовательских страницах нет, "
    f"английский текст релиза на месте."
)
PY
