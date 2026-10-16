#!/usr/bin/env bash
# Decompile the player's own Townscaper install into work/ (gitignored).
#   tools/decompile.sh ["C:/GOG Games/Townscaper"]
# Steps (each skipped if its output already exists; delete the folder to redo):
#   1. Il2CppDumper  -> work/il2cppdumper  (dump.cs, script.json, il2cpp.h, DummyDll/)
#   2. Cpp2IL        -> work/cpp2il-cs     (diffable C# with signatures, offsets, attributes)
#   3. Cpp2IL        -> work/cpp2il-isil   (per-method ISIL: readable instruction-level bodies)
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
GAME="${1:-${TOWNSCAPER_DIR:-C:/GOG Games/Townscaper}}"
BIN="$ROOT/tools/bin"
WORK="$ROOT/work"
DATA="$GAME/Townscaper_Data"

[ -f "$BIN/Cpp2IL.exe" ] || bash "$ROOT/tools/fetch_tools.sh"
[ -f "$GAME/GameAssembly.dll" ] || { echo "no GameAssembly.dll in $GAME" >&2; exit 1; }
mkdir -p "$WORK"

python -I "$ROOT/tools/identify.py" "$GAME" | tee "$WORK/identify.txt"

if [ ! -f "$WORK/il2cppdumper/dump.cs" ]; then
  mkdir -p "$WORK/il2cppdumper"
  "$BIN/Il2CppDumper/Il2CppDumper.exe" "$GAME/GameAssembly.dll" \
    "$DATA/il2cpp_data/Metadata/global-metadata.dat" "$WORK/il2cppdumper"
fi

if [ ! -d "$WORK/cpp2il-cs" ]; then
  "$BIN/Cpp2IL.exe" --game-path "$GAME" --exe-name Townscaper \
    --use-processor attributeanalyzer,attributeinjector,callanalyzer,nativemethoddetector \
    --output-as diffable-cs --output-to "$WORK/cpp2il-cs"
fi

if [ ! -d "$WORK/cpp2il-isil" ]; then
  "$BIN/Cpp2IL.exe" --game-path "$GAME" --exe-name Townscaper \
    --output-as isil --output-to "$WORK/cpp2il-isil"
fi

echo "done: $WORK"
