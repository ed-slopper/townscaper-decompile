# Mesh building (E7)

How the collapsed qubes' modules become meshes: `Placemaker.ModuleBuilder` writes each module's packed
vertices (`ModuleMesh`, E5d) into pooled ranges of big shared meshes (`BigMeshMaster`), bent into the qube's
square. Port: `townscaper-rs/game/src/placemaker/module_builder.rs`, mesh data kept as plain data for Bevy.

## Module geometry (E7a, proven)

A module vertex lives in the unit qube: x and z across the square, y up. The square's four corner matrices
(`Square.m0..m3`, one per corner, from the graph, E4) carry it into the world:

- `GetTs` 0x32FE30: the corners' bilinear weights `((1 - z)(1 - x), (1 - z)x, zx, (1 - x)z)`.
- `MultiplyPoint` 0x3302D0: `(0, y, 0)` through each corner matrix (`Matrix4x4.MultiplyPoint`), blended
  `((a·ts.x + b·ts.y) + c·ts.z) + d·ts.w` per component (grouping from the asm). The vertex's x and z enter only
  through the weights.
- `MultiplyVector` 0x3304E0: the same with the full vector and `Matrix4x4.MultiplyVector` (normals, tangents).

Checked on 40 random sets of corner matrices × 100 vertices (12,000 results, bit for bit).

## Big meshes (E7b, proven)

`BigMeshMaster` holds one `BigMeshGroup` per material (gathered from its children at `OnStart`); a group's
`BigMesh`es share up to 65,535 vertices each. A module's share is a `BigMeshPart`: its vertex slots and the
start of each of its triangles in the big mesh's `tris`. Port: `placemaker::big_mesh` (plain data; the Unity
`Mesh` upload that `MaybeApply` does stands as an `applied` count, STANDIN).

- `availableCount` = free slots + room left (`availableIndexes.Count - createdCount + 0xFFFF`).
- `GetBigMesh` 0x7B9100: the first group whose material is the one asked for (none: `Debug.Log(material)` and
  null), in it the first big mesh with `availableCount >= vertCount`, else a copy of the group's template (child
  0) with a fresh mesh from `WorldMeshes.GetMesh`, added to the group.
- `AddMesh` 0x7B86D0: the part's vertices go first into freed slots, taken from the back of `availableIndexes`,
  then appended (`createdCount++`); normals, tangents and uvs only when the part has them, the big mesh's lists
  first padded with zeros up to `createdCount`. Triangles likewise: into freed triangles from the back of
  `availableTris`, then appended; indexes are the part's slots. The big mesh is dirty. (The `BigMeshList`
  overload forwards.)
- `ReturnMesh` 0x7B9A30: the part's triangles are zeroed (degenerate), its vertex and triangle slots go back to
  the free lists (vertex data stays), the part forgets the big mesh; nothing for a part without one.
- `MaybeApply` 0x7B9460 (each big mesh, from `OnLateUpdate`, groups and meshes from the back): only when dirty.
  Every created slot free → the object is hidden and everything cleared. Else shown; normals / tangents / uvs
  (when not empty) padded to `createdCount`; whole zero triangles at the end of `tris` are cut off, each first
  removed from `availableTris` (one that isn't there is logged and stops the cut); the lists go to the mesh.
- `BigMeshList.Populate` 0x7B8250 (props, E10): a mesh filter's mesh into world space by its transform
  (points, normals, tangents with w kept). Not exercised yet.

Checked on the scene's `BigMeshMaster`: one group's big meshes copied into ours, then 300 random steps of
`AddMesh` (0–400 vertices, now and then 20,000–50,000, optional normals / tangents / uvs, random triangles) and
`ReturnMesh`, each followed by `OnLateUpdate`; the parts' slots every step and the group's big meshes (all lists,
counts, dirty and active) every 25 steps, then with everything returned: equal. The group grew to 4 big meshes.

## The module builder (E7c, proven)

Port: `placemaker::module_builder::ModuleBuilder`. Module containers (`ModuleContainer`: the shown module and its
parts in the big meshes) sit under the qubes, the module's first and its decor after it; spare ones wait in
`moduleContainerPool`, taken-down ones in `moduleContainersToDeconstruct`. The sandbanker, sounds, camera and
props stay behind `ModuleBuilderHost`.

- `ApplyModule` 0x32E5F0: a real module (index ≠ 0) in a qube that isn't fully covered: unless the qube already
  shows it (its first container's module, then nothing happens), that container is taken down; a bottom qube
  resets the sandbanker; a qube whose normal faces away from `Camera.main` plays sound 3 (unless dimmed); a
  container from the pool (`GetModuleContainer`: its last, or a new one) goes first under the qube; the qube
  joins the grass queue. Then each of the module's meshes, turned by `orientations[MulOrientations(module,
  mesh)]`:
  - vertex: the orientation's sbyte `MultiplyPoint`, as floats (s/254), + 0.5 per axis; a bottom qube clamps y
    to `Mathf.Max(0, y)`; `GetTs`, then `MultiplyPoint` into the square, + the qube's world position;
  - normal / tangent: the orientation's `MultiplyVector`, `MultiplyVector` into the square, normalized (tangent
    w = 1);
  - uv: the colour `c` is 15 when the qube's corner 0 is 17, else the qube's corner value at
    `cornerTransform1[top bit of mesh.corner]`; `u = ((c & 3) + uv.x)·0.25`, `v = ((c >> 2) + uv.y)·0.25 +
    2·outlineUv` (the outline byte rides in v);
  - triangles reversed unless the orientation is inverted; meshes of the fence material are doubled back to
    back (tangent w = 0, triangles reversed).
  Meshes of one material in a row go into the big meshes together (`BigMeshMaster.AddMesh`), each into the
  container's next part (its old parts first, `GetNewBigMeshPart`). Module 0 or a fully covered qube: every
  container under it taken down.
- `ApplyDecor` 0x32D340: for each decor after the module (`possibleModules[1..]`), a container already under the
  qube with that decor is moved to its index; otherwise a new container at that index, and each decor mesh is
  built like a module mesh but turned by the orientation's float matrix (`Orientation.matrix`), with no bottom
  clamp, colour 15 for dirt meshes (`materialType == 2`), no outline in v, and one part per mesh. When anything
  was built: sound 3 and the grass queue. Containers past the decor are taken down.
- `MarkModuleContainerForDeconstruction` 0x330090: props removed (`PropPlacer`), the container moved to the
  deconstruction list, its parts returned to the big meshes from the back (the parts stay in the container,
  empty, for reuse). With no prop placer on the master, the game skips all of it.
  `MarkQubeForDeconstruction` 0x330190 does that for each container under a qube (the graph pools the qube, E4).
- `IterateDeconstructQubes` 0x32FED0: from the last, while `KeepGoing`, each container back to the pool with
  `OrientedModule.empty` (`DeconstructModuleContainer` 0x32FB90; child mesh filters, which the shipped modules
  don't make, would go back to `WorldMeshes`). `Iterate` 0x32FF90: the grass queue, `PropPlacer.PlacePropsInQube`
  for the qubes still awaiting (E10).

Checked with the wave function collapse's loop test (`tests/placemaker_wfc.rs`): the game's stand-in master gets
a fresh `BigMeshMaster` whose groups copy the scene groups' materials and templates (so the scene's own
`LateUpdate` never applies it between steps), ours the same groups empty, materials matched through the module
meshes. After each of 12 actions and the collapse, `IterateDeconstructQubes` and `OnLateUpdate`; every three
steps every big mesh of every group (all lists, bit for bit), every qube's containers (module and parts' slots),
the pool, the deconstruction list and the grass queue: equal, up to 5,177 live vertices, 796 modules and 357 decor
built. The fence doubling was not reached (no fence module came up); `Iterate` (props) is not run.

## Pools, materials, palette (E7d, proven)

- `WorldMeshes` (`placemaker::world_meshes`): a stack of spare `Mesh`es and a pool of `"Mesh"` objects (a
  `MeshRenderer` and a `MeshFilter`) kept under its own transform. `GetMesh` pops the stack or makes a mesh
  (`MarkDynamic`). `GetMeshFilterOnly` takes the pool's last object (activated, moved under the caller), else
  makes one there; `GetMeshFilter` also gives it a mesh (dropping whatever mesh it had). `ReturnMeshFilterOnly`
  deactivates the object, puts it on `LayerMaster`'s first layer and back under the pool with an identity local
  transform; `ReturnMeshFilter` first clears its mesh onto the stack; `ReturnMeshFilters` does that for a
  container's children from the last; `ReturnMesh` clears a mesh onto the stack. `IterateRefillPools` runs
  `GetMeshFilter` under the pool itself for `10 - pool size` rounds (after the first new object it hands the
  same last object out again, each time with another mesh: kept), then tops the stack up to 10 meshes, checking
  `KeepGoing` after each step. Checked on a fresh `WorldMeshes`: the refill, then 400 random gets and returns
  between two parents; the pool, the stack and every object (active, parent, mesh) matched one to one.
- `MaterialMaster` (`placemaker::materials`): `CreatePaletteTexture` makes a 16 × 2 `RGBA32` texture (clamped,
  point filtered): row 1 transparent black, row 0 the palette's first 15 colours then `stoneColor` (the bytes
  `SetPixel` stores; `GetPixels32` is stripped, so the test reads `GetPixels` back to bytes). `OnStart`:
  `_ShadingTexLightStack = (1, 1, 1, 0)`; `OnOnEnable`: `_GrassColor`, `_GroundColor = backgroundColor × 0.6`
  (alpha too), `_RoofColor` (read back with `GetGlobalVector`: stored as given). The constructor's colours
  (blue, green, yellow, red, gray) are replaced by the scene's.
- `PaletteHolder`: `SelectNextPicker(delta)` = `(selected + count + delta) % count` (C# remainder: negative for a
  negative sum), `SelectPicker(i)` as given, each then `onNewColor`; checked over 300 steps.
- `BitMeshQuadChange.CompareTo` (unused by the game): `hexPos.x`, `hexPos.y`, then `quadIndex` compared with
  itself (always equal; confirmed on the harness).
