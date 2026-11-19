#!/usr/bin/env bash
# Генерирует GitHub Wiki OrbitFM из двуязычного руководства docs/user.
#
# Источник истины — docs/user/{en,ru}: вики не пишется руками и не расходится
# с руководством. Скрипт переименовывает страницы в вики-имена, переписывает
# относительные ссылки в вики-ссылки, собирает двуязычный _Sidebar и _Footer,
# затем проверяет результат: переключатель языка на каждой странице, живые
# ссылки, отсутствие упоминаний ADR.
#
#   packaging/wiki-sync.sh              # собрать в target/wiki
#   packaging/wiki-sync.sh --out DIR    # собрать в свой каталог (например, клон вики)
#   packaging/wiki-sync.sh --check      # только проверить, ничего не писать на диск
#   packaging/wiki-sync.sh --push       # собрать в клон вики и запушить
#
# Для --push нужен доступ на запись в https://github.com/ModernByte/orbitfm.wiki.git
# (через gh auth setup-git или SSH в ORBITFM_WIKI_REMOTE).

set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

OUT="target/wiki"
MODE=generate
WIKI_REMOTE="${ORBITFM_WIKI_REMOTE:-https://github.com/ModernByte/orbitfm.wiki.git}"
WIKI_CLONE="target/wiki-repo"

usage() {
    cat <<EOF
Использование: packaging/wiki-sync.sh [--out DIR] [--check] [--push] [--help]

  --out DIR   куда собрать вики (по умолчанию target/wiki)
  --check     проверить руководство, ничего не записывая
  --push      обновить клон $WIKI_CLONE и запушить в вики
  --help      эта справка
EOF
}

while [ $# -gt 0 ]; do
    case "$1" in
        --out)
            [ $# -ge 2 ] || { echo "--out требует каталог" >&2; exit 1; }
            OUT="$2"
            shift 2
            ;;
        --check) MODE=check; shift ;;
        --push) MODE=push; shift ;;
        -h | --help) usage; exit 0 ;;
        *) echo "неизвестный флаг: $1 (см. --help)" >&2; exit 1 ;;
    esac
done

if [ "$MODE" = push ]; then
    if [ -d "$WIKI_CLONE/.git" ]; then
        git -C "$WIKI_CLONE" pull --ff-only
    else
        mkdir -p "$(dirname -- "$WIKI_CLONE")"
        git clone "$WIKI_REMOTE" "$WIKI_CLONE"
    fi
    OUT="$WIKI_CLONE"
elif [ "$MODE" = check ]; then
    OUT="$(mktemp -d "${TMPDIR:-/tmp}/orbitfm-wiki-check.XXXXXX")"
    trap 'rm -rf -- "$OUT"' EXIT
fi

python3 - "$ROOT" "$OUT" <<'PY'
import pathlib
import re
import shutil
import sys

root = pathlib.Path(sys.argv[1])
out = pathlib.Path(sys.argv[2])
user = root / "docs" / "user"
version = re.search(
    r'^version = "([^"]+)"', (root / "Cargo.toml").read_text(encoding="utf-8"), re.MULTILINE
).group(1)

# Имя страницы руководства -> имя страницы вики. Английские имена канонические
# (по умолчанию английский), русские получают суффикс -ru.
SLUG = {
    "index": "Home",
    "quick-reference": "Quick-reference",
    "install": "Install",
    "install-macos-apple-silicon": "Install-macOS-Apple-Silicon",
    "install-macos-intel": "Install-macOS-Intel",
    "first-launch": "First-launch",
    "window-modes": "Window-modes",
    "settings": "Settings",
    "controls": "Controls",
    "integrations": "Integrations",
    "panels": "Panels",
    "browser": "Browser",
    "window-rules": "Window-rules",
    "tray-and-mpris": "Tray-and-MPRIS",
    "use-cases": "Use-cases",
    "troubleshooting": "Troubleshooting",
    "privacy": "Privacy",
    "glossary": "Glossary",
}

ORDER = [
    "index",
    "quick-reference",
    "install",
    "install-macos-apple-silicon",
    "install-macos-intel",
    "first-launch",
    "window-modes",
    "settings",
    "controls",
    "integrations",
    "panels",
    "browser",
    "window-rules",
    "tray-and-mpris",
    "use-cases",
    "troubleshooting",
    "privacy",
    "glossary",
]

# Порядок и вложенность боковой панели. Заданы здесь, чтобы панель не зависела
# от текста страниц: подписи берутся из первых заголовков руководства.
NAV_ORDER = [
    ("index", 0),
    ("quick-reference", 0),
    ("install", 0),
    ("install-macos-apple-silicon", 1),
    ("install-macos-intel", 1),
    ("first-launch", 0),
    ("window-modes", 0),
    ("settings", 0),
    ("controls", 0),
    ("integrations", 0),
    ("panels", 1),
    ("browser", 1),
    ("window-rules", 1),
    ("tray-and-mpris", 0),
    ("use-cases", 0),
    ("troubleshooting", 0),
    ("privacy", 0),
    ("glossary", 0),
]

TITLE = {"en": "OrbitFM wiki", "ru": "Вики OrbitFM"}

failures: list[str] = []


def label(text: str) -> str:
    """Заголовок первого уровня страницы без решётки."""
    for line in text.splitlines():
        if line.startswith("# "):
            return line[2:].strip()
    return "OrbitFM"


def page_name(stem: str, lang: str) -> str:
    name = SLUG.get(stem)
    if name is None:
        raise SystemExit(f"нет вики-имени для страницы {stem}")
    return name if lang == "en" else f"{name}-ru"


def rewrite_target(target: str, lang: str) -> str:
    """Относительная ссылка руководства -> ссылка вики."""
    if re.match(r"^[a-z]+:", target) or target.startswith("#") or target.startswith("//"):
        return target

    path, _, anchor = target.partition("#")
    suffix = f"#{anchor}" if anchor else ""

    if path.startswith("../images/"):
        return "images/" + path[len("../images/") :] + suffix

    if path.startswith("../../../"):
        return (
            "https://github.com/ModernByte/orbitfm/blob/main/"
            + path[len("../../../") :]
            + suffix
        )

    m = re.match(r"^\.\./(en|ru)/([a-z0-9-]+)\.md$", path)
    if m:
        return page_name(m.group(2), m.group(1)) + suffix

    m = re.match(r"^([a-z0-9-]+)\.md$", path)
    if m:
        return page_name(m.group(1), lang) + suffix

    return target


LINK = re.compile(r"\]\(([^)\s]+)(\s+\"[^\"]*\")?\)")


def rewrite_links(text: str, lang: str) -> str:
    def repl(match: re.Match[str]) -> str:
        return f"]({rewrite_target(match.group(1), lang)}{match.group(2) or ''})"

    return LINK.sub(repl, text)


def build() -> None:
    if out.exists():
        for item in out.iterdir():
            if item.name == ".git":
                continue
            if item.is_dir():
                shutil.rmtree(item)
            else:
                item.unlink()
    out.mkdir(parents=True, exist_ok=True)

    shutil.copytree(user / "images", out / "images")

    labels: dict[str, dict[str, str]] = {"en": {}, "ru": {}}

    for lang in ("en", "ru"):
        source = user / lang
        pages = sorted(p.stem for p in source.glob("*.md") if not p.name.startswith("_"))
        missing = [s for s in ORDER if s not in pages]
        extra = [s for s in pages if s not in SLUG]
        if missing:
            failures.append(f"docs/user/{lang}: нет страниц {', '.join(missing)}")
        if extra:
            failures.append(f"docs/user/{lang}: нет вики-имени для {', '.join(extra)}")

        for stem in ORDER:
            page = source / f"{stem}.md"
            if not page.is_file():
                continue
            text = page.read_text(encoding="utf-8")
            labels[lang][stem] = label(text)
            body = rewrite_links(text, lang)
            # Переключатель языка — первая ссылка после заголовка; на всякий
            # случай проверяем, что после переписывания он ведёт на вики-страницу.
            if f"]({page_name(stem, 'ru' if lang == 'en' else 'en')})" not in body:
                failures.append(f"{page}: нет переключателя языка на вики-страницу")
            (out / f"{page_name(stem, lang)}.md").write_text(body, encoding="utf-8")

    sidebar(labels)
    footer()
    validate()


def sidebar(labels: dict[str, dict[str, str]]) -> None:
    lines: list[str] = []
    for lang in ("en", "ru"):
        lines.append(f"### {TITLE[lang]}")
        lines.append("")
        if lang == "en":
            lines.append("**English** · [Русский](Home-ru)")
        else:
            lines.append("[English](Home) · **Русский**")
        lines.append("")
        for stem, depth in NAV_ORDER:
            if stem not in labels[lang]:
                continue
            name = page_name(stem, lang)
            text = labels[lang][stem]
            if stem == "index":
                text = "Home" if lang == "en" else "Главная"
            indent = "  " * depth
            lines.append(f"{indent}* [{text}]({name})")
        lines.append("")
    lines.append("---")
    lines.append(f"`{version}` · [Releases](https://github.com/ModernByte/orbitfm/releases)")
    lines.append("")
    (out / "_Sidebar.md").write_text("\n".join(lines), encoding="utf-8")


def footer() -> None:
    text = (
        "---\n\n"
        f"OrbitFM `{version}` · "
        "[Releases](https://github.com/ModernByte/orbitfm/releases) · "
        "[Issues](https://github.com/ModernByte/orbitfm/issues) · "
        "[Source](https://github.com/ModernByte/orbitfm) · "
        "[Security](https://github.com/ModernByte/orbitfm/blob/main/SECURITY.md) · "
        "[License](https://github.com/ModernByte/orbitfm/blob/main/LICENSE)\n"
    )
    (out / "_Footer.md").write_text(text, encoding="utf-8")


def validate() -> None:
    pages = {p.stem for p in out.glob("*.md")}
    adr = re.compile(r"\bADR[-\s]?\d|\bADRs?\b")
    for page in sorted(out.glob("*.md")):
        text = page.read_text(encoding="utf-8")
        if adr.search(text):
            failures.append(f"{page.name}: упоминание ADR на пользовательской странице")
        for target in re.findall(r"\]\(([^)\s]+)\)", text):
            if re.match(r"^[a-z]+:", target) or target.startswith("#"):
                continue
            candidate = target.split("#", 1)[0]
            if candidate.startswith("images/"):
                if not (out / candidate).is_file():
                    failures.append(f"{page.name}: нет картинки {candidate}")
            elif candidate and candidate not in pages:
                failures.append(f"{page.name}: ссылка на несуществующую страницу {candidate}")
    if not (out / "Home.md").is_file():
        failures.append("нет Home.md")
    if not (out / "_Sidebar.md").is_file():
        failures.append("нет _Sidebar.md")


build()

if failures:
    print("Вики не собрана, проблемы:", file=sys.stderr)
    for failure in failures:
        print(f"  {failure}", file=sys.stderr)
    sys.exit(1)

pages = len([p for p in out.glob("*.md") if not p.name.startswith("_")])
print(f"Вики собрана в {out}: {pages} страниц (EN + RU), боковая панель, подвал.")
PY

if [ "$MODE" = push ]; then
    VERSION="$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -n 1)"
    cd "$WIKI_CLONE"
    git add -A
    if git diff --cached --quiet; then
        echo "Вики уже актуальна: коммитить нечего."
        exit 0
    fi
    git commit -q -F - <<MSG
docs(wiki): двуязычная вики под $VERSION

Собрано из docs/user/{en,ru} скриптом packaging/wiki-sync.sh: английский
по умолчанию, русский с суффиксом -ru, переключатель языка на каждой
странице, общий двуязычный _Sidebar.
MSG
    git push origin HEAD
    echo "Вики обновлена: $(git log --oneline -1)"
fi
