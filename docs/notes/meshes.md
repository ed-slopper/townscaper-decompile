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
