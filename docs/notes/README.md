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
| [modules.md](modules.md) | Modules (E5): the records, `Orientation` and `ModuleMath`'s tables |
| [wfc.md](wfc.md) | The wave function collapse (E6): queues and their order, filling a qube, the loops |
| [meshes.md](meshes.md) | Mesh building (E7): module geometry, big meshes, the module builder |
| [main_loop.md](main_loop.md) | Boot and the main loop (E8): frame budget, time, the world master's iterate order |
| [bakes.md](bakes.md) | Lighting and ground bakes (E9): the sun and its shadow camera, AO, sandbanks, the draw plane and border |
| [props.md](props.md) | Props (E10): placements, prop nodes and their relations, placing props on modules |
| [graph.md](graph.md) | The voxel graph (E4): objects and containers, editing, the update loop, saving and loading, sampling, the flow field |
| [saving.md](saving.md) | Saving (G3): the save folder, the town and settings files (XmlSerializer form), the save system |
| [life.md](life.md) | Life (G5): birds on the props (perches, creation, take-off, flight), butterflies |
| [audio.md](audio.md) | Audio (G6): the sound effects' blending and pools, the UI sounds |
