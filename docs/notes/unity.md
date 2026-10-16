# Unity and .NET behaviour the game depends on (F11)

Ported in `townscaper-rs/game/src/unity/`, checked against the running game with the harness
(`game/tests/unity_*.rs`, `TS_HARNESS=1`). Engine addresses: `GameAssembly.dll` RVAs, or `UnityPlayer.dll`
VAs (image base 0x10000000, imported with its shipped PDB by `tools/ghidra_import_player.sh`).

## Which game calls go where (survey, 2026-10-15)

Cpp2IL `[Calls]` over `Placemaker` and `Os`: `List<T>` 317 call sites, `Mathf` 287, `Vector3` 166, `Time` 130,
`RectTransform` 104, `Mesh` 95, `Dictionary` 90, `Matrix4x4` 78, `Vector2` 46, `Quaternion` 36, `Color` 35,
`Vector4` 27, `Rect` 24, `Random` 20, `Bounds` 19, `Unity.Mathematics.math` 16 (most of `Unity.Mathematics` is
inlined by IL2CPP and does not show as calls). Rows: F11a `Mathf`, F11b vectors/matrices, F11c `Random`,
F11d `Unity.Mathematics`, F11e collections, F11f host APIs.

## Mathf (F11a, proven)

All 22 overloads the game calls, equal to the game on 41,360 calls (edge values, NaN, infinities, out of range,
fixed-seed spreads): float functions 26,886, int/array 8,150, `PerlinNoise` 6,324.

- Follow Unity's C# definitions, **not** Ghidra's rendering of them: `Sign(NaN)` = -1 (`f >= 0 ? 1 : -1`),
  `Clamp01(NaN)` = NaN, `Min`/`Max` as `a < b ? a : b` / `a > b ? a : b`. The harness caught each of these.
- `(int)` of a float/double is x86 `cvttss2si`/`cvttsd2si`: NaN and out-of-range give `int.MinValue`
  (`unity::conv`). `Mathf.Abs(int.MinValue)` is `int.MinValue` (branchless, no exception).
- **x87 results:** a `float` function returns in `ST0` on x86. Where the compiled code does its last arithmetic
  on the x87 stack, the result is rounded to `float` once from higher precision: `LerpAngle` (last multiply-add,
  `conv::x87_mul_add`) and `Mathf_CUSTOM_PerlinNoise` (`FADD 0.69`, `FDIV 1.483`). Windows runs the x87 unit at
  53-bit precision, so an `f64` computation rounded once to `f32` is exact.
- `NextPowerOfTwo` (native) is bit smearing on `value - 1` as unsigned.
- `PerlinNoise` (native, `PerlinNoise::Noise` 0x105DE770): improved Perlin noise of **|x|, |y|** (mirrored
  around 0), Ken Perlin's permutation (the engine's `int[512]` is the same 256 twice), integer part by
  truncation, fade input `minss(1, f)`, fade `((6t − 15)t + 10)·((t·t)·t)`, lerps `(b − a)t + a`, all SSE
  single precision; the wrapper's `(n + 0.69) / 1.483` is x87.
