//! `UnityEngine.Random` (row F11c): the engine's `Rand`, Marsaglia's xorshift128, as UnityPlayer.dll runs it
//! (`Random_CUSTOM_*`, read with the shipped PDB). The game never seeds it (the engine seeds it at start-up), so
//! runs can't repeat the original's draws; given the same state, every draw here equals the game's
//! (`tests/unity_random.rs`, `TS_HARNESS=1`).

use super::vector::{v3, Vector3};

/// `Random.State`: the four xorshift words.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rand {
    pub s: [u32; 4],
}

/// `1 / 8388607` as the engine has it (float bits 0x34000001).
const SCALE: f32 = f32::from_bits(0x3400_0001);
/// `2π` as a float (bits 0x40C90FDB).
const TWO_PI: f32 = f32::from_bits(0x40C9_0FDB);
/// The cube-root exponent: `(double)(1f / 3f)`, not 1/3.
const THIRD: f64 = (1.0f32 / 3.0f32) as f64;

impl Rand {
    /// `UnityEngine.Random.InitState` → `Random_CUSTOM_InitState` (native 0x107A74A0).
    pub fn new(seed: i32) -> Rand {
        let s0 = seed as u32;
        let s1 = s0.wrapping_mul(0x6C07_8965).wrapping_add(1);
        let s2 = s1.wrapping_mul(0x6C07_8965).wrapping_add(1);
        let s3 = s2.wrapping_mul(0x6C07_8965).wrapping_add(1);
        Rand { s: [s0, s1, s2, s3] }
    }

    /// One xorshift128 step.
    pub fn next_u32(&mut self) -> u32 {
        let t = self.s[0] ^ (self.s[0] << 11);
        let w = self.s[3];
        self.s = [self.s[1], self.s[2], w, w ^ (w >> 19) ^ t ^ (t >> 8)];
        self.s[3]
    }

    /// `UnityEngine.Random.get_value` → `Random_Get_Custom_PropValue` (native 0x107A77C0): the low 23 bits
    /// times 1/8388607, so 0..=1 inclusive.
    pub fn value(&mut self) -> f32 {
        (self.next_u32() & 0x7F_FFFF) as f32 * SCALE
    }

    /// `UnityEngine.Random.Range(float, float)` → `Random_CUSTOM_Range` (native 0x107A7580):
    /// `(1 − v)·max + v·min`.
    pub fn range(&mut self, min: f32, max: f32) -> f32 {
        let v = self.value();
        (1.0 - v) * max + v * min
    }

    /// `UnityEngine.Random.Range(int, int)` → `Random_CUSTOM_RandomRangeInt` (native 0x107A7730): `min + r %
    /// (max − min)` (unsigned), or `min − r % (min − max)` when `max < min`; no draw when equal.
    pub fn range_int(&mut self, min: i32, max: i32) -> i32 {
        if min < max {
            let r = self.next_u32();
            min.wrapping_add((r % (max.wrapping_sub(min) as u32)) as i32)
        } else if max < min {
            let r = self.next_u32();
            min.wrapping_sub((r % (min.wrapping_sub(max) as u32)) as i32)
        } else {
            min
        }
    }

    /// `RandomUnitVector` (native 0x107C5080): z uniform in [-1, 1], angle uniform in [0, 2π].
    pub fn unit_vector(&mut self) -> Vector3 {
        let v = self.value();
        let z = (1.0 - v) - v;
        let a = self.value();
        let theta = (1.0 - a) * TWO_PI + a * 0.0;
        let r = (((1.0 - z * z) as f64).sqrt()) as f32;
        v3(((theta as f64).cos() as f32) * r, ((theta as f64).sin() as f32) * r, z)
    }

    /// `UnityEngine.Random.get_insideUnitSphere` (native 0x107A7870): a unit vector times `value^(1/3)`.
    pub fn inside_unit_sphere(&mut self) -> Vector3 {
        let u = self.unit_vector();
        let k = (self.value() as f64).powf(THIRD) as f32;
        v3(k * u.x, k * u.y, u.z * k)
    }
}
