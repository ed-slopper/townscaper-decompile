#!/usr/bin/env bash
# Import GameAssembly.dll into a Ghidra project of its own (work/ghidra/townscaper, gitignored)
# and auto-analyse it with the shipped GameAssembly.pdb (Ghidra's PDB analyzer picks it up from
# the game folder). Takes a long time (the PDB is 240 MB): run it once, in the background.
#   GHIDRA_DIR=D:/code/modding/ghidra_12.1.4_PUBLIC tools/ghidra_import.sh ["C:/GOG Games/Townscaper"]
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
GAME="${1:-${TOWNSCAPER_DIR:-C:/GOG Games/Townscaper}}"
GHIDRA_DIR="${GHIDRA_DIR:-D:/code/modding/ghidra_12.1.4_PUBLIC}"
mkdir -p "$ROOT/work/ghidra"
export GHIDRA_HEADLESS_MAXMEM="${GHIDRA_HEADLESS_MAXMEM:-12G}"
"$GHIDRA_DIR/support/analyzeHeadless.bat" "$(cygpath -w "$ROOT/work/ghidra")" townscaper \
  -import "$GAME/GameAssembly.dll" -overwrite \
  -scriptPath "$(cygpath -w "$ROOT/ghidra/scripts")" \
  -postScript ApplyIl2CppNames.java "$(cygpath -w "$ROOT/work/il2cppdumper/script.json")" \
  -log "$(cygpath -w "$ROOT/work/ghidra/analyze.log")"
