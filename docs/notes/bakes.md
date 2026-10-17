# Lighting and ground bakes (E9)

`placemaker::shadow_baker` (E9a). The GPU passes are described for Bevy (`ShadowBakePass`); checking what they
draw needs the game's shaders (row C3).

## The sun: `ShadowBaker` (E9a, proven)

State: `current` and `target` (`Vector2`, 0..1: x the turn, y the height), `lamps`, and the flags
`framingDirty`, `angleDirty`, `snap`, `shouldBake`. `.ctor` 0x976000: `resolution = 1024`, `onNewSunAngle` an
empty lambda. `.cctor` 0x975EC0: the ids of `_WorldToShadow`, `_ShadowToWorld`, `_ShadowRange`, `_SunDir`,
`_SunColor`, `_ShadowColor`, `_BgColor`, `_Lamp`, in that order.

- `GetQuaternion(a)` = `Euler(Lerp(2, 90, a.y), a.x * 360, 0)`: the sun is between 2° and 90° up.
- `SetAngle` / `SetX` / `SetY` / `RotateAngle`: set `target` (`RotateAngle` adds, clamps y to 0..1 and wraps x
  as `fmod((double)(x + 10f), 1.0)`), then `angleDirty`, `WorldMaster.SetDirty` (the town gets saved), baker on,
  `shouldBake`. `OnBuilt`: `current = target`, everything dirty, `snap` (the next frame doesn't ease).
- `Load(save)`: `current = target = (sun.r, sun.h) * 0.003921569f`, `lamps = 0`, framing and angle dirty,
  baker and camera on, `onNewSunAngle`. `Save`: `sun.h = (byte)(int)(target.y * 255)`, `sun.r` likewise from x.
- `LateUpdate`: when `current != target` (Unity's 1e-5 `Vector2` test), it snaps when closer than 1e-5
  (squared), then eases with `WorldMaster.expTFast` (0 after `OnBuilt`): x by `LerpAngle` in degrees, y by
  `Lerp(target, current, t)`. `LerpAngle` returns on the x87 stack and the `* (1/360f)` is done there, so the
  turn is rounded to `float` once (`mathf::lerp_angle_x87`). Then `MaybeUpdateAngle`, `MaybeUpdateFraming`, the
  lamps, and a due bake turns the camera on.
- `MaybeUpdateAngle`: the camera turned to `GetQuaternion(current)`; `_SunColor`, `_ShadowColor`, `_BgColor`
  from the three gradients at `current.y`.
- Lamps (`LateUpdate`, and the same in `UpdateLamps`): towards 1 at night (`target.y == 0`), else -1, at
  `0.7 * deltaTime`; when they moved, `_Lamp = (lamps, night, 1 - night, 0)`.
- `MaybeUpdateFraming`: with `rot` the camera's rotation, `sun = rot * forward`. Into the camera's frame
  (`Inverse(rot) *`): a box from the centre of `graph.bounds` on the ground (`HexToPlane`, y = 0), grown by each
  corner column's own position and its last child's (columns with children, the last column first), then 3
  bigger each way. `count` sums the columns' child counts. The camera goes to `rot * center` at `rot`,
  orthographic size `max(extents.y, extents.x)`, clip planes `∓extents.z`. `_WorldToShadow = Scale(0.5/size,
  0.5/size, 1/size.z) * Translate(size, size, extents.z) * cam.worldToLocalMatrix`, `_ShadowToWorld` its inverse,
  `_ShadowRange = (size.z, 1/size.z, 0, 0)`, `_SunDir = (sun, 0)`. The shadow plane goes where the sun ray
  through the camera meets the ground, turned by `LookRotation(up with y = 0) * Euler(0, 180, 0)`, scaled
  `(size.x * 0.1, 1, size.y / sun.y * 0.1)`.
- `OnStart`: `tex0` (1024², 24-bit depth, point filter) as the camera's target, `opaqueSortMode = 2`, replacement
  shader with tag "Shadow"; `lampTex` (32², clamp): per row `i` a random blue byte (an xorshift from 0x1E36FFC,
  `(s * 255) >> 32`) and a lit window from `Clamp(i, 1, 28)` on (`(170, 20)`, then `(20, 100)`, then white; alpha
  1); the globals `_LampTex`, `_ShadowTex`, `_ShadowNoiseTex`, `_ShadowTex_TexelSize`.
- `ShadowBakerCamera.OnPostRender`: the camera turns itself off: one render per bake.

Engine detail met here: a `Transform` gives back a rotation a little different from the one set (an ulp or two,
again on each set; it isn't a renormalisation). The framing reads the camera's rotation in the same frame it
was set, and sets it again before reading `transform.up`: the port reads both through `SunHost::camera_rotation`.
Exact transform storage is row E11.

Checked (`tests/placemaker_shadow_baker.rs`): ids, the scene baker's `lampTex` and `_ShadowTex_TexelSize`; a
fresh baker and camera on an inactive object with the scene's gradients: 300 random edits, saves and loads, then
40 `LateUpdate` frames around the scene's town (state, colours, lamps, both matrices, range, sun direction, camera
position, plane position, rotation and scale), all bit-exact. The camera's size and clip getters are stripped;
`_WorldToShadow` is built from them.

## Ambient occlusion: `AoBaker` (E9b, proven)

`placemaker::ao_baker`. The graph fills the shadow meshes (E4d, `GetShadowMesh`); the baker turns them into an
atlas of height slices.

- `.cctor` 0x7B4050: `uncoveredCol` white, `uncoveredCol32` its bytes, `onTextures` an empty lambda, the ids of
  `_Tex3dUvSize` and `_Tex3dWorldBounds`. Constants: `texSize` 2048, `texelSize` 1/2048,
  `shadowMeshMaxVertCount` 65532.
- `OnStart`: `renderTexture0` / `renderTexture1` (2048², 24-bit depth), four materials, and the marching-squares
  lookup: `n = 17^4 = 83521` cells (four corners, 17 values each: 16 greys `(i + 0.5)/16` with alpha 0 and the
  uncovered white), each a 2x2 texel block, the texture `2 * NextPowerOfTwo(sqrt n)` by `2 * NextPowerOfTwo(n /
  that)` = 1024 x 512. Cell `i` is at `(2i mod 1024, 2 * (2i / 1024))`, bottom-left = digit 0 (lowest, base 17),
  bottom-right = digit 1, top-right = digit 2, top-left = digit 3. `powers = 17^k` (1, 17, 289, 4913). The
  lookup is blitted through the marching-squares material into `coloredMarchingSquaresTex`; then `Restore`.
- `Restore`: `_MainTex` of the AO and stack materials = the coloured lookup; `_VoxelTex` = `renderTexture0`,
  `_AoTex` = `renderTexture1`; the camera targets `renderTexture0`. `OnEnable` restores once a material exists.
- `BakeAo`: the graph's `shadowMeshBounds` grows 2 in x and z; a slice is (size.x, size.z), there are
  `boundsHeight + 1` slices, `countX` across and `countY = CeilToInt(total / (float)countX)` down. `countX` goes
  up from 1 while the atlas's longer side (`max(countY * size.z, countX * size.x)`, x when the other is NaN)
  strictly shrinks; `size` becomes the slice's share of that side. `_Tex3dUvSize = texParams = (size,
  size.x / bounds.x, countX)`, `_Tex3dWorldBounds = (min.x, min.z, size.x, size.z)`. Dirty shadow meshes are
  uploaded (the last first, bounds `float.MaxValue` so they are never culled) and the camera turned on.
- `OnPostRender`: `renderTexture0` blitted into `renderTexture1` through the spreader; the camera off.
  `ApplyMaterialColors`: the lookup blitted again, then `BakeAo`.

Checked (`tests/placemaker_ao_baker.rs`): ids, uncovered colours, the scene baker's lookup texture (every texel)
and `powers`; 300 random bounds and heights through `BakeAo` on a fresh inactive baker (fields, both globals,
the camera), bit-exact.

## Sandbanks and shore water: `Sandbanker` (E9c, proven)

`placemaker::sandbanker`. `Iterate` 0x84FBA0 is a state machine (`Reset`, `Gather`, `Smooth`, `ApplyWater`,
`ApplySand`, `Done`), run by `WorldMaster.Iterate` until it returns false (`Done`).

- `Reset`: lists and dictionary cleared, `index = 0`.
- `Gather` (profiled "Sandbanker Gather"): while `index < graph.squares.childCount` and `WorldMaster.KeepGoing`,
  square `index` (if it has a qube child): the first qube's `possibleModules[0]` (when
  `possibleModuleListIndex != -1`), its module's oriented meshes with `materialType` 0, oriented by
  `MulOrientations(module, mesh)`. For each triangle, the first rotation whose first edge is on the water line
  (`|(a.y + b.y) * 0.5 - 0.1| < 0.01`): ends swapped unless the orientation is inverted, `+ 0.5`, through the
  square (`GetTs`, `ModuleBuilder.MultiplyPoint`, the square transform's `TransformPoint`), `y = 0`. Each end is
  a vertex pair (ground, raised twin), shared through `RoundToInt(z * 100) * 6113 + RoundToInt(x * 100) * 6101`
  (taken out of the dictionary by the second edge using it); new pairs get the coverage `ts · corners`, a
  corner's value `groundCoverage * angle * 0.00034722223` (0.8 everywhere when the qube has no upper voxel,
  `voxelCount & 0xF0 == 0`). The quad is `(i0, i1, i1+1, i1+1, i0+1, i0)`. Done when every square is read.
- `Smooth` (whole, one call): coverage spread 3x along edges (half the difference); pair normals = the sum of
  edge `(-dz, 0, dx)`; ground vertices out by the raw sum times `Lerp(0.8, 0.2, coverage)`, normals normalised;
  6 rounds of edge relaxing (half the edge); twins up 0.186; per quad (last first) the twins' sides along the
  normals: neither outside → the quad lies flat on its twins' ground points, only the second → turned and the
  first twin laid down, only the first → the second laid down, both → turned when that diagonal is shorter;
  3 rounds over triangles (last first, each rotation): a ground edge under a raised point spreads its even
  vertices 0.3 of the edge apart; in round 1, an odd-odd edge with a raised end pushes the even point opposite
  to 0.3 off it (0.6 of the gap); slope caps of 0.4 (raised points at most 0.4 of the distance to the ground
  point they lean on, or across a ground edge); twins pulled in along the normal by 0.4 of their height.
- `ApplyWater`: water mesh cleared; two vertices per pair at the ground point, the first pushed out along the
  water normal (sum of `(d.z, 0, -d.x) * 0.5` of the normalised edges) by `Lerp(2, 0.3, coverage)`; a middle vertex
  per quad and triangles `(m, b+1, a+1), (a, m, a+1), (m, b, b+1)`; 2 rounds relaxing the a-m and m-b edges
  (0.3); UVs: `x = LerpUnclamped(PerlinNoise(b.x/10, b.z/10), 0.5, 0.1)` (+1 on the outer vertex), middles the
  average, then `y` along the flow (half and 0.3 of the bank lengths, plus 0.2 of the middle's offset across).
- `ApplySand`: sand mesh cleared; triangles flat on the ground dropped (last first); each triangle gets a back
  face on a copy of the vertices; tangents are the summed face normals, normalised, `w = PerlinNoise(x/5, z/5) +
  0.5` (negated on the copy; the `+ 0.5` is done on the x87 stack: `mathf::perlin_noise_x87`), the copy's
  tangents in swapped pair order (as the game appends them); vertices doubled; bounds recalculated.
- `UpdateShadeUvs` (`OnEnable`, `OnValidate`): `_SandUv = (uvPos, uvSize) * 0.25`. `Clear`: both meshes cleared,
  back to `Reset`. `OnStart`: both mesh filters get a pooled mesh (`WorldMeshes.GetMesh`), 32-bit indices.

Squares are only translated, so `TransformPoint` is `p + position` (exact here). Checked
(`tests/placemaker_sandbanker.rs`): a fresh sandbanker with the scene master and two fresh meshes stepped by the
game's `Iterate` from `Reset` to `Done`, ours replaying each step (`Gather` as far as the game's frame went): the
lists after every state, the water mesh (vertices, UVs, triangles) and the sand mesh (vertices, triangles,
tangents) bit-exact, and `_SandUv`.

## Border and draw plane (E9d)

- `BorderDrawer` (`placemaker::border_drawer`, ported): `Draw` 0x7C2A40 fills its mesh with
  `GridGenerator.PopulateBorderMesh(hoverData.srcVert, mesh)` (E3, checked) and draws it with
  `Graphics.DrawMesh(mesh, identity, material, layer 0, no camera, submesh 0, block)`; the block's `_MainTex` is
  `aoBaker.marchingSquaresTex` (`SetupBlock`, from `OnEnable` once the texture exists and from `OnStart`, which
  also makes the mesh). `BoundsUpdated` is empty.
- `DrawPlane` (read, not ported): a development aid that is in none of the game's scenes (the harness finds no
  instance). With right shift held it starts a strip of quads at the hovered vert (`SetToHoverData`: the vert's
  quad on the hovered side and the next, turned to start at the vert), grows it with
  `GridGenerator.AppendQuadList` along the camera's plane direction (`IterateUpdate`, `AllowedQuad`: quads
  facing away from `camDir`, the camera's forward turned 90° on the plane), and meshes each quad as four corners
  at y = 0 / -1 plus a centre. Nothing in the game reads it.
