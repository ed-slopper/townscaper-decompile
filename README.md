# townscaper-decompile

Townscaper's game code (Oskar Stålberg / Raw Fury, Unity 2020.3.18f1, IL2CPP) recovered method by method from
the shipped build and rewritten 1:1 as readable C# in a Unity project.

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

Needs Python 3.10+, .NET Framework 4.x (Il2CppDumper), and for Ghidra 12.x a JDK 21+.
