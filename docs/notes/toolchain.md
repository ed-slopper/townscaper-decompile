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

## Ghidra (F3, proven 2026-10-15)

- `tools/ghidra_import.sh`: import + auto-analysis with the PDB took **1,761 s** (29 min) headless with 12 GB
  heap. The PDB applicator resolves ~290,000 types; a few hundred "failed to align" warnings on CRT structs
  are harmless.
- Every il2cpp function has its PDB name: `<Type>_<Method>_m<hash>` (`WaveFunctionCollapse_IterateDecor_mDE24…`).
  `ApplyIl2CppNames.java` adds a plate comment with the C# name(s) and il2cpp signature to 57,582 functions,
  labels 25,445 metadata globals (`Method$…`, `Field$…`, `TypeInfo` slots) and 11,492 string literals
  (`StringLiteral_…`, value as EOL comment). No function was left `FUN_*`, so it renamed none.
- The PDB's structs carry the fields: bodies read `__this->___decorUpdates_12`, `list->____size_2`
  (il2cpp's mangling: `___<name>_<index>`). No need to import `il2cpp.h`.
- `tools/ghidra_decompile.sh 0x3D88D0 [...]` prints bodies headless in ~25 s per run (project must not be
  open in the GUI). To use the Ghidra MCP instead, open `work/ghidra/townscaper.gpr` in the GUI and start the
  MCP server there; check the active program is `GameAssembly.dll`.
- Middleware types can have obfuscated names (`EONGqEcSkgpSiKfAgKEiugWqjmzy`: Rewired); game types don't.
