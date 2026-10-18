# Building (G1)

Placing and removing blocks: what the pointer is over (`HoverData`), the actions (`Maker`), the input that drives
them (`GroundClicker`, `MasterClicker`) and the feedback (`HoverHightlight`, click and voxel effects).

## Picking: `HoverData` (G1a, proven)

`placemaker::hover_data`, checked against a fresh game HoverData on the graph tests' stand-in town
(`tests/placemaker_hover_data.rs`: 300 hovers through the scene camera, half aimed at voxels, every field;
the hit point and normal through row E12's stand-in hull planes, bit-exact on 298).

- `SetHover(position, actionId)` 0x3E0530: no `Camera.main`: nothing valid. A new action id clears the flood
  (`lastHash = -1`, `floodIndex = 0`) and forgets the last voxel. `lastHash = position.GetHashCode() +
  actionId·3943 + camera.worldToLocalMatrix.GetHashCode()`. Both verts reset (`quadCount = -1`), `dstHeight = 0`,
  no voxel, not occupied.
- The camera's `ScreenPointToRay(position)` is cast with `Physics.Raycast(…, 1e7, LayerMaster.clickMask)`. The
  click mask also holds HoverData's `LastHitVoxel` / `LastNewVoxel` children (scaled 1.2 / 1.5), but nothing gives
  them a mesh: they never collide.
- On a voxel: `pointerHitPos` / `pointerHitDir` are the hit's point and normal; source = the voxel's corner's
  vert (`GridGenerator.GetVertOrIterate`) and height; the destination starts as the same vert, then
  - `normal.y > 0.5`: above (`side` -1, height + 1);
  - `normal.y < -0.5`: below (`side` -1, height - 1);
  - else beside: round the corner's squares (offsets in the plane from the corner), the sector where the hit's
    offset is on the left of square `i - 1`'s offset turned 90° and on the right of square `i`'s
    (`0 <= dot(rot90(prev), hit)` and `dot(rot90(cur), hit) <= 0`); `side = (i - 1) mod n`, the destination is
    the next corner of square `i` after this one (its `hexPos`, its plane position; the rest of the vert stays
    the source's). No sector: no destination check.
  `occupied` if the destination corner has a voxel at the destination height (`BinaryTreeSearchVoxel`).
- A miss is cast at the ground plane `y = 0` (`Plane.Raycast`): source height -1, destination height 0, `side`
  -1, normal up; both verts are `TryGetVert(WorldToPlane(point))` (`GetClostestVertPosition`, then the full vert
  if it closes round its corner, else no vert at that point); the corner's ground voxel, if any, is the voxel
  (occupied).
- Afterwards a newly hovered voxel restarts the flood with itself (unless already in it). `insideBorders =
  Graph.IsCoordinateAllowed(dst)`; paint, remove and bucket are valid on a voxel; add on an unoccupied, valid
  destination with height 0..=255 inside the borders. `valid` is not set here (the clickers set it).
- `Iterate` 0x3DFCA0 (the bucket flood, one queued voxel a call): for each square of the voxel's corner and the two
  qubes it is a corner of (at its height it is a top corner, one up a bottom one), the qube's collapsed module
  (`possibleModules[0]`) and orientation: `cornerTransform0` gives the voxel's module corner, and each module
  corner linked to it (`cornerLinks` by `GetCornerLinkIndex`) is mapped back by `cornerTransform1` to a square
  corner and layer, whose voxel joins the queue. (Checked only on uncollapsed qubes so far: the flood is the
  voxel alone.)
- `JumpForwardOnAdd` 0x3E01C0 (gamepad, after an add): the gamepad plane through the destination: horizontal when
  the camera looks steeply (`|forward.y|` above 0.3 for vertical hovers, 0.8 for side ones), else facing the
  flattened forward with the destination's vert refreshed; the destination becomes the source.
- `GamepadAddMove` (the gamepad's hover walk) is left to row G1c.

## Placing: `Maker` (G1b, proven)

`placemaker::maker`, checked against the stand-in master's `Maker` (`tests/placemaker_maker.rs`: hovers at the
same screen points, then the same add / paint / remove / undo / redo / clear on both sides; the whole graph, the
action list, the counters and `lastActionPos` compared).

- One click is one **action id**: `BeginNewAction` 0x328F90 first trims the list (while more than 998 ids are
  kept the oldest id's actions go; then the redo tail past `actionIndex`), resets the world
  (`WorldMaster.ResetState`), steps the collapse's propagation counter, and sets `actionCount++`, `actionId++`,
  `actionRedoMax = actionCount`; `onQueueChange(count, redoMax - count)` tells the undo / redo buttons.
- `AddClick(hover, type)` 0x328DE0: the stopwatch restarts; the hover's destination vert (refreshed through
  `GetVertOrIterate` when not full; nothing unless it closes round its corner and `IsCoordinateAllowed`); a new
  action; `Graph.AddVoxel(…, instantIntegration: true)` with `Ground` at height 0; `lastAddedVoxel`,
  `lastActionPos = hover.GetDstPos()`; the action (`Empty` → type) recorded; `SetDirty`.
- `PaintClick` 0x329570: nothing if the hovered voxel already has the type; else a new action, the action at the
  hover's source (old type → new), `Graph.PaintVoxel`, `SetDirty`. `RemoveClick` 0x329800: a new action,
  `lastActionPos = hover.GetSrcPos()`, the action (type → `Empty`), `Graph.RemoveVoxel`, `SetDirty`.
- `Clear` 0x329270 (with any voxels): one action id removing everything, corners from the last child, each
  corner's voxels from the top.
- `Undo` 0x329A30 / `Redo` 0x329680: the hover cleared, the world reset, the propagation counter stepped; the
  last (next) id's actions applied through `Graph.ApplyAction(…, playEffect: true)`, reversed newest first for
  undo; `actionCount` down (up). `CanUndo`: `actionCount > 0`; `CanRedo`: `actionIndex != actions.Count`.
  The `…Command` forms play the UI sound first; `HoldUndo` / `HoldRedo` wait for `RapidButton.Press(1)` (G1c).

## Input: `MasterClicker` (G1c, ported)

`placemaker::ui::master_clicker`. `GroundClicker` is an older copy that is in no scene (the harness finds no
instance): read, not ported. In `MasterClicker`, `BucketClick`, `BulldozeClick`, `PaintClick` and `RemoveHover`
have no callers in this build (the shipped game adds, removes and picks colours only). `InputReady` is a shared
stub that returns true.

- Unity's EventSystem raises the pointer events; ids -1 left, -2 right, -3 middle, 0 and up touches. The mouse
  hovers on enter (`Input.mousePresent`), and leaving clears the hover.
- Each frame: `Update` takes the mouse position; `LateUpdate` hovers there (once a frame, not on a click frame,
  not while pressed or dragging: `SetHover(position, maker.actionId)`) and draws the hover highlight in the
  touch-and-mouse input mode; `OnGUI` zooms with the scroll wheel while hovered.
- Press (`OnPointerDown`, mouse buttons): hover at the press, the press time; on a voxel the bob starts at the
  source position. While held (`Update`): past 0.35 s "about to remove" (the press sound) and the bob sinks,
  `Lerp(-0.2, -0.4, 1 - (1 - t)²)` with `t = InverseLerp(0.32, 1.5, held)` eased into `bobCurrent` by
  `WorldMaster.expTFaster`; at 1.5 s the colour is picked (the press is used up).
- Click (`OnPointerClick`, once a frame, not while dragging, not when used): left adds, or removes when the press
  lasted over 0.25 s; right removes; middle picks the colour (voxel types 0..=14: the palette selects it).
  `AddClick` / `RemoveClick` are `Maker.AddClick` / `RemoveClick` inlined, with the click effect (after an add,
  before a remove).
- Drag: a left drag turns the orbital camera by `delta / min(width, height) · 130` (y negated), kept in a ring of
  five `(x, y, Time.time)`; on release the drags of the last 0.2 s, summed and divided by the oldest one's age,
  become the camera's rotation speed (the fling). Right and middle drags pan.
- Touch: `TouchDown` hovers and bobs; `TouchUp(couldAct)` adds, or removes when held over 0.25 s.
- Enabled only in menu states 0x20 / 0x22 (`UiMaster.SubscribeToMenuState`); `touchLocationThreshold =
  |screen| · 0.022`.
- `RapidButton` (`placemaker::rapid_button`): `Press` is true on the first frame of a press, then each time
  `t += (unscaledDeltaTime · speed) · intensity` passes 1 (the slow speed for the first `countThreshold`
  repeats). Presets: undo / redo 2 → 10, palette 3 → 6, UI navigation 2 → 6 (threshold 2).

## Feel: `ClickEffect`, `VoxelBobEffect` (G1d, proven)

`placemaker::click_effect`, `placemaker::voxel_bob_effect`, checked on the scene's own components
(`tests/placemaker_feel.rs`). `VoxelBounceEffect` is in no scene and nothing calls it; `ClickBig` is only
reached from the caller-less bucket tool; `SetColorPickColor` has no callers: read, not ported.

- `ClickEffect.Click(add, planePos, height, type)` 0x2008A0, at `PlaneToWorld(planePos, height + 0.5)`: the burst
  (`particle`, shape scale ½, start-colour red = `type / paletteTex.width` on a new type, also for `dust`); at
  height 0 the water splash at `y = 0` with a splash sound (volume 1, pitch `Random.Range(0.5, 0.8)`), else dust;
  `VoxelBobEffect.ClickBobEffect(pos, add)`; the add / remove clip (volume 0.4) with the walking pitch: `p = 100`
  after two quiet seconds (`Time.time - lastClick > 2`), else `lastPitch ± 0.2`, played as
  `Lerp(0.4, 2, |(float)fmod((double)(p - 0.5), 2) - 1|) · slowmo`; `flock.UprootRadius(pos, 1, 1)`; the second
  clip (volume 0.25, pitch `(Random.Range(1, 1.2) · first) · slowmo`). Clips are `Random.RangeInt(0, count)` on
  Unity's global generator; the birds draw from it too, between the two clip draws, so the port calls the flock
  at that point. Checked: 200 clicks from known `Random` states (the state afterwards, `lastPitch`, `lastClick`,
  `lastVoxelType`).
- `Click(hover, add, voxel)`: at the destination (adding) or the source, with the voxel's type. `ColorPick(hover)`:
  at the voxel, birds within 1 (force 0.5), an upward bob, the pick particles and sound (volume 1, pitch
  `Range(0.8, 1.2)`).
- `VoxelBobEffect`: `_PressPosAmount = (bobPos, bobCurrent)`. `Update` steps a spring at 25 Hz of unscaled time,
  `v = cur·-0.6 + vel; vel = v·0.4; cur += v`, and stops below `|vel| + |cur| < 0.001`. `SetPressed` holds it
  (the clicker sinks it), `StopPressing(v)` releases with a velocity, `ClickBobEffect` kicks ±0.4. Checked: 300
  springs stepped by the game's own `Update`.
