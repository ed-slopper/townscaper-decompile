# The voxel graph (E4)

`Placemaker.Graphs.Graph` keeps the town: the **voxels** the player places, the grid **corners** they stand on,
the grid **squares** round those corners and, per square and height, the **qubes** the wave function collapse
fills with modules. Ported in `ts_game::placemaker::graphs`; checked against the running game
(`game/tests/placemaker_graph*.rs`).

## Objects and containers (proven, E4a / E4b)

Every object is a `MonoBehaviour` on its own `GameObject`, kept under a container transform; the game pools
them and never destroys one. Our port keeps one arena per type (typed ids) and, for each container, a list in
sibling order:

| Container | Holds | Order |
|---|---|---|
| `corners` / `cornerPool` | `Corner` | by `hexPos` (x, then y; `BinaryTreeSearchCorner`) |
| `squares` / `squarePool` | `Square` | by `position.x` (`BinaryTreeSearchSquare`) |
| a corner's transform | its `Voxel`s | by `FloorToInt(localPosition.y)` (`BinaryTreeSearchVoxel`) |
| a square's transform | its `Qube`s | one per height, index = height |
| `voxelPool`, `qubePool`, `looseVoxels` | pooled objects | the last child is taken first |

All containers sit at the origin, so a local position is also a world position. A pooled voxel keeps its world
position (`SetParent` keeps it); a deconstructed qube keeps its local one (`SetParent(pool, false)`).

Constructors: `Qube..ctor` 0x843780 sets `shadowIndex = -1` and `cornerValues = ByteQube(18)` (all `Empty`);
`GetNewSquare` gives the four relations `Square.Relation.empty` = (null, -1). `FlowData.empty` (`..cctor`
0x6CF900): no voxel, `dist = newDist = float.MaxValue`, the rest zero.

Dictionaries: `cornerDict` (hexPos → corner), `squareDict` (sum of the four corner hexPos → square), `edgeDict`
(`int2x2` (from, to) of an **open** edge → the square and edge index that own it). An edge is stored the way
its square walks it (`c_k` → `c_k+1`); the neighbour that closes it looks up the reverse.

## Editing (proven, E4c)

### `AddVoxel` 0xE829E0

`GetOrCreateCorner`, then `BinaryTreeSearchVoxel`; an existing voxel logs `Voxel already exists at {0} {1}` and
returns null. The voxel comes from the pool (last) or `GetNewVoxel`; `cost = int3(hexPos, height).GetHashCode()`
(`math.hash`: `y·0x4822A3E9 + x·0x4C7F6DD1 + h·0xAAC3C25D + 0xD21D0945`, wrapping), sibling index `~search`,
`localPosition = (0, h + 0.5, 0)`.

The corner's **first** voxel: when `bounds` does not contain `hex3(hexPos)`, bounds become that point (when it is
the only corner) or grow to it, `boundsUpdated`; a corner with no squares is put at
`PlaneToWorld(GridGenerator.HexToPlane(hexPos), 0)`; then `IntegrateCorner` at once when `instantIntegration`
and `cornerIntegrationQueue` is empty and it succeeds, else the corner is queued. A **later** voxel is
integrated (`IntegrateVoxel`) only when the corner already has its collider mesh.

`boundsHeight = max(boundsHeight, h + 1)`; `totalVoxelCount++`; at the 100th and 1000th voxel the Fury SDK
achievements `Blocks100` / `Blocks1000` (logged as `Achevement: ...`).

### `IntegrateCorner` 0xE84F60

`GridGenerator.GetVertOrIterate(hexPos, WorldMaster.KeepGoing)`. No valid position: false. A vert that does not
close (`!full`) removes the corner's voxels from the top down, logs `Removing invalid corner at ...`, true.

Otherwise, for each quad of the vert: the square in `squareDict` (by `hexPosSum`), else a new one:

- at `PlaneToWorld(quad.planePos, 0)`, from the pool or new, put at `~BinaryTreeSearchSquare` among
  `squares`. On an exact x match the search returns a non-negative index, `~index` is negative and Unity's
  `SetSiblingIndex` then leaves the square **last** (proven on the harness: a negative index is ignored, a large
  one clamped);
- `quadIndex`, `squareDict.Add(hexPosSum)`;
- four corner matrices from `w_k = PlaneToWorld(p_k, -0.1)`:
  `m_k = (worldToLocal · Translate(0, -0.5, 0)) · [col0, (0,1,0,0), col2, (w_k, 1)]` with
  (col0, col2) = (w1-w0, w3-w0), (w1-w0, w2-w1), (w2-w3, w2-w1), (w2-w3, w3-w0) for k = 0..3;
- edges k = 0..3, walking `b = v_k`, `a = v_k+1`, `d = v_k-1` (an `ExtraMath.Swap` rotation of four `int2`s):
  if `edgeDict` has (a, b) the two squares are linked both ways (`SetRelation`) and the entry removed, else
  (b, a) → (square, k) is added; corner `b` is got or made, put at `PlaneToWorld(quad.GetPlanePos(k), 0)`, set
  as `c_k`, gets the square (`Corner.AddSquare`, sorted by angle) and
  `angle += SignedAngle(Vector2(d - b), Vector2(a - b))`.

Every square of the vert then gets `motivations++` (the number of integrated corners using it).

The corner's collider mesh (`WorldMeshes.GetMesh`): per quad i, the quad centre relative to the vert at
y = -0.7 and 0.3, side triangles `(j-1, j+1, j)`, `(j, j+1, j+2)` (j = 2i + 1, mod 2n), then fans
`(0, j+1, j-1)`, `(1, j, j+2)` for i = 1..n-1 (j from 3; the last pair is degenerate). `blitVerts` gets two
points per quad: the quad corners after the vert's own corner (p1, p2 when v0 is the vert, ...), relative to
the vert. Then every voxel of the corner is integrated. True.

### `IntegrateVoxel` 0xE86B70

The voxel's `MeshCollider` and `MeshFilter` share the corner's collider mesh. Height 0 resets the
`Sandbanker` and counts as `Ground` (15). For each square round the corner:

- the four corners' `flowDatas` are extended with `FlowData.empty` to height + 1;
- the square's qubes are extended to height + 2 (pool first, `existanceIndex++`, else new):
  `lastPropagation = 0`, ground qube `voxelCount = 0x0F` and lower corner values `Water` (17), others 0 and
  `Empty` (18), `localPosition = (0, i, 0)`; qubes below the voxel are queued (`QubeUpdated`);
- the voxel's type goes into `lower.cornerValues[4 + k]` and `upper.cornerValues[k]` (k = the corner's slot:
  c0, c1, c2, else 3), and `upper.voxelCount ^= 1 << k`, `lower.voxelCount ^= 0x10 << k` — for slot 3 the
  game **ors** (8 / 0x80) where the others xor (checked in the asm);
- height 0: `groundCoverage++` on the four corners; heights `max(0, h-1) .. h+1`: `neighbourCount++` on the
  four corners' flow data;
- upper and lower are queued.

Finally `flowDatas[h].voxel = voxel`.

### `RemoveVoxel` 0xE8B720

No parent corner: `Trying to remove voxel without a corner`. Height 0 resets the `Sandbanker`; with a collider
mesh `flowDatas[h].voxel = null`; meshes cleared, voxel to the pool. For each square of the corner (last
first): corner values back to `Empty`, the bits xored off (all four slots xor here), `groundCoverage--` (h 0),
each of the four corners queued once for the flow (`inFlowUpdateQueue`, `cornerFlowQueue`), `neighbourCount--`.
When the qube above is now empty and the column is exactly h + 2 high, the column is taken down from the top
while each qube is empty (the ground one `0x0F`): `WaveFunctionCollapse.QubeRemoved(qube, side mask)` (mask =
`Bitwise.GetByte(q0..q3 linked, true)`), `Sandbanker.Reset` for a filled ground qube, a shadow index returned
(`shadowMeshIndexes`, its four `AoBaker` shadow vertices zeroed), `ModuleBuilder.MarkQubeForDeconstruction`
(the qube to `master.graph.qubePool` at once), `existanceIndex++`. Upper and lower are queued.

The corner's **last** voxel also: each of its squares gets `motivations--`; at 0 the square is released — each
edge k (a = c_k, b = c_k+1, d = c_k-1): a linked neighbour is unlinked both ways and its edge (b, a) added to
`edgeDict`, an open edge (a, b) removed; the square leaves `a.squares`; a corner left with no squares (other
than the one being emptied) leaves `cornerDict` for the pool with `angle = 0`, else
`angle -= SignedAngle(Vector2(d - a), Vector2(b - a))`; then `squareDict` (by the corner sum) and the square to
the pool. The collider mesh goes back (`WorldMeshes.ReturnMesh`, `blitVerts` cleared); a corner on the edge of
`bounds` sets `recalculateHexBounds`; a corner with no squares left leaves `cornerDict`, gets hexPos (-1, -1) and
goes to the pool. Always: `totalVoxelCount--`, and `recalculateHeightBounds` when h + 1 was `boundsHeight`.

### `PaintVoxel` 0xE8B3E0, `ApplyAction` 0xE83110, `QubeUpdated` 0xE8B6C0

`PaintVoxel` does nothing at height 0; else the type is set and written into the corner values above and below
(as `IntegrateVoxel`, counts untouched), both qubes queued. `ApplyAction`: from `Empty` adds (instant
integration = `playEffect`, then `ClickEffect.Click(true, ...)`); otherwise a missing corner logs `Failed to get
corner at ...`, to a type paints, to `Empty` clicks (`false`) and removes. `QubeUpdated` queues a qube once
(`awaitingGraphUpdate`).

## The loop (proven, E4d)

### `Iterate` 0xE89CF0

One kind of work per call, each while `WorldMaster.KeepGoing` (the frame's time budget) allows:

1. **Corners.** `cornerIntegrationQueue` from the last: a corner without a collider mesh and with voxels is
   integrated; one that cannot be yet (`IntegrateCorner` false) ends the call and stays queued; the others are
   dropped from the queue. Returns true even when the queue empties.
2. **Qubes.** `qubeUpdateQueue` from the last: a qube under a square in `squares` goes to
   `WaveFunctionCollapse.QubeUpdated`; `awaitingGraphUpdate = false`. A qube above the ground (`localPosition.y
   > 0.1`) with any lower corner value not `Empty` gets a **shadow quad**: its marching-squares cell is
   `2 · (powers.x·v0 + powers.y·v1 + powers.z·v2 + powers.w·v3)` (`Empty` counts 16), row / column of
   `marchingSquaresTex` by its width; UVs `(fx, fy)` = `texel / 2 + (col, 2·row) · texel`, corners
   `(fx, fy)`, `(fx + tx + 1, fy)`, `(fx + tx + 1, fy + ty + 1)`, `(fx, fy + ty + 1)` (the `+ 1` is in the
   asm: constant 1.0f at 0x111F1A60). A qube with a quad only gets new UVs; one without takes a freed index
   (`shadowMeshIndexes`, last) and writes its four vertices — the square's corners at `y - 0.5` — and UVs, or
   the next new index (`shadowMeshIndexIterator++`) and appends two triangles, vertices and UVs. A qube that no
   longer needs its quad returns the index, zeroes the four vertices, `shadowIndex = -1`. Then `AoBaker.BakeAo`.
3. **Flow.** The last corner of `cornerFlowQueue` leaves it; a pooled corner's `flowDatas` are cleared, else
   trimmed to the tallest column of its squares.

False when all three queues are empty. `AoBaker.GetShadowMesh` 0x7B3500: mesh `4·i / 65532`, vertex
`4·i % 65532` (16,383 quads per mesh); past the last mesh one new mesh is made, whatever the index.

### `IterateFocus` 0xE87940

Recalculations first, one call each: `recalculateHexBounds` (the hex bounds of every corner with voxels, from
the last; `ExtraMath.Center` of the old bounds when there are no corners; `boundsUpdated` if they changed),
then `recalculateHeightBounds` (`CeilToInt` of the highest top voxel's `localPosition.y`). Otherwise, when
`boundsUpdated`:

- `border`: `(e, maxHeight) = GetMaxExtents(bounds, boundsHeight)`; `border.min = bounds.max - 2e`,
  `border.max = bounds.min + 2e` (how far every part of the town may reach); with no corners 1024 instead of
  2e, and `maxHeight = 256`;
- `genBounds` = bounds ± 96, clamped to the border ± 2; `viewBounds` = genBounds clamped to the border;
  `_GridViewMin/Max`, `_GridBorderMin/Max` set as global shader vectors;
- `shadowMeshBounds`: a zero-size box at `PlaneToWorld(HexToPlane(bounds.min), boundsHeight)` grown by the
  eight corners of the hex box (min/max per axis, in the order x-major) at height 0, then extents + (2, 0, 2);
- `groundCollider.size = (size.x + 50, 2, size.z + 50)`, `center = (center.x, -1, center.z)`;
- `onBoundsUpdated()` (called unchecked; the constructor sets an empty one), `BakeAo`, `grid.focusUpdated = true`, `boundsUpdated = false`; at
  `boundsHeight == 256` the `MaxHeight` achievement.

### Loading and saving: `Save` 0xE8F920, `BeginLoad` 0xE83440, `IterateLoad` 0xE88FF0

`Save` writes `SaveData.corners` (hexPos and voxel count of each corner with voxels, in `corners` order) and
`SaveData.voxels` (type, height, in sibling order); an empty town saves its bounds' centre as one corner of no
voxels (and logs it). Loading goes through `Maker`'s action list, one step per call (`loadState`):

0. `Maker.ClearUndoQueue`.
1. A removal action (`Ground` for height 0) for every voxel, corners and voxels from the last.
2. Every qube of every square queued (`QubeUpdated`, its module containers to `ModuleBuilder`), square by square
   while the budget lasts (`Maker.actionIndex` is the cursor).
3. The save: no corners → `SetEmptyBoundsCenter(0)` and `onEmptyBoundsLoaded()`; corners but no voxels → the
   same at the only corner (if there is just one). Else the saved hex bounds and **the height of the last voxel
   read** (one value passed to `Mathf.Max(params int[])`) give `GetMaxExtents`; corners outside centre ± e and
   voxels at or above `maxHeight` are dropped (`Some voxels fell outside the border and was not loaded`). Each
   kept voxel (`Ground` at height 0) meets the action list (only the actions there before this step): an action
   at its place that removes the same type is cancelled (removed, `actionIndex--`), one of another type becomes
   a repaint; else a new add action.
4. `WaveFunctionCollapse.IteratePropagationCount`, then the actions applied in order (`ApplyAction`, no effect)
   while the budget lasts; when all are applied, `ClearUndoQueue` and done.

`OnStart` 0xE8B180 makes the seven containers (pools inactive) and bakes once. `Test` 0xE8FD10 is an editor
context-menu helper (scales every qube to 0.9): read, not ported.

## Sampling (proven, E4e)

What the town looks like as a field, for the props, birds and effects that sit on it.

- `GetQubeWithContext` 0xE84460: the qube under a world point. Inside the hex bounds + 18 and `0 <= y`,
  `RoundToInt(y) <= boundsHeight`, the squares nearest in x are tried (from `BinaryTreeSearchSquare`,
  alternately the next one up and the next one down `squares`; a side stops once a square is 1.42 past the
  point in x), skipping any farther than 2.1 squared, until one has the point on the inner side of all four
  edges (`dot(Rotate2D90(edge), p - from) >= 0`). Returns the qube at that height (or nothing if the column is
  lower), the square, its corners on the plane and the height.
- `Qube.SampleNormalCoverage` 0x842670 at `ts` in the qube: `coverage` is the trilinear weight of its filled
  corners (lower four, then upper four), `normal` the gradient towards empty neighbours: sideways along the
  square's corner-matrix directions (`-m0` column 0, `-m2` column 0, `-m0` column 2, `-m1` column 2), up and
  down as `y`. Empty qube (zero, 0), full qube (zero, 1). `GetNormal` 0x841200 is the normal at the centre.
- `SampleNormalCoverage(point)` 0xE8CE30: the qube's sampler at its `ExtraMath.InvBilinear` position; no qube:
  (up, `Clamp01(0.5 - y)`) at or below 0.5. `SampleNormalCoverage(point, qube)` 0xE8C970 (and the transform
  overload 0xE8D270) does the same for a given qube with `RoundToInt(y)` as its height. `SampleNormal` 0xE8F8B0
  is normal × coverage.
- `SampleNormalDistance2` 0xE8D380: the same gradient on the **grid** (the quad under the point from
  `GridGenerator.GetQuad`, its corners' voxels at the two heights round `y - 0.5`, edges from the quad's
  corners), so it works outside the squares; below -1: (zero, `1 - Clamp01(y - 0.5)`); above the town + 1, more
  than 288 from the bounds or off the grid: (zero, 2). `SampleNormalDistance` 0xE8E060 is the same written out by
  hand for the four corners and two heights, weighing each corner by its **flow data** instead of its voxel (0
  under a voxel, 2 with no voxel next to it, else 1); it keeps two slips of the original: the fourth corner
  uses the first edge (p0 - p1), and the weight multiplies only one side of each upward term. Below -1 it
  returns up rather than zero.

The operand grouping of these float sums matters (results are compared bit for bit): Ghidra's printout regroups
products (`a * b * c` for the compiled `a * (b * c)`) and sums, so both samplers were transcribed from the asm.

## Flow (proven, E4f)

`Placemaker.Graphs.Flow` keeps a distance field through the open space round the town in each corner's
`flowDatas` (one `FlowData` per height: `dist`, `newDist`, `denom`, `dir`, `newDir`). One `Update` 0x6D2BA0 per
frame:

1. Every corner's column takes its new values: `dir = newDir / denom` when `denom > 0` (then `denom = 0`),
   `dist = newDist`, `newDist = Min(newDist + 1, float.MaxValue)`, `newDir = 0`; then, up the column, two open
   cells (no voxel) one above the other flow: the higher-`dist` one gets ±up in `newDir` and the other's
   `dist + 1` as a `newDist` candidate — **cut to a 16-bit integer** (`(float)(ushort)(int)`, asm
   `cvttss2si` + `movzx ax`; `float.MaxValue` becomes the x86 indefinite integer, so 0).
2. Each target (the `Flow` transform's children, from the last) seeds the field (`AddFlowTarget`).
3. Over every finished grid patch within 72 of the hex bounds, each quad whose first corner is within 18: its
   four corners' cells (`FlowCalcData`, positions at the corner + 0.5 up) at every height flow pairwise with
   `FlowTwoFlows` 0x6D0500 — across the quad (√2) when all four are open, along each open edge (1); and to the
   next height: across the cube (√3) when all eight are open, across each side face (√2), straight up (1).
   `FlowTwoFlows`: for two available cells, `t = Clamp(d0 - d1, -s, s)`; the farther one (the second on a tie)
   gets `t · (p1 - p0) / s` added to `newDir`, `Min(newDist, other.dist + s)` and `denom + 1`.

`AddFlowTarget` 0x6CF970 lowers `newDist` of the eight cells round a point (the grid quad under it,
`ExtraMath.InvBilinear` position, the two heights round `y - 0.5`) to a product of its cell coordinates.
`SampleFlow` 0x6D0940 returns the trilinear blend of the cells' `dir`, the solid cells' normal × 2 (as
`SampleNormalDistance`, on the quad's plane edges), its coverage, and the total weight of the cells found; below
the ground or above the town: (0, 0, `1 - Clamp01(y - 0.5)`, 0); off the grid: zeros.

## The other systems

The graph reaches the rest of the game through `master` (`WorldMaster`): its grid, `WorldMeshes`,
`Sandbanker`, `WaveFunctionCollapse`, `AoBaker`, `ModuleBuilder`, `ClickEffect` and the platform SDK. Our port
calls them through the `GraphHost` trait; the rows that port those systems implement it.

## How it was checked

`game/tests/placemaker_graph_flow.rs` (E4f) adds a `Flow` component (disabled, stepped by the test) with two
targets to a 60-voxel town and compares the whole graph after each of 14 frames, after 30 more targets, and
`SampleFlow` at 500 random points.

`game/tests/placemaker_graph_sampling.rs` (E4e) builds a 60-voxel town with the full update and compares every
sampler bit for bit at 600 random points round it, and the qube sampler at 400 points in random qubes.

`game/tests/placemaker_graph_loop.rs` (E4d) adds the scene's `AoBaker` (the test graph's shadow quads start past
the scene's meshes), a fresh `Maker`, a fresh `BoxCollider` and `Action` delegates for the bounds events, and
runs 40 actions each followed by `Iterate` and `IterateFocus` to the end on both sides, `Save`, more actions,
then loads the save (and an empty one) through `IterateLoad`; graph, shadow meshes (`dirty` aside: the scene's
baker clears it every frame) and the action list are compared. The ground collider is read through the
engine's internal calls (`get_size_Injected`): the managed getters are stripped from the build.


`game/tests/placemaker_graph_editing.rs` runs random actions on a fresh game `Graph` whose `master` is a
stand-in `WorldMaster` (made with `new`) holding a fresh grid grown like ours, the test graph, and a fresh
`WaveFunctionCollapse` / `ModuleBuilder` whose master is the stand-in (so nothing of the scene touches the test
graph, and their queues never run). After every action every object, reference (by address), dictionary,
queue and counter is compared.
