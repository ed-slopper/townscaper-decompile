# Overview: the game's code

Reference build: AGENTS.md section 2. Counts from `docs/type-index.csv` (types include nested and
compiler-generated ones; methods include those without bodies).

## What is in `Assembly-CSharp` (+ `-firstpass`)

| Owner | Types | Methods |
|---|---:|---:|
| **game** (`Placemaker*`, `Os*`, global) | 448 | 2,340 |
| Rewired (input) | 170 | 2,489 |
| I2 Localization | 115 | 806 |
| Crosstales FileBrowser | 64 | 735 |
| Thread Ninja (`CielaSpike`) | 6 | 25 |
| StandaloneFileBrowser (`SFB`) | 5 | 31 |

Other assemblies shipped and **not** ours: `FuryStudios.*` (Raw Fury's platform SDK and build system),
`Rewired_Core`/`Rewired_Windows`, `Unity.Mathematics`, `Unity.TextMeshPro`, `UnityEngine.UI`, `Ookii.Dialogs`,
`System.Windows.Forms`, `GooglePlayGames`, `NintendoSDK`, Mono's BCL.

## The game's namespaces

| Namespace | Types | Methods | Board | What (from names and signatures: inferred until read) |
|---|---:|---:|---|---|
| `Os.Utils` | 19 | 180 | E1 | Oskar's maths and helpers: `ExtraMath`, intersections, packed floats (`ByteFloat*`, `SbyteFloat3`), pools |
| `Os.Hex` | 4 | 39 | E2 | Hex coordinates and the ring walk (`HexOnionIterator`) |
| `Placemaker.Quads`, `.Quads.GridGeneration` | 11 | 34 | E3 | Quad/vertex structs of the irregular grid; hex patches, custom grids |
| `Placemaker` (`GridGenerator`) | | 45 | E3 | Builds the irregular quad grid |
| `Placemaker.Graphs` | 19 | 88 | E4 | The town's data: corners, squares, qubes (cells), voxels, flow, actions |
| `Placemaker.Modules`, `.Modules.Processing` | 14 | 32 | E5 | Module library and orientations; editor-time module import |
| `Placemaker` (`WaveFunctionCollapse`) | | 18 | E6 | Picks a module for every cell by constraint propagation |
| `Placemaker` (`ModuleBuilder`, `WorldMeshes`, `BigMesh*`) | | ~45 | E7 | Turns chosen modules into meshes |
| `Placemaker` (`BootMaster`, `WorldMaster`, ...) | | ~60 | E8 | Start-up and the frame loop |
| `Placemaker` (`AoBaker`, `ShadowBaker`, `Sandbanker`, ...) | | ~50 | E9 | GPU bakes: ambient occlusion, shadows, sand banks |
| `Placemaker.Props` + `PropPlacer` | 27 | 107 | E10 | Decorations: wires, flags, windows, propellers, wind vanes |
| `Placemaker` (`Maker`, clickers, hover) | | ~110 | G1 | Placing and removing blocks |
| `Placemaker` (`SaveSystem`, `TextSaveSystem`, ...) | | ~120 | G3 | Save files and the shareable town string |
| `Placemaker.Life` | 9 | 47 | G5 | Birds and butterflies |
| `Placemaker.Audio` + sound classes | 4 | ~30 | G6 | Ambience, water, UI sounds, music |
| `Placemaker.Ui` | 174 | 963 | G7 | Menus, buttons, palette, sun, controls, screenshots |
| global (`PlatformManager`, ...) | 29 | 105 | G8, L2 | Platform glue, debug helpers |
| `Placemaker.SceneProcessing` | 3 | 3 | G8 | Per-platform scene stripping |

## Reading order

Leaf first, so every method you port has its callees ported: `Os.Utils` → `Os.Hex` → grid → graph → modules →
WFC → meshes → main loop → the rest. Cpp2IL's `[Calls]` / `[CallerCount]` attributes in
`work/cpp2il-cs/DiffableCs/Assembly-CSharp/` give the call graph per method.
