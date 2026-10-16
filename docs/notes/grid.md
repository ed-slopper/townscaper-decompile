# The irregular grid (E3)

`Placemaker.GridGenerator` builds the irregular quad grid in hexagonal **patches**, groups them into **clusters**
for relaxation and meshes them in **bits**. Ported in `ts_game::placemaker::grid_generator` and
`ts_game::placemaker::quads`; checked against the running game (`game/tests/placemaker_*.rs`).

## Units (proven, E3b)

Grid positions are cube hex coordinates × `GridConstants.hexScale` (12), stored as `int2` (`z = -x - y`).
`HexToPlane` = `HexMath.HexToPlane` × `hexToPlane` (1 / 8.22). Patch centres sit on a 72 lattice: multiples of 216
skewed back by `(p + RotateCCW(p)) / 3`. Grid centroids (`HexToGridCentroid`) use a 2304 lattice offset by 2160
in y and z.

## Patches: `IteratePatch` 0xE30F50 (proven, E3c)

One call advances one `HexPatch.State` step (Quadrangulate: one merge per call).

- **Verts.** `d = hex3(hexPos) - HexToGridCentroid(hex3(hexPos))`, and a `Unity.Mathematics.Random` seeded with
  `hex3(hexPos).GetHashCode()`. A hand-made `CustomGridPatch` is drawn (`CustomGridLibrary.GetRandom`, `NextUInt`
  inlined) when
  - `d` is zero: `centroidPatches`, rotation `NextUInt(12)`;
  - two components of `d` are equal (base rotation 0 / 1 / 2 for x = y, y = z, z = x): at distance 72 a
    `centroidCurvePatches` one, re-seeded with the **centroid's** hash, rotation base (+3 when `d.x > 0`); else a
    `straightPatches` one, rotation `NextUInt(4) * 3 + base`;
  - otherwise in the new-grid noise (the `IsNewGrid` test, inlined): `genericPatches`, rotation `NextUInt(12)`.

  A rotation above 5 mirrors (swaps the x / y axes; the corner order of every polygon is then reversed). The
  patch's corners go through the int3x3 of the rotated axes; state jumps to Quadrangulate with the custom
  patch's `subdivisions` and `quadTriCutoff`. A 255 slot (no patch) or no case falls through to the procedural
  hexagon: `subdivisions = GetSubdivisions(hexPos)`, corners `hexPos + HexOnionIterator.hexPos2 × 9 × 2^s` while
  `layerIndex < 8 / 2^s + 1`.
- **Triangulate.** Ring by ring, side by side: `ring - 1` triangle pairs plus one more triangle per side, corner
  indexes as bytes (wrapping). `quadTriCutoff = quads.Count`.
- **Quadrangulate.** Start offset `Mathf.Abs(((u ^ 0xFC000000) << 5) ^ u)` with `u` = xorshift (13, 17) of
  `x * 0x83B58237 - 0x5626E641 + y * 0x833E3E29 - quadTriCutoff`. Careful: the `0xFC000000 << 5` keeps bit 31
  (found by measuring the game's first merged edge on a closed mesh). From there it walks the triangle edges; an
  edge with both ends `IsBorder` is skipped; the first edge whose reverse is in another triangle merges the two
  (both removed, the quad appended, `quadTriCutoff -= 6`). No merge left: Subdivide.
- **Subdivide.** `subdivisions` times: every quad into four around its centre (`sum / 4`), then every triangle
  into three quads around `sum / 3`; edge midpoints are shared through a `Dictionary<int2, byte>` (the second
  ask removes the entry); the triangles are then removed and `quadTriCutoff = 0`.
- **Sort.** Quads into a new list by binary search on their lowest `GetVertCost`, each rotated to start at that
  corner. **Original bug, kept:** the lowest cost is `Min(Min(c0, c1), Min(c1, c2))`, which leaves out the fourth
  corner, while the rotation choice does look at it.

`MotivatePatch` 0xE34FE0 reuses the first patch with no motivations left (lists cleared, `quadTriCutoff` and
state reset, `subdivisions` kept, re-keyed in `patchDict`); `UnmotivatePatch` 0xE392C0 only decrements (the
count is a byte and wraps).
