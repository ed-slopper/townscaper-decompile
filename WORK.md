# WORK.md: the board

Goal and rules: [`AGENTS.md`](AGENTS.md). Every address here is an RVA in the GOG Windows build of Townscaper
(`GameAssembly.dll` md5 `4470ec364697ebe141925da367a2c147`, Unity 2020.3.18f1, IL2CPP x86, metadata v27).

Columns:
- **ID**: stable, letter per section (F foundations, E engine, G gameplay, C content, X finishing, L later).
  Split rows get a letter or digit added (`E3` → `E3a`). IDs are never reused.
- **Area**: a person, `unowned`, `any`, or `humans` (a decision, not for agents).
- **Needs**: rows that must be in Done first.
- **Who**: empty = open, or `person/agent (date)`.
- **Notes**: `[est]` = my estimate of what the row needs; type/method counts are from `docs/type-index.csv`.

Where things stand (2026-10-15): the build is identified and the decompile pipeline runs from the player's own
install (`tools/decompile.sh`: Il2CppDumper + Cpp2IL into `work/`). The game's own code is indexed: 448 types,
2,269 methods with bodies in `docs/function-index.csv`, all `dumped`, none read, ported or checked. Ghidra has the
whole build with PDB names, C# names and field names (F3). There is no Unity project yet (F6) and no checking harness (F9).

## P0 Foundations

| ID | Task | Area | Needs | Who | Notes |
|---|---|---|---|---|---|
| F5 | `ghidra/scripts/ExportDecomp.java` (start from `DecompileRva.java`): write every game method's pseudo-C to `work/decomp/<Namespace>/<Type>/<Method>_<rva>.c` (only `owner = game` rows of `function-index.csv`), so agents without the Ghidra MCP can read bodies | any | F3 | | Output stays in `work/` |
| F6 | `unity/`: an empty Unity 2020.3.18f1 project with the packages the game ships (Unity.Mathematics, TextMeshPro, uGUI; versions from `ScriptingAssemblies.json` and the DLLs' assembly versions), `Assets/Scripts/Placemaker`, `Assets/Scripts/Os`, assembly layout as the original (`Assembly-CSharp-firstpass` for `Plugins/`), and it opens and compiles | any | | | Needs Unity 2020.3.18f1 on the PC (Hub) |
| F7 | Decide what to do with the paid / closed middleware the game calls: Rewired (input, 170 types), I2 Localization (115), Crosstales FileBrowser (64), Thread Ninja, StandaloneFileBrowser, FuryStudios SDK: buy and install them locally (gitignored), write `STANDIN` facades with the same API, or replace with Unity's own | humans | | | Blocks the Ui and input rows; facades are the cheapest path [est] |
| F8 | Assets from the player's own install into `unity/Assets/Game/` (gitignored): AssetRipper (pinned, fetched like the other tools) over `Townscaper_Data`, with scenes `level0..2`, prefabs, ScriptableObjects (module library, palettes, custom grids), meshes, textures, audio; script references re-pointed at our `Placemaker.*` types by GUID / name | any | F6 | | [est] Shaders come out as compiled DXBC: C3 |
| F9 | Checking harness: run code inside the original game and compare with ours. Install MelonLoader or BepInEx 6 (IL2CPP) on the player's copy, a plugin that calls a game method with given inputs and writes the result, and an NUnit test in `unity/` that does the same on our port; exact compare | any | F6 | | [est] Split: F9a (loader + hello world in the original), F9b (call any static method by RVA/name and dump the result as JSON), F9c (compare runner) |
| F10 | `tools/guard.py` + CI: fail if a game file or decompiler output is committed (extensions, `dump.cs`, `il2cpp.h`, size/magic checks), `function-index.csv` well-formed, every `/// … RVA 0x…` in `unity/` names a real row | any | F2 | | |

## P1 Core engine

The procedural core, leaf first: maths, the irregular grid, the graph, modules, wave function collapse, meshes.

| ID | Task | Area | Needs | Who | Notes |
|---|---|---|---|---|---|
| E1 | `Os.Utils`: `ExtraMath` (56 methods), `Intersections`, `Bitwise`, `ByteFloat`/`ByteFloat2`/`ByteFloat3`/`SbyteFloat3`, `NumberStrings`, list/dict pool extensions, `UnityFunctions`, `ScopedProfiler` | unowned | F3, F6 | | Leaf code, good first port; everything else calls it |
| E2 | `Os.Hex`: `HexMath` (21), `HexOnionIterator` (14, the ring-by-ring walk), visualiser gizmos | unowned | E1 | | |
| E3 | The irregular grid: `GridGenerator` (45 methods: hex patch → random merge of triangles into quads → subdivision → relaxation), `Placemaker.Quads` (`Quad`, `Vert`), `Placemaker.Quads.GridGeneration` (`HexPatch`, `HexCluster`, `CustomGridPatch`/`Importer`/`Library`, `MotivationCounter`, `GridConstants`), `GridBit` | unowned | E2 | | [est] The heart of the look; seed and RNG order must match (`UnityEngine.Random` vs `System.Random`: find which). Split before claiming |
| E4 | The graph: `Placemaker.Graphs` (`Graph` 42 methods incl. `AddVoxel` 0xE829E0, `Iterate` 0xE89CF0, `Save` 0xE8F920; `Corner`, `Square`, `Qube`, `Flow`/`FlowData`/`FlowTarget`, `Action`, `ModuleContainer`, `OrientedModuleSides`, `SideCounts`, `ShadowMesh`, `VoxelMesh`), `Voxel`, `VoxelType`, `MaterialType` | unowned | E3 | | [est] Split before claiming |
| E5 | Modules: `ModuleMath` (17), `ByteQube`, `Orientation`, `SideProfile`, `Module`/`BaseModule`/`DecorModule`/`ModuleMesh`, `Placemaker.Modules` (`ModuleLibrary`, `OrientedModule`, `CornerLookup`, `OutlineUv`) | unowned | E4 | | |
| E5a | `Placemaker.Modules.Processing` (`ModuleImporter`, `ProtoModule`, `DecorProfile`, `MaterialCluster`, ...): the editor-time import of module meshes; port only what the shipped game runs | unowned | E5 | | Mostly editor code [est] |
| E6 | `WaveFunctionCollapse` (18 methods incl. `IterateDecor` 0x3D88D0, `FillQubePossibilities` 0x3D6DD0, `RemovePossibleModule` 0x3DA190, the `Propagation` queue and its `CompareTo` 0x4FC880) | unowned | E5 | | Order of the propagation queue decides the result: port exactly |
| E7 | Mesh building: `ModuleBuilder` (16, `Iterate` 0x32FF90), `WorldMeshes` (11), `BigMesh`/`BigMeshGroup`/`BigMeshMaster`/`BigMeshPart`/`BigMeshList`, `BitMeshQuadChange`, `MaterialMaster`, `Palette`/`PaletteHolder` | unowned | E6 | | |
| E8 | Boot and the main loop: `BootMaster`, `BootGameCore` (`Update` 0x7C1FA0), `WorldMaster` (23: `Iterate` 0x1F8C00, `Update` 0x1F9AF0, `LateUpdate` 0x1F91B0, `Load`), `TimeManager`, `FastCoroutines`/`FastCoroutineUpdater`, `LayerMaster`, `UniversalStopwatch`, `ShaderIds`, `MiscReferences` | unowned | E7 | | Who ticks whom each frame; scene `level0..2` wiring from F8 |
| E9 | Lighting and ground bakes: `AoBaker` (9), `ShadowBaker` (17), `ShadowBakerCamera`, `Sandbanker` (9, `Iterate` 0x84FBA0), `DrawPlane`, `BorderDrawer` | unowned | E7 | | GPU passes: need the shaders (C3) to check |
| E10 | Props: `PropPlacer` (22), `PropPlacement`, `Placemaker.Props` (27 types: `PropNode`, wires and flags, windows, propellers, wind vanes, `PropAchievements`, ...), `Prop` | unowned | E7 | | |

## P2 Gameplay systems

| ID | Task | Area | Needs | Who | Notes |
|---|---|---|---|---|---|
| G1 | Building: `Maker` (24), `GroundClicker` (23), `HoverData` (21), `HoverHightlight`, `MasterClicker` (27), `ClickEffect`, `RapidButton`, `VoxelBobEffect`, `VoxelBounceEffect`, `ToolType` | unowned | E4, E8 | | Placing and removing blocks, colours, the click feel |
| G2 | Camera: `OrbitalCamera` (23), `CustomCameraProjection`, `SaveCamera`, `ScreenshotCamera` | unowned | E8 | | |
| G3 | Saving: `SaveSystem` (40), `SaveData`, `MetaSave`, `SettingsData`, `StorageUtils` (19), `SaveCardsHandler` (23), `TexturePngMaster`, `SaveDataScripableObject` | unowned | E4 | | Save files must stay byte-compatible with the original's |
| G3a | The shareable town string: `TextSaveSystem` (`SaveToString` / `LoadFromString`, bit packing `AddValue`/`ReadValue`/`GetBitCount`), `TextSaveAlphabet` | unowned | E4, F9 | | Easiest end-to-end check: a string from the original must load to the same corners/voxels in ours, and back to the same string |
| G4 | Undo / redo: find where it lives (`Graph` actions? `UndoRedoFeedback`, `UndoRedoButtons`), port it | unowned | E4 | | |
| G5 | Life: `BirdFlock` (17), `Bird`, `BirdLanding`, `BirdSpot`, `ButterflyFlock`, `Butterfly`, `ButterflyLanding` | unowned | E8 | | |
| G6 | Audio: `AmbientSound`, `AudioSnapshotManager`, `AudioSourcePool`, `ClickParticleSound`, `WaterSound`, `UiAudio`, `Beatbox`, `ManySound`/`ManySoundsManager`, `MenuMusic` | unowned | E8 | | |
| G7 | UI (`Placemaker.Ui`, 174 types, 963 methods): split before claiming | unowned | E8, F7 | | Split: G7a–G7f |
| G7a | UI core: `UiMaster` (39), `UpdateState` (25), `SideMenu`/`SideMenuNavigator`, `GameViewRect`, `UiScaler`, `UiElementScaler`, `ScaleUiButtons`, `PinnedUi`/`PinnedUiMobile`, `Dim`, `LoadingUi` | unowned | E8, F7 | | |
| G7b | Save menu: `SaveMenu` (35), `SaveCard`, `FocusedSaveCard`, `MaxSavesCounter`, `SavingIndicator`, `TextSaveButtons` | unowned | G3, G7a | | |
| G7c | Look controls: `PaletteMenu` (21), `PalettePicker`, `SunButton`, `SunButtonGamepad`, `SunSlider`, `ShadingButtons` (17), `GridButtons`, `ToolsButton` | unowned | G7a | | |
| G7d | Input and controls: `ActionInputManager`, `InputModeManager`, `GamepadScreenCursor`, `ControlBinding` (26), `ControlBindingKey`, `KeyboardAndMouseControlRemap` (22), `KeyboardControlsMenu` (22), `GenericControlsMenu`, `GamepadControlsRow`, `ControlLegend`, `BindingRow` | unowned | G7a, F7 | | All on Rewired |
| G7e | Widgets and dialogs: `BaseButton`, `StandardButton`, `ToggleButton`, `NavigableButton`, `DisableableButton`, `ProgressBarButton`, `Slider`, `GamepadStepper`, `BetterScrollRect`, `DialogSystem`, `DialogWindow`, `SimpleMessage`, `GenericMenuNavigator`, `ButtonNavigator`, `ButtonAudio*`, `DistanceFieldMask`/`Settings`, `EmptyGraphic`, `CornerImage`, `CloseSymbol`, `CommandFeedbacker`, `Toucher` | unowned | G7a | | |
| G7f | Menus and the rest: `SettingsMenu`, `CreditsMenu`/`CreditsData`, `LanguageButtons`, `Localization`, `LocalizeRTLFlipper`, `SoundButton`, `FullscreenButton`, `ScreenshotButtons`, `ScreenshotFrame`, `ScreenshotSwitch` (16), `ObjExporter`, `UndoRedoButtons`, `Username`, `WebLinkButton`, `XBoxSignoutButton`, `GamepadXboxDialog` | unowned | G7a | | |
| G8 | Platform glue: `PlatformManager`, `FurySystemMessenger`, `LocalizationService`, `Placemaker.SceneProcessing` (`PlatformDependent`, `TargetPlatformFlags`), achievements (`PropAchievements`), the Fury SDK calls | unowned | F7 | | PC/GOG paths first; console paths are L1 |

## P3 Content

| ID | Task | Area | Needs | Who | Notes |
|---|---|---|---|---|---|
| C1 | Scenes `level0`, `level1`, `level2`: what each holds, which components on which objects, serialized values; written up in `docs/notes/scenes.md` (names and values only) | unowned | F8 | | |
| C2 | The module library: the `ModuleLibrary` asset's layout (modules, sides, profiles, decor) loaded by our `Placemaker.Modules` types from the player's asset | unowned | F8, E5 | | |
| C3 | Shaders: the game's shaders (water, outlines, AO/shadow spreaders, marching squares, ground) recovered from their compiled form into readable `.shader` source in `unity/` | unowned | F8 | | [est] Biggest content row; DXBC → HLSL by hand. Split before claiming |
| C4 | Palettes, custom grids (`CustomGridLibrary`), credits and localisation tables (I2 `LanguageSourceData`) read from the player's assets | unowned | F8, F7 | | |

## P4 Finishing

| ID | Task | Area | Needs | Who | Notes |
|---|---|---|---|---|---|
| X1 | Our build runs end to end from the player's assets: boot, build a town, save, load, screenshot | unowned | E8, G1, G3, G7a | | |
| X2 | Save compatibility: a save folder from the original loads in ours and back, byte for byte | unowned | G3, G3a, F9 | | |
| X3 | Empty `grep -rn STANDIN unity` (except middleware facades decided in F7) | unowned | X1 | | |
| X4 | Every `owner = game` row of `function-index.csv` at `checked` or explained in its notes | unowned | F9 | | |

## P5 Later

| ID | Task | Area | Needs | Who | Notes |
|---|---|---|---|---|---|
| L1 | The other platforms' code paths in the same assembly (Switch `NintendoSDK`, Google Play, Xbox sign-out, mobile touch `PinnedUiMobile`, `Toucher`) | unowned | X1 | | |
| L2 | Editor-only and debug code shipped in the build (`ExampleScript`, `ExampleDebugMenu`, `DebugUI`, `TextFps`, `StoreAssetExporter`/`Frame`, `EditorRoutineAttribute`) | unowned | | | Low value; port for completeness |
| L3 | Mod hooks: once X1 runs, a documented way to add modules, palettes and grids | humans | X1 | | |

## Done

| ID | Task | Area | Who | Date | Commit |
|---|---|---|---|---|---|
| F1 | Identify the reference build (GOG, Unity 2020.3.18f1, IL2CPP x86, metadata v27, PDB shipped; `tools/identify.py`) and the decompile pipeline from the player's own install: pinned Il2CppDumper v6.7.46 + Cpp2IL 2022.1.0-pre-release.21 (`tools/fetch_tools.sh`), `tools/decompile.sh` → `work/` (dump.cs, script.json, il2cpp.h, DummyDll, diffable C#, ISIL) | P0 | ed-slopper/wren | 2026-10-15 | 3bbb81c |
| F2 | Index the game's own code: `tools/index.py` → `docs/type-index.csv` (every type of `Assembly-CSharp` and `-firstpass`, owner game/middleware) and `docs/function-index.csv` (2,269 game methods with RVA, VA, signature, status) | P0 | ed-slopper/wren | 2026-10-15 | 3bbb81c |
| F3 | Ghidra project of `GameAssembly.dll` with the shipped PDB (`tools/ghidra_import.sh`, own project in `work/ghidra/`): auto-analysis 1,761 s; `ApplyIl2CppNames.java` put the C# name and il2cpp signature on 57,582 functions (none left `FUN_`), 25,445 metadata labels, 11,492 string literals; `tools/ghidra_decompile.sh <rva>` prints a body (WFC `IterateDecor` 0x3D88D0: 750 lines, fields by name). Notes in `docs/notes/toolchain.md` | P0 | ed-slopper/wren | 2026-10-15 | F3COMMIT |
| F4 | il2cpp's C structs in Ghidra: **nothing to do**, the PDB carries them with their fields (`__this->___decorUpdates_12`, `list->____size_2`), checked on 0x3D88D0. Il2CppDumper's `il2cpp.h` is only needed for builds without the PDB | P0 | ed-slopper/wren | 2026-10-15 | F3COMMIT |

## Changes to the board

- 2026-10-15 ed-slopper/wren: board created (F1–F10, E1–E10, G1–G8, C1–C4, X1–X4, L1–L3).
- 2026-10-15 ed-slopper/wren: F3 done; F4 closed as not needed (the PDB has the structs).
