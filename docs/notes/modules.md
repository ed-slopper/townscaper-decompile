# Modules (E5)

The building pieces the wave function collapse places in the qubes, and the integer maths of turning them.
Port: `townscaper-rs/game/src/placemaker/modules.rs`; checked against the running game by
`game/tests/placemaker_modules.rs` (`TS_HARNESS=1`).

## Records (E5b, proven)

- `ByteQube`: one byte per qube corner, the lower four round the square then the upper four. Corner values:
  below 15 a material, 15 ground, 16 any, 18 air. `ByteQube..cctor` 0x1FD5D0 holds the corner permutations of
  the turns (`rotateForward`, `rotateBackward`, `mirror`, `tipForward`, `tipBackward`, `upsideDown`).
  `GetOriented(p)[i] = this[p[i]]`.
- `SideProfile`: six pairs of ushorts (sides 0..=3 round y, 4, 5). `OrientedModule`: a module index plus flags
  (orientation in the low bits, bit 4 decor, bit 5 empty). `OutlineUv`: two packed offsets and a border bit.

## Orientation and ModuleMath (E5c, proven)

`Orientation` (0x5C bytes): `int4x4`, `cornerTransform0`, `cornerTransform1`, `sideTransform` (ByteQubes) and
`inverted`. `ModuleMath..cctor` 0x331880 builds every table; all of them are equal to the game's, byte for byte.

- `corners`: `SbyteFloat3(±0.5)` = ±127, in ByteQube corner order.
- `sideMatrices[0..4]` = `(int4x4)inverse(Rotate(Euler(0, -90·s, 0)) * Translate(0, 0, -127))`; sides 4 and 5
  both use `Euler(-90, 0, 0)`, with z -127 and +127. Values are 0/±1 with a ±127 translation, so the exact
  `Quaternion.Euler` matters only so that the truncation to int lands right (ported natively, see unity.md).
  `invSideMatrices` is allocated and never filled.
- `orientations` (8): starting from identity, four times "record, then `m = m * rotY`" (`rotY` has columns
  (0,0,1), (0,1,0), (-1,0,0)), then `m = m * mirrorZ` and four more. The loops around them run once; the extra
  matrices they multiply by after the eighth orientation change nothing that is kept. Per orientation:
  - `int4x4` = `m` truncated.
  - `cornerTransform0[i] = j` and `cornerTransform1[j] = i` for the first `j` with
    `corners[i] == m.MultiplyVector(corners[j])`.
  - `sideTransform[a] = b` and `sideTransform[b + 4] = a` for the first `b` with
    `c3(inverse(side[a])) == mul(transpose(o), c3(inverse(side[b])))`. The inlined code dots each **column**
    with the vector; the plain `mul(o, v)` gives the inverse permutation, which the game's table rejects.
  - `inverted` = the float 3×3 determinant < 0 (orientations 4..=7).
- `orientationMulLookup[i, j]` = the orientation whose `int4x4` equals `mul(o_i, o_j)`.
- `orientationInverseLookup[i]` = the last `k` with `o_i.cornerTransform0 == o_k.cornerTransform1`.
- `cornerLinkLookup[a, b]`: a counter over the pairs `a < b`, both ways; 0 on the diagonal.
- `baryVectors[s]`: `c = GetClusterOffset()`; per sixth, `mid = (HexToPlane(RotateCW(c)) + HexToPlane(c)) * 0.5`,
  `v = -((mid / |mid|) / |mid|)`, `c = RotateCW(c)`. `GridGenerator`'s Compose reads them (E3).
- `groundByteQube` = (15 ×4, 18 ×4); `anyByteQube` = 16 ×8; `qubeForward` / `qubeBackward` / `qubeUpsideDown`.

Functions:

- `AreCornersLinked` 0x330E60 tests `(1 << (index & 31) & link) == 1`: true only for the pair with index 0
  (corners 0 and 1) when bit 0 is set. A game bug, kept.
- `GetMaterialCount` 0x3310E0 keeps the seen materials in a byte (`Bitwise.SetBit` drops bits 8+), so materials
  8..=14 count each time they appear. Kept.
- `GetIndexedMaterials` renumbers the materials 0, 1, ... by first appearance; values 15+ stay.
- `MarkHiddenCornersAmbigous`: a corner that is not air in the qube nor in its forward, backward and
  upside-down turns becomes 16 (any).
- `Mul(parent, child)`: `mul(p, c)`, the corner transforms composed with `GetOriented`, `inverted` xor; the
  child's `sideTransform` is kept as it is.
- `MultiplyPoint` / `MultiplyVector` (int3 and SbyteFloat3, w 1 / 0, SbyteFloat3 cut to low bytes),
  `MultiplyBounds` (both corners as points, then min / max). `GetSideTransform0/1` map sides 0..=3 through the
  table, return 4 and 5 as they are, throw otherwise.
