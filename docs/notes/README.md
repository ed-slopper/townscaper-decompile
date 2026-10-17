# Notes

One file per system. Write what is **proven** (read from the code, with RVAs) apart from what is **inferred**.
Names, signatures, addresses and constants only: no pasted decompiler output.

| File | System |
|---|---|
| [toolchain.md](toolchain.md) | The reference build, the decompile tools, what each output is good for, snags |
| [overview.md](overview.md) | Map of the game's code: namespaces, systems, who calls whom |
| [files.md](files.md) | The game's files: SerializedFile layout, what each file holds |
| [unity.md](unity.md) | Unity / .NET behaviour the game depends on (F11): what is ported, how it was proven |
| [grid.md](grid.md) | The irregular grid (E3): units, patch generation step by step |
| [graph.md](graph.md) | The voxel graph (E4): objects and containers, adding, integrating and removing voxels |
