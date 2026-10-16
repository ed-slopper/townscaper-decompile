# Toolchain

## The reference build (proven, `tools/identify.py`)

- GOG Windows build `55528005367341443`, Unity **2020.3.18f1**, IL2CPP, **x86** (32-bit) `GameAssembly.dll`
  (md5 `4470ec364697ebe141925da367a2c147`), image base `0x10000000`.
- `global-metadata.dat` v27; Il2CppDumper detects 27, switches to **27.1**.
- The build ships **`GameAssembly.pdb`** (240 MB) and the Unity player PDBs. The PDB carries the il2cpp C++
  names of every generated function, so Ghidra names all game methods without help.
- 56 managed assemblies (`ScriptingAssemblies.json`). The game's code is in `Assembly-CSharp`
  (TypeDefIndex 7477..) and `Assembly-CSharp-firstpass` (7411..7476).

## Tools (pinned in `tools/fetch_tools.sh`)

| Tool | Version | Output in `work/` | Use |
|---|---|---|---|
| Il2CppDumper | v6.7.46 (.NET Framework build) | `il2cppdumper/`: `dump.cs`, `script.json`, `il2cpp.h`, `stringliteral.json`, `DummyDll/` | Types, offsets, RVAs; names for Ghidra |
| Cpp2IL | 2022.1.0-pre-release.21 | `cpp2il-cs/DiffableCs/<assembly>/**.cs` | One file per type with signatures, offsets, `[Calls]`, `[CallerCount]` |
| Cpp2IL | same | `cpp2il-isil/` | Instruction-level bodies (second opinion) |
| Ghidra | 12.1.4 | `ghidra/townscaper.gpr` | Method bodies as pseudo-C, with PDB names |

Run times on the reference PC (16 threads, 32 GB): Il2CppDumper ~10 s, each Cpp2IL pass ~45 s.

## Snags

- Il2CppDumper waits for a key at the end unless `RequireAnyKey` is false (`fetch_tools.sh` sets it).
- In `dump.cs` nested types (`Outer.Inner`) are printed with an empty namespace; `tools/index.py` takes the
  outer type's.
- Identical-code folding: several C# methods can share one body (e.g. `BootMaster.Awake` and a lambda of
  `AoBaker.<>c` both at 0x1D4430). One RVA, several rows; the plate comment in Ghidra lists every name.
- Ghidra 12 has no Jython, so Il2CppDumper's `ghidra*.py` scripts don't run as is; `ApplyIl2CppNames.java` is
  our Java replacement for the naming part.
- Don't import `GameAssembly.dll` into another project's Ghidra (one may be open in the GUI with the MCP
  server): `tools/ghidra_import.sh` makes its own project in `work/ghidra/`.
