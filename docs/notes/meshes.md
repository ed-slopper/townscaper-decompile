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
