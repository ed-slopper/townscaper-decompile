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
