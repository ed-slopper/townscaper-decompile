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

## The hover outline: `HoverHightlight` (G1f, proven)

`placemaker::hover_hightlight`, checked against a fresh game `HoverHightlight` on the stand-in master
(`tests/placemaker_hover_hightlight.rs`: every qube given the same random house module and orientation on both
sides, then hovers through the scene camera; `_HoverPos` and the whole mesh compared to the bit).

- `Draw` 0x3E18F0 (called by the clicker's `LateUpdate`; `Update` switches the object off on a frame it was not
  drawn): `_HoverPos = (srcVert.planePos, 0, 0)`, the mesh cleared, and nothing more when the source vert is not
  full or the hover is outside the borders.
- A top / bottom hover (`side == -1`): for each quad round the source vert (its square from `squareDict`), from the
  quad corner that is the vert: at height `(dst > src) + srcHeight` 0 a flat quad at `y = 0` (the quad's corners
  from the vert, normals up, uvs `(0,0) (1,0) (1,1) (0,1)`, second uvs `(0, 1)`); above, that height's qube
  (`AddQube` with corners `a + u`, `b + u`, `((u + 1) & 3) + a`, `a`/`b` = 4/0 when adding, 0/4 when removing).
- A side hover: the two quads at `side` and `side + 1`, both layers (heights `srcHeight` and one up): `AddQube`
  with the vert's corner (top layer first), the corner across (`((k != 0)·2 + 3 + u) & 3`) and the one above /
  below.
- `AddQube` 0x3E2770: the qube's `possibleModules[0]`; the frame `Matrix4x4.LookAt(corner, opposite, up - corner)`
  of three qube corners (`ModuleMath.corners + 0.5`) and its inverse; each of the module's meshes with the house
  material, under the product of the module's and mesh's orientations: per vertex `p` the uv `(inverse·p).xy`, the
  second uv `((inverse·p).z, (inverse·t).z · 2)` where `t` is the vertex's **tangent** (the game reads field 0x14,
  `tangents`, for the "normal"); a vertex whose uv is `(0, 0)` sets the `zOffset` later taken off every second uv;
  the world vertex `qube position + ModuleBuilder.MultiplyPoint(square, GetTs(p), p)` and normal
  `MultiplyVector(…, t)`; the triangles reversed unless the orientation is inverted.
- Bounds: centre 0, size `float.MaxValue` (never culled).

## The camera: `OrbitalCamera`, `SaveCamera`, `CustomCameraProjection` (G2, proven)

`placemaker::ui::orbital_camera` and `placemaker::save_camera`, checked on the scene's own components
(`tests/placemaker_orbital_camera.rs`, `tests/placemaker_save_camera.rs`): random state set, the game's method
run in the same batch as the frame's inputs are read, every field (and the camera pose) compared to the bit.

- State: targets and eased currents for zoom (0..1), pan (`panHex{X,Y}`: shares of the town's hex bounds;
  `panHeight`: a share of `heightCurrent`), the bounds (`float3x2`) and height; turn `rotationY` (0..1 = 360°)
  and tilt `rotationX` (0..1 between the lowest the pan height allows and 88°), with fling speeds.
- `LateUpdate` 0x6F7F60: currents ease as `(cur - tgt)·t + tgt` with `t = (float)exp(-dt·10)` (zoom
  `exp(-dt·20)`; 0 on the first frame / after `Load`, `snap`); `anyVoxelsCurrent` towards 0/1. A fling decays by
  `k = exp(-dt·4) - dt·0.4` (on the x87 unit: one rounding), `Lerp(0, speed, k)`; turn `fmod(dt·speed +
  (rotationY + 10), 1)` in double; tilt `Clamp01(dt·speed·(1 - Pow2(2·rotX - 1)) + rotX)`.
  Placing: the pan point `GetWorldPos`; the radius the longest half diagonal of the bounds box (scaled by
  0.121654496, through `HexMath.HexToPlane`, at least `minRadius` 3.5, NaN skipped); `fit = Max(Max(r·h/w, r),
  sqrt(r² + (height/2)²))`; fov `Lerp(35, 16, zoom)`; `distMin = 2`, `distMax = Max(fit·1.7, 4)·Lerp(1.4, 1,
  anyVoxels)`, both times `Lerp(1, 1.03, dim alpha)`; `nonDollyDist = Lerp(distMin, distMax, zoom)`,
  `currentDist = 2 / tan(fov°)·nonDolly`; tilt `Lerp(asin(Max(pan.y / -currentDist, -0.95))°, 88, rotationX)`;
  `Euler(tilt, rotationY·360, 0)`; position `pan + rot·(0, 0, -currentDist)`; when the game view rect's centre is
  off the screen's (`dx² + dy² ≥ 1e-10`), minus `rot·((tan·currentDist/2)/(h/w)·dx, dy, 0)`: **only x is scaled**,
  y is the raw centre offset (a quirk kept). Far clip `currentDist + 1000`.
- Inputs: `RotateCamera` (drag: tilt by `delta.y / (88 - minAngle)`, turn by `delta.x / 360`, wrapped by `fmod`
  then `+1` while negative), `SetRotationSpeed` (fling), `ScrollZoom` (`2^(-4·delta)` times the distance),
  `PinchZoom`, `GamepadZoom` (`exp(-unscaledDt)·delta/10` as a scroll), `Focus`, `WASDCamera` (keys times
  `nonDollyDist·unscaledDt·2`, turned with the camera; the pan point clamped to the bounds by `HexClamp`, the
  height share by the vertical part), `PanCamera` (the two screen rays at `currentDist`; the height share moves
  by their height difference, the pan point by where the second ray meets the level plane through the first,
  `target = (p1 - p2) + p2` per axis as compiled). With no voxels both move the empty town's bounds and ask the
  graph to recentre (`SetEmptyBoundsCenter` + `WorldMaster.ResetState`) when the rounded centre changed: checked
  with a fresh `WorldMaster` and `Graph` swapped in for one batch.
- `Save` / `Load`: `SaveData.cam` bytes `(byte)Clamp(v·255, 0, 255)` (tilt, turn, zoom); load is `byte·(1/255)`
  clamped, pans centred, `snap`. `BoundsUpdated` (the graph's `onBoundsUpdated`) remaps the pan target when the
  gamepad cursor is adding so the view stays on it. `OnStart` sets the camera's `opaqueSortMode` to 2.
- `SaveCamera.GetImage` 0x965000: `Euler(15, cam.y/255·360, 0)` at `PlaneToWorld(HexToPlane(bounds centre),
  boundsHeight/2)`, then a box (from the origin) round each corner column's first and last voxel in the camera's
  `worldToLocalMatrix` (columns from the last child), backed off by `Max((ext.x + 1)·h/w, (ext.y + 1)·1.1)·2 /
  tan(16°)` and shifted by the box centre; rendered 1024² (8× AA) with `_AaLineBoth (2, 0.5, 1, 4)`,
  `_ShadingTexLightStack (1, 1, 0, 0)`, `_Dim (0, 1, 0, 0)`, blitted to 512², read into the 512² texture, JPEG 90.
  Checked: the pose on 12 thumbnails of the scene town (camera three transforms deep, read into our model).
- `CustomCameraProjection` keeps the projection matrix from `Start` and sets it back every `Update`.
- `ScreenshotCamera` (the photo mode's render pipeline, called from `ScreenshotButtons` only) moved to G7f.

## Undo and redo (G4)

The undo queue itself is the maker's (G1b: `Maker.Undo` / `Redo` through `Graph.ApplyAction`, `Action.GetReverse`,
checked there). G4 adds the buttons and `Action.ToString`.

- `Action.ToString` 0x7B2AD0: `({x}, {y}, {height}) {in} -> {out}` with the voxel types by name; checked on the
  game for every type (`tests/placemaker_undo.rs`).
- `UndoRedoButtons` (`placemaker::ui::undo_redo_buttons`, ported; a fake-maker test): the maker's
  `onQueueChange` sets each label to the count (`NumberStrings.Get`) or `-` and disables a button at zero. A press
  (`onButtonDownChange`) enables the component and arms a one-second delay (`initialDelay`); `Update` then holds
  (`Maker.HoldUndo` / `HoldRedo`, the maker's own repeat timing) every frame past it, undo before redo, and switches
  itself off when nothing is pressed. A click (`onClickAction`) steps once unless that press already held. Each
  successful step squashes the button (`StandardButton.scaleXY` / `scaleZ` currents × 0.95). The gamepad stepper
  on the object holds redo for a positive step, undo otherwise, and undoes once on press. `Button_Undo` /
  `Button_Redo` are empty in the build.
- `UndoRedoFeedback.OnSetup` subscribes its rect to the safe area (`UiMaster.SubscribeToSafeArea`, G7a).
