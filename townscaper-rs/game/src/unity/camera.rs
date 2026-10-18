//! `UnityEngine.Camera` (row E12): the matrices and the screen / viewport / world conversions the game reads
//! (`HoverData.SetHover` picks blocks through `ScreenPointToRay`, `ManySoundsManager` and the prop animator use
//! `WorldToViewportPoint` / `ViewportPointToRay`). The engine computes them natively in UnityPlayer.dll; the
//! routines are transcribed here from its disassembly (SSE columns on [`X`] where the engine vectorises, scalar
//! single-precision steps in the engine's order elsewhere). Checked against the running game by
//! `tests/unity_camera.rs` (`TS_HARNESS=1`).

use super::geometry::{Rect, Ray};
use super::quaternion::Matrix4x4;
use super::sse::X;
use super::transform::{TransformId, Transforms};
use super::vector::{sqrt_f64, v3, Vector2, Vector3};

/// The camera state the conversions read (the engine's `Camera` fields).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    /// The camera's transform.
    pub transform: TransformId,
    /// `fieldOfView` (vertical, degrees; 0x144).
    pub field_of_view: f32,
    /// `aspect` (0x458).
    pub aspect: f32,
    /// `nearClipPlane` (0x3CC), `farClipPlane` (0x3D0).
    pub near: f32,
    pub far: f32,
    /// `rect`: the normalised viewport (0x394).
    pub rect: Rect,
    /// `projectionMatrix` when set explicitly (`ProjectionMatrixMode` 0); `None` for the implicit perspective.
    pub projection: Option<Matrix4x4>,
}

/// `(x, y, width, height)` in whole pixels (`RectT<int>`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RectInt {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

fn cols(m: &Matrix4x4) -> [X; 4] {
    let c = |i: usize| X::from_f([m.m[i * 4], m.m[i * 4 + 1], m.m[i * 4 + 2], m.m[i * 4 + 3]]);
    [c(0), c(1), c(2), c(3)]
}

fn from_cols(c: [X; 4]) -> Matrix4x4 {
    let mut m = [0f32; 16];
    for (i, x) in c.iter().enumerate() {
        m[i * 4..i * 4 + 4].copy_from_slice(&x.f());
    }
    Matrix4x4 { m }
}

/// `Matrix4x4f::operator*=` 0x105D2110 (and the product inlined in `Camera::GetWorldToClipMatrix` 0x104B5410):
/// column `j` of `a · b` = `((a0·bj.x + a1·bj.y) + a2·bj.z) + a3·bj.w`.
pub fn mul(a: &Matrix4x4, b: &Matrix4x4) -> Matrix4x4 {
    let a = cols(a);
    let col = |v: X| a[1].mul(v.pshufd(0x55)).add(a[0].mul(v.pshufd(0x00))).add(a[2].mul(v.pshufd(0xAA))).add(a[3].mul(v.pshufd(0xFF)));
    let b = cols(b);
    from_cols([col(b[0]), col(b[1]), col(b[2]), col(b[3])])
}

/// `Matrix4x4f::SetPerspective` 0x105D73A0: `a = ((fov·0.5) / 360 · 2)·π`, `cot = (float)cos(a) / (float)sin(a)`
/// (both in double); `m00 = cot / aspect`, `m11 = cot`, `m22 = (near + far) / (near - far)`,
/// `m23 = (near + near)·far / (near - far)`, `m32 = -1`.
pub fn perspective(fov: f32, aspect: f32, near: f32, far: f32) -> Matrix4x4 {
    let mut a = (fov * 0.5) / 360.0;
    a += a;
    a *= std::f32::consts::PI;
    let cot = ((a as f64).cos() as f32) / ((a as f64).sin() as f32);
    let d = near - far;
    let mut m = [0f32; 16];
    m[0] = cot / aspect;
    m[5] = cot;
    m[10] = (near + far) / d;
    m[11] = -1.0;
    m[14] = ((near + near) * far) / d;
    Matrix4x4 { m }
}

/// `InvertMatrix4x4_Full` 0x105D3B40 (the Mesa routine in [`super::quaternion::invert_full`]); `None` when a
/// pivot is zero (the engine writes zeros and returns false).
pub fn invert_full(m: &Matrix4x4) -> Option<Matrix4x4> {
    super::quaternion::invert_full(&m.m).map(|m| Matrix4x4 { m })
}

/// `RectfToRectInt` 0x1047A930: the corners rounded (`+0.5`, less `0.99999994` below zero, truncated; the far
/// corner through `__ftoui3`), the size their difference.
pub fn rect_to_int(r: &Rect) -> RectInt {
    let round = |v: f32| {
        let mut v = v + 0.5;
        if v < 0.0 {
            v -= 0.99999994;
        }
        v as i32
    };
    let x = round(r.x);
    let y = round(r.y);
    let x1 = ((r.width + r.x) + 0.5) as u32 as i32;
    let y1 = ((r.height + r.y) + 0.5) as u32 as i32;
    RectInt { x, y, width: x1.wrapping_sub(x), height: y1.wrapping_sub(y) }
}

impl Camera {
    /// `Camera::GetWorldToCameraMatrix` 0x104B53B0: `Scale(1, 1, -1) · worldToLocalMatrixNoScale`.
    pub fn world_to_camera_matrix(&self, tr: &Transforms) -> Matrix4x4 {
        let mut s = Matrix4x4 { m: [0f32; 16] };
        s.m[0] = 1.0;
        s.m[5] = 1.0;
        s.m[10] = -1.0;
        s.m[15] = 1.0;
        mul(&s, &tr.world_to_local_matrix_no_scale(self.transform))
    }

    /// `Camera::GetProjectionMatrix` 0x104B3CE0: the explicit matrix, or `SetPerspective(fieldOfView, aspect,
    /// near, far)`. (Orthographic and physical cameras are not ported: no camera the game picks through is.)
    pub fn projection_matrix(&self) -> Matrix4x4 {
        match self.projection {
            Some(m) => m,
            None => perspective(self.field_of_view, self.aspect, self.near, self.far),
        }
    }

    /// `Camera::GetWorldToClipMatrix` 0x104B5410: `projection · worldToCamera`.
    pub fn world_to_clip_matrix(&self, tr: &Transforms) -> Matrix4x4 {
        mul(&self.projection_matrix(), &self.world_to_camera_matrix(tr))
    }

    /// The viewport in pixels (`GetScreenViewportRectInt` 0x104B4310 before rounding, `ViewportToScreenPoint`
    /// 0x104C0B90): the target `(0, 0, w, h)` scaled by `rect`, clamped to the target, sizes not below 0.
    pub fn screen_viewport_rect(&self, screen: Vector2) -> Rect {
        let (tx, ty, tw, th) = (0.0f32, 0.0f32, screen.x, screen.y);
        let mut x = tx + tw * self.rect.x;
        let mut y = ty + th * self.rect.y;
        let x1 = tw * self.rect.width + x;
        let y1 = th * self.rect.height + y;
        if tx > x {
            x = tx;
        }
        let mut xe = tx + tw;
        if !(xe < x1) {
            xe = x1;
        }
        if ty > y {
            y = ty;
        }
        let mut ye = ty + th;
        if !(ye < y1) {
            ye = y1;
        }
        let mut w = xe - x;
        if 0.0 > w {
            w = 0.0;
        }
        let mut h = ye - y;
        if 0.0 > h {
            h = 0.0;
        }
        Rect { x, y, width: w, height: h }
    }

    /// `Camera.ScreenPointToRay` (`Camera::ScreenPointToRay` 0x104BBCF0, mono): the screen point unprojected
    /// ([`unproject`]) at the near plane and 1000 beyond it, through the inverse clip and camera matrices; the
    /// ray from the first towards the second, normalised by `(float)sqrt((double)(dx² + dy² + dz²))`. Out of the
    /// frustum the engine logs and returns the camera's position looking along +z.
    pub fn screen_point_to_ray(&self, tr: &Transforms, screen: Vector2, p: Vector2) -> Ray {
        let rect = rect_to_int(&self.screen_viewport_rect(screen));
        let clip_to_world = invert_full(&self.world_to_clip_matrix(tr)).unwrap_or(Matrix4x4 { m: [0.0; 16] });
        let cam_to_world = invert_full(&self.world_to_camera_matrix(tr)).unwrap_or(Matrix4x4 { m: [0.0; 16] });
        let fallback = Ray { origin: tr.position(self.transform), direction: v3(0.0, 0.0, 1.0) };
        let Some(a) = unproject(v3(p.x, p.y, self.near), &cam_to_world, &clip_to_world, &rect) else { return fallback };
        let Some(b) = unproject(v3(p.x, p.y, self.near + 1000.0), &cam_to_world, &clip_to_world, &rect) else { return fallback };
        let (dx, dy, dz) = (b.x - a.x, b.y - a.y, b.z - a.z);
        let m = sqrt_f64((dx * dx + dy * dy) + dz * dz);
        Ray { origin: a, direction: v3(dx / m, dy / m, dz / m) }
    }

    /// `Camera.WorldToScreenPoint` (`Camera::WorldToScreenPoint` 0x104C11F0 over `CameraProjectionCache`):
    /// [`project`] with the inverse camera matrix and the clip matrix.
    pub fn world_to_screen_point(&self, tr: &Transforms, screen: Vector2, p: Vector3) -> Vector3 {
        let rect = rect_to_int(&self.screen_viewport_rect(screen));
        let cam_to_world = invert_full(&self.world_to_camera_matrix(tr)).unwrap_or(Matrix4x4 { m: [0.0; 16] });
        project(p, &cam_to_world, &self.world_to_clip_matrix(tr), &rect).unwrap_or(Vector3::ZERO)
    }

    /// `Camera.WorldToViewportPoint` (`Camera::WorldToViewportPoint` 0x104C1260): the screen point, less the
    /// viewport's corner, over its size (the float viewport).
    pub fn world_to_viewport_point(&self, tr: &Transforms, screen: Vector2, p: Vector3) -> Vector3 {
        let vr = self.screen_viewport_rect(screen);
        let s = self.world_to_screen_point(tr, screen, p);
        v3((s.x - vr.x) / vr.width, (s.y - vr.y) / vr.height, s.z)
    }

    /// `Camera.ViewportToScreenPoint` (`Camera::ViewportToScreenPoint` 0x104C0B90): `v · size + corner`.
    pub fn viewport_to_screen_point(&self, screen: Vector2, v: Vector3) -> Vector3 {
        let vr = self.screen_viewport_rect(screen);
        v3(v.x * vr.width + vr.x, v.y * vr.height + vr.y, v.z)
    }

    /// `Camera.ViewportPointToRay` (`Camera::ViewportPointToRay` 0x104C09F0): through the screen point.
    pub fn viewport_point_to_ray(&self, tr: &Transforms, screen: Vector2, v: Vector2) -> Ray {
        let s = self.viewport_to_screen_point(screen, v3(v.x, v.y, 0.0));
        self.screen_point_to_ray(tr, screen, Vector2 { x: s.x, y: s.y })
    }
}

/// `CameraUnProject` 0x10476330: the screen point to clip space (`((p - corner)·2) / size - 1`, depth 0.95),
/// through `clip_to_world` (each row `((m[1]·y + m[0]·x) + m[2]·0.95) + m[3]`), divided by `w` when `|w| > 1e-7`;
/// then moved along the camera's view direction (`-cam_to_world` column 2) so its depth is `p.z`: for a
/// perspective clip matrix scaled out from the camera's position, for an affine one shifted.
pub fn unproject(p: Vector3, cam_to_world: &Matrix4x4, clip_to_world: &Matrix4x4, r: &RectInt) -> Option<Vector3> {
    let c = &clip_to_world.m;
    let mut x = p.x - r.x as f32;
    x += x;
    let x = x / r.width as f32 - 1.0;
    let mut y = p.y - r.y as f32;
    y += y;
    let y = y / r.height as f32 - 1.0;
    let z = 0.95f32;
    let wx = ((c[4] * y + c[0] * x) + c[8] * z) + c[12];
    let wy = ((c[5] * y + c[1] * x) + c[9] * z) + c[13];
    let wz = ((c[6] * y + c[2] * x) + c[10] * z) + c[14];
    let ww = ((c[7] * y + c[3] * x) + c[11] * z) + c[15];
    if !(ww.abs() > 1e-7) {
        return None;
    }
    let inv = 1.0 / ww;
    let (wx, wy, wz) = (wx * inv, wy * inv, wz * inv);
    let m = &cam_to_world.m;
    let dz = wz - m[14];
    let dx = wx - m[12];
    let dy = wy - m[13];
    let f = neg(m[8]);
    let g = neg(m[9]);
    let h = neg(m[10]);
    let d = (g * dy + dx * f) + dz * h;
    if !(d.abs() >= 1e-6) {
        return None;
    }
    if c[3] == 0.0 && c[7] == 0.0 && c[11] == 0.0 && c[15] == 1.0 {
        let d = d - p.z;
        return Some(v3(wx - f * d, wy - g * d, wz - h * d));
    }
    let s = p.z / d;
    Some(v3(m[12] + dx * s, dy * s + m[13], m[14] + dz * s))
}

/// `CameraProject` 0x10476190: through `world_to_clip` (rows `((m[0]·x + m[1]·y) + m[2]·z) + m[3]`), divided by
/// `w` when `|w| > 1e-7`, to pixels (`(size·(c + 1))·0.5 + corner`); the depth along the view direction
/// (`(-col2.x·(x - t.x) + -col2.y·(y - t.y)) + -col2.z·(z - t.z)` with `cam_to_world`'s columns).
pub fn project(p: Vector3, cam_to_world: &Matrix4x4, world_to_clip: &Matrix4x4, r: &RectInt) -> Option<Vector3> {
    let c = &world_to_clip.m;
    let cx = ((c[4] * p.y + c[0] * p.x) + c[8] * p.z) + c[12];
    let cy = ((c[1] * p.x + c[5] * p.y) + c[9] * p.z) + c[13];
    let cw = ((c[3] * p.x + c[7] * p.y) + c[11] * p.z) + c[15];
    if !(cw.abs() > 1e-7) {
        return None;
    }
    let inv = 1.0 / cw;
    let m = &cam_to_world.m;
    let sx = ((r.width as f32 * (cx * inv + 1.0)) * 0.5) + r.x as f32;
    let sy = ((r.height as f32 * (cy * inv + 1.0)) * 0.5) + r.y as f32;
    let sz = (neg(m[8]) * (p.x - m[12]) + neg(m[9]) * (p.y - m[13])) + neg(m[10]) * (p.z - m[14]);
    Some(v3(sx, sy, sz))
}

/// `xorps` with the sign mask.
fn neg(x: f32) -> f32 {
    f32::from_bits(x.to_bits() ^ 0x8000_0000)
}
