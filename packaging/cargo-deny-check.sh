# Проверка контура поставки: дисциплина игнорирований + `cargo deny check`.
#
# Исполняется и в CI (job `supply-chain`, и как часть гейта релиза), и локально
# перед тегом. Смысл ровно один: граф зависимостей обязан быть проверен, а
# любое «мы это проигнорировали» — иметь дату пересмотра, иначе проверка
# превращается в декорацию.
#
# Версия cargo-deny пиннится и проверяется по SHA256. Тот же musl-тарбол
# скачивает и экшен EmbarkStudios — мы не тянем десятый экшен и не компилируем
# инструмент на каждом пуше.
set -euo pipefail

DENY_VERSION="0.20.2"
DENY_SHA256="9f12ed4c49936e09b48bf862b595cde2fe64fcbd9d74dfacac6131ca824c8d5f"
DENY_ARCHIVE="cargo-deny-${DENY_VERSION}-x86_64-unknown-linux-musl"
DENY_URL="https://github.com/EmbarkStudios/cargo-deny/releases/download/${DENY_VERSION}/${DENY_ARCHIVE}.tar.gz"

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cfg="${repo_root}/deny.toml"

# --- 1. Дисциплина `ignore`: каждая прощённая запись обязана иметь дату. ---
# Считаем блоки вида `{ id = … }` / `{ crate = … }` внутри `ignore = [...]` и
# сравниваем с числом комментариев `# revisit: ГГГГ-ММ-ДД`.
ignored=$(awk '/^ignore = \[/,/^\]/' "${cfg}" | grep -cE '^\s*\{ ' || true)
marked=$(grep -cE '# revisit: [0-9]{4}-[0-9]{2}-[0-9]{2}' "${cfg}" || true)

if [ "${ignored}" -ne "${marked}" ]; then
  echo "У каждой записи в [advisories] ignore обязан быть комментарий '# revisit: ГГГГ-ММ-ДД'." >&2
  echo "  записей в ignore: ${ignored}, дат пересмотра: ${marked}" >&2
  exit 1
fi

# Дата в формате ISO сравнивается лексикографически — это корректно для ГГГГ-ММ-ДД.
today="$(date -u +%Y-%m-%d)"
expired="$(grep -oE '# revisit: [0-9]{4}-[0-9]{2}-[0-9]{2}' "${cfg}" \
  | sed 's/.*revisit: //' | awk -v today="${today}" '$1 < today' || true)"

if [ -n "${expired}" ]; then
  echo "Срок пересмотра игнорирований истёк — запись не имеет права пережить то, что за неё заступилось:" >&2
  printf '  просрочено: %s\n' ${expired} >&2
  echo "  нужно либо обновить граф, либо снять игнор, либо явно пересмотреть дату в ADR." >&2
  exit 1
fi

echo "Дисциплина ignore соблюдена: ${ignored} записей, все с датой пересмотра (сегодня ${today})."

# --- 2. Инструмент: только пиннованная версия, только по проверенному дайджесту. ---
if ! command -v cargo-deny >/dev/null 2>&1 \
  || [ "$(cargo-deny --version 2>/dev/null | awk '{print $NF}')" != "${DENY_VERSION}" ]; then
  install_dir="${TMPDIR:-/tmp}/${DENY_ARCHIVE}"
  if [ ! -x "${install_dir}/cargo-deny" ]; then
    curl -sSfL --proto '=https' --tlsv1.2 \
      -o "${TMPDIR:-/tmp}/${DENY_ARCHIVE}.tar.gz" "${DENY_URL}"
    echo "${DENY_SHA256}  ${TMPDIR:-/tmp}/${DENY_ARCHIVE}.tar.gz" | sha256sum --check --quiet
    tar xzf "${TMPDIR:-/tmp}/${DENY_ARCHIVE}.tar.gz" -C "${TMPDIR:-/tmp}"
  fi
  export PATH="${install_dir}:${PATH}"
fi

echo "Инструмент: $(cargo-deny --version)"

# --- 3. Проверка графа. ---
# Резолв идёт по Cargo.lock (`--locked` на уровне cargo-deny нет, поэтому лок
# фиксируется заранее через `cargo fetch`). Без `-t/--target`: проверяем граф
# целиком, включая платформенные зависимости macOS и Windows — они так же
# попадают в вендинг и в артефакты.
cd "${repo_root}"
cargo fetch --locked
cargo-deny check
