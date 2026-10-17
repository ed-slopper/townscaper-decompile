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
