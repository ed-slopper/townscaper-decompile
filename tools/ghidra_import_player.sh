#!/usr/bin/env bash
# Import UnityPlayer.dll (the engine's native code, with the shipped UnityPlayer_Win32_il2cpp_x86.pdb) into
# the same Ghidra project as GameAssembly.dll, for engine functions the game calls natively (F11: PerlinNoise,
# NextPowerOfTwo, Random). Slow: run once, in the background.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
GAME="${1:-${TOWNSCAPER_DIR:-C:/GOG Games/Townscaper}}"
GHIDRA_DIR="${GHIDRA_DIR:-D:/code/modding/ghidra_12.1.4_PUBLIC}"
export GHIDRA_HEADLESS_MAXMEM="${GHIDRA_HEADLESS_MAXMEM:-12G}"
"$GHIDRA_DIR/support/analyzeHeadless.bat" "$(cygpath -w "$ROOT/work/ghidra")" townscaper \
  -import "$GAME/UnityPlayer.dll" -overwrite \
  -log "$(cygpath -w "$ROOT/work/ghidra/analyze_player.log")"
