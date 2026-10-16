//! `Unity.Mathematics`: the types and the out-of-line functions the game calls (row F11d), as the reference
//! build computes them. Most of the package is inlined into the game's code as plain `float`/`int` arithmetic,
//! which the game's own ports write out. Integer arithmetic wraps (C#'s unchecked default).
//! Checked against the running game by `tests/unity_mathematics.rs` (`TS_HARNESS=1`).

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Float2 {
    pub x: f32,
    pub y: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Float3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

/// Column-major 3x2: `c0`, `c1` (the game keeps integer bounds in it: `c0` min, `c1` max).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Int3x2 {
    pub c0: Int3,
    pub c1: Int3,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Float4 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Hash)]
pub struct Int2 {
    pub x: i32,
    pub y: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Hash)]
pub struct Int3 {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Int4 {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub w: i32,
}

/// Column-major: `c0..c2` are columns.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Int3x3 {
    pub c: [Int3; 3],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Int4x4 {
    pub c: [Int4; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Float4x4 {
    pub c: [Float4; 4],
}

pub const fn f2(x: f32, y: f32) -> Float2 {
    Float2 { x, y }
}
pub const fn f3(x: f32, y: f32, z: f32) -> Float3 {
    Float3 { x, y, z }
}
pub const fn f4(x: f32, y: f32, z: f32, w: f32) -> Float4 {
    Float4 { x, y, z, w }
}
pub const fn i3(x: i32, y: i32, z: i32) -> Int3 {
    Int3 { x, y, z }
}

impl Float4 {
    fn get(self, i: usize) -> f32 {
        [self.x, self.y, self.z, self.w][i]
    }
    fn mul(self, o: Float4) -> Float4 {
        f4(self.x * o.x, self.y * o.y, self.z * o.z, self.w * o.w)
    }
    fn sub(self, o: Float4) -> Float4 {
        f4(self.x - o.x, self.y - o.y, self.z - o.z, self.w - o.w)
    }
    fn add(self, o: Float4) -> Float4 {
        f4(self.x + o.x, self.y + o.y, self.z + o.z, self.w + o.w)
    }
}

/// `math.shuffle(a, b, x, y, z, w)`: components 0..=3 from `a`, 4..=7 from `b`.
fn shuffle(a: Float4, b: Float4, i: [usize; 4]) -> Float4 {
    let pick = |k: usize| if k < 4 { a.get(k) } else { b.get(k - 4) };
    f4(pick(i[0]), pick(i[1]), pick(i[2]), pick(i[3]))
}

/// `Unity.Mathematics.math.sqrt(float)`: `(float)Math.Sqrt((double)x)`.
pub fn sqrt(x: f32) -> f32 {
    (x as f64).sqrt() as f32
}

/// `Unity.Mathematics.math.length(float2)` 0x3246C0.
pub fn length2(v: Float2) -> f32 {
    sqrt(v.x * v.x + v.y * v.y)
}

/// `Unity.Mathematics.math.normalize(float2)` 0xE283C0: `x * rsqrt(dot(x, x))`, rsqrt = `1 / sqrt`.
pub fn normalize2(v: Float2) -> Float2 {
    let r = 1.0 / sqrt(v.x * v.x + v.y * v.y);
    f2(v.x * r, v.y * r)
}

/// `Unity.Mathematics.math.abs(int3)` 0xE28300 (int.MinValue stays int.MinValue).
pub fn abs3(v: Int3) -> Int3 {
    i3(v.x.wrapping_abs(), v.y.wrapping_abs(), v.z.wrapping_abs())
}

/// `Unity.Mathematics.math.max(int3, int3)` 0x324760.
pub fn max3(a: Int3, b: Int3) -> Int3 {
    i3(a.x.max(b.x), a.y.max(b.y), a.z.max(b.z))
}

/// `Unity.Mathematics.math.mul(int3x3, int3)` 0xE28350.
pub fn mul_i3x3(a: &Int3x3, b: Int3) -> Int3 {
    let c = &a.c;
    let comp = |f: fn(&Int3) -> i32| f(&c[0]).wrapping_mul(b.x).wrapping_add(f(&c[1]).wrapping_mul(b.y)).wrapping_add(f(&c[2]).wrapping_mul(b.z));
    i3(comp(|v| v.x), comp(|v| v.y), comp(|v| v.z))
}

/// `Unity.Mathematics.math.mul(int4x4, int4)` 0x324790.
pub fn mul_i4x4_v(a: &Int4x4, b: Int4) -> Int4 {
    let c = &a.c;
    let comp = |f: fn(&Int4) -> i32| {
        f(&c[0]).wrapping_mul(b.x)
            .wrapping_add(f(&c[1]).wrapping_mul(b.y))
            .wrapping_add(f(&c[2]).wrapping_mul(b.z))
            .wrapping_add(f(&c[3]).wrapping_mul(b.w))
    };
    Int4 { x: comp(|v| v.x), y: comp(|v| v.y), z: comp(|v| v.z), w: comp(|v| v.w) }
}

/// `Unity.Mathematics.math.mul(int4x4, int4x4)` 0x324840.
pub fn mul_i4x4(a: &Int4x4, b: &Int4x4) -> Int4x4 {
    Int4x4 { c: std::array::from_fn(|i| mul_i4x4_v(a, b.c[i])) }
}

/// `Unity.Mathematics.float4x4..ctor(int4x4)` 0x324530.
pub fn float4x4_from_int(v: &Int4x4) -> Float4x4 {
    Float4x4 { c: v.c.map(|c| f4(c.x as f32, c.y as f32, c.z as f32, c.w as f32)) }
}

/// `Unity.Mathematics.int3.GetHashCode` 0x1E6F70: `(int)math.hash(v)`.
pub fn hash3(v: Int3) -> i32 {
    (v.x as u32)
        .wrapping_mul(0x4C7F_6DD1)
        .wrapping_add((v.y as u32).wrapping_mul(0x4822_A3E9))
        .wrapping_add((v.z as u32).wrapping_mul(0xAAC3_C25D))
        .wrapping_add(0xD21D_0945) as i32
}

/// `Unity.Mathematics.math.inverse(float4x4)` 0x10FB180: the package's shuffle-based cofactor inverse.
pub fn inverse(m: &Float4x4) -> Float4x4 {
    let [c0, c1, c2, c3] = m.c;
    let r0y_r1y_r0x_r1x = shuffle(c1, c0, [0, 1, 4, 5]);
    let r0z_r1z_r0w_r1w = shuffle(c2, c3, [0, 1, 4, 5]);
    let r2y_r3y_r2x_r3x = shuffle(c1, c0, [2, 3, 6, 7]);
    let r2z_r3z_r2w_r3w = shuffle(c2, c3, [2, 3, 6, 7]);
    let r1y_r2y_r1x_r2x = shuffle(c1, c0, [1, 2, 5, 6]);
    let r1z_r2z_r1w_r2w = shuffle(c2, c3, [1, 2, 5, 6]);
    let r3y_r0y_r3x_r0x = shuffle(c1, c0, [3, 0, 7, 4]);
    let r3z_r0z_r3w_r0w = shuffle(c2, c3, [3, 0, 7, 4]);

    let r0_wzyx = shuffle(r0z_r1z_r0w_r1w, r0y_r1y_r0x_r1x, [2, 0, 4, 6]);
    let r1_wzyx = shuffle(r0z_r1z_r0w_r1w, r0y_r1y_r0x_r1x, [3, 1, 5, 7]);
    let r2_wzyx = shuffle(r2z_r3z_r2w_r3w, r2y_r3y_r2x_r3x, [2, 0, 4, 6]);
    let r3_wzyx = shuffle(r2z_r3z_r2w_r3w, r2y_r3y_r2x_r3x, [3, 1, 5, 7]);
    let r0_xyzw = shuffle(r0y_r1y_r0x_r1x, r0z_r1z_r0w_r1w, [2, 0, 4, 6]);

    let inner12_23 = r1y_r2y_r1x_r2x.mul(r2z_r3z_r2w_r3w).sub(r1z_r2z_r1w_r2w.mul(r2y_r3y_r2x_r3x));
    let inner02_13 = r0y_r1y_r0x_r1x.mul(r2z_r3z_r2w_r3w).sub(r0z_r1z_r0w_r1w.mul(r2y_r3y_r2x_r3x));
    let inner30_01 = r3z_r0z_r3w_r0w.mul(r0y_r1y_r0x_r1x).sub(r3y_r0y_r3x_r0x.mul(r0z_r1z_r0w_r1w));

    let inner12 = shuffle(inner12_23, inner12_23, [0, 2, 6, 4]);
    let inner23 = shuffle(inner12_23, inner12_23, [1, 3, 7, 5]);
    let inner02 = shuffle(inner02_13, inner02_13, [0, 2, 6, 4]);
    let inner13 = shuffle(inner02_13, inner02_13, [1, 3, 7, 5]);

    let minors0 = r3_wzyx.mul(inner12).sub(r2_wzyx.mul(inner13)).add(r1_wzyx.mul(inner23));
    let mut denom = r0_xyzw.mul(minors0);
    denom = denom.add(shuffle(denom, denom, [1, 0, 7, 6]));
    denom = denom.sub(shuffle(denom, denom, [2, 2, 4, 4]));
    let rcp = f4(1.0 / denom.x, 1.0 / denom.y, 1.0 / denom.z, 1.0 / denom.w);

    let inner30 = shuffle(inner30_01, inner30_01, [0, 2, 6, 4]);
    let inner01 = shuffle(inner30_01, inner30_01, [1, 3, 7, 5]);
    let minors1 = r2_wzyx.mul(inner30).sub(r0_wzyx.mul(inner23)).sub(r3_wzyx.mul(inner02));
    let minors2 = r0_wzyx.mul(inner13).sub(r1_wzyx.mul(inner30)).sub(r3_wzyx.mul(inner01));
    let minors3 = r1_wzyx.mul(inner02).sub(r0_wzyx.mul(inner12)).add(r2_wzyx.mul(inner01));
    Float4x4 { c: [minors0.mul(rcp), minors1.mul(rcp), minors2.mul(rcp), minors3.mul(rcp)] }
}

/// `Unity.Mathematics.Random`: xorshift32.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Random {
    pub state: u32,
}

fn xorshift32(s: u32) -> u32 {
    let s = s ^ (s << 13);
    let s = s ^ (s >> 17);
    s ^ (s << 5)
}

impl Random {
    /// `Unity.Mathematics.Random..ctor(uint)` 0x10F5280: the state is the seed's first xorshift step.
    pub fn new(seed: u32) -> Random {
        Random { state: xorshift32(seed) }
    }

    /// `Unity.Mathematics.Random.NextState` 0x10F5250: returns the state, then advances it.
    pub fn next_state(&mut self) -> u32 {
        let s = self.state;
        self.state = xorshift32(s);
        s
    }

    /// `Unity.Mathematics.Random.NextFloat()` 0x10F51A0: `asfloat(0x3F800000 | (s >> 9)) - 1`.
    pub fn next_float(&mut self) -> f32 {
        f32::from_bits(0x3F80_0000 | (self.next_state() >> 9)) - 1.0
    }

    /// `Unity.Mathematics.Random.NextFloat(float, float)` 0x6F7580: `NextFloat() * (max - min) + min`.
    pub fn next_float_range(&mut self, min: f32, max: f32) -> f32 {
        self.next_float() * (max - min) + min
    }

    /// `Unity.Mathematics.Random.NextInt(int, int)` 0x4392E0: `min + (int)(((ulong)(uint)(max - min) * s) >> 32)`.
    pub fn next_int_range(&mut self, min: i32, max: i32) -> i32 {
        let range = max.wrapping_sub(min) as u32 as u64;
        min.wrapping_add(((range * self.next_state() as u64) >> 32) as i32)
    }
}
