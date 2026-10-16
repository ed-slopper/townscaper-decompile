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
whole build with PDB names, C# names and field names (F3). The target is a Rust rewrite on Bevy: the `townscaper-rs/` workspace builds and opens a window,
checks the player's install is the reference build and reads Unity file headers (F6); no game code is ported yet. There is no asset reader (F8) and no checking harness (F9) yet.

## P0 Foundations

| ID | Task | Area | Needs | Who | Notes |
|---|---|---|---|---|---|
| F7 | What replaces the middleware the game calls: Rewired (input) → Bevy input + our port of `ActionInputManager`/`ControlBinding`; I2 Localization → port of the parts the game uses, reading the player's `LanguageSourceData`; Crosstales / StandaloneFileBrowser (file dialogs) → `rfd` or ours; Fury SDK (platform, achievements) → no-op STANDINs. Confirm or change | humans | | | Proposal; blocks G7d, G8, C4 |
| F8a | `ts_data::serialized`: the whole metadata of a SerializedFile (format 22, no type trees): types (class id, script type index, script id hash), object table (path id, offset, size, type), script refs, externals; every reference file loads and every object's bytes lie inside the file | unowned | F6 | | Split of F8 |
| F8b | Built-in objects needed to see the scenes: `GameObject` (components, layer, name, active), `Transform` (local rotation/position/scale, children, father), `MonoBehaviour` header (game object, enabled, script PPtr, name), `MonoScript` (class, namespace, assembly), with Unity 2020.3.18f1's field layouts; `PPtr` across files via externals | unowned | F8a | | Split of F8 |
| F8c | `Mesh` (vertex channels and streams, index buffer, sub-meshes, bounds) | unowned | F8b | | Split of F8 |
| F8d | `Texture2D` (format, mips, image data in `.resS`), decoded for the formats the game uses | unowned | F8b | | Split of F8 |
| F8e | `AudioClip` (FSB5 in `.resource`) → samples (Vorbis) | unowned | F8b | | Split of F8 |
| F8f | Scenes: `level0..2` as a GameObject tree with components; a dump tool printing it (names only) | unowned | F8b | | Split of F8 |
| F8g | The game's MonoBehaviour/ScriptableObject bodies: their serialized field layouts generated from `dump.cs` by Unity's serialization rules (public or `[SerializeField]`, not static/const/readonly, serializable types, 4-byte alignment after byte/bool/arrays), then read by name | unowned | F8b | | Split of F8 |
| F9 | Checking harness: run code inside the original game and compare with ours. MelonLoader or BepInEx 6 (IL2CPP) on the player's copy, a plugin that calls a game method with given inputs and writes the result as JSON, and Rust tests in `townscaper-rs/game/tests/` that feed the same inputs to our port; exact compare | any | F6 | | [est] Split: F9a (loader + hello world in the original), F9b (call a method by name, dump the result), F9c (Rust compare runner) |
| F10 | `tools/guard.py` + CI: fail if a game file or decompiler output is committed (extensions, `dump.cs`, `il2cpp.h`, size/magic checks), `function-index.csv` well-formed, every `` `Ns.Type.Method` 0x… `` doc comment in `townscaper-rs/` names a real row; `cargo test -p ts_game -p ts_data` on push | any | F2, F6 | | |
| F11 | `ts_game::unity`: the Unity and .NET semantics the game's results depend on, exact: `Mathf` (Unity's own `Approximately`, `Repeat`, `PingPong`, `SmoothDamp`, `RoundToInt` banker's rounding, ...), `Vector2/3/4`, `Quaternion` (Unity's `Euler`, `LookRotation`, `Slerp`), `Matrix4x4`, `UnityEngine.Random` (Xorshift128, `InitState`, `Range` int/float rules), `System.Random` if used, `Unity.Mathematics` (`float2/3`, `math.*`), `List<T>.Sort` (introsort: unstable, the game's order depends on it), `Dictionary`/`HashSet` iteration order where the game iterates them | unowned | F6 | | [est] Port only what the game calls: find the calls in `function-index.csv` / Cpp2IL `[Calls]`. Split before claiming |

## P1 Core engine

The procedural core, leaf first: maths, the irregular grid, the graph, modules, wave function collapse, meshes.

| ID | Task | Area | Needs | Who | Notes |
|---|---|---|---|---|---|
| E1 | `ts_game::os::utils` = `Os.Utils`: `ExtraMath` (56 methods), `Intersections`, `Bitwise`, `ByteFloat`/`ByteFloat2`/`ByteFloat3`/`SbyteFloat3`, `NumberStrings`, list/dict pool extensions, `UnityFunctions`, `ScopedProfiler` | unowned | F6, F11 | | Leaf code, good first port; everything else calls it |
| E2 | `ts_game::os::hex` = `Os.Hex`: `HexMath` (21), `HexOnionIterator` (14, the ring-by-ring walk), visualiser gizmos | unowned | E1 | | |
| E3 | The irregular grid: `GridGenerator` (45 methods: hex patch → random merge of triangles into quads → subdivision → relaxation), `Placemaker.Quads` (`Quad`, `Vert`), `Placemaker.Quads.GridGeneration` (`HexPatch`, `HexCluster`, `CustomGridPatch`/`Importer`/`Library`, `MotivationCounter`, `GridConstants`), `GridBit` | unowned | E2 | | [est] The heart of the look; seed and RNG order must match (`UnityEngine.Random` vs `System.Random`: find which). Split before claiming |
| E4 | The graph: `Placemaker.Graphs` (`Graph` 42 methods incl. `AddVoxel` 0xE829E0, `Iterate` 0xE89CF0, `Save` 0xE8F920; `Corner`, `Square`, `Qube`, `Flow`/`FlowData`/`FlowTarget`, `Action`, `ModuleContainer`, `OrientedModuleSides`, `SideCounts`, `ShadowMesh`, `VoxelMesh`), `Voxel`, `VoxelType`, `MaterialType` | unowned | E3 | | [est] Split before claiming |
| E5 | Modules: `ModuleMath` (17), `ByteQube`, `Orientation`, `SideProfile`, `Module`/`BaseModule`/`DecorModule`/`ModuleMesh`, `Placemaker.Modules` (`ModuleLibrary`, `OrientedModule`, `CornerLookup`, `OutlineUv`) | unowned | E4 | | |
| E5a | `Placemaker.Modules.Processing` (`ModuleImporter`, `ProtoModule`, `DecorProfile`, `MaterialCluster`, ...): the editor-time import of module meshes; port only what the shipped game runs | unowned | E5 | | Mostly editor code [est] |
| E6 | `WaveFunctionCollapse` (18 methods incl. `IterateDecor` 0x3D88D0, `FillQubePossibilities` 0x3D6DD0, `RemovePossibleModule` 0x3DA190, the `Propagation` queue and its `CompareTo` 0x4FC880) | unowned | E5 | | Order of the propagation queue decides the result: port exactly |
| E7 | Mesh building: `ModuleBuilder` (16, `Iterate` 0x32FF90), `WorldMeshes` (11), `BigMesh`/`BigMeshGroup`/`BigMeshMaster`/`BigMeshPart`/`BigMeshList`, `BitMeshQuadChange`, `MaterialMaster`, `Palette`/`PaletteHolder` | unowned | E6 | | |
| E8 | Boot and the main loop as Bevy systems in the game's order: `BootMaster`, `BootGameCore` (`Update` 0x7C1FA0), `WorldMaster` (23: `Iterate` 0x1F8C00, `Update` 0x1F9AF0, `LateUpdate` 0x1F91B0, `Load`), `TimeManager`, `FastCoroutines`/`FastCoroutineUpdater`, `LayerMaster`, `UniversalStopwatch`, `ShaderIds`, `MiscReferences` | unowned | E7 | | Who ticks whom each frame (Unity's Awake/Start/Update/LateUpdate order, script execution order from `globalgamemanagers`); scene `level0..2` wiring from F8f |
| E9 | Lighting and ground bakes (GPU passes as Bevy render passes): `AoBaker` (9), `ShadowBaker` (17), `ShadowBakerCamera`, `Sandbanker` (9, `Iterate` 0x84FBA0), `DrawPlane`, `BorderDrawer` | unowned | E7 | | GPU passes: need the shaders (C3) to check |
| E10 | Props: `PropPlacer` (22), `PropPlacement`, `Placemaker.Props` (27 types: `PropNode`, wires and flags, windows, propellers, wind vanes, `PropAchievements`, ...), `Prop` | unowned | E7 | | |

## P2 Gameplay systems

| ID | Task | Area | Needs | Who | Notes |
|---|---|---|---|---|---|
| G1 | Building: `Maker` (24), `GroundClicker` (23), `HoverData` (21), `HoverHightlight`, `MasterClicker` (27), `ClickEffect`, `RapidButton`, `VoxelBobEffect`, `VoxelBounceEffect`, `ToolType` | unowned | E4, E8 | | Placing and removing blocks, colours, the click feel |
| G2 | Camera: `OrbitalCamera` (23), `CustomCameraProjection`, `SaveCamera`, `ScreenshotCamera` | unowned | E8 | | |
| G3 | Saving (`ts_game::placemaker::save_system` + the file side in `ts_data`): `SaveSystem` (40), `SaveData`, `MetaSave`, `SettingsData`, `StorageUtils` (19), `SaveCardsHandler` (23), `TexturePngMaster`, `SaveDataScripableObject` | unowned | E4 | | Save files must stay byte-compatible with the original's |
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
| C1 | Scenes `level0`, `level1`, `level2`: what each holds, which components on which objects, serialized values (via F8f); written up in `docs/notes/scenes.md` (names and values only), and built by our app from the player's files | unowned | F8 | | |
| C2 | The module library: the `ModuleLibrary` asset (modules, meshes, sides, profiles, decor) read by `ts_data` into `ts_game::placemaker::modules` types | unowned | F8, E5 | | |
| C3 | Shaders: the game's shaders (water, outlines, AO/shadow spreaders, marching squares, ground, sky) read from their compiled form (DXBC in the player's assets) and rewritten as WGSL materials in the app | unowned | F8 | | [est] Biggest content row. Split before claiming |
| C4 | Palettes, custom grids (`CustomGridLibrary`), credits and localisation tables (I2 `LanguageSourceData`) read from the player's assets | unowned | F8, F7 | | |
| C5 | Sounds and music from the player's `.resource` files (FSB5 banks) played through Bevy audio | unowned | F8 | | |

## P4 Finishing

| ID | Task | Area | Needs | Who | Notes |
|---|---|---|---|---|---|
| X1 | Our build runs end to end from the player's files: boot, build a town, save, load, screenshot | unowned | E8, G1, G3, G7a | | |
| X2 | Save compatibility: a save folder from the original loads in ours and back, byte for byte | unowned | G3, G3a, F9 | | |
| X3 | Empty `grep -rn STANDIN townscaper-rs` (except the middleware replacements decided in F7) | unowned | X1 | | |
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
| F3 | Ghidra project of `GameAssembly.dll` with the shipped PDB (`tools/ghidra_import.sh`, own project in `work/ghidra/`): auto-analysis 1,761 s; `ApplyIl2CppNames.java` put the C# name and il2cpp signature on 57,582 functions (none left `FUN_`), 25,445 metadata labels, 11,492 string literals; `tools/ghidra_decompile.sh <rva>` prints a body (WFC `IterateDecor` 0x3D88D0: 750 lines, fields by name). Notes in `docs/notes/toolchain.md` | P0 | ed-slopper/wren | 2026-10-15 | 7184754 |
| F4 | il2cpp's C structs in Ghidra: **nothing to do**, the PDB carries them with their fields (`__this->___decorUpdates_12`, `list->____size_2`), checked on 0x3D88D0. Il2CppDumper's `il2cpp.h` is only needed for builds without the PDB | P0 | ed-slopper/wren | 2026-10-15 | 7184754 |
| F5 | `ghidra/scripts/ExportDecomp.java` + `tools/ghidra_export.sh`: the pseudo-C of every game method in `docs/function-index.csv` to `work/decomp/<Namespace>/<Type>/<Method>_<rva>.c` (gitignored), with the C# signature on top: 2,269 written, 0 failed, 448 share a body with an earlier row (identical-code folding), 5 min, 82 MB | P0 | ed-slopper/wren | 2026-10-15 | afc6bc1 |
| F6 | `townscaper-rs/` Cargo workspace: app `townscaper` on Bevy 0.20 (window titled with the build check, STANDIN sky colour and fixed camera), `ts_game` (module skeleton `os::{utils, hex}`, `placemaker`, `unity`) and `ts_data` without Bevy; `ts_data::install` finds and identifies the install (Unity version, metadata v27, both md5s), `ts_data::serialized::Header` reads the SerializedFile header (formats 9–22). Tests: header unit tests + every `.assets`/`level*`/`globalgamemanagers` of the reference install parses with its exact file size. First app build 12 min (MSVC); the window opens (RTX 3060, no errors) | P0 | ed-slopper/wren | 2026-10-15 | d09981a |

## Changes to the board

- 2026-10-15 ed-slopper/wren: board created (F1–F10, E1–E10, G1–G8, C1–C4, X1–X4, L1–L3).
- 2026-10-15 ed-slopper/wren: F3 done; F4 closed as not needed (the PDB has the structs).
- 2026-10-15 ed-slopper/wren: target changed to a Rust + Bevy rewrite (asked by ed-slopper): F6 is now the Cargo workspace, F7/F8/F9/F10 reworded for Rust, F11 (Unity semantics) and C5 (sounds) added, C1–C4 and X1–X4 reworded.
- 2026-10-15 ed-slopper/wren: F6 done.
- 2026-10-15 ed-slopper/wren: F5 done.
- 2026-10-15 ed-slopper/wren: split F8 into F8a–F8g (no type trees in the build: we supply the layouts).
