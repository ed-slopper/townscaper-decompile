//! A 4-lane SSE register, for transcribing the engine's hand-vectorised native code (UnityPlayer.dll) one
//! instruction at a time: each operation is the instruction's IEEE single-precision lane-wise result, so a
//! transcription computes exactly what the engine does.

/// An `xmm` register: four lanes, kept as bits (the engine mixes float and integer instructions on them).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct X(pub [u32; 4]);

impl X {
    pub const ZERO: X = X([0; 4]);

    pub fn from_f(f: [f32; 4]) -> X {
        X(f.map(f32::to_bits))
    }
    pub fn from_bits(b: [u32; 4]) -> X {
        X(b)
    }
    pub fn f(self) -> [f32; 4] {
        self.0.map(f32::from_bits)
    }
    pub fn lane(self, i: usize) -> f32 {
        f32::from_bits(self.0[i])
    }

    fn map2(self, o: X, op: impl Fn(f32, f32) -> f32) -> X {
        let (a, b) = (self.f(), o.f());
        X::from_f([op(a[0], b[0]), op(a[1], b[1]), op(a[2], b[2]), op(a[3], b[3])])
    }
    fn bits2(self, o: X, op: impl Fn(u32, u32) -> u32) -> X {
        X([op(self.0[0], o.0[0]), op(self.0[1], o.0[1]), op(self.0[2], o.0[2]), op(self.0[3], o.0[3])])
    }

    /// `mulps`.
    pub fn mul(self, o: X) -> X {
        self.map2(o, |a, b| a * b)
    }
    /// `addps`.
    pub fn add(self, o: X) -> X {
        self.map2(o, |a, b| a + b)
    }
    /// `subps`.
    pub fn sub(self, o: X) -> X {
        self.map2(o, |a, b| a - b)
    }
    /// `divps`.
    pub fn div(self, o: X) -> X {
        self.map2(o, |a, b| a / b)
    }
    /// `sqrtps` (correctly rounded, as the instruction).
    pub fn sqrt(self) -> X {
        X::from_f(self.f().map(f32::sqrt))
    }
    /// `rcpps`: the processor's approximate reciprocal (12 bits; the exact bits are the CPU's own, so this runs
    /// the instruction itself and matches the engine on the same machine).
    pub fn rcp(self) -> X {
        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        {
            #[cfg(target_arch = "x86")]
            use std::arch::x86::{_mm_loadu_ps, _mm_rcp_ps, _mm_storeu_ps};
            #[cfg(target_arch = "x86_64")]
            use std::arch::x86_64::{_mm_loadu_ps, _mm_rcp_ps, _mm_storeu_ps};
            let f = self.f();
            let mut out = [0f32; 4];
            // SAFETY: SSE is baseline on these targets; the pointers are to local arrays of four floats.
            unsafe { _mm_storeu_ps(out.as_mut_ptr(), _mm_rcp_ps(_mm_loadu_ps(f.as_ptr()))) };
            X::from_f(out)
        }
        #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
        {
            // STANDIN: no rcpps here; the exact reciprocal (the Newton steps after it make this close).
            X::from_f(self.f().map(|v| 1.0 / v))
        }
    }
    /// `minps`: the second operand when either is NaN or they are equal.
    pub fn min(self, o: X) -> X {
        self.map2(o, |a, b| if a < b { a } else { b })
    }
    /// `maxps`: the second operand when either is NaN or they are equal.
    pub fn max(self, o: X) -> X {
        self.map2(o, |a, b| if a > b { a } else { b })
    }
    /// `andps` / `pand`.
    pub fn and(self, o: X) -> X {
        self.bits2(o, |a, b| a & b)
    }
    /// `andnps` / `pandn`: `!self & o`.
    pub fn andn(self, o: X) -> X {
        self.bits2(o, |a, b| !a & b)
    }
    /// `orps` / `por`.
    pub fn or(self, o: X) -> X {
        self.bits2(o, |a, b| a | b)
    }
    /// `xorps` / `pxor`.
    pub fn xor(self, o: X) -> X {
        self.bits2(o, |a, b| a ^ b)
    }
    fn mask(c: bool) -> u32 {
        if c {
            u32::MAX
        } else {
            0
        }
    }
    /// `cmpltps`.
    pub fn cmplt(self, o: X) -> X {
        let (a, b) = (self.f(), o.f());
        X([0, 1, 2, 3].map(|i| X::mask(a[i] < b[i])))
    }
    /// `cmpleps`.
    pub fn cmple(self, o: X) -> X {
        let (a, b) = (self.f(), o.f());
        X([0, 1, 2, 3].map(|i| X::mask(a[i] <= b[i])))
    }
    /// `cmpneqps` (true when unordered).
    pub fn cmpneq(self, o: X) -> X {
        let (a, b) = (self.f(), o.f());
        X([0, 1, 2, 3].map(|i| X::mask(a[i] != b[i])))
    }
    /// `cmpeqps`.
    pub fn cmpeq(self, o: X) -> X {
        let (a, b) = (self.f(), o.f());
        X([0, 1, 2, 3].map(|i| X::mask(a[i] == b[i])))
    }
    /// `movmskps`.
    pub fn movmsk(self) -> u32 {
        (0..4).map(|i| (self.0[i] >> 31) << i).sum()
    }
    /// `psrad imm`.
    pub fn psrad(self, n: u32) -> X {
        X(self.0.map(|v| ((v as i32) >> n.min(31)) as u32))
    }
    /// `pshufd imm`: lane i from lane `(imm >> 2i) & 3`.
    pub fn pshufd(self, imm: u8) -> X {
        X([0, 1, 2, 3].map(|i| self.0[((imm >> (2 * i)) & 3) as usize]))
    }
    /// `shufps self, o, imm`: lanes 0, 1 from `self`, 2, 3 from `o`.
    pub fn shufps(self, o: X, imm: u8) -> X {
        let s = |v: X, i: u8| v.0[((imm >> (2 * i)) & 3) as usize];
        X([s(self, 0), s(self, 1), s(o, 2), s(o, 3)])
    }
    /// `movlhps self, o`: `(self.0, self.1, o.0, o.1)`.
    pub fn movlhps(self, o: X) -> X {
        X([self.0[0], self.0[1], o.0[0], o.0[1]])
    }
    /// `movhlps self, o`: `(o.2, o.3, self.2, self.3)`.
    pub fn movhlps(self, o: X) -> X {
        X([o.0[2], o.0[3], self.0[2], self.0[3]])
    }
    /// `unpcklps self, o`: `(self.0, o.0, self.1, o.1)`.
    pub fn unpcklps(self, o: X) -> X {
        X([self.0[0], o.0[0], self.0[1], o.0[1]])
    }
    /// `unpckhps self, o`: `(self.2, o.2, self.3, o.3)`.
    pub fn unpckhps(self, o: X) -> X {
        X([self.0[2], o.0[2], self.0[3], o.0[3]])
    }
    /// `movss self, o` (register form): lane 0 from `o`.
    pub fn movss(self, o: X) -> X {
        X([o.0[0], self.0[1], self.0[2], self.0[3]])
    }
    /// `movss` from memory / `cvt…ss`: a scalar in lane 0, the rest zero.
    pub fn scalar(f: f32) -> X {
        X([f.to_bits(), 0, 0, 0])
    }
    /// `mulss` / `addss` / `subss` / `divss`: lane 0 only.
    pub fn mulss(self, o: X) -> X {
        self.movss(X::scalar(self.lane(0) * o.lane(0)))
    }
    pub fn addss(self, o: X) -> X {
        self.movss(X::scalar(self.lane(0) + o.lane(0)))
    }
    pub fn subss(self, o: X) -> X {
        self.movss(X::scalar(self.lane(0) - o.lane(0)))
    }
    pub fn divss(self, o: X) -> X {
        self.movss(X::scalar(self.lane(0) / o.lane(0)))
    }
    /// `sqrtss`.
    pub fn sqrtss(self) -> X {
        self.movss(X::scalar(self.lane(0).sqrt()))
    }
}
