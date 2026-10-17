# Boot and the main loop (E8)

## Timing and statics (E8a, proven)

Port: `townscaper-rs/game/src/placemaker/timing.rs`; wall-clock time comes from a `Clock`.

- `UniversalStopwatch`: `StartCounting` 0x3CD020 restarts one shared stopwatch at most once per frame
  (`Time.frameCount` against `lastCountFrame`; true when it did). `KeepGoing` 0x3CCFB0 is "less than **8 ms**
  since then"; `WorldMaster.KeepGoing` is the same function. Every `Iterate` loop of the game checks it, so how
  much work a frame does depends on the machine; the work itself does not.
- `TimeManager`: `Time.timeScale = (slowmo ? 0.25 : 1) × dimScale`, applied by every setter. `SetDimScale` and
  `SetRecordingScale` write the same field as `set_dimScale` (`.cctor`: 1, no slow motion).
- `FastCoroutines`: `FastIterate(ms)` (16 ms each frame from `FastCoroutineUpdater.Update`) restarts its
  stopwatch and runs the queue's first item while the time is not past `ms`: an enumerator steps until it ends
  (then it is removed) or until `ms`; an action is invoked — and never removed (queued actions repeat until the
  frame's time is up; kept). `FastCoroutine(enumerator, ms)` (default 6 ms) is a Unity coroutine: each frame it
  steps the enumerator, restarting the stopwatch before every step, until it ends, yields false, or
  `KeepGoing` (`staticMilliseconds`) fails.
- `LayerMaster`: Default 0, Voxels 8, Ground 9, Props 10, Grid 11, LastClick 13, Ao 14, AoStack 16,
  OnlyShadows 17, NoShadows 18 (the project's layer table); `clickMask` = Voxels | LastClick.
- `ShaderIds`: `_Color`, `_Rect`, `_AaLineBoth`, `_Dim`, `_ShadingTexLightStack` (engine ids; names here).

Checked (`tests/placemaker_timing.rs`): the layer statics, the shader ids against `Shader.PropertyToID`, 200
`TimeManager` changes against `Time.timeScale` (restored after), `StartCounting` twice per frame for 20 frames;
`FastCoroutines` on a fake clock.

## The world master's loop (E8b, proven)

Port: `placemaker::world_master`; every system, ported or not, is reached through `WorldSystems`.

`Update` 0x1F9AF0 each frame: `UniversalStopwatch.StartCounting`; the smoothing factors `expTSlow / Mid / Fast /
Faster = (float)exp(-unscaledDeltaTime × 5 / 11 / 17 / 21)`; from `NormalStart` on, the collapse's `LaterUpdate`,
the town saved when quiet (`lastChangeTime > 0` and 2 s past it and 10 s past the first change, or 15 s past the
first change; `SaveSystem.MaybeSaveCurrent`) and the settings every 10 s; then **`while (KeepGoing) if
(!Iterate()) break;`** (the 8 ms budget, each step inside a profiler marker named after the state); then, from
`NormalStart` on, `LaterUpdate` again, and the bird and butterfly flocks and the sounds update. `LateUpdate`
0x1F91B0: the prop mesh animator, then the big meshes (`BigMeshMaster.OnLateUpdate`).

`Iterate` 0x1F8C00 (`WorldMaster.State`, a byte; the jump table at 0x101F90AC): a loop state calls its system
and stays while it reports work (true), else moves to the next state; a one-shot state acts and moves on; every
step returns true except `Done`.

| State | Does |
|---|---|
| 0 DoNothing | stays |
| 1 StartLoad, 40 SaveToUrl | next |
| 2 PreLoad | sandbanker `Clear`, flocks `OnReset`, prop placer `OnReset`, shadow baker `Load(saveData)`, maker `ClearUndoQueue` |
| 3, 11, 36 | module builder `IterateDeconstructQubes` (loop) |
| 4, 12, 37 | prop placer `IterateDeconstructProps` (loop) |
| 5 | prop mesh animator `OnReset` |
| 6 AwaitDim | stays while `awaitingDim` |
| 7 / 8 | graph `BeginLoad(saveData)` / `IterateLoad(saveData)` (loop) |
| 9, 20 | graph `IterateFocus` (loop) |
| 10 LoadOthers | `onCameraLoad(saveData)` |
| 13, 21 | grid `IterateFocus` (loop) |
| 14, 22 | graph `Iterate` (loop) |
| 15 | grid `IterateGenerateCenter` (loop) |
| 16, 24 / 17 | collapse `Iterate0` / `IterateApplyModules` (loops) |
| 18 LoadBakeShadows0 | shadow `SetFramingDirty` + `BakeShadow`, then **jumps to 25** (skips the normal start) |
| 19 NormalStart | shadow `OnBuilt` |
| 23 | shadow `SetFramingDirty` + `BakeShadow` |
| 26, 28, 34 | shadow `BakeShadow` |
| 25 / 27 / 29 | collapse `Iterate1` / `IterateDecor` (loops) / `ResetPropagationCounter` |
| 30 / 31 / 32 | sandbanker `Iterate` / graph `IterateRefillPools` / module builder `Iterate` (props) (loops) |
| 33 | prop achievements `TriggerAchievements` (if any) |
| 35 | bird flock `IterateBirdCreation` (loop) |
| 38 | prop placer `OnDoneResetting` |
| 39 | world meshes `IterateRefillPools` (loop) |
| 41 / 42 | grid `IterateBits(KeepGoing)` / save system `IterateRefresh(KeepGoing)` (loops) |
| 43 Done | returns false |
| > 43 | stays |

`ResetState` (after any change) goes back to 19; `OnOnEnable` restarts a finished master at 19 (it also names
the states and calls `IOnOnEnable.OnOnEnable` on every component under the master). `SetDirty`: the first change
since the last save sets `lastStartChangeTime`, every change `lastChangeTime` (unscaled time).

Checked (`tests/placemaker_world_master.rs`): the graph tests' stand-in master, with the scene's shadow baker,
flocks, props, save system and sounds, walked by the game's `Iterate` from `StartLoad` (an empty save) and from
`NormalStart` to `Done` (86 steps, every state 1–43 visited); our machine replayed each step to the same state
and result. Also `DoNothing`, `Done`, states past the table, `AwaitDim`, `ResetState`, `SetDirty`. `Update` and
its save timing on a fake world (the game's own `Update` would drive and save the scene).

## Saving, loading, new towns (E8c, proven)

- `New(SaveData)` 0x1F94C0 / `New()` 0x1F93E0: the save cleared, one empty corner at
  `GridGenerator.GetStartingHex(Random.Range(0, 40))` moved by `Random.Range(-12, 12)` in x, then in y (draws in
  that order). Checked on 200 seeded engine random states.
- `Load(SaveData)` 0x1F91F0: the save copied into the master's (`ReadNonImageDataFrom`, unless it is the same),
  `state = StartLoad`, `anyChange = false`, `lastChangeTime = -1`. `Load(string)` 0x1F9240: the master's save
  cleared and read with `TextSaveSystem.LoadFromString`; false and nothing else when it fails, else the same
  start. `NothingToLoadAtStartup`: an empty save to the camera and the shadow baker, then `BeginLoadGraph`.
- `GetSaveString`: the graph saved into the cleared save, `TextSaveSystem.SaveToString` (also `lastSaveString`).
  `PopulateSaveData(save, meta)`: camera, shadow baker, save camera and graph into the save, `lastWriteTime =
  DateTime.UtcNow` ticks, the text (`metaSave` unused).
- `MaybeUpdateGameVersionInSettingsData`: a new `Application.version` is logged ("Updating
  settingsData.lastRunVersion to …"), stored and the settings marked dirty. `shouldBeLoading` = state < 19.
- The text format and the rest of `SaveData` are G3 / G3a (behind `WorldSaves`).

## Boot (E8d, ported)

`placemaker::boot`. The boot scene's `BootMaster.Start` runs `FirstBootWrapper` as a fast coroutine (6 ms a
frame); every routine is an `IEnumerator<bool>` (a `true` lets the coroutine go on in the same frame, a `false`
waits for the next one).

- `GameBootRoutine`: `BootMaster.instance`, yield false; load "Placemaker" (the town) and "FlatscreenUi"
  additively unless loaded; one false a frame until each load is done; the `UiMaster` from the UI scene's roots
  (the last root first, the first `GetComponentInChildren` hit); a false a frame until the town scene is loaded;
  the `WorldMaster` the same way; the two linked (`worldMaster.uiMaster`, `uiMaster.worldMaster`), yield true;
  UI object on, the boot scene's audio listener off, `UiMaster.MaybeSetup`, true; world master disabled, object
  on, `OnOnEnable`, true; `OnStart` (all systems, E8b's order), true; `enabled = true`, true; end.
- `UserBootRoutine`: `SaveSystem.ResetBoot`, then `IterateBoot` once a frame while it says more; vsync,
  antialiasing, `onSettingsLoaded`, the version check (inlined `MaybeUpdateGameVersionInSettingsData`, a new
  version also `TexturePngMaster.ResetColorAndMaterial`), `SaveMenu.ResetSaveCardOrder`; then by
  `SaveSystem.bootState`: 8 loads `currentMetaSave` (inlined `Load`), `awaitingDim = false`,
  `StartFinishLoading(false)`; 10 only `StartFinishLoading(false)`; 9 clears the save, makes a new town (inlined
  `New`, `UnityEngine.Random`), `Load`, `LoadingUi.StopLoadingNow`, `SaveMenu.OpenWithoutRefresh`,
  `awaitingDim = false`; anything else nothing.
- `FirstBootWrapper`: `LoadingUi.StartLoading`, the game boot passed through, and the user boot started in the
  step the game boot ends. `SignedOutBootWrapper` (`OnSignedOut`): `CloseEverything`, `ResetUsersUIChoises`,
  `BootGameCore.StartNewSignIn`, `ShowLogo`, `Dim.SetState(2)`, then a false a frame until the core is signed in
  (state 2), `StartLoading` and the user boot.
- `OnEnable`: the instance, `UiMaster.MaybeSetup`, `QualitySettings.antiAliasing =
  UiScaler.GetDefaultAntiAliasing()`, `_AaLineBoth = (1, s, s, s)` with `s = Clamp(GetScale() * 0.7f, 1, 2)`
  (float multiply), `WorldMaster.OnOnEnable`. `DoFullscreen`: on `WindowsPlayer` (2) `SetResolution(int.Max,
  int.Max, true)`, else the last listed resolution, or `fullScreen = true`.
- `BootGameCore` (console sign-in, FurySDK + Rewired): `StartNewSignIn` → state 1 with the prompt on. In state 1
  with an SDK and no player, the first player with `GetAnyButtonDown` is asked to sign in, mode 3 the first time
  (a static flag, `.cctor` true) unless a sign-in failed, else 2. `OnComplete`: status 2 stores the SDK user's id
  in `PlatformManager` and signs in (state 2, prompt off); status 4 remembers the failure; the request is
  cleared either way. In state 2 a different or missing user logs "Signed Out!", state 3, and starts
  `SignedOutBootWrapper`.

Checked: the boot has already run in the game the harness talks to, so the routines are checked for their step
order on a recording host (`tests/placemaker_boot.rs`); `_AaLineBoth` against the running game's
`UiScaler.GetScale`.
