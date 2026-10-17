//! `UnityEngine.Quaternion`, `Matrix4x4` and the engine's native routines behind them (UnityPlayer.dll, read
//! with its PDB): row F11b. Unity's memory layouts: `Quaternion` (x, y, z, w), `Matrix4x4` column-major
//! (`m[row + col * 4]`), the engine's `Matrix3x3f` column-major (`m[row + col * 3]`).
//! Checked against the running game by `tests/unity_vectors.rs` (`TS_HARNESS=1`).

use super::vector::{sqrt_f64, v3, v4, Vector3, Vector4};

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quaternion {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

pub const fn quat(x: f32, y: f32, z: f32, w: f32) -> Quaternion {
    Quaternion { x, y, z, w }
}

/// `Vector3f::epsilon` (UnityPlayer.dll).
const ENGINE_EPSILON: f32 = 1e-5;

impl Quaternion {
    pub const IDENTITY: Quaternion = quat(0.0, 0.0, 0.0, 1.0);

    /// `UnityEngine.Quaternion.Dot`.
    pub fn dot(a: Quaternion, b: Quaternion) -> f32 {
        a.x * b.x + a.y * b.y + a.z * b.z + a.w * b.w
    }

    /// `UnityEngine.Quaternion.op_Multiply(Quaternion, Quaternion)`.
    pub fn mul(l: Quaternion, r: Quaternion) -> Quaternion {
        quat(
            l.w * r.x + l.x * r.w + l.y * r.z - l.z * r.y,
            l.w * r.y + l.y * r.w + l.z * r.x - l.x * r.z,
            l.w * r.z + l.z * r.w + l.x * r.y - l.y * r.x,
            l.w * r.w - l.x * r.x - l.y * r.y - l.z * r.z,
        )
    }

    /// `UnityEngine.Quaternion.op_Multiply(Quaternion, Vector3)`: rotate a point.
    pub fn rotate(q: Quaternion, p: Vector3) -> Vector3 {
        let (x, y, z) = (q.x * 2.0, q.y * 2.0, q.z * 2.0);
        let (xx, yy, zz) = (q.x * x, q.y * y, q.z * z);
        let (xy, xz, yz) = (q.x * y, q.x * z, q.y * z);
        let (wx, wy, wz) = (q.w * x, q.w * y, q.w * z);
        v3(
            (1.0 - (yy + zz)) * p.x + (xy - wz) * p.y + (xz + wy) * p.z,
            (xy + wz) * p.x + (1.0 - (xx + zz)) * p.y + (yz - wx) * p.z,
            (xz - wy) * p.x + (yz + wx) * p.y + (1.0 - (xx + yy)) * p.z,
        )
    }

    /// `UnityEngine.Quaternion.Inverse` → `Quaternion_CUSTOM_Inverse_Injected` (native 0x1079FA00): the conjugate.
    pub fn inverse(q: Quaternion) -> Quaternion {
        quat(-q.x, -q.y, -q.z, q.w)
    }

    /// `UnityEngine.Quaternion.Lerp` → `Quaternion_CUSTOM_Lerp_Injected` (native 0x107A00D0): `t` clamped,
    /// the shorter way by the dot's sign, normalised by `(float)sqrt` in double.
    pub fn lerp(a: Quaternion, b: Quaternion, t: f32) -> Quaternion {
        // below 0 → 0, above 1 → 1, else t: NaN stays NaN (proven by the harness)
        let t = if t < 0.0 {
            0.0
        } else if t > 1.0 {
            1.0
        } else {
            t
        };
        let q = if 0.0 <= Quaternion::dot(a, b) {
            quat(a.x + (b.x - a.x) * t, (b.y - a.y) * t + a.y, a.z + (b.z - a.z) * t, (b.w - a.w) * t + a.w)
        } else {
            quat(a.x - (b.x + a.x) * t, a.y - (b.y + a.y) * t, a.z - (b.z + a.z) * t, a.w - (b.w + a.w) * t)
        };
        let len = sqrt_f64(q.y * q.y + q.x * q.x + q.z * q.z + q.w * q.w);
        quat(q.x / len, q.y / len, q.z / len, q.w / len)
    }

    /// `UnityEngine.Quaternion.Euler(float, float, float)`: degrees times `Mathf.Deg2Rad`, then
    /// `Quaternion_CUSTOM_Internal_FromEulerRad_Injected` → native `EulerToQuaternion` in Unity's default
    /// order (Z, then X, then Y): one quaternion per axis from the half angle's `sin`/`cos` in double, then
    /// `CreateQuaternionFromAxisQuaternions(y, x, z)` = `(y * x) * z`. Bit-exact up to about 1e12 degrees;
    /// past that the game's `__libm_sse2_sin_precise` reduces the argument differently from Rust's `sin`.
    pub fn euler(x: f32, y: f32, z: f32) -> Quaternion {
        const DEG2RAD: f32 = 0.017453292;
        let axis = |a: f32| {
            let h = (a * DEG2RAD * 0.5) as f64;
            (h.sin() as f32, h.cos() as f32)
        };
        let (sx, cx) = axis(x);
        let (sy, cy) = axis(y);
        let (sz, cz) = axis(z);
        let q1 = quat(0.0, sy, 0.0, cy);
        let q2 = quat(sx, 0.0, 0.0, cx);
        let q3 = quat(0.0, 0.0, sz, cz);
        let t = quat(
            q1.x * q2.w + q2.x * q1.w + q2.z * q1.y - q2.y * q1.z,
            q1.y * q2.w + q2.y * q1.w + q1.z * q2.x - q2.z * q1.x,
            q1.z * q2.w + q2.z * q1.w + q2.y * q1.x - q1.y * q2.x,
            q2.w * q1.w - q1.x * q2.x - q2.y * q1.y - q1.z * q2.z,
        );
        quat(
            q3.x * t.w + q3.w * t.x + q3.z * t.y - q3.y * t.z,
            q3.y * t.w + q3.w * t.y + q3.x * t.z - q3.z * t.x,
            q3.z * t.w + q3.w * t.z + q3.y * t.x - q3.x * t.y,
            q3.w * t.w - q3.x * t.x - q3.y * t.y - q3.z * t.z,
        )
    }

    /// `UnityEngine.Quaternion.LookRotation(Vector3)`: up = `Vector3.up`.
    pub fn look_rotation(forward: Vector3) -> Quaternion {
        Quaternion::look_rotation_up(forward, Vector3::UP)
    }

    /// `UnityEngine.Quaternion.LookRotation(Vector3, Vector3)` → `Quaternion_CUSTOM_LookRotation_Injected`
    /// (native): the look matrix's quaternion; if `forward` and `up` don't make a frame, the from-to rotation
    /// from +Z to `forward`; identity for a zero `forward` (the engine also logs "Look rotation viewing vector
    /// is zero").
    pub fn look_rotation_up(forward: Vector3, up: Vector3) -> Quaternion {
        if let Some(m) = look_rotation_to_matrix(forward, up) {
            return matrix_to_quaternion(&m);
        }
        let mag = sqrt_f64(forward.x * forward.x + forward.y * forward.y + forward.z * forward.z);
        if ENGINE_EPSILON < mag {
            let to = v3(forward.x / mag, forward.y / mag, forward.z / mag);
            return matrix_to_quaternion(&from_to_rotation(Vector3::FORWARD, to));
        }
        Quaternion::IDENTITY
    }
}

/// The engine's `Matrix3x3f`, column-major: `m[row + col * 3]`.
pub type Matrix3 = [f32; 9];

/// What `LookRotationToMatrix` does with its output.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LookMatrix {
    /// A frame was written (the function returns true).
    Frame(Matrix3),
    /// `forward` or `up × forward` shorter than `Vector3f::epsilon`: identity written, returns false.
    Degenerate,
    /// The frame failed its last check (also every NaN input): nothing written, returns false. The engine's
    /// callers then use whatever was in their buffer (`Matrix4x4.LookAt`: uninitialised stack).
    Unwritten,
}

/// `LookRotationToMatrix` (native 0x105E0F80): columns x = normalize(up × z), y = z × x, z = normalize(forward).
/// The length checks are `len < epsilon` (NaN passes), the last check is `|y·y − 1| <= 1e-6` (NaN fails):
/// proven by the harness on NaN inputs.
pub fn look_rotation_matrix(forward: Vector3, up: Vector3) -> LookMatrix {
    let (mut zx, mut zy, mut zz) = (forward.x, forward.y, forward.z);
    let len = sqrt_f64(zx * zx + zy * zy + zz * zz);
    if len < ENGINE_EPSILON {
        return LookMatrix::Degenerate;
    }
    zy /= len;
    zz /= len;
    zx /= len;
    let mut xx = up.y * zz - up.z * zy;
    let mut xy = up.z * zx - up.x * zz;
    let mut xz = up.x * zy - up.y * zx;
    let len = sqrt_f64(xy * xy + xx * xx + xz * xz);
    if len < ENGINE_EPSILON {
        return LookMatrix::Degenerate;
    }
    xy /= len;
    xz /= len;
    xx /= len;
    let yx = zy * xz - xy * zz;
    let yy = zz * xx - zx * xz;
    let yz = zx * xy - zy * xx;
    if ((yx * yx + yy * yy + yz * yz) - 1.0).abs() <= 1e-6 {
        LookMatrix::Frame([xx, xy, xz, yx, yy, yz, zx, zy, zz])
    } else {
        LookMatrix::Unwritten
    }
}

/// [`look_rotation_matrix`] as "a frame or nothing".
pub fn look_rotation_to_matrix(forward: Vector3, up: Vector3) -> Option<Matrix3> {
    match look_rotation_matrix(forward, up) {
        LookMatrix::Frame(m) => Some(m),
        _ => None,
    }
}

/// `MatrixToQuaternion(Matrix3x3f)` (native 0x105D4890): Shoemake's method, `sqrt` in double, renormalised.
pub fn matrix_to_quaternion(m: &Matrix3) -> Quaternion {
    let g = |r: usize, c: usize| m[r + c * 3];
    let mut q = [0f32; 4]; // x, y, z, w
    let trace = m[0] + m[4] + m[8];
    if trace > 0.0 {
        let root = sqrt_f64(trace + 1.0);
        q[3] = root * 0.5;
        let r = 0.5 / root;
        q[0] = (m[5] - m[7]) * r;
        q[1] = (m[6] - m[2]) * r;
        q[2] = (m[1] - m[3]) * r;
    } else {
        const NEXT: [usize; 3] = [1, 2, 0];
        let mut i = (m[0] < m[4]) as usize;
        if g(i, i) < m[8] {
            i = 2;
        }
        let j = NEXT[i];
        let k = NEXT[j];
        let root = sqrt_f64(((g(i, i) - g(j, j)) - g(k, k)) + 1.0);
        let r = 0.5 / root;
        q[i] = root * 0.5;
        q[3] = (g(k, j) - g(j, k)) * r;
        q[j] = (g(j, i) + g(i, j)) * r;
        q[k] = (g(k, i) + g(i, k)) * r;
    }
    let len = sqrt_f64(q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]);
    quat(q[0] / len, q[1] / len, q[2] / len, q[3] / len)
}

/// `Matrix3x3f::SetFromToRotation` (native 0x105E13B0): the rotation taking unit `from` to unit `to`
/// (Möller & Hughes), with the engine's handling of (nearly) equal and opposite vectors.
pub fn from_to_rotation(from: Vector3, to: Vector3) -> Matrix3 {
    let (fx, fy, fz) = (from.x, from.y, from.z);
    let vx = to.z * fy - to.y * fz;
    let vy = to.x * fz - fx * to.z;
    let vz = fx * to.y - to.x * fy;
    let e = fx * to.x + to.y * fy + fz * to.z;
    let mut m = [0f32; 9];
    const LIMIT: f64 = 0.999999;
    if (e as f64) > LIMIT {
        m[0] = 1.0;
        m[4] = 1.0;
        m[8] = 1.0;
        return m;
    }
    if -LIMIT <= e as f64 {
        let h = (1.0 - e) / (vy * vy + vx * vx + vz * vz);
        let hvx = vx * h;
        let hvxz = vy * vz * h;
        m[0] = vx * hvx + e;
        m[3] = vy * hvx - vz;
        m[6] = vy + vz * hvx;
        m[1] = vz + vy * hvx;
        m[4] = vy * h * vy + e;
        m[7] = hvxz - vx;
        m[2] = vz * hvx - vy;
        m[5] = hvxz + vx;
        m[8] = vz * vz * h + e;
        return m;
    }
    // from and to (nearly) opposite: rotate 180 degrees about an axis perpendicular to from
    let (mut ux, mut uy, mut uz) = (fz, -fy, 0.0f32);
    if fz * fz + 0.0 + (-fy) * (-fy) < 1e-6 {
        ux = 0.0;
        uz = -fz;
        uy = fx;
    }
    let inv = 1.0 / sqrt_f64(uz * uz + ux * ux + uy * uy);
    ux *= inv;
    uy *= inv;
    uz *= inv;
    let c1 = ux * fz - uy * fy;
    let c2 = uy * fx - uz * fz;
    let c3 = uz * fy - fx * ux;
    m[0] = -(fx * fx) + c1 * c1 + -(uz * uz);
    m[3] = c2 * c1 + -(fx * fy) + -(uz * ux);
    m[6] = c1 * c3 + -(fx * fz) + -(uz * uy);
    m[1] = m[3];
    m[4] = c2 * c2 + -(fy * fy) + -(ux * ux);
    m[7] = c2 * c3 + -(fz * fy) + -(uy * ux);
    m[2] = m[6];
    m[5] = m[7];
    m[8] = c3 * c3 + -(fz * fz) + -(uy * uy);
    m
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Matrix4x4 {
    /// Column-major: `m[row + col * 4]` (Unity's field order m00, m10, m20, m30, m01, ...).
    pub m: [f32; 16],
}

impl Matrix4x4 {
    pub const IDENTITY: Matrix4x4 =
        Matrix4x4 { m: [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0] };

    pub fn get(&self, row: usize, col: usize) -> f32 {
        self.m[row + col * 4]
    }

    /// `UnityEngine.Matrix4x4..ctor(Vector4, Vector4, Vector4, Vector4)`: the four columns.
    pub fn from_columns(c0: Vector4, c1: Vector4, c2: Vector4, c3: Vector4) -> Matrix4x4 {
        let mut m = [0f32; 16];
        for (i, c) in [c0, c1, c2, c3].iter().enumerate() {
            m[i * 4..i * 4 + 4].copy_from_slice(&[c.x, c.y, c.z, c.w]);
        }
        Matrix4x4 { m }
    }

    /// `UnityEngine.Matrix4x4.GetColumn` (0..=3; the original throws `IndexOutOfRangeException` otherwise).
    pub fn column(&self, i: usize) -> Vector4 {
        v4(self.m[i * 4], self.m[i * 4 + 1], self.m[i * 4 + 2], self.m[i * 4 + 3])
    }

    /// `UnityEngine.Matrix4x4.SetColumn`.
    pub fn set_column(&mut self, i: usize, v: Vector4) {
        self.m[i * 4..i * 4 + 4].copy_from_slice(&[v.x, v.y, v.z, v.w]);
    }

    /// `UnityEngine.Matrix4x4.op_Multiply`: `res[r][c] = Σ lhs[r][k] * rhs[k][c]`, k = 0..3 left to right.
    pub fn mul(l: &Matrix4x4, r: &Matrix4x4) -> Matrix4x4 {
        let mut m = [0f32; 16];
        for row in 0..4 {
            for col in 0..4 {
                m[row + col * 4] = l.get(row, 0) * r.get(0, col)
                    + l.get(row, 1) * r.get(1, col)
                    + l.get(row, 2) * r.get(2, col)
                    + l.get(row, 3) * r.get(3, col);
            }
        }
        Matrix4x4 { m }
    }

    /// `UnityEngine.Matrix4x4.MultiplyPoint`: with the perspective divide (`w = 1 / w`, then `* w`).
    pub fn multiply_point(&self, p: Vector3) -> Vector3 {
        let g = |r, c| self.get(r, c);
        let x = g(0, 0) * p.x + g(0, 1) * p.y + g(0, 2) * p.z + g(0, 3);
        let y = g(1, 0) * p.x + g(1, 1) * p.y + g(1, 2) * p.z + g(1, 3);
        let z = g(2, 0) * p.x + g(2, 1) * p.y + g(2, 2) * p.z + g(2, 3);
        let w = 1.0 / (g(3, 0) * p.x + g(3, 1) * p.y + g(3, 2) * p.z + g(3, 3));
        v3(x * w, y * w, z * w)
    }

    /// `UnityEngine.Matrix4x4.MultiplyVector`.
    pub fn multiply_vector(&self, v: Vector3) -> Vector3 {
        let g = |r, c| self.get(r, c);
        v3(
            g(0, 0) * v.x + g(0, 1) * v.y + g(0, 2) * v.z,
            g(1, 0) * v.x + g(1, 1) * v.y + g(1, 2) * v.z,
            g(2, 0) * v.x + g(2, 1) * v.y + g(2, 2) * v.z,
        )
    }

    /// `UnityEngine.Matrix4x4.Scale`.
    pub fn scale(v: Vector3) -> Matrix4x4 {
        let mut m = [0f32; 16];
        m[0] = v.x;
        m[5] = v.y;
        m[10] = v.z;
        m[15] = 1.0;
        Matrix4x4 { m }
    }

    /// `UnityEngine.Matrix4x4.Translate`.
    pub fn translate(v: Vector3) -> Matrix4x4 {
        let mut r = Matrix4x4::IDENTITY;
        r.m[12] = v.x;
        r.m[13] = v.y;
        r.m[14] = v.z;
        r
    }

    /// `UnityEngine.Matrix4x4.Rotate` (managed).
    pub fn rotate(q: Quaternion) -> Matrix4x4 {
        let (x, y, z) = (q.x * 2.0, q.y * 2.0, q.z * 2.0);
        let (xx, yy, zz) = (q.x * x, q.y * y, q.z * z);
        let (xy, xz, yz) = (q.x * y, q.x * z, q.y * z);
        let (wx, wy, wz) = (q.w * x, q.w * y, q.w * z);
        Matrix4x4 {
            m: [
                1.0 - (yy + zz), xy + wz, xz - wy, 0.0,
                xy - wz, 1.0 - (xx + zz), yz + wx, 0.0,
                xz + wy, yz - wx, 1.0 - (xx + yy), 0.0,
                0.0, 0.0, 0.0, 1.0,
            ],
        }
    }

    /// `UnityEngine.Matrix4x4.TRS` → `Matrix4x4f::SetTRS` (native 0x105D77A0): `QuaternionToMatrix`, columns
    /// scaled by `s`, translation `t`.
    pub fn trs(t: Vector3, q: Quaternion, s: Vector3) -> Matrix4x4 {
        let mut r = quaternion_to_matrix(q);
        r.m[0] *= s.x;
        r.m[1] *= s.x;
        r.m[2] *= s.x;
        r.m[4] *= s.y;
        r.m[5] *= s.y;
        r.m[6] *= s.y;
        r.m[8] *= s.z;
        r.m[9] *= s.z;
        r.m[10] *= s.z;
        r.m[12] = t.x;
        r.m[13] = t.y;
        r.m[14] = t.z;
        r
    }

    /// `UnityEngine.Matrix4x4.LookAt` → `Matrix4x4_CUSTOM_LookAt_Injected` (native): the look matrix of
    /// `target - from` and `up` as rotation, `from` as translation (identity rotation when degenerate).
    /// STANDIN: identity where the engine leaves the rotation uninitialised ([`LookMatrix::Unwritten`]).
    pub fn look_at(from: Vector3, target: Vector3, up: Vector3) -> Matrix4x4 {
        let rot = match look_rotation_matrix(target.sub(from), up) {
            LookMatrix::Frame(m) => m,
            LookMatrix::Degenerate | LookMatrix::Unwritten => [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
        };
        Matrix4x4 {
            m: [
                rot[0], rot[1], rot[2], 0.0, rot[3], rot[4], rot[5], 0.0, rot[6], rot[7], rot[8], 0.0, from.x, from.y,
                from.z, 1.0,
            ],
        }
    }

    /// `UnityEngine.Matrix4x4.get_inverse` → `InvertMatrix4x4_Full` (native 0x105D3B40): Gauss-Jordan with
    /// partial pivoting (the Mesa routine); all zeros when singular.
    pub fn inverse(&self) -> Matrix4x4 {
        invert_full(&self.m).map(|m| Matrix4x4 { m }).unwrap_or(Matrix4x4 { m: [0.0; 16] })
    }

    /// `UnityEngine.Matrix4x4.get_lossyScale` → `Matrix4x4f::GetLossyScale` (native 0x105D3250): the lengths
    /// of the first three columns, x negated when the 3x3 part's determinant is negative.
    pub fn lossy_scale(&self) -> Vector3 {
        let m = &self.m;
        let mut x = sqrt_f64(m[0] * m[0] + m[1] * m[1] + m[2] * m[2]);
        let y = sqrt_f64(m[5] * m[5] + m[4] * m[4] + m[6] * m[6]);
        let z = sqrt_f64(m[9] * m[9] + m[8] * m[8] + m[10] * m[10]);
        let a = [m[0], m[1], m[2], m[4], m[5], m[6], m[8], m[9], m[10]];
        if determinant3(&a) < 0.0 {
            x *= -1.0;
        }
        v3(x, y, z)
    }

    /// `UnityEngine.Matrix4x4.GetHashCode`: `c0 ^ (c1 << 2) ^ (c2 >> 2) ^ (c3 >> 1)` of the columns' hashes.
    pub fn hash_code(&self) -> i32 {
        self.column(0).hash_code()
            ^ (self.column(1).hash_code() << 2)
            ^ (self.column(2).hash_code() >> 2)
            ^ (self.column(3).hash_code() >> 1)
    }
}

/// `QuaternionToMatrix(Quaternionf, Matrix4x4f)` (native 0x105D62F0).
pub fn quaternion_to_matrix(q: Quaternion) -> Matrix4x4 {
    let (x, y, z, w) = (q.x, q.y, q.z, q.w);
    let z2 = z + z;
    let y2 = y + y;
    let xx2 = x * (x + x);
    let wx2 = w * (x + x);
    let mut m = [0f32; 16];
    m[0] = 1.0 - (z * z2 + y * y2);
    m[1] = w * z2 + x * y2;
    m[4] = x * y2 - w * z2;
    m[2] = x * z2 - w * y2;
    m[8] = w * y2 + x * z2;
    m[5] = 1.0 - (z * z2 + xx2);
    m[6] = wx2 + y * z2;
    m[9] = y * z2 - wx2;
    m[10] = 1.0 - (y * y2 + xx2);
    m[15] = 1.0;
    Matrix4x4 { m }
}

/// `Matrix3x3f::GetDeterminant` (native 0x105E0CD0), in its evaluation order.
pub fn determinant3(a: &Matrix3) -> f32 {
    (((a[0] * a[4] * a[8] + a[3] * a[7] * a[2] + a[1] * a[6] * a[5]) - a[6] * a[4] * a[2]) - a[1] * a[3] * a[8])
        - a[0] * a[7] * a[5]
}

/// `InvertMatrix4x4_Full` (native): the Mesa Gauss-Jordan elimination on rows `[m | I]`.
pub fn invert_full(m: &[f32; 16]) -> Option<[f32; 16]> {
    let at = |r: usize, c: usize| m[c * 4 + r];
    let mut w = [[0f32; 8]; 4];
    for r in 0..4 {
        for c in 0..4 {
            w[r][c] = at(r, c);
        }
        w[r][4 + r] = 1.0;
    }
    let mut r = [0usize, 1, 2, 3]; // row order
    if w[r[3]][0].abs() > w[r[2]][0].abs() {
        r.swap(3, 2);
    }
    if w[r[2]][0].abs() > w[r[1]][0].abs() {
        r.swap(2, 1);
    }
    if w[r[1]][0].abs() > w[r[0]][0].abs() {
        r.swap(1, 0);
    }
    if w[r[0]][0] == 0.0 {
        return None;
    }
    let (r0, r1, r2, r3) = (r[0], r[1], r[2], r[3]);
    let m1 = w[r1][0] / w[r0][0];
    let m2 = w[r2][0] / w[r0][0];
    let m3 = w[r3][0] / w[r0][0];
    for c in 1..8 {
        let s = w[r0][c];
        if c >= 4 && s == 0.0 {
            continue;
        }
        w[r1][c] -= m1 * s;
        w[r2][c] -= m2 * s;
        w[r3][c] -= m3 * s;
    }
    let mut r = [r1, r2, r3];
    if w[r[2]][1].abs() > w[r[1]][1].abs() {
        r.swap(2, 1);
    }
    if w[r[1]][1].abs() > w[r[0]][1].abs() {
        r.swap(1, 0);
    }
    let (r1, r2, r3) = (r[0], r[1], r[2]);
    if w[r1][1] == 0.0 {
        return None;
    }
    let m2 = w[r2][1] / w[r1][1];
    let m3 = w[r3][1] / w[r1][1];
    for c in 2..8 {
        let s = w[r1][c];
        if c >= 4 && s == 0.0 {
            continue;
        }
        w[r2][c] -= m2 * s;
        w[r3][c] -= m3 * s;
    }
    let (r2, r3) = if w[r3][2].abs() > w[r2][2].abs() { (r3, r2) } else { (r2, r3) };
    if w[r2][2] == 0.0 {
        return None;
    }
    let m3 = w[r3][2] / w[r2][2];
    for c in 3..8 {
        w[r3][c] -= m3 * w[r2][c];
    }
    if w[r3][3] == 0.0 {
        return None;
    }
    let s = 1.0 / w[r3][3];
    for c in 4..8 {
        w[r3][c] *= s;
    }
    let m2 = w[r2][3];
    let s = 1.0 / w[r2][2];
    for c in 4..8 {
        w[r2][c] = s * (w[r2][c] - w[r3][c] * m2);
    }
    let m1 = w[r1][3];
    for c in 4..8 {
        w[r1][c] -= w[r3][c] * m1;
    }
    let m0 = w[r0][3];
    for c in 4..8 {
        w[r0][c] -= w[r3][c] * m0;
    }
    let m1 = w[r1][2];
    let s = 1.0 / w[r1][1];
    for c in 4..8 {
        w[r1][c] = s * (w[r1][c] - w[r2][c] * m1);
    }
    let m0 = w[r0][2];
    for c in 4..8 {
        w[r0][c] -= w[r2][c] * m0;
    }
    let m0 = w[r0][1];
    let s = 1.0 / w[r0][0];
    for c in 4..8 {
        w[r0][c] = s * (w[r0][c] - w[r1][c] * m0);
    }
    let mut out = [0f32; 16];
    for (row, &ri) in [r0, r1, r2, r3].iter().enumerate() {
        for c in 0..4 {
            out[c * 4 + row] = w[ri][4 + c];
        }
    }
    Some(out)
}
