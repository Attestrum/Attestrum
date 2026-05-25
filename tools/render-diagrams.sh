#!/usr/bin/env bash
# Render every Mermaid block under docs/diagrams/ to a PNG in /diagrams-png/.
#
# Mirrors the source-tree layout: docs/diagrams/<area>/<topic>.md → diagrams-png/<area>/<topic>.png
# (and -1.png / -2.png / ... when a file contains multiple mermaid blocks).
#
# Authorized by CLAUDE.md §2: Mermaid stays the source of truth; PNGs are derived,
# gitignored, local-only convenience artifacts. Re-run this whenever a diagram changes.
#
# mmdc resolution precedence (matches tools/diagram-linter conventions):
#   1. $ATTESTRUM_MMDC                 — explicit override
#   2. `mmdc` on PATH              — global install (`npm i -g @mermaid-js/mermaid-cli@10.9.1`)
#   3. `npx -y @mermaid-js/mermaid-cli@10.9.1` — fallback (slow; pulls each run)

set -euo pipefail

# Resolve repo root from this script's location (tools/render-diagrams.sh).
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/.." && pwd)"

SRC_ROOT="${REPO_ROOT}/docs/diagrams"
OUT_ROOT="${REPO_ROOT}/diagrams-png"

if [[ ! -d "${SRC_ROOT}" ]]; then
  echo "render-diagrams: ${SRC_ROOT} not found" >&2
  exit 1
fi

# Resolve mmdc command.
if [[ -n "${ATTESTRUM_MMDC:-}" ]]; then
  MMDC=(${ATTESTRUM_MMDC})
elif command -v mmdc >/dev/null 2>&1; then
  MMDC=(mmdc)
elif command -v npx >/dev/null 2>&1; then
  MMDC=(npx -y "@mermaid-js/mermaid-cli@10.9.1")
else
  echo "render-diagrams: no mmdc found (set ATTESTRUM_MMDC, install mmdc globally, or have npx on PATH)" >&2
  exit 2
fi

# Rendering defaults: white background, 2x scale (≈ crisp for slides without going huge).
THEME="${ATTESTRUM_MMDC_THEME:-default}"
BG="${ATTESTRUM_MMDC_BG:-white}"
SCALE="${ATTESTRUM_MMDC_SCALE:-2}"

mkdir -p "${OUT_ROOT}"

rendered=0
failed=0

# Walk every .md under docs/diagrams/.
while IFS= read -r -d '' src; do
  rel="${src#${SRC_ROOT}/}"
  rel_dir="$(dirname "${rel}")"
  base="$(basename "${rel}" .md)"
  out_dir="${OUT_ROOT}/${rel_dir}"
  mkdir -p "${out_dir}"

  # Count mermaid blocks in this file. Robust against indentation: matches ^```mermaid$ only.
  block_count="$(grep -c '^```mermaid$' "${src}" || true)"
  if [[ "${block_count}" -eq 0 ]]; then
    continue
  fi

  # Extract block N (1-indexed) to a temp file, render with mmdc.
  for ((i=1; i<=block_count; i++)); do
    tmp_in="$(mktemp -t attestrum-mmd-XXXXXX).mmd"
    awk -v want="${i}" '
      /^```mermaid$/ { in_block=1; n++; next }
      /^```$/        { if (in_block) { in_block=0; if (n==want) exit } ; next }
      in_block && n==want { print }
    ' "${src}" > "${tmp_in}"

    if [[ "${block_count}" -eq 1 ]]; then
      out_png="${out_dir}/${base}.png"
    else
      out_png="${out_dir}/${base}-${i}.png"
    fi

    if "${MMDC[@]}" -i "${tmp_in}" -o "${out_png}" \
                    -e png -t "${THEME}" -b "${BG}" -s "${SCALE}" \
                    >/dev/null 2>&1; then
      rendered=$((rendered+1))
      printf "  ✓ %s\n" "${out_png#${REPO_ROOT}/}"
    else
      failed=$((failed+1))
      printf "  ✗ %s (mmdc failed on block %d of %s)\n" \
             "${out_png#${REPO_ROOT}/}" "${i}" "${rel}" >&2
    fi
    rm -f "${tmp_in}"
  done
done < <(find "${SRC_ROOT}" -type f -name '*.md' -print0)

echo
echo "render-diagrams: ${rendered} PNG(s) written to ${OUT_ROOT#${REPO_ROOT}/}/, ${failed} failure(s)"

if [[ "${failed}" -gt 0 ]]; then
  exit 1
fi
