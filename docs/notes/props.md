# Props (E10)

Chimneys, lamps, flags, wires, plants: the small objects modules carry. `placemaker::props`.

## The data (E10a, proven)

- `PropPlacement` (20 bytes: `pos` SbyteFloat3, `scale` ByteFloat3, `rotationByte` ByteFloat3, `prop`, `corner`,
  `prioAdd`). `rotation` = bytes × 1.40625° (256 to a turn). `set_rotation`: per angle
  `(byte)(int)((float)fmod((double)(a + 360f), 360.0) * 0.7111111f + 0.5f)` — truncation, so a full turn wraps
  to 0 through the byte. `GetMatrix` = `TRS(pos, Euler(rotation), scale * 2)`.
- `Prop`: a prefab; `voxelTypeMask` (all types by default), `srcProp` on instances.
- `PropNode`: one placed prop in the graph that decides which props show. `relations` is one list in five runs —
  parents, children, overlaps, motevees, anchors — with a count each; the `…Index` getters are the run starts.
  `Add…` inserts at the end of its run; `InserOverlap` inserts into the overlaps where `List.BinarySearch` (the
  node's own comparer: `priority`, `Single.CompareTo`, NaN first) puts it. Quirk kept: every `Remove…` uses
  `List.Remove` (the first occurrence anywhere) and lowers its own count even if the node was in another run, so
  a count can wrap at 0; later inserts past the end then throw (`ArgumentOutOfRangeException`, and
  `BinarySearch`'s range check an `ArgumentException`): the port returns false and changes nothing.
  `.ctor`: `motivations = 1`.

Checked (`tests/placemaker_props.rs`): 500 random placements (rotation, matrix, `set_rotation` on random and
whole-turn angles), and 400 random relation edits on fresh nodes (relations, counts, index getters, which edits
throw) and every pair through `CompareTo`.

## Placing props (E10b2, proven)

`PropPlacer.PlacePropsInQube` 0x70A800 (`placemaker::prop_placer::place_props_in_qube`). The decompile hides
half of it: the `using` block's `finally` funclet (0x1070D3E7 and the fragment 0x1070DA93) holds the node part,
and Ghidra mislabels several of the vectors; the port follows the disassembly.

- Nothing without possible modules. The random state is `Unity.Mathematics.Random(|hash(qube.position)|)`.
- Each child of the qube (a module container; from the last) without props yet gets them (`hasProps`). Child 0
  takes the module's placements, the others the decor modules' (`moduleIndex` out of range logs an error and
  then throws). The orientation is `orientations[flags & 7]`.
- Each placement (from the last) draws `NextFloat(prioAdd + p, (p + 1) + prioAdd)` with `p` the prefab root
  node's priority; a draw below 0 skips it. `cornerTransform1[corner]` is read first (a negative corner
  throws), the corner value is 0 for a negative corner, and its type must be in `voxelTypeMask`
  (`1 << (value & 3)`).
- `m = orientation.matrix * placement.GetMatrix()`; `p = column3 + 0.5`, `ts = GetTs(p)`; up and forward are
  columns 1 and 2 through `ModuleBuilder.MultiplyVector`, the position `MultiplyPoint(p)`. (Column 0, the right
  vector, only feeds a `LookRotation(..) * Euler(0, -90, 0)` the code throws away.)
- Sides: x side 1 (x > 0) or 3, z side 2 (z > 0) or 0. A side counts when `|c| > 0.4` and the container's side
  profile there is non-zero. Not both: for each axis with `|c| > 0`, where the *module's* profile (container
  0's) is open (at height 0 not a water side) and the neighbour exists, forward leans towards the neighbour:
  `forward += MV(neighbour square, GetTs(Min(one, Max(zero, (n + c3) + 0.5))), N.MultiplyVector(±column 2)) *
  2|c|` with `N` the orientation `(side + indexInOther) % 4`, `n` its column 0, the column negated for z. A
  decor (child > 0) on a counting side only stays on the corner of smaller x (`GetCorner(side)` against the
  next). Both sides: the corner pair (`(x + 1) & 3` against z) must straddle along x, or it's skipped.
- `GetProp(prefab, container)`; mirrored (`(lossy.z < 0) != (lossy.x < 0)` differs from `lossy.y < 0`) flips
  scale x and forward; `localPosition`, `rotation = LookRotation(-forward, up)`, `localScale = lossyScale`.
- The instance's objects breadth first (a pooled `List<Transform>`): a `PropColoredMesh` takes
  `GetMeshWithVoxelType(mesh, corner >= 0 && value < 15 ? value : 15)`; a `PropTransformer` with `fitInSquare`
  is bent in again (`m * inst.worldToLocal * t.localToWorld`, a new `ts`; position `qube.position +
  MultiplyPoint`, rotation as above); every `IPropModifier` (from the last) gets `Apply(master, ref random,
  {square, qube, ts, cornerIndex, orientedModule, cornerValue})` with the latest `ts`.
- A `PropNode`: turned off and hidden, `hideCount` 0. No parents: priority = the placement's draw, into the
  placed list. Otherwise `lo = Min(Max(pp - 0.1, pp * 0.9), lo)`, `hi = Min(pp, hi)` over the parents (each
  parent not turned on hides it once), then `priority = NextFloat(lo, hi)`; below 0 hides it and stops its
  branch (no anchors, no children). A `PropWireAnchor` takes the corner value as `voxelType`, its node joins the
  dictionary, and to every anchor partner with an anchor a wire is strung: a `srcWire` prop under the secondary
  props, at the middle of the two `TransformPoint(connectPos)`, `localRotation = LookRotation(dir)` (the
  smaller-x end first), child of both anchor nodes, motivated by both `wireConnectorNode`s, its
  `PropWireFlagMesh` / `PropWireItalianMesh.Setup(master, wire, length)`, then into the breadth-first list.
- Last: the placed nodes sorted by priority (`List.Sort`), `RecursiveCheck` from the last.

The modifiers it runs (row E10c, ported here): `PropTransformer` (scale × `(float)pow(2, NextFloat(-v, v))`,
local rotation × `Euler(0, NextFloat(-v, v), 0)`, position + `localToWorld.MultiplyVector` of three draws;
`Reset` copies the prefab's local scale, position, rotation), `PropNgonScaler` (local scale × the distance
between two opposite square corners' translations / √2, the diagonal chosen by `(ts.x > 0.5) == (ts.y <
0.5)`), `PropPropeller` (the splash effect at `y = 0` and shown below 1.5, else hidden), `WindVane` (compass
identity, rotator 23.5° about y), `Window` (a variation hashed from the square corner and the rounded height;
`mf0` the mesh reference's first item, `mf1` the last other one drawn with `hole0`; both take
`Window.GetMeshWithVoxelType`). Mesh copies are data (`MeshClone`): `GetMeshWithVoxelType` keeps sixteen
copies per mesh in `materialMeshes` (copy `k`: uv + 0.25·(k mod 4, k div 4)), `Window` sixty per mesh in its
static list (type `g`, variation `r`: u + 0.25·(g mod 4), ((7g + r) + v) + 0.25·(g div 4)).

## The collider dictionary and turning nodes on (E10b3, proven)

- `PropCollider.GetMatrix`: `localToWorld * Translate(center) * Scale(extents)` (anchors, layer 3: the x and z
  columns `(c2 ± c0) / 2`); `CalculateWorldBounds` grows `Bounds(0, -MaxValue)` over the eight ±1 corners.
  `Collides`: 0 with 0, 2, 5; 1, 3, 4, 5 with themselves.
- `AddColliderToDict` 0x707C40: `inDict`; each collider (from the last) gets its world bounds and goes into
  every 2-unit chunk its box meets (`OrientedBoxOrientedBox` with `GetChunkMatrix`), inserted before the first
  collider of a higher priority (or its own node's). Against the chunk's colliders: other nodes, bounds
  touching, not already related, colliding layers; layer 7 only within the same `srcProp`; anchors must face
  each other (rights ≥ 0.3 along the line, ≥ 0.5 against each other) and share `srcWire`; then the boxes must
  intersect and neither be the other's child. Anchors `AddAnchor` both ways; others `InserOverlap` both ways,
  and a turned-on node of at least its priority hides it.
- `RemoveColliderFromDict` 0x70E920 (its body in the funclets 0x1070EAEA / 0x1070EF8F): out of every chunk (a
  missing one logs "Prop Collider not int dict"), empty chunks removed, `RecursiveCheck(node)`, then its
  anchors and overlaps drop it (from the last) and are cleared.
- `RecursiveCheck` 0x70DC20: shown when `hideCount == 0` and motivated: into the dictionary if needed (which
  may hide it), then if not yet on: on and active; the first time its children are bubble-sorted by priority and
  its first `IPropEnable` gets `OnFirstEnable`; `OnEnable`, sound (only active in hierarchy), achievement; the
  overlaps of lower priority hidden (in order, up to the first that isn't lower), children shown, motevees
  motivated, then those overlaps checked in order, children and motevees from the last. Otherwise a node that
  is on turns off the mirror way (every check from the last) and is hidden unless turned back on. The first
  `IPropEnable` is the prefab's (`BirdLanding`, `ButterflyLanding`) before the added `PropMeshReference`.

## Taking props down (E10b4, proven)

`ReturnProp` 0x70F2B0 walks the prefab and the instance side by side, breadth first: each node hidden,
checked, taken out of the dictionary, given back the prefab's priority and motivations, not turned on before;
an anchor's wires (from the last) unhooked from both anchors and returned; then every object's modifiers reset
(from the last), and the instance back into its pool keeping its world pose. `RemovePropsFromContainer`: each
child (from the last; one without a node ends it) hidden, checked, moved to the deconstruction container.
`IterateDeconstructProps` returns them from the last while `KeepGoing`. `OnReset` / `OnDoneResetting` hide and
show the secondary props.

Checked by `tests/placemaker_prop_placing.rs`: the WFC loops build a town on both sides with a fresh game
`PropPlacer`; after each step every prop object (poses bit for bit, flags, node states and relations, meshes,
collider bounds, anchors, wires, windows), `srcProps`, `materialMeshes` and the collider dictionary match. The
game's time-budgeted loops (`ModuleBuilder.Iterate`, `IterateDeconstructQubes`, `IterateDeconstructProps`) run
on `UniversalStopwatch.KeepGoing`, the scene's frame budget, which can stay spent for a stand-in: the test calls
their bodies directly in the loops' order. The game's squares and qubes hold the rotation (-0, -0, -0, 1),
which reaches the props' local rotations. Twelve steps (159 qubes, 329 prop objects) match; that town met no
wire anchor pair, propeller, wind vane or n-gon scaler, so those paths are ported but not yet seen (row E10d's
check covers the wires).

## Prefabs and pools (E10b1, proven)

`ts_data::props` (+ `scene::subtree`): the 71 prop prefabs the module library places, read from the player's
files: each a tree (root first, depth first) with local poses as stored, mesh, the renderer's `m_Enabled` and
materials (Unity 2020.3 `Renderer`: nine flag bytes, then aligned), and the fields of `Prop`, `PropNode`
(relations resolved to nodes of the prefab), `PropCollider` (a node can have two), `PropTransformer`,
`PropWireAnchor`. Checked against the game's own prefab objects node by node (`placemaker_prop_prefabs.rs`).

`placemaker::prop_placer` (`PropScene`: the engine transforms of row E11 and each prop object's state):

- `GetProp(src, parent)` 0x709EE0: the prefab's pool (made on first use by `UnityFunctions.GetNewGameObject`,
  which ignores its name: a `New Game Object` put under the pool container with `SetParent(.., false)`; the
  prefab appended to `srcProps`). An empty pool instantiates (`Instantiate(prefab, parent, false)`: local poses
  as stored, the root `<name>(Clone)`, relations inside the prefab pointing at the clones), sets `srcProp` and
  runs `RecursivePropNodeSetup`; otherwise the pool's last child is moved under `parent` keeping its local pose.
- `RecursivePropNodeSetup` 0x70E500: a node is reset (`inDict`, `hasBeenTurnedOn`, `hideCount`, the parent,
  child, overlap and motevee counts — not `anchorCount`, not the relations the prefab carries) and becomes the
  child of the nearest node above (`AddChild` / `AddParent`); each mesh filter with a mesh gives that node's
  `PropMeshReference` (added disabled when missing, `worldMaster` set) an item: batched (a new `BigMeshPart`,
  the renderer turned off) unless on the no-shadows layer (18) or drawn with the door material
  (`ShouldBeBatched`). Children the last first.
- `OnEnable`: `srcProps[i]`'s pool is the container's child `i`. `ChangeHideCount`: a byte up or down, 255 logs
  an error. `GetChunkMatrix(c)`: translation `2c + 1`.
