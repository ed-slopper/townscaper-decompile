#!/usr/bin/env bash
# Print Ghidra's pseudo-C for one or more RVAs from the project made by tools/ghidra_import.sh.
#   tools/ghidra_decompile.sh 0x3D88D0 [0x...]
# Prints to the terminal only. Close the project in the Ghidra GUI first (headless needs the lock).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
GHIDRA_DIR="${GHIDRA_DIR:-D:/code/modding/ghidra_12.1.4_PUBLIC}"
export GHIDRA_HEADLESS_MAXMEM="${GHIDRA_HEADLESS_MAXMEM:-8G}"
"$GHIDRA_DIR/support/analyzeHeadless.bat" "$(cygpath -w "$ROOT/work/ghidra")" townscaper \
  -process GameAssembly.dll -noanalysis -readOnly \
  -scriptPath "$(cygpath -w "$ROOT/ghidra/scripts")" \
  -postScript DecompileRva.java "$@" 2>&1 \
  | sed -n '/==== RVA/,$p' | grep -v -E '^(INFO|WARN|DEBUG) |\(HeadlessAnalyzer\)|^20[0-9]{2}-'
