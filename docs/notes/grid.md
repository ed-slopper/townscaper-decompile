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

## Clusters and bits (proven, E3d)

A **bit** (`GridBit`, a `GameObject` under `bitContainer`) sits at a patch centre; its **clusters** are the six
neighbours `RotateCW^i((72, -72))` around it, each made of three patches (`GetPatchOffset`, `RotateCW2`).
`MotivateBit` / `MotivateCluster` / `MotivatePatch` count reasons to exist and reuse the first unwanted object.
A reused bit's child `MeshFilter`s go back to `WorldMeshes` only when it was done. A new bit's `GameObject` is
named "New Game Object": `UnityFunctions.GetNewGameObject` 0x3CCE80 drops the `"Bit " + n` name it is given.

`IterateBit` 0xE2D6A0: finishes the bit's patch first, then one `GridBit.State` step per call:
- **Verts**: the patch's corners, `HexToPlane(v - patch.hexPos)`.
- **Relax**: squaring passes while `keepGoing()` holds, up to 255 (`relaxationSteps`). Per quad the centre is
  `((r1 + r0) + r2) + r3) / 4`; the corners' offsets from it, turned onto corner 0 (270°, 180°, 90°), are summed,
  normalised (`math.normalize`) and scaled by `diagonal` (√2/2). Each corner's delta in the shared
  `GridGenerator.deltas` grows by `(centre + rotate(n)) - corner`; then every corner moves by `0.4 × delta` and
  the delta is zeroed.
- **Clusters**: `IterateCluster` on each of the six until done. A cluster finishes its three patches (one step
  per call), gathers their corners once (only corners on the three lines through the cluster centre are merged
  between patches), relaxes them like a bit (same centre order), and on the call after 255 steps lets go of its
  patches and is done.
- **Compose**: each corner takes the sixth `s = FloorToInt(fmod(SignedAngle(p, HexToPlane(1,-1)) + 360, 360) /
  60)` it lies in, and blends the bit's own relaxed corner with the matching relaxed corners of the clusters at
  `RotateCW^s((72,-72))` and one step on (their `patchVertOffsets` component by `s % 3` and `(s + 1) % 3`), with
  weights from `ModuleMath.baryVectors[s, s+2, s+4]`: `bitPlane + ((f0·w + f1·ra) + f2·rb)`. A corner off the
  bit lattice indexes past the cluster's list: `ArgumentOutOfRangeException` in the game.
- **Mesh**: per quad 5 vertices (corners, centre `(((0+1)+2)+3)/4`), relative to the bit; UV0 the grid hex
  positions, UV1 `(±1, ±1)` / `(0, 0)`, triangles round the centre; a new mesh every 35 quads.

`IterateFocus` 0xE30990 recentres `hexOrigo` on the patch at the centre of `Graph.genBounds`, motivates every bit
whose hex lies in the bounds grown by `bitExtent` (84), walking an onion one ring past the last ring used, then
releases the bits wanted before. `IterateGenerateCenter` motivates the centre bit on every call (its count is
the number of calls). `Measure` 0xE333E0 has no callers.

## Queries and meshes (proven, E3e)

- `GetQuad(float2)` 0xE2AD80: the plane point's hex, rounded (`math.round`, ties to even), picks the nearest
  cluster; the three bits round it are tried nearest first (`(x² + y²) + z²`, three compare-swaps), each only
  when `Done` and wanted. In a bit, a binary search of exactly 5 halvings (no `lo ≤ hi` test) over the sorted
  quads for `GetVertCost(hex) - 0.5`, then a scan alternating after / before that point. Each side stops when its
  quad's first corner leaves `y - 3.5 .. y + 4.5`; a quad counts when its first corner is within √6 and the
  point is on the inner side of all four edges (edge 0 sums the y term first, the others x first).
- `GetQuad(int2, int2, int2)` 0xE2A740: the quad of the patch's finished bit with edge `c0 → c1`, its
  `quadIndex` left 0.
- `GetClostestVertPosition` 0xE2A2A0: the quad's corner picked by `!(a2 > a0)` / `!(a3 > a1)` of the four edge
  distances.
- `GetAdjecentQuad` 0xE29AB0 (no callers) and `AppendQuadList` 0xE289A0 (`DrawPlane`): the neighbour across edge
  `v0–v3` (or `v1–v2`), searched from `(n + (quadIndex - 20) * 4) % n` (Ghidra drops the `* 4`), returned
  rotated so the shared edge is its `v1–v2` (or `v3–v0`). The strip walk stops at a quad whose edge `p1 → p0`
  faces along the camera direction's perpendicular. The cached patch carries over between the two walks; the
  return is true when any quad was added; `hoverIndex` counts the quads put in front.
- `GetVertOrIterate` 0xE2BC70: the vert's quads from the up to three bits sharing it (`HexToPatchHexes` mask),
  each added with `Vert.AddQuad` and its corner angle (`Vector2.Angle(prev - a, next - a)`) summed, stopping past
  350°. An unfinished bit is iterated `while (keepGoing())` (not until done; an always-true `keepGoing` never
  returns); `quadCount` -1 / -2 / -3 for no bit / no `keepGoing` / still unfinished.
- `PopulateBorderMesh` 0xE37920 (`BorderDrawer`): quads near the vert that cross `Graph.border` and touch the
  vert: 4 world vertices, triangles `0 1 2 2 3 0`, UVs into `AoBaker.marchingSquaresTex` by the inside pattern
  weighted with `AoBaker.powers` (`u = tx/2 + 2·Σ·tx`), +1 at the vert's corner.
- `SetQuadMesh` 0xE38BB0 has no callers and assumes a 4-vertex-per-quad, 300-vertex mesh layout the Mesh step no
  longer builds (left unported); `OnDrawGizmos` is editor-only.
