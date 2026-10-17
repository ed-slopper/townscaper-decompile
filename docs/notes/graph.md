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

## The other systems

The graph reaches the rest of the game through `master` (`WorldMaster`): its grid, `WorldMeshes`,
`Sandbanker`, `WaveFunctionCollapse`, `AoBaker`, `ModuleBuilder`, `ClickEffect` and the platform SDK. Our port
calls them through the `GraphHost` trait; the rows that port those systems implement it.

## How it was checked

`game/tests/placemaker_graph_editing.rs` runs random actions on a fresh game `Graph` whose `master` is a
stand-in `WorldMaster` (made with `new`) holding a fresh grid grown like ours, the test graph, and a fresh
`WaveFunctionCollapse` / `ModuleBuilder` whose master is the stand-in (so nothing of the scene touches the test
graph, and their queues never run). After every action every object, reference (by address), dictionary,
queue and counter is compared.
