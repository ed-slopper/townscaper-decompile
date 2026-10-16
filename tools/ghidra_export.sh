#!/usr/bin/env bash
# Write the pseudo-C of every game method (docs/function-index.csv) to work/decomp/ (gitignored).
# Needs the project from tools/ghidra_import.sh, closed in the Ghidra GUI. ~20-40 minutes.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
GHIDRA_DIR="${GHIDRA_DIR:-D:/code/modding/ghidra_12.1.4_PUBLIC}"
export GHIDRA_HEADLESS_MAXMEM="${GHIDRA_HEADLESS_MAXMEM:-12G}"
mkdir -p "$ROOT/work/decomp"
"$GHIDRA_DIR/support/analyzeHeadless.bat" "$(cygpath -w "$ROOT/work/ghidra")" townscaper \
  -process GameAssembly.dll -noanalysis -readOnly \
  -scriptPath "$(cygpath -w "$ROOT/ghidra/scripts")" \
  -postScript ExportDecomp.java "$(cygpath -w "$ROOT/docs/function-index.csv")" "$(cygpath -w "$ROOT/work/decomp")" \
  -log "$(cygpath -w "$ROOT/work/ghidra/export.log")"
grep -h "ExportDecomp:" "$ROOT/work/ghidra/export.log" | tail -1
