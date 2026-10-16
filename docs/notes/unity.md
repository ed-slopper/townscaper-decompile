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

## Vectors, quaternions, matrices, geometry (F11b, proven)

`unity::{vector, quaternion, geometry}`: the members the game calls of `Vector2/3/4`, `Vector2Int`,
`Quaternion`, `Matrix4x4`, `Bounds`, `Rect`, `Plane`, `Ray`, `Color`, `Color32`, equal to the game on fixed-seed
inputs with zeros, axes, NaN, huge values (`game/tests/unity_vectors.rs`: 6 groups, ~70,000 calls).

- Managed members are Unity's C# definitions; magnitudes are `(float)Math.Sqrt((double)sum)`;
  `float.GetHashCode` is the bits with ±0 → 0; `Color32(Color)` is `(byte)Mathf.Round(Clamp01(c) * 255)`;
  `Color.yellow` is (1, 0.92156863, 0.015686275, 1).
- Native members read from UnityPlayer.dll: `Quaternion.Inverse` = conjugate; `Quaternion.Lerp` clamps `t`
  (NaN stays NaN), picks the hemisphere by the dot, normalises with `(float)sqrt` in double;
  `LookRotation` = `LookRotationToMatrix` + Shoemake `MatrixToQuaternion`, else from-to rotation from +Z,
  else identity; `Matrix4x4.inverse` = Mesa's Gauss-Jordan (`InvertMatrix4x4_Full`), zeros when singular;
  `TRS` = `QuaternionToMatrix` with scaled columns; `lossyScale` = column lengths, x negated by a negative
  3×3 determinant; `LookAt` = `LookRotationToMatrix` + translation.
- `LookRotationToMatrix` with a NaN input or a frame failing `|y·y − 1| <= 1e-6` writes **nothing**:
  `Matrix4x4.LookAt` then returns uninitialised stack. Our port uses identity there (STANDIN) and the test skips
  those inputs (`LookMatrix::Unwritten`).
- The harness passes value-type `this` unboxed (il2cpp's value-type methods take the data pointer).

## UnityEngine.Random (F11c, proven)

`unity::random::Rand`: the engine's generator (UnityPlayer.dll `Random_CUSTOM_*`), Marsaglia's xorshift128.
Equal to the game on 7,200 draws from 300 random states plus 300 `InitState` seeds, the state compared after
every round (`game/tests/unity_random.rs`).

- `InitState(seed)`: `s0 = seed`, `sᵢ = sᵢ₋₁ · 0x6C078965 + 1`.
- `value` = `(next & 0x7FFFFF) · f32::from_bits(0x34000001)` (1/8388607, so 0..=1 inclusive).
- `Range(min, max)` (float) = `(1 − v)·max + v·min`; `Range(int)` = `min + next % (max − min)` (unsigned), the
  mirror when `max < min`, no draw when equal.
- `insideUnitSphere`: a unit vector (z = `(1 − v) − v`, θ = `(1 − v')·2π`, `cos`/`sin`/`sqrt` in double) times
  `pow(v'', (double)(1f/3f))`.
- The game never seeds it and draws from it every frame (birds, butterflies, click effects, audio pitch, the
  loading screen): runs can't repeat the original's draws, only the generator's behaviour. The harness compares
  inside one main-thread `batch` so no game frame draws in between; `InitState` and the state accessors are
  stripped from the managed side, so the test calls the engine's icalls through the harness's `native` call.

## Unity.Mathematics (F11d, proven)

`unity::mathematics`: the out-of-line members the game calls (most of the package is inlined into the game's
code as plain arithmetic and is ported with each game function). Equal to the game on 1,500 rounds of
`math.length`/`normalize(float2)`, `abs`/`max(int3)`, `mul(int3x3, int3)`, `mul(int4x4, int4)`,
`mul(int4x4, int4x4)`, `float4x4(int4x4)`, `math.inverse(float4x4)` (the shuffle-based cofactor inverse),
`int3.GetHashCode`, and 1,000 seeds × 6 steps of `Unity.Mathematics.Random` (`game/tests/unity_mathematics.rs`).

- `sqrt` is `(float)Math.Sqrt((double)x)`; `normalize(x)` = `x * (1 / sqrt(dot(x, x)))`.
- `Random` is xorshift32: the constructor stores the seed's first step; `NextState` returns the old state;
  `NextFloat` = `asfloat(0x3F800000 | (s >> 9)) − 1` (Ghidra renders the bit cast as a convert);
  `NextFloat(min, max)` = `f·(max − min) + min`; `NextInt(min, max)` = `min + (range·s >> 32)`.
  `PropPlacer` uses `NextFloat(min, max)` and `StorageUtils` `NextInt(min, max)`: those draws are reproducible.
- `int3.GetHashCode` = `x·0x4C7F6DD1 + y·0x4822A3E9 + z·0xAAC3C25D + 0xD21D0945` (wrapping).

## Collections (F11e, proven)

`unity::collections`, equal to the game's BCL (`game/tests/unity_collections.rs`): 514 sorts of tie-heavy
`float` arrays (+0/−0 and NaN payloads compare equal but are told apart by their bits, so the permutation of
equal keys is checked), whole arrays and sub-ranges, 4 adversarial inputs (McIlroy's antiqsort run against
our port) that reach the heapsort fallback; `BinarySearch` with duplicates; 60 random add / indexer-set /
remove / clear sequences on a real `Dictionary<int,int>` and `HashSet<int>` with the iteration order compared.

- Sorting is the .NET reference introsort (`ArraySortHelper<T>`), not stable: insertion sort at ≤ 16, special
  cases 2 and 3, median-of-three with the pivot parked at `hi − 1`, heapsort when the depth budget
  `2 · FloorLog2(keys.Length)` runs out. `keys.Length` is the **backing array's** length (a `List<T>`'s
  capacity: `unity::collections::list_grow`), and `FloorLog2` is really the bit length.
- `BinarySearch` returns whichever equal element the halving meets first, else `~insertionPoint`.
  `WaveFunctionCollapse` keeps its propagation queue sorted with `List<Propagation>.BinarySearch` + `Insert`.
- `Dictionary` / `HashSet` enumerate their slots in order; a new key takes the most recently freed slot (LIFO),
  else the next unused one; `Clear` empties everything (`unity::collections::SlotMap`). Hash codes decide
  buckets only, never order.

## Host-side APIs: Bevy or port? (F11f, decided 2026-10-15)

Survey: Cpp2IL `[Calls]` from the game's code into everything not covered above (call sites in brackets).
Rule: **port** what decides a result the game computes and reads back (positions, picks, saved bytes);
**Bevy hosts** what only shows, plays or times things, marked `STANDIN:` where our behaviour differs.

| Unity / .NET API | Sites | Decision |
|---|---:|---|
| `Transform`, `Component`, `GameObject`, `Behaviour` | 569+513+124+70 | Scene graph → Bevy entities. The **transform math the game reads back** (`position`, `rotation`, `lossyScale`, `TransformPoint`, `InverseTransformPoint`, `localToWorldMatrix`, parent composition) is ported exactly: row **E11** |
| `Camera` (`ScreenPointToRay`, `WorldToScreenPoint`, projection), `Physics.Raycast`, `MeshCollider`, `RaycastHit` | 48+3+… | Picking which block the pointer hits decides what is built: ported exactly, row **E12**. Drawing through the camera is Bevy's |
| `string`, `int`/`float` `ToString`/`Parse`, `StringBuilder` | 144+40 | Where text reaches a file or the shareable town string (`TextSaveSystem`, `SaveSystem`, `MetaSave`), .NET's exact formatting/parsing (invariant culture, `float` round-trip) is ported: row **E13**; UI text is Bevy's |
| `Time` (`unscaledTime`, `deltaTime`, `frameCount`, `timeScale`) | 134 | Bevy `Time` (STANDIN where Unity's frame timing differs); frame-exact timing of the original can't be reproduced anyway |
| `Mesh`, `MeshFilter` (`SetVertices`, `SetTriangles`, `SetUVs`, `RecalculateBounds`) | 95+44 | The game builds the vertex data (ported with E7); uploading/drawing is Bevy's `Mesh` |
| `Shader`, `Material`, `Texture2D`, `RenderTexture`, `ImageConversion`, `Graphics` | 33+25+12+10+16 | Rendering: rows C3 (shaders as WGSL) and E9 (bake passes); PNG encode/decode for save cards via the `image` crate (byte-exact only if G3 needs it) |
| `Screen`, `RectTransform`, `CanvasGroup`, uGUI, TextMeshPro | 68+105+19 | Bevy UI (G7); layout numbers the game reads back (`GameViewRect`, `UiScaler`) are ported with their rows |
| `AudioSource`, `AudioMixer` snapshots | 46 | Bevy audio, clips from F8e (G6, C5) |
| `Input`, Rewired (`Player`, `ReInput`, `InputMapper`, `ControllerHelper`, `MapHelper`) | 26+45+16+40+15+16 | Row **F7** (humans): Bevy input under a port of the game's `ActionInputManager` / `ControlBinding` layer |
| `File`, `Directory`, `Path`, `Application.persistentDataPath`, `XmlSerializer`, `DateTime`, `PlayerPrefs` | 13+14+12+11+11+15 | Saving (G3): same folders, file names and bytes; `DateTime` for save card dates as .NET formats them |
| `Debug`, `Gizmos`, `Stopwatch`, `Profiler` | 95+57+11 | Debug only: dropped (no STANDIN needed) |
| `Delegate`, `Action`, exceptions, `ListPool` | — | Language / the game's own code (`Os.Utils`, E1) |

## Middleware (F7, decided by ed-slopper 2026-10-15)

| Middleware | Replacement |
|---|---|
| Rewired (input) | Bevy input (keyboard, mouse, gamepads via gilrs) under a port of the game's own `ActionInputManager` / `ControlBinding` / `KeyboardAndMouseControlRemap` layer (G7d) |
| I2 Localization | A port of the parts the game uses (`Localize`, `LocalizationManager` lookups, `LocalizationParamsManager`), reading the player's `LanguageSourceAsset` / `LanguageSourceData` through `ts_data` (C4) |
| Crosstales FileBrowser, StandaloneFileBrowser | The `rfd` crate (native file dialogs) |
| Fury SDK (platform, achievements, store) | No-op STANDINs |

## Trigonometry and pow (E1b)

The game's `Math.Cos` / `Math.Sin` / `Math.Pow` run in Intel's libm (`__libm_sse2_*_precise`, linked into
GameAssembly.dll); the port uses Rust's `f64` functions (the platform CRT). Rounded to `float` they agree on
everything the harness tried in the game's range (`tests/os_extra_math.rs`: angles |x| ≤ 10⁴, pitch
exponents), but **huge arguments differ**: `cos(1e20)` reduces differently (0.754 here, 0.618 in the game).
The game never passes such angles; if a port ever needs them, port Intel's argument reduction.

Also from E1b: `ExtraMath.Cross2` (arguments `in`, by reference) computes on the x87 stack and rounds once;
plain `f32` arithmetic is 1 ulp off there (`conv::x87_mul_add` pattern).
