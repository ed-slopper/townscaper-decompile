# AGENTS.md: how agents work in this repo

## 1. The goal

All of **Townscaper**'s own game code recovered from the shipped IL2CPP build and rewritten 1:1 as readable
**C#** in a **Unity 2020.3.18f1** project (`unity/`), method by method, so that the project built with the
player's own game assets behaves like the original. The game's code is the `Placemaker` and `Os` namespaces of
`Assembly-CSharp` (448 types, 2,269 methods with bodies: [`docs/function-index.csv`](docs/function-index.csv)).
The bundled middleware (Rewired, I2 Localization, Crosstales FileBrowser, Thread Ninja, StandaloneFileBrowser,
the Fury SDK, Unity's own packages) is not ours to recover: see board row F7.

The board is [`WORK.md`](WORK.md). The rules below let any number of AI agents, working for any number of
people, pick work from it without clashing.

## 2. The reference build

Every address in this repo, in code, notes, the board and commit messages, belongs to:

| | |
|---|---|
| Game | Townscaper (Oskar Stålberg / Raw Fury), Windows, GOG build `55528005367341443` |
| Engine | Unity 2020.3.18f1, IL2CPP, **x86 (32-bit)** |
| `GameAssembly.dll` | md5 `4470ec364697ebe141925da367a2c147`, 23,678,440 bytes, image base `0x10000000` |
| `global-metadata.dat` | IL2CPP metadata v27 (dumper: 27.1), md5 `23b32bc8b632646f179b3e9d0c17c23d` |
| `GameAssembly.pdb` | shipped with the game, md5 `bb5ffca03916d6a12fd0ebc1362b4b12` (full il2cpp C++ symbols) |
| Assemblies | `Assembly-CSharp` = TypeDefIndex 7477.., `Assembly-CSharp-firstpass` = 7411..7476 |

Addresses are written as **RVAs** (`0x3D88D0`), the form Il2CppDumper and Cpp2IL print; the VA in Ghidra is
`0x10000000 + RVA`. `python -I tools/identify.py "<game dir>"` prints these values for your install. **If your
md5s differ, stop and tell the human**: the addresses and type indices here do not apply to your build.

Other builds exist (Steam, Mac, iOS, Android, Switch, Xbox). The GOG Windows build above is the only reference.

## 3. When to self-assign

- **No task given** ("start", "go", "work on the board", a link to `WORK.md`): pick a row with the rules in
  section 6 and start without asking.
- **A task named** ("do E3", "port the save string"): do that one. Claim its row first, or add a row for it.
- **Only a question**: answer it and claim nothing.

What the human says always beats the board's order.

## 4. Who you work for, and your agent name

1. Find the person's GitHub name: from `git config user.email`, `gh auth status` or the remote; ask once if you
   can't tell. Write it as GitHub shows it (`ed-slopper`).
2. Give yourself an agent name, a short plain word (`wren`, `otter`, `birch`), saved once per clone:
   `git config --local agent.name <word>`. If the clone already has one, use it. Each agent needs its own clone
   or worktree and its own name.
3. Every claim, branch and commit carries `<person>/<agent>` (`ed-slopper/wren`). In a commit, put it on its
   own line above the `Co-Authored-By` lines.

## 5. What is taken

Before you pick, `git fetch --all --prune` and look at:

- every row with anything in **Who**, on `main` and on every other branch;
- every file someone else changed in the last 7 days, on any branch or open pull request
  (`git log --all --since=7.days --name-only --format='%an %s'`, `gh pr list`). Read them freely; change them
  only if your row needs it and the owner is you, or the human says so.

## 6. How to pick

Go through the tiers in order: P0, P1, P2, P3, P4, P5. Take the **first** open row (empty Who) where:

- **Area** is your person, `unowned` or `any` (never `humans`: those are decisions for people);
- every row in **Needs** is in the Done table;
- none of the files it will touch are taken (section 5).

If the row is big, split it first (`E3` → `E3a`, `E3b`, same Area and Needs) and claim only the part you will
finish this session. **Finish the row you claimed before you start the next one.**

## 7. How to claim

1. On `main`, write `<person>/<agent> (<YYYY-MM-DD>)` in the row's **Who**.
2. Commit **only `WORK.md`**, message `claim <ID>: <task in a few words>`, and push straight away. If you can't
   push to `main`, push a branch `claim-<ID>-<words>-<person>-<agent>` and open a pull request.
3. If the push is rejected, `git pull --rebase`, re-read the board, and if the row is still free try once more.

## 8. Branches

`<what>-<ID>-<two to five plain words>-<person>-<agent>`, where `<what>` is `claim`, `join` (sharing a row),
`done` (moving a row to Done), `board` (other board or rule changes) or `work` (the code). Example:
`work-E6-wave-function-collapse-ed-slopper-wren`. One agent on its own may commit straight to `main`.

## 9. Finishing and stopping

- **Done**: move the row to the **Done** table with the date and the commit hash, in the same commit as the
  last change or right after it. Every gap you found becomes a new open row (next free ID in that tier, Notes:
  `Gap of <ID>`).
- **Stopping early**: clear Who, and write in Notes what is finished, what is left and where. Commit and push.

## 10. Sharing a row

Only when a human asks. Add the second name with `+` and say in Notes who takes which part.

## 11. Changing the board

Anyone may change priorities, rows, areas or these rules when their human asks. On its own an agent may only
**add rows, split rows and fix facts**. Every change gets one line in **Changes to the board** at the bottom of
`WORK.md`: `2026-10-15 ed-slopper/wren: split E3 into E3a-E3c`.

## 12. Areas

| Person | Branch | Area | Files |
|---|---|---|---|
| ed-slopper | `main` | Everything so far | `tools/**`, `ghidra/**`, `docs/**`, `unity/**` |

New people: add your own line (in a `board-` commit) before you claim a row, and take `unowned` or `any` rows.

## 13. Rules of the project

- **No game files in the repo, ever.** No `GameAssembly.dll`, `global-metadata.dat`, `.pdb`, `.assets`,
  `.resS`, `level*` files, nothing extracted from them (meshes, textures, sounds, shaders, scenes, prefabs,
  ScriptableObjects), **no decompiler output** (Il2CppDumper's `dump.cs`/`il2cpp.h`/DummyDll, Cpp2IL output,
  Ghidra pseudo-C, ISIL, disassembly), no Ghidra projects. All of that lives in `work/` (gitignored), made from
  each person's own copy by `tools/decompile.sh`. Names, signatures, addresses, field offsets and constants
  read out of the build are fine (`docs/*.csv`, `ghidra/symbols.txt`).
- **Rewrite, don't paste.** C# in `unity/` is written by us from reading the original: same algorithm, same
  order, same constants, same float maths (`float`, not `double`, where the original uses `float`; the same
  `Unity.Mathematics` / `Mathf` calls), same bugs. Never paste Cpp2IL/Ghidra output into `unity/`.
- **Write the original's address next to every recreated method**, in its doc comment:
  `/// <summary>Placemaker.WaveFunctionCollapse.IterateDecor, RVA 0x3D88D0</summary>`. Keep the original's
  type, member and field names and namespaces (they are in the metadata), so saves, scenes and serialized
  fields line up.
- **Mark everything that is ours and not the game's** with `STANDIN:` and what it stands in for:
  `// STANDIN: no-op, for Rewired's ReInput.players (middleware, F7)`. `grep -rn STANDIN unity` lists every one.
- **Mark progress in `docs/function-index.csv`**, column `status`: `dumped` (nothing yet) → `read` (understood,
  notes written) → `ported` (C# in `unity/`) → `checked` (proven equal to the original, section 15). The
  `notes` column takes a short pointer (`notes/wfc.md`). `tools/index.py` keeps these when it rebuilds.
- **Do not chase a gap outside your task.** Note it in the system's notes and add a row.

## 14. Decompiling

- Run `tools/decompile.sh` once per clone (fetches pinned Il2CppDumper and Cpp2IL into `tools/bin`, writes
  `work/il2cppdumper`, `work/cpp2il-cs`, `work/cpp2il-isil`). Then `python -I tools/index.py` rebuilds the
  indexes in `docs/`.
- What each source is good for:
  - `work/cpp2il-cs/DiffableCs/Assembly-CSharp/**`: every type with fields, offsets, attributes, method
    signatures, RVAs, and the calls each method makes (`[Calls]`, `[CallerCount]`). Start here.
  - **Ghidra with the PDB** (`tools/ghidra_import.sh`, project `work/ghidra/townscaper`): the method bodies.
    The shipped `GameAssembly.pdb` names every il2cpp function (`Placemaker_WaveFunctionCollapse__IterateDecor_m…`),
    `ghidra/scripts/ApplyIl2CppNames.java` adds the C# name, signature and string literals. Read bodies here.
  - `work/cpp2il-isil/**`: Cpp2IL's instruction-level dump, a second opinion when Ghidra's output is unclear.
  - `work/il2cppdumper/il2cpp.h`: the C structs of every type, for Ghidra's data types (row F4).
- The **Ghidra MCP server** (<https://github.com/bethington/ghidra-mcp>) lets an agent read and rename in a running
  Ghidra. Before trusting an address, check the open program is `GameAssembly.dll` with the md5 above, **not**
  another project's program.
- Names Ghidra doesn't have yet (locals, our struct fields, helper labels) go into `ghidra/symbols.txt` in the
  same commit as your code.
- IL2CPP patterns to strip when reading bodies: `il2cpp_codegen_initialize_runtime_metadata`, the
  `<Type>_TypeInfo_var` class-init checks, null checks (`NullCheck`, `ThrowNullReferenceException`), array bounds
  checks, write barriers, and `MethodInfo*` last arguments. They are the runtime's, not the game's.
- Notes, one file per system, live in [`docs/notes/`](docs/notes/README.md). Keep "proven" (read from code) and
  "inferred" (a guess) apart.

## 15. Building and checking

- `unity/` is a Unity **2020.3.18f1** project (the reference engine version; row F6). The player's own assets are
  imported into `unity/Assets/Game/` locally by `tools/` (row F8); that folder is gitignored.
- A method is **checked** only when it is shown equal to the original on the comparison harness (row F9):
  the same inputs fed to the original (in the running game) and to ours, outputs compared exactly.

## 16. End of every session

Finish with, for each person with an agent on this repo, a ready-to-paste prompt for their next task: repo URL
and branch, row ID and the task in one sentence, the RVAs and the build (md5 above), the files it touches, how
to tell it is done, and what not to touch.
