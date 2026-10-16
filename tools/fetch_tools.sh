#!/usr/bin/env bash
# Download the third-party decompile tools into tools/bin (gitignored).
# Pinned versions so every clone gets the same output.
set -euo pipefail
cd "$(dirname "$0")"
mkdir -p bin
cd bin

IL2CPPDUMPER_VER=v6.7.46
CPP2IL_VER=2022.1.0-pre-release.21

if [ ! -f Il2CppDumper/Il2CppDumper.exe ]; then
  curl -fL -o il2cppdumper.zip \
    "https://github.com/Perfare/Il2CppDumper/releases/download/${IL2CPPDUMPER_VER}/Il2CppDumper-win-${IL2CPPDUMPER_VER}.zip"
  mkdir -p Il2CppDumper && unzip -oq il2cppdumper.zip -d Il2CppDumper && rm il2cppdumper.zip
fi

if [ ! -f Cpp2IL.exe ]; then
  curl -fL -o Cpp2IL.exe \
    "https://github.com/SamboyCoding/Cpp2IL/releases/download/${CPP2IL_VER}/Cpp2IL-${CPP2IL_VER}-Windows.exe"
fi

echo "tools ready in $(pwd)"

# Run without waiting for a key press at the end.
sed -i 's/"RequireAnyKey": true/"RequireAnyKey": false/' Il2CppDumper/config.json
