# townscaper-decompile

Townscaper (Oskar Stålberg / Raw Fury, Unity 2020.3.18f1, IL2CPP) decompiled and rewritten 1:1 in **Rust** on
**Bevy** (`townscaper-rs/`), method by method from the shipped build, running from the player's own game files.

**This repo contains no game files and no decompiler output.** You need your own copy of Townscaper; the tools
here decompile *your* install into `work/` (gitignored). What is committed: our tools, our notes, name and
address indexes, and our own rewrite.

- The board: [`WORK.md`](WORK.md). The rules for people and agents: [`AGENTS.md`](AGENTS.md).
- Indexes: [`docs/type-index.csv`](docs/type-index.csv), [`docs/function-index.csv`](docs/function-index.csv).
- Notes per system: [`docs/notes/`](docs/notes/README.md).

## Quick start (Windows, Git Bash)

```bash
python -I tools/identify.py "C:/GOG Games/Townscaper"   # compare with AGENTS.md section 2
bash tools/decompile.sh "C:/GOG Games/Townscaper"       # fetches tools, dumps into work/
python -I tools/index.py                                 # rebuilds docs/*.csv from work/
bash tools/ghidra_import.sh "C:/GOG Games/Townscaper"   # Ghidra project with the PDB (slow)
```

Build the rewrite (MSVC toolchain):

```bash
cd townscaper-rs && cargo +stable-x86_64-pc-windows-msvc run --release
```

Needs Rust (MSVC toolchain), Python 3.10+, .NET Framework 4.x (Il2CppDumper), and for Ghidra 12.x a JDK 21+.
