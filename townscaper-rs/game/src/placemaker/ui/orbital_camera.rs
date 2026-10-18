//! `Placemaker.Ui.OrbitalCamera` (row G2): the camera that circles the town. It keeps targets and eased
//! "current" values for zoom, pan (a point in the town's hex bounds, in 0..1 per axis, and a height share) and
//! rotation, and each `LateUpdate` places the camera: the eased pan point in the world, an orbit distance fitted
//! to the town's bounds and the screen, a tilt between the lowest the pan height allows and 88°, a turn, and a
//! shift that keeps the town centred in the part of the screen the menus leave free.

use crate::os::hex;
use crate::placemaker::graphs::Graph;
use crate::placemaker::grid_generator;
use crate::placemaker::save_data::SaveCam;
use crate::unity::geometry::{Plane, Ray, Rect};
use crate::unity::mathematics::{f2, f3, Float2, Float3, Int2};
use crate::unity::mathf;
use crate::unity::quaternion::Quaternion;
use crate::unity::vector::{v3, Vector3};

/// `Unity.Mathematics.float3x2`: `c0` the low corner, `c1` the high one.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Float3x2 {
    pub c0: Float3,
    pub c1: Float3,
}

impl Float3x2 {
    /// `(float3x2)int3x2`: the graph's integer bounds as floats.
    pub fn from_int(b: &crate::unity::mathematics::Int3x2) -> Float3x2 {
        Float3x2 {
            c0: f3(b.c0.x as f32, b.c0.y as f32, b.c0.z as f32),
            c1: f3(b.c1.x as f32, b.c1.y as f32, b.c1.z as f32),
        }
    }
}

/// `OrbitalCamera.maxAngle`: the steepest tilt, degrees.
pub const MAX_ANGLE: f32 = 88.0;

/// The world the camera reads in `LateUpdate`.
#[derive(Clone, Copy, Debug)]
pub struct OrbitContext {
    /// `Time.deltaTime`.
    pub dt: f32,
    /// `master.graph.anyVoxels`.
    pub any_voxels: bool,
    /// `Screen.width`, `Screen.height`.
    pub screen_width: i32,
    pub screen_height: i32,
    /// `master.dim.alphaState.current` (row G7a).
    pub dim_alpha: f32,
    /// `master.gameViewRect.GetRelativeRect()` (row G7a): the screen share the menus leave free, 0..1.
    pub game_view_rect: Rect,
}

/// What `LateUpdate` sets on `cam`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CameraPose {
    pub position: Vector3,
    pub rotation: Quaternion,
    pub field_of_view: f32,
    pub far_clip_plane: f32,
}

/// The component's state (the fields after the scene references `master`, `cam`, `skyBoxCam`, `mirrorCam`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OrbitalCamera {
    pub zoom_target: f32,
    pub zoom_current: f32,
    pub rotation_y: f32,
    pub rotation_x: f32,
    pub pan_hex_current_x: f32,
    pub pan_hex_current_y: f32,
    pub pan_height_current: f32,
    pub pan_hex_target_x: f32,
    pub pan_hex_target_y: f32,
    pub pan_height_target: f32,
    pub rotation_speed_x: f32,
    pub rotation_speed_y: f32,
    pub bounds_current: Float3x2,
    pub bounds_target: Float3x2,
    pub height_current: f32,
    pub height_target: f32,
    pub any_voxels_current: f32,
    pub min_radius: f32,
    pub snap: bool,
    pub current_dist: f32,
    pub non_dolly_dist: f32,
    pub last_bounds_change_time: f32,
    pub dist_min: f32,
    pub dist_max: f32,
}

impl Default for OrbitalCamera {
    /// `OrbitalCamera..ctor` 0x6FBF30: zoom, tilt and pans at ½, `minRadius` 3.5, snapping on the first frame.
    fn default() -> OrbitalCamera {
        OrbitalCamera {
            zoom_target: 0.5,
            zoom_current: 0.5,
            rotation_y: 0.0,
            rotation_x: 0.5,
            pan_hex_current_x: 0.5,
            pan_hex_current_y: 0.5,
            pan_height_current: 0.5,
            pan_hex_target_x: 0.5,
            pan_hex_target_y: 0.5,
            pan_height_target: 0.5,
            rotation_speed_x: 0.0,
            rotation_speed_y: 0.0,
            bounds_current: Float3x2::default(),
            bounds_target: Float3x2::default(),
            height_current: 0.0,
            height_target: 0.0,
            any_voxels_current: 0.0,
            min_radius: 3.5,
            snap: true,
            current_dist: 0.0,
            non_dolly_dist: 0.0,
            last_bounds_change_time: 0.0,
            dist_min: 0.0,
            dist_max: 0.0,
        }
    }
}

/// `(float)exp(-dt * k)`, or 0 when snapping.
fn decay(snap: bool, dt: f32, k: f32) -> f32 {
    if snap {
        0.0
    } else {
        (((-dt) * k) as f64).exp() as f32
    }
}

/// `asin(Max(y / dist, -0.95))` in degrees: the tilt that looks from `dist` away down to height `y`.
fn min_angle(y: f32, dist: f32) -> f32 {
    let s = mathf::max(y / dist, -0.95);
    ((s as f64).asin() as f32) * 57.29578
}

/// `Distance(HexToPlane(a), HexToPlane(b)) / 2` in the build's float/double steps.
fn half_span(a: Float3, b: Float3) -> f32 {
    let p = hex::hex_to_plane(a);
    let q = hex::hex_to_plane(b);
    let (dx, dy) = (p.x - q.x, p.y - q.y);
    let d2 = dx * dx + dy * dy;
    ((d2 as f64).sqrt() as f32) * 0.5
}

/// `x` unless it is NaN or below `floor`.
fn at_least(x: f32, floor: f32) -> f32 {
    if !x.is_nan() && x > floor {
        x
    } else {
        floor
    }
}

/// The `(float)fmod(x, 1)` the build does on the x87 unit.
fn fmod1(x: f32) -> f32 {
    ((x as f64) % 1.0) as f32
}

impl OrbitalCamera {
    /// `OrbitalCamera.GetWorldPos(panHexX, panHexY, panHeight, bounds, height)` 0x6F7D30: the pan point in
    /// the bounds (`lerp(c0, c1, (x, y, 0))`, as a hex position with `z = -x - y`) on the plane, at
    /// `LerpUnclamped(0, height, panHeight)`.
    pub fn get_world_pos(pan_hex_x: f32, pan_hex_y: f32, pan_height: f32, bounds: Float3x2, height: f32) -> Vector3 {
        let (b0, b1) = (bounds.c0, bounds.c1);
        let h = f3((b1.x - b0.x) * pan_hex_x + b0.x, (b1.y - b0.y) * pan_hex_y + b0.y, (b1.z - b0.z) * 0.0 + b0.z);
        let hex = f3(h.x, h.y, -h.x - h.y);
        let p = grid_generator::hex_to_plane(hex);
        let y = mathf::lerp_unclamped(0.0, height, pan_height);
        let w = hex::plane_to_world(p, y);
        v3(w.x, w.y, w.z)
    }

    /// `OrbitalCamera.GetRelativePos(worldPos, bounds, height)` 0x6F7BA0: the inverse: the position's hex
    /// coordinates as shares of the bounds, and its height as a share of `height` (each clamped to 0..1).
    pub fn get_relative_pos(world_pos: Vector3, bounds: Float3x2, height: f32) -> (f32, f32, f32) {
        let h = grid_generator::plane_to_hex(hex::world_to_plane(f3(world_pos.x, world_pos.y, world_pos.z)));
        (
            mathf::inverse_lerp(bounds.c0.x, bounds.c1.x, h.x),
            mathf::inverse_lerp(bounds.c0.y, bounds.c1.y, h.y),
            mathf::inverse_lerp(0.0, height, world_pos.y),
        )
    }

    /// `OrbitalCamera.GetMinAngle(dist)` 0x6F7B40: the tilt that looks from `dist` down to the pan height.
    pub fn get_min_angle(&self, dist: f32) -> f32 {
        min_angle(self.pan_height_current * self.height_current, dist)
    }

    /// `OrbitalCamera.RemapPans(oldBounds, newBounds, oldHeight, newHeight)` 0x6FAEB0: keeps the pan target on
    /// the same world point when the bounds change.
    pub fn remap_pans(&mut self, old_bounds: Float3x2, new_bounds: Float3x2, old_height: f32, new_height: f32) {
        let w = Self::get_world_pos(self.pan_hex_target_x, self.pan_hex_target_y, self.pan_height_target, old_bounds, old_height);
        let (x, y, z) = Self::get_relative_pos(w, new_bounds, new_height);
        self.pan_hex_target_x = x;
        self.pan_hex_target_y = y;
        self.pan_height_target = z;
    }

    /// `OrbitalCamera.SetBounds` 0x6FB4E0 (the graph's `onEmptyBoundsLoaded`): the target bounds become the
    /// graph's, stamped with `Time.unscaledTime`.
    pub fn set_bounds(&mut self, graph: &Graph, unscaled_time: f32) {
        self.bounds_target = Float3x2::from_int(&graph.bounds);
        self.last_bounds_change_time = unscaled_time;
    }

    /// `OrbitalCamera.BoundsUpdated` 0x6F7670 (the graph's `onBoundsUpdated`, and once at setup): with voxels,
    /// the target bounds become the graph's (first remapping the pan target when the gamepad cursor is
    /// adding, so the view stays on the cursor); the target height becomes `boundsHeight` either way.
    pub fn bounds_updated(&mut self, graph: &Graph, gamepad_adding: bool, unscaled_time: f32) {
        if graph.any_voxels() {
            if gamepad_adding {
                let new_bounds = Float3x2::from_int(&graph.bounds);
                self.remap_pans(self.bounds_target, new_bounds, self.height_target, graph.bounds_height as f32);
            }
            self.set_bounds(graph, unscaled_time);
        }
        self.height_target = graph.bounds_height as f32;
    }

    /// `OrbitalCamera.LateUpdate` 0x6F7F60: eases every current value towards its target (`exp(-10 dt)`, zoom
    /// `exp(-20 dt)`, nothing when snapping); a spinning camera keeps turning with its speed decaying
    /// (`exp(-4 dt) - 0.4 dt`); then places the camera (see the module doc) and stops snapping.
    pub fn late_update(&mut self, cx: &OrbitContext) -> CameraPose {
        let t1 = decay(self.snap, cx.dt, 10.0);
        let t2 = decay(self.snap, cx.dt, 20.0);
        let ease = |cur: f32, tgt: f32, t: f32| (cur - tgt) * t + tgt;
        self.height_current = ease(self.height_current, self.height_target, t1);
        let (bc, bt) = (&mut self.bounds_current, self.bounds_target);
        bc.c0 = f3(ease(bc.c0.x, bt.c0.x, t1), ease(bc.c0.y, bt.c0.y, t1), ease(bc.c0.z, bt.c0.z, t1));
        bc.c1 = f3(ease(bc.c1.x, bt.c1.x, t1), ease(bc.c1.y, bt.c1.y, t1), ease(bc.c1.z, bt.c1.z, t1));
        self.pan_hex_current_x = ease(self.pan_hex_current_x, self.pan_hex_target_x, t1);
        self.pan_hex_current_y = ease(self.pan_hex_current_y, self.pan_hex_target_y, t1);
        self.pan_height_current = ease(self.pan_height_current, self.pan_height_target, t1);
        self.zoom_current = ease(self.zoom_current, self.zoom_target, t2);
        let any = cx.any_voxels as i32 as f32;
        self.any_voxels_current = ease(self.any_voxels_current, any, t1);
        if self.rotation_speed_y != 0.0 || self.rotation_speed_x != 0.0 {
            let e = decay(self.snap, cx.dt, 4.0);
            // x87: the product and the difference in double, rounded to float once
            let k = (e as f64 - cx.dt as f64 * 0.4f32 as f64) as f32;
            self.rotation_speed_y = mathf::lerp(0.0, self.rotation_speed_y, k);
            self.rotation_y = fmod1(cx.dt * self.rotation_speed_y + (self.rotation_y + 10.0));
            self.rotation_speed_x = mathf::lerp(0.0, self.rotation_speed_x, k);
            let rx = self.rotation_x;
            let p = crate::os::extra_math::pow2((rx + rx) - 1.0);
            self.rotation_x = mathf::clamp01(cx.dt * self.rotation_speed_x * (1.0 - p) + rx);
        }
        let pan = Self::get_world_pos(self.pan_hex_current_x, self.pan_hex_current_y, self.pan_height_current, self.bounds_current, self.height_current);
        // the bounds' radius on the plane: the longest of the box's three diagonals, at least minRadius
        const K: f32 = 0.121654496;
        let b = self.bounds_current;
        let (l, h) = (f3(b.c0.x * K, b.c0.y * K, b.c0.z * K), f3(b.c1.x * K, b.c1.y * K, b.c1.z * K));
        let r = at_least(half_span(f3(l.x, h.y, h.z), f3(h.x, l.y, l.z)), self.min_radius);
        let r = at_least(half_span(f3(h.x, l.y, h.z), f3(l.x, h.y, l.z)), r);
        let r = at_least(half_span(f3(h.x, h.y, l.z), f3(l.x, l.y, h.z)), r);
        let hh = self.height_current * 0.5;
        let hyp = ((r * r + hh * hh) as f64).sqrt() as f32;
        let ratio = cx.screen_height as f32 / cx.screen_width as f32;
        let fit = mathf::max(mathf::max(r * ratio, r), hyp);
        let fov = mathf::lerp(35.0, 16.0, self.zoom_current);
        self.dist_min = 2.0;
        self.dist_max = fit * 1.7;
        self.dist_max = mathf::max(fit * 1.7, 4.0);
        self.dist_max = mathf::lerp(1.4, 1.0, self.any_voxels_current) * self.dist_max;
        let dim = mathf::lerp(1.0, 1.03, cx.dim_alpha);
        self.dist_max = dim * self.dist_max;
        self.dist_min = dim * self.dist_min;
        self.non_dolly_dist = mathf::lerp(self.dist_min, self.dist_max, self.zoom_current);
        let tan = ((fov * 0.017453292) as f64).tan() as f32;
        self.current_dist = (2.0 / tan) * self.non_dolly_dist;
        let neg_dist = f32::from_bits(std::hint::black_box(self.current_dist.to_bits()) ^ 0x8000_0000);
        let low = min_angle(pan.y, neg_dist);
        let angle = mathf::lerp(low, MAX_ANGLE, self.rotation_x);
        let rot = Quaternion::euler(angle, self.rotation_y * 360.0, 0.0);
        let back = Quaternion::rotate(rot, v3(0.0, 0.0, neg_dist));
        let mut pos = v3(back.x + pan.x, back.y + pan.y, back.z + pan.z);
        let c = cx.game_view_rect.center();
        let (dx, dy) = (c.x - 0.5, c.y - 0.5);
        if !(9.9999994e-11 > dy * dy + dx * dx) {
            // (the build scales only the x shift; y is the raw centre offset)
            let sx = tan * self.current_dist * 0.5 / ratio * dx;
            let o = Quaternion::rotate(rot, v3(sx, dy, 0.0));
            pos = v3(pos.x - o.x, pos.y - o.y, pos.z - o.z);
        }
        self.snap = false;
        CameraPose { position: pos, rotation: rot, field_of_view: fov, far_clip_plane: self.current_dist + 1000.0 }
    }

    /// `OrbitalCamera.StopRotation` 0x6FB680.
    pub fn stop_rotation(&mut self) {
        self.rotation_speed_y = 0.0;
        self.rotation_speed_x = 0.0;
    }

    /// `1 / (88 - minAngle)`: a drag's tilt share per unit.
    fn tilt_scale(&self) -> f32 {
        let neg = f32::from_bits(std::hint::black_box(self.current_dist.to_bits()) ^ 0x8000_0000);
        1.0 / (MAX_ANGLE - min_angle(self.pan_height_current * self.height_current, neg))
    }

    /// `OrbitalCamera.SetRotationSpeed(delta)` 0x6FB5C0: a flick: turn speed `delta.x / 360`, tilt speed
    /// `delta.y` over the tilt range; a negative turn wrapped into 0..1.
    pub fn set_rotation_speed(&mut self, delta: Float2) {
        let s = self.tilt_scale();
        self.rotation_speed_y = delta.x * 0.0027777778;
        self.rotation_speed_x = s * delta.y;
        while 0.0 > self.rotation_y {
            self.rotation_y += 1.0;
        }
    }

    /// `OrbitalCamera.RotateCamera(delta)` 0x6FB2A0: a drag: tilt by `delta.y` over the tilt range (clamped),
    /// turn by `delta.x / 360` (wrapped).
    pub fn rotate_camera(&mut self, delta: Float2) {
        let s = self.tilt_scale();
        self.rotation_x = mathf::clamp01(s * delta.y + self.rotation_x);
        self.rotation_y = fmod1((self.rotation_y + 1.0) + delta.x * 0.0027777778);
        while 0.0 > self.rotation_y {
            self.rotation_y += 1.0;
        }
    }

    /// The zoom that puts the camera at `f` times the current distance.
    fn zoom_to(&self, f: f32) -> f32 {
        mathf::inverse_lerp(self.dist_min, self.dist_max, f * self.non_dolly_dist)
    }

    /// `OrbitalCamera.ScrollZoom(delta)` 0x6FB470: the target distance `2^(-4 delta)` times the current.
    pub fn scroll_zoom(&mut self, delta: f32) {
        let f = 2f64.powf((delta * -4.0) as f64) as f32;
        self.zoom_target = self.zoom_to(f);
    }

    /// `OrbitalCamera.PinchZoom(multiplier)` 0x6FAC00: target and current at `multiplier` times the distance.
    pub fn pinch_zoom(&mut self, multiplier: f32) {
        let z = self.zoom_to(multiplier);
        self.zoom_target = z;
        self.zoom_current = z;
    }

    /// `OrbitalCamera.GamepadZoom(delta)` 0x6F7AA0: as a scroll of `exp(-unscaledDeltaTime) * delta / 10`.
    pub fn gamepad_zoom(&mut self, delta: f32, unscaled_dt: f32) {
        let e = ((-unscaled_dt) as f64).exp() as f32;
        let f = 2f64.powf((e * delta * 0.1 * -4.0) as f64) as f32;
        self.zoom_target = self.zoom_to(f);
    }

    /// `OrbitalCamera.Focus(pos)` 0x6F7950: the pan target onto a world point (the height share only when
    /// there is height; each hex axis only when the bounds have width on it).
    pub fn focus(&mut self, pos: Vector3) {
        if 0.0 < self.height_target {
            self.pan_height_target = mathf::clamp01(pos.y / self.height_target);
        }
        let h = grid_generator::plane_to_hex(hex::world_to_plane(f3(pos.x, pos.y, pos.z)));
        let b = self.bounds_target;
        if b.c0.x < b.c1.x {
            self.pan_hex_target_x = mathf::inverse_lerp(b.c0.x, b.c1.x, h.x);
        }
        if b.c0.y < b.c1.y {
            self.pan_hex_target_y = mathf::inverse_lerp(b.c0.y, b.c1.y, h.y);
        }
    }

    /// The pan point's hex position from `bounds` (`lerp(c0, c1, (x, y, 0))`, `z = -x - y`) on the plane.
    fn pan_plane(bounds: &Float3x2, x: f32, y: f32) -> Float2 {
        let (b0, b1) = (bounds.c0, bounds.c1);
        let hx = (b1.x - b0.x) * x + b0.x;
        let hy = (b1.y - b0.y) * y + b0.y;
        let _hz = (b1.z - b0.z) * 0.0 + b0.z;
        grid_generator::hex_to_plane(f3(hx, hy, -hx - hy))
    }

    /// Moves a pan share by the plane offset `d`: the pan point moved on the plane, back to hex coordinates,
    /// clamped to the bounds, and each axis's share taken where the current and target bounds have width.
    fn pan_by(&self, bounds: &Float3x2, x: f32, y: f32, d: Float2) -> (Option<f32>, Option<f32>) {
        let p = Self::pan_plane(bounds, x, y);
        let h = grid_generator::plane_to_hex(f2(d.x + p.x, d.y + p.y));
        let c = hex::hex_clamp(h, bounds.c0, bounds.c1);
        let t = &self.bounds_target;
        let nx = (bounds.c0.x != bounds.c1.x && t.c0.x != t.c1.x).then(|| mathf::inverse_lerp(bounds.c0.x, bounds.c1.x, c.x));
        let ny = (bounds.c0.y != bounds.c1.y && t.c0.y != t.c1.y).then(|| mathf::inverse_lerp(bounds.c0.y, bounds.c1.y, c.y));
        (nx, ny)
    }

    /// The empty town's bounds moved by a hex offset (the target's and, for a drag, the current's too); the
    /// empty bounds centre to set (`Graph.SetEmptyBoundsCenter`, then `WorldMaster.ResetState`) when the
    /// target's rounded centre left the graph's.
    fn move_empty_bounds(&mut self, h: Float3, current_too: bool, graph: &Graph) -> Option<Int2> {
        let add = |b: &mut Float3x2| {
            b.c0 = f3(b.c0.x + h.x, b.c0.y + h.y, b.c0.z + h.z);
            b.c1 = f3(b.c1.x + h.x, b.c1.y + h.y, b.c1.z + h.z);
        };
        add(&mut self.bounds_target);
        if current_too {
            add(&mut self.bounds_current);
        }
        let t = self.bounds_target;
        let cx = mathf::round_to_int((t.c1.x + t.c0.x) * 0.5);
        let cy = mathf::round_to_int((t.c1.y + t.c0.y) * 0.5);
        if cx == graph.bounds.c0.x && cy == graph.bounds.c0.y {
            return None;
        }
        Some(Int2 { x: cx, y: cy })
    }

    /// `OrbitalCamera.WASDCamera(delta)` 0x6FB870: keys move the view by `delta * nonDollyDist *
    /// unscaledDeltaTime * 2`, turned with the camera (`Euler(0, rotationY * 360, 0)`). With voxels the
    /// pan target moves on the plane (clamped to the bounds) and its height share by the vertical part; an
    /// empty town moves its bounds instead (the returned centre is for `Graph.SetEmptyBoundsCenter` and
    /// `WorldMaster.ResetState`).
    pub fn wasd_camera(&mut self, delta: Vector3, unscaled_dt: f32, graph: &Graph) -> Option<Int2> {
        let bc = self.bounds_current;
        let s = self.non_dolly_dist;
        let (mut dx, mut dy, mut dz) = (delta.x * s * unscaled_dt, delta.y * s * unscaled_dt, delta.z * s * unscaled_dt);
        dx = dx + dx;
        dy = dy + dy;
        dz = dz + dz;
        let v = Quaternion::rotate(Quaternion::euler(0.0, self.rotation_y * 360.0, 0.0), v3(dx, dy, dz));
        let w = hex::world_to_plane(f3(v.x, v.y, v.z));
        if !graph.any_voxels() {
            let h = grid_generator::plane_to_hex(w);
            return self.move_empty_bounds(h, false, graph);
        }
        let t = self.bounds_target;
        let current_flat = bc.c0.x == bc.c1.x && bc.c0.y == bc.c1.y && bc.c0.z == bc.c1.z;
        let target_flat = t.c0.x == t.c1.x && t.c0.y == t.c1.y && t.c0.z == t.c1.z;
        if !current_flat && !target_flat {
            let (nx, ny) = self.pan_by(&bc, self.pan_hex_target_x, self.pan_hex_target_y, w);
            if let Some(x) = nx {
                self.pan_hex_target_x = x;
            }
            if let Some(y) = ny {
                self.pan_hex_target_y = y;
            }
        }
        if self.height_current > 0.0 {
            let h = self.pan_height_target;
            self.pan_height_target = (mathf::clamp01(v.y / self.height_current + h) - h) + h;
        }
        None
    }

    /// `OrbitalCamera.PanCamera(pos, delta2)` 0x6F9DF0: a drag from screen point `pos` by `delta2`; `r1`, `r2`
    /// are `Camera.main.ScreenPointToRay` of `pos` and `pos + delta2`. With voxels (and height): the points
    /// at the orbit distance along both rays; the pan height share moves by their height difference
    /// (clamped), and the pan point by where the second ray meets the level plane through the first point
    /// (moved by the height actually taken), clamped to the bounds; current and target both. An empty town:
    /// the bounds move by the drag on the ground plane (the returned centre as for [`Self::wasd_camera`]).
    /// (The original also draws debug lines.)
    pub fn pan_camera(&mut self, r1: &Ray, r2: &Ray, graph: &Graph) -> Option<Int2> {
        if !graph.any_voxels() {
            let ground = Plane::new(Vector3::UP, 0.0);
            let (hit1, d1) = ground.raycast(r1);
            if !hit1 {
                return None;
            }
            let (hit2, d2) = ground.raycast(r2);
            if !hit2 {
                return None;
            }
            let (a, b) = (r1.point(d1), r2.point(d2));
            let d = f3(a.x - b.x, a.y - b.y, a.z - b.z);
            let h = grid_generator::plane_to_hex(hex::world_to_plane(d));
            return self.move_empty_bounds(h, true, graph);
        }
        if self.height_current == 0.0 {
            return None;
        }
        let p1 = r1.point(self.current_dist);
        let p2 = r2.point(self.current_dist);
        let dy = p1.y - p2.y;
        let hs = dy / self.height_current;
        let c = mathf::clamp01(self.pan_height_current + hs) - self.pan_height_current;
        self.pan_height_current = self.pan_height_current + c;
        let target = v3((p1.x - p2.x) + p2.x, (1.0 - c / hs) * dy + p2.y, (p1.z - p2.z) + p2.z);
        let level = Plane::from_point(Vector3::UP, target);
        let (hit, enter) = level.raycast(r2);
        let at = if hit { r2.point(enter) } else { p2 };
        let d = f2(target.x - at.x, target.z - at.z);
        let bc = self.bounds_current;
        let flat = bc.c0.x == bc.c1.x && bc.c0.y == bc.c1.y && bc.c0.z == bc.c1.z;
        if !flat {
            let (nx, ny) = self.pan_by(&bc, self.pan_hex_current_x, self.pan_hex_current_y, d);
            if let Some(x) = nx {
                self.pan_hex_current_x = x;
            }
            if let Some(y) = ny {
                self.pan_hex_current_y = y;
            }
        }
        self.pan_hex_target_x = self.pan_hex_current_x;
        self.pan_hex_target_y = self.pan_hex_current_y;
        self.pan_height_target = self.pan_height_current;
        None
    }

    /// `OrbitalCamera.Save(saveData)` 0x6FB3A0: zoom, tilt, turn as `(byte)Clamp(v * 255, 0, 255)`.
    pub fn save(&self) -> SaveCam {
        let b = |v: f32| mathf::clamp(v * 255.0, 0.0, 255.0) as i32 as u8;
        SaveCam { z: b(self.zoom_target), x: b(self.rotation_x), y: b(self.rotation_y) }
    }

    /// `OrbitalCamera.Load(saveData)` 0x6F9390 (the world's `onCameraLoad`): snap to the saved zoom, tilt and
    /// turn (`byte / 255`) with the pan centred.
    pub fn load(&mut self, cam: SaveCam) {
        self.snap = true;
        let f = |b: u8| mathf::clamp01(b as f32 * 0.003921569);
        let z = f(cam.z);
        self.zoom_target = z;
        self.zoom_current = z;
        self.rotation_x = f(cam.x);
        self.rotation_y = f(cam.y);
        self.pan_hex_target_x = 0.5;
        self.pan_hex_current_x = 0.5;
        self.pan_hex_target_y = 0.5;
        self.pan_hex_current_y = 0.5;
        self.pan_height_target = 0.5;
        self.pan_height_current = 0.5;
    }
}
