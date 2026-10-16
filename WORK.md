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

## P1 Core engine

The procedural core, leaf first: maths, the irregular grid, the graph, modules, wave function collapse, meshes.

| ID | Task | Area | Needs | Who | Notes |
|---|---|---|---|---|---|
| E1d | The Unity glue in `Os.Utils`: `UnityFunctions` (+ `HirearchyEnumerator`), `Extensions`, `ListPoolExtension` / `DictPoolExtension`, `ScopedProfiler`, `UnityEvent_Float` / `_Int`: port what game logic needs, the rest as Bevy-side helpers | unowned | E1a, E11 | | Split of E1 |
| E3b | `GridGenerator` coordinate maths: static `HexToPlane` (4) / `PlaneToHex`, `HexToPatchHex` (2), `HexToPatchHexes`, `HexToGridCentroid`, `GetClosestClusterPos`, `GetPatchOffset`, `GetClusterOffset`, `GetStartingHex`, `GetSubdivisions`, `IsBorder`, `IsNewGrid`, `GetVertCost`, `.cctor` | unowned | E3a | ed-slopper/wren (2026-10-16)
| E3c | Patch generation: `GridGenerator.IteratePatch` 0xE30F50 (hex patch → triangulate → random merge into quads → subdivide → sort, `HexPatch.State`), `MotivatePatch` / `UnmotivatePatch`; which RNG and in what order | unowned | E3b | | Split of E3 |
| E3d | Clusters and bits: `IterateCluster`, `IterateBit` 0xE2D6A0 (relaxation, 255 iterations), `Motivate`/`Unmotivate` for clusters and bits, `Measure`, `IterateBits` / `IterateFocus` / `IterateGenerateCenter` / `SceneProcessIteration`, `OnEnable` | unowned | E3c | | Split of E3 |
| E3e | Grid queries and meshes: `GetQuad` (2), `GetVertOrIterate`, `GetClostestVertPosition`, `GetAdjecentQuad`, `AppendQuadList`, `PopulateBorderMesh`, `SetQuadMesh`; gizmos read only | unowned | E3d | | Split of E3 |
| E4 | The graph: `Placemaker.Graphs` (`Graph` 42 methods incl. `AddVoxel` 0xE829E0, `Iterate` 0xE89CF0, `Save` 0xE8F920; `Corner`, `Square`, `Qube`, `Flow`/`FlowData`/`FlowTarget`, `Action`, `ModuleContainer`, `OrientedModuleSides`, `SideCounts`, `ShadowMesh`, `VoxelMesh`), `Voxel`, `VoxelType`, `MaterialType` | unowned | E3e | | [est] Split before claiming |
| E5 | Modules: `ModuleMath` (17), `ByteQube`, `Orientation`, `SideProfile`, `Module`/`BaseModule`/`DecorModule`/`ModuleMesh`, `Placemaker.Modules` (`ModuleLibrary`, `OrientedModule`, `CornerLookup`, `OutlineUv`) | unowned | E4 | | |
| E5a | `Placemaker.Modules.Processing` (`ModuleImporter`, `ProtoModule`, `DecorProfile`, `MaterialCluster`, ...): the editor-time import of module meshes; port only what the shipped game runs | unowned | E5 | | Mostly editor code [est] |
| E6 | `WaveFunctionCollapse` (18 methods incl. `IterateDecor` 0x3D88D0, `FillQubePossibilities` 0x3D6DD0, `RemovePossibleModule` 0x3DA190, the `Propagation` queue and its `CompareTo` 0x4FC880) | unowned | E5 | | Order of the propagation queue decides the result: port exactly |
| E7 | Mesh building: `ModuleBuilder` (16, `Iterate` 0x32FF90), `WorldMeshes` (11), `BigMesh`/`BigMeshGroup`/`BigMeshMaster`/`BigMeshPart`/`BigMeshList`, `BitMeshQuadChange`, `MaterialMaster`, `Palette`/`PaletteHolder` | unowned | E6 | | |
| E8 | Boot and the main loop as Bevy systems in the game's order: `BootMaster`, `BootGameCore` (`Update` 0x7C1FA0), `WorldMaster` (23: `Iterate` 0x1F8C00, `Update` 0x1F9AF0, `LateUpdate` 0x1F91B0, `Load`), `TimeManager`, `FastCoroutines`/`FastCoroutineUpdater`, `LayerMaster`, `UniversalStopwatch`, `ShaderIds`, `MiscReferences` | unowned | E7 | | Who ticks whom each frame (Unity's Awake/Start/Update/LateUpdate order, script execution order from `globalgamemanagers`); scene `level0..2` wiring from F8f |
| E9 | Lighting and ground bakes (GPU passes as Bevy render passes): `AoBaker` (9), `ShadowBaker` (17), `ShadowBakerCamera`, `Sandbanker` (9, `Iterate` 0x84FBA0), `DrawPlane`, `BorderDrawer` | unowned | E7 | | GPU passes: need the shaders (C3) to check |
| E10 | Props: `PropPlacer` (22), `PropPlacement`, `Placemaker.Props` (27 types: `PropNode`, wires and flags, windows, propellers, wind vanes, `PropAchievements`, ...), `Prop` | unowned | E7 | | |
| E11 | `ts_game::unity::transform`: Unity's `Transform` hierarchy math the game reads back (`position`/`rotation`/`lossyScale` from the parent chain, `TransformPoint`, `InverseTransformPoint`, `TransformDirection`, `localToWorldMatrix`, `SetParent` keeping world pose), exact (native code in UnityPlayer.dll), checked with the harness on real scene transforms | unowned | F11b | | Gap of F11f |
| E12 | `ts_game::unity::{camera, physics}`: `Camera.ScreenPointToRay` / `WorldToScreenPoint` / projection matrices and `Physics.Raycast` against `MeshCollider`s (closest hit, triangle test, `RaycastHit` fields) as the engine computes them, for picking blocks (G1) | unowned | F11b | | Gap of F11f; PhysX inside UnityPlayer.dll: read before porting |
| E13 | .NET text formatting and parsing where it reaches files: `int`/`float`/`DateTime` `ToString` and `Parse` (invariant culture, float round-trip rules), `StringBuilder`, string hashing if used for keys; checked with the harness | unowned | F11a | | Gap of F11f; needed by G3/G3a |

## P2 Gameplay systems

| ID | Task | Area | Needs | Who | Notes |
|---|---|---|---|---|---|
| G1 | Building: `Maker` (24), `GroundClicker` (23), `HoverData` (21), `HoverHightlight`, `MasterClicker` (27), `ClickEffect`, `RapidButton`, `VoxelBobEffect`, `VoxelBounceEffect`, `ToolType` | unowned | E4, E8, E12 | | Placing and removing blocks, colours, the click feel |
| G2 | Camera: `OrbitalCamera` (23), `CustomCameraProjection`, `SaveCamera`, `ScreenshotCamera` | unowned | E8 | | |
| G3 | Saving (`ts_game::placemaker::save_system` + the file side in `ts_data`): `SaveSystem` (40), `SaveData`, `MetaSave`, `SettingsData`, `StorageUtils` (19), `SaveCardsHandler` (23), `TexturePngMaster`, `SaveDataScripableObject` | unowned | E4 | | Save files must stay byte-compatible with the original's |
| G3a | The shareable town string: `TextSaveSystem` (`SaveToString` / `LoadFromString`, bit packing `AddValue`/`ReadValue`/`GetBitCount`), `TextSaveAlphabet` | unowned | E4, F9, E13 | | Easiest end-to-end check: a string from the original must load to the same corners/voxels in ours, and back to the same string |
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
| F8a | `ts_data::serialized::SerializedFile`: the whole metadata of a format-22 SerializedFile without type trees (types, object table, script refs, externals, ref types, user info); all nine files of the reference install read to exactly their metadata end, 8,478 objects, none overlapping; layout in `docs/notes/files.md` | P0 | ed-slopper/wren | 2026-10-15 | ba03e8b |
| F8b | `ts_data::objects`: `GameObject`, `Transform`/`RectTransform`, `MonoBehaviour` header, `MonoScript` with Unity 2020.3 layouts, and `AssetSet` (all nine files, `PPtr` resolution across externals, script of a MonoBehaviour). Proven on the install: 1,468 + 381 + 1,087 + 1,109 + 2,612 objects read to their exact size, Transform/GameObject/children/father references agree, every MonoBehaviour's script resolves (one null-script asset noted in `files.md`) | P0 | ed-slopper/wren | 2026-10-15 | 3893498 |
| F8c | `ts_data::mesh::Mesh`: Unity 2020.3 Mesh layout, index decode (u16/u32), vertex stream layout and channel decode (all `VertexFormat`s, half floats), streamed data via `.resS`. Proven on all 114 meshes of the install: exact size, indices < vertex count, sub-meshes inside the index buffer, every position inside the mesh's bounds; notes in `files.md` (the modules are not Unity meshes: see C2) | P0 | ed-slopper/wren | 2026-10-15 | 18ecc85 |
| F8d | `ts_data::texture::Texture2D`: Unity 2020.3 layout, streamed data, mip 0 to RGBA8 for every format the build uses (Alpha8, RGB24, RGBA32, ARGB32, BGRA32, DXT1, DXT5, BC7 via `texture2ddecoder`). Proven on all 36 textures: exact size, data size = complete image size; BC7 and DXT5 checked by eye (`--example dump_textures`, output in `work/`) | P0 | ed-slopper/wren | 2026-10-15 | 39f16ef |
| F8e | `ts_data::audio` (`AudioClip` layout, FSB5 bank parser, Vorbis packets) + `ts_data::vorbis` (headers FMOD strips rebuilt by CRC match with unpatched libvorbis 1.3.4 in the new crate `ts_vorbisref`, vendored from the Xiph releases; decode with `lewton`). All 90 clips decode to exactly their sample count; the five setup headers and their qualities are in `files.md`; `birdsappear` is silent in the build (-97.6 dBFS) | P0 | ed-slopper/wren | 2026-10-15 | c24d4d2 |
| F8f | `ts_data::scene`: a scene file as a GameObject tree with named components (Unity class names, MonoBehaviour script names), `--example dump_scene`. Proven on `level0..2` (34 / 115 / 1,065 GameObjects, each once, every component named); what each scene holds in `files.md` | P0 | ed-slopper/wren | 2026-10-15 | fbbe816 |
| F8g | `ts_data::layout`: the game's MonoBehaviour / ScriptableObject fields read by generated layouts (`data/layouts.json`, names and types only, from `tools/layouts.ps1` over Cpp2IL's attributed DLLs + `dump.cs` fallback) and Unity 2020.3's rules (enums 4 bytes, align after small fields and arrays, built-in structs). **All 2,611 MonoBehaviours with a script read to their exact size**; `--example dump_script`; rules and steps in `files.md` | P0 | ed-slopper/wren | 2026-10-15 | 6c4f1e6 |
| F9a | `townscaper-rs/harness` (32-bit, own workspace): `ts_harness_launch` copies the install to `work/harness/game` once (install untouched), starts it windowed and loads `ts_harness.dll` with `LoadLibraryW` on a remote thread; the DLL finds the il2cpp C API, attaches and logs. Run: IL2CPP up 0.3 s after start, 56 assemblies, `Placemaker.TextSaveSystem` found with its 10 methods | P0 | ed-slopper/wren | 2026-10-15 | 08a4f03 |
| F9b | Call server in `ts_harness.dll`: JSON lines on 127.0.0.1:47615 (protocol in `harness/src/server.rs`): static or instance (`FindObjectOfType`, inactive too) calls by `Namespace.Class.Method`, overloads by parameter types, value types as raw bytes, strings, value-type arrays both ways, managed exceptions as text, `main: true` runs on Unity's main thread via a `WH_GETMESSAGE` hook. Checked: `TextSaveSystem.GetBitCount(255)` = 8, `Mathf.RoundToInt(2.5)` = 2, `Random.Range` on the main thread, `Application.version` = 1.02, `WorldMaster` found 1.3 s after start, `BitConverter` byte arrays in and out, `FormatException` reported | P0 | ed-slopper/wren | 2026-10-15 | 5954b66 |
| F9c | `townscaper-rs/check` (`ts_check`): harness client for tests (starts the copy if needed, `TS_HARNESS=1` to run, arg/ret helpers); first exact comparison `ts_game::unity::mathf::round_to_int` vs `Mathf.RoundToInt` 0xA3C0A0: 3,317 inputs equal, specials and out-of-range included (`ts_game::unity::conv`: x86 truncating converts). Notes in `toolchain.md` | P0 | ed-slopper/wren | 2026-10-15 | 44f5e1b |
| F10 | `tools/guard.py` (tracked files: forbidden names, executables, SerializedFiles, FSB5 banks, size, decompiler-output markers in source/data files; `function-index.csv` columns/status/RVAs/duplicates; every `` `Placemaker.*`/`Os.*` `` address ref in `townscaper-rs` is a real row) and `.github/workflows/ci.yml` (guard + `cargo test` of the Bevy-free crates on every push). Guard proven to catch planted pseudo-C and a sound bank | P0 | ed-slopper/wren | 2026-10-15 | c51b525 |
| F11a | `ts_game::unity::mathf`: all 22 `Mathf` overloads the game calls, equal to the game on 41,360 harness calls (float 26,886, int/array 8,150, `PerlinNoise` 6,324), NaN/inf/out-of-range included. Findings (C# semantics over Ghidra's rendering, x87 final roundings, native `PerlinNoise` read from UnityPlayer.dll with its PDB) in `docs/notes/unity.md` | P0 | ed-slopper/wren | 2026-10-15 | 4546e08 |
| F11b | `ts_game::unity::{vector, quaternion, geometry}`: every `Vector2/3/4`, `Vector2Int`, `Quaternion`, `Matrix4x4`, `Bounds`, `Rect`, `Plane`, `Ray`, `Color`, `Color32` member the game calls, managed ones from Unity's C#, native ones (`LookRotation`, `Lerp`, `Inverse`, `inverse`, `TRS`, `LookAt`, `lossyScale`) read from UnityPlayer.dll; equal to the game in 6 harness test groups (~70,000 calls, NaN/axes/huge included). Harness learnt value-type `this`, `out` args, constructors. Notes in `unity.md` | P0 | ed-slopper/wren | 2026-10-15 | 3ad6d9d |
| F11c | `ts_game::unity::random::Rand`: the engine's xorshift128 (`InitState`, `value`, `Range` float/int, `insideUnitSphere` with its unit vector) read from UnityPlayer.dll; equal to the game on 7,200 draws + 300 seeds with the state compared each round. Harness learnt `native` (engine functions by PDB address) and `batch` (several calls in one main-thread job, no frame between). The game never seeds `Random` (cosmetic uses only): notes in `unity.md` | P0 | ed-slopper/wren | 2026-10-15 | 4766afb |
| F11d | `ts_game::unity::mathematics`: the out-of-line `Unity.Mathematics` members the game calls (`length`/`normalize(float2)`, int `abs`/`max`/`mul`, `float4x4(int4x4)`, the shuffle-based `math.inverse(float4x4)`, `int3.GetHashCode`) and `Unity.Mathematics.Random` (xorshift32: ctor, `NextFloat` both forms, `NextInt`), equal to the game (1,500 rounds + 1,000 seeds × 6 steps); notes in `unity.md` | P0 | ed-slopper/wren | 2026-10-15 | 8fe48ea |
| F11e | `ts_game::unity::collections`: the BCL's introsort (`sort_by`, depth limit from the backing array's length), `binary_search`, `list_grow` (List capacity), `SlotMap` (Dictionary / HashSet slot-order iteration with LIFO slot reuse). Equal to the game: 514 tie-heavy sorts incl. 4 through heapsort (antiqsort inputs), binary searches with duplicates, 60 Dictionary/HashSet op sequences. Harness learnt generic classes (`Type.GetType` names), object handles (`new`, `keep`), arrays returned after calls; notes in `unity.md` | P0 | ed-slopper/wren | 2026-10-15 | c25105f |
| F11f | Host-side Unity / .NET APIs surveyed (Cpp2IL `[Calls]` from the game's code) and decided per family: Bevy hosts display, audio, timing, UI, mesh upload; exact ports for what the game reads back: new rows E11 (Transform math), E12 (camera projection + `Physics.Raycast` picking), E13 (.NET string/number formatting in saves). Table in `docs/notes/unity.md` | P0 | ed-slopper/wren | 2026-10-15 | 5648cdd |
| F7 | Middleware decided by ed-slopper (2026-10-15: "accept the proposal on the board"): Rewired → Bevy input under a port of the game's `ActionInputManager` / `ControlBinding` layer; I2 Localization → a port of the parts the game uses, reading the player's `LanguageSourceData`; Crosstales / StandaloneFileBrowser → `rfd`; Fury SDK (platform, achievements) → no-op STANDINs. In `docs/notes/unity.md` | humans | ed-slopper | 2026-10-15 | c2f22b1 |
| E1a | `ts_game::os::utils`: `Bitwise`, `ByteFloat`, `ByteFloat2`, `ByteFloat3`, `SbyteFloat3` (61 rows `checked` against the game in `tests/os_utils.rs`, the 4 boxed `Equals(object)` `ported`). Found by the harness: `SbyteFloat3 * int3x3` dots each column with the vector (the transpose of `math.mul`); `FloatToByte` clamps only the top (negatives wrap); SbyteFloat3's scale is 254 and its `normal` has no −1. `tools/mark.py` sets index statuses | P1 | ed-slopper/wren | 2026-10-15 | 0fb2e05 |
| E1b | `ts_game::os::extra_math`: all of `ExtraMath` (42 rows `checked` against the game in `tests/os_extra_math.rs`, 1,200 rounds; `BinarySearchTransformX` `ported` on child x positions, its harness check waits for E11). Found: `Cross2` rounds once from x87 precision; huge trig arguments differ between the game's Intel libm and the CRT (outside the game's range; `unity.md`) | P1 | ed-slopper/wren | 2026-10-16 | d1f62c4 |
| E1c | `ts_game::os::intersections`: `Intersections` (`BoxBox`, the oriented box/box and box/triangle separating-axis tests `PropPlacer` uses, Möller–Trumbore `TriangleRay`, plane-based `TriangleTriangle`) and `NumberStrings`; all 10 rows `checked` (1,500 random near-touching cases per test with both answers frequent, 300 numbers) | P1 | ed-slopper/wren | 2026-10-16 | 35d327e |
| E2 | `ts_game::os::hex`: `HexMath` (cube/plane/world conversions, rotations, `HexRound`, `HexClamp` with the original's corner bug, every static axis and matrix bit for bit) and `HexOnionIterator` (the ring walk: `GetAt`, `Increment`/`Decrement`, a 1,500-step random walk); all 35 rows `checked`; the two editor gizmos `read`, not ported. Harness gained `static_field` | P1 | ed-slopper/wren | 2026-10-16 | 66f0f22 |
| E3a | `ts_game::placemaker::quads` (`Quad`, `Vert`: byte for byte, `AddQuad`'s angle sort is `atan2(d.x, d.y)`), `grid_generation` (`GridConstants`, `MotivationCounter`, `HexPatch`, `HexCluster`, `CustomGridPatch`, `CustomGridLibrary.GetRandom`), `GridBit`; `ts_data::grid` reads the 40-patch library asset. 19 rows `checked`, 15 `ported` (records, empty enumerator), `Quad.ToString` waits on E13 | P1 | ed-slopper/wren | 2026-10-16 | 4f54363 |

## Changes to the board

- 2026-10-15 ed-slopper/wren: board created (F1–F10, E1–E10, G1–G8, C1–C4, X1–X4, L1–L3).
- 2026-10-15 ed-slopper/wren: F3 done; F4 closed as not needed (the PDB has the structs).
- 2026-10-15 ed-slopper/wren: target changed to a Rust + Bevy rewrite (asked by ed-slopper): F6 is now the Cargo workspace, F7/F8/F9/F10 reworded for Rust, F11 (Unity semantics) and C5 (sounds) added, C1–C4 and X1–X4 reworded.
- 2026-10-15 ed-slopper/wren: F6 done.
- 2026-10-15 ed-slopper/wren: F5 done.
- 2026-10-15 ed-slopper/wren: split F8 into F8a–F8g (no type trees in the build: we supply the layouts).
- 2026-10-15 ed-slopper/wren: F8a done.
- 2026-10-15 ed-slopper/wren: F8b done.
- 2026-10-15 ed-slopper/wren: F8c done.
- 2026-10-15 ed-slopper/wren: F8d done.
- 2026-10-15 ed-slopper/wren: F8e done; new crate `ts_vorbisref` (vendored BSD libvorbis 1.3.4 + libogg 1.3.5) added to the workspace.
- 2026-10-15 ed-slopper/wren: F8f done.
- 2026-10-15 ed-slopper/wren: F8g done (F8 complete).
- 2026-10-15 ed-slopper/wren: split F9 into F9a–F9c (Rust + il2cpp C API, no C# loader); C1/C3/C5 Needs `F8` = F8a–F8g, all done.
- 2026-10-15 ed-slopper/wren: F9a done.
- 2026-10-15 ed-slopper/wren: F9b done.
- 2026-10-15 ed-slopper/wren: F9c done (F9 complete).
- 2026-10-15 ed-slopper/wren: F10 done.
- 2026-10-15 ed-slopper/wren: split F11 into F11a–F11f from a survey of the game's calls into Unity/.NET.
- 2026-10-15 ed-slopper/wren: F11a done.
- 2026-10-15 ed-slopper/wren: F11b done.
- 2026-10-15 ed-slopper/wren: F11c done.
- 2026-10-15 ed-slopper/wren: F11d done.
- 2026-10-15 ed-slopper/wren: F11e done.
- 2026-10-15 ed-slopper/wren: F11f done (F11 complete); added E11, E12, E13 (gaps of F11f); G1 Needs E12, G3a Needs E13.
- 2026-10-15 ed-slopper/wren: F7 decided by ed-slopper (proposal accepted); P0 complete.
- 2026-10-15 ed-slopper/wren: split E1 into E1a–E1d; E2 Needs E1a, E1b.
- 2026-10-15 ed-slopper/wren: E1a done.
- 2026-10-16 ed-slopper/wren: E1b done.
- 2026-10-16 ed-slopper/wren: E1c done.
- 2026-10-16 ed-slopper/wren: E2 done.
- 2026-10-16 ed-slopper/wren: split E3 into E3a–E3e; claim E3a.
- 2026-10-16 ed-slopper/wren: E3a done.
- 2026-10-16 ed-slopper/wren: claim E3b.
