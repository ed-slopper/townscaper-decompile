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
