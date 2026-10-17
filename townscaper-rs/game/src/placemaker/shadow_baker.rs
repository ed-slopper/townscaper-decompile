//! `Placemaker.ShadowBaker` and `Placemaker.ShadowBakerCamera` (row E9a): the sun. The baker keeps the sun angle
//! (`current` easing towards `target`, both in 0..1: x the turn, y the height), points an orthographic camera
//! along it, fits that camera around the town (`MaybeUpdateFraming`) and, when anything changed, lets the camera
//! render the town's depth once into `tex0` with the "Shadow" replacement shader. The rest of the game's shaders
//! read the shadow through the globals set here (`_WorldToShadow`, `_SunDir`, ...).
//!
//! The CPU side is plain Rust; the engine side (the camera's render, `Gradient.Evaluate`, transforms) is the
//! host's ([`SunHost`]). The bake itself is described by [`ShadowBakePass`] for Bevy's renderer: checking it
//! needs the shaders (row C3). Checked against the running game by `tests/placemaker_shadow_baker.rs`
//! (`TS_HARNESS=1`).

use crate::os::{extra_math, hex};
use crate::placemaker::grid_generator;
use crate::unity::geometry::{Bounds, Color, Color32};
use crate::unity::mathematics::Int3x2;
use crate::unity::mathf;
use crate::unity::quaternion::{Matrix4x4, Quaternion};
use crate::unity::vector::{v3, v4, Vector2, Vector3, Vector4};

/// The shader globals the baker sets (`.cctor` 0x975EC0 looks up the first eight with `Shader.PropertyToID`, in
/// this order: `worldToShadowId` .. `lampId`; `OnStart` sets the textures by name).
pub mod ids {
    pub const WORLD_TO_SHADOW: &str = "_WorldToShadow";
    pub const SHADOW_TO_WORLD: &str = "_ShadowToWorld";
    pub const SHADOW_RANGE: &str = "_ShadowRange";
    pub const SUN_DIR: &str = "_SunDir";
    pub const SUN_COLOR: &str = "_SunColor";
    pub const SHADOW_COLOR: &str = "_ShadowColor";
    pub const BG_COLOR: &str = "_BgColor";
    pub const LAMP: &str = "_Lamp";
    pub const LAMP_TEX: &str = "_LampTex";
    pub const SHADOW_TEX: &str = "_ShadowTex";
    pub const SHADOW_NOISE_TEX: &str = "_ShadowNoiseTex";
    pub const SHADOW_TEX_TEXEL_SIZE: &str = "_ShadowTex_TexelSize";
}

/// `ShadowBaker.windowSizeX` / `windowSizeY` (the lamp texture is that size).
pub const WINDOW_SIZE: i32 = 32;

/// The bake for Bevy's renderer: what `OnStart` 0x975860 sets up on the baker's camera and what
/// `LateUpdate` / `ShadowBakerCamera.OnPostRender` do with it (enable for one frame, render, disable).
/// STANDIN: a description, not a pass; checking it needs the "Shadow" shader (row C3).
#[derive(Clone, Debug, PartialEq)]
pub struct ShadowBakePass {
    /// `tex0 = new RenderTexture(resolution, resolution, 24)`, point filtered (`filterMode = 0`).
    pub resolution: i32,
    pub depth_bits: i32,
    /// `cam.SetReplacementShader(shader, "Shadow")`: every object drawn with the shadow shader's subshader whose
    /// "Shadow" tag matches its own.
    pub replacement_tag: &'static str,
    /// `cam.opaqueSortMode = 2` (`OpaqueSortMode.NoDistanceSort`).
    pub opaque_sort_mode: i32,
}

/// `ShadowBaker.resolution` (`.ctor` 0x976000: 1024).
pub const RESOLUTION: i32 = 1024;

impl Default for ShadowBakePass {
    fn default() -> Self {
        ShadowBakePass { resolution: RESOLUTION, depth_bits: 24, replacement_tag: "Shadow", opaque_sort_mode: 2 }
    }
}

/// `SaveData.sun`: the saved sun angle, a byte each (`h` the height, `r` the turn).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct SaveSun {
    pub h: u8,
    pub r: u8,
}

/// What the baker reaches in the engine.
pub trait SunHost {
    /// `sunColor.Evaluate(t)`, `shadowColor.Evaluate(t)`, `bgColor.Evaluate(t)` (`Gradient.Evaluate`, native).
    fn sun_color(&mut self, t: f32) -> Color;
    fn shadow_color(&mut self, t: f32) -> Color;
    fn bg_color(&mut self, t: f32) -> Color;
    /// `cam.transform.rotation = ..`.
    fn set_camera_rotation(&mut self, q: Quaternion);
    /// `cam.transform.rotation`: within a frame the engine gives back exactly what was set (a later read can
    /// find it renormalised).
    fn camera_rotation(&mut self) -> Quaternion;
    /// `Shader.SetGlobalVector`.
    fn set_global_vector(&mut self, name: &str, v: Vector4);
    /// `Shader.SetGlobalMatrix`.
    fn set_global_matrix(&mut self, name: &str, m: Matrix4x4);
    /// `WorldMaster.SetDirty` (the angle changed: the town is saved).
    fn set_world_dirty(&mut self) {}
    /// `onNewSunAngle` (`Load`; the `.ctor` 0x976000 sets it to an empty lambda).
    fn on_new_sun_angle(&mut self) {}
}

/// `Color` to `Vector4` (`Color.op_Implicit`).
fn color_v4(c: Color) -> Vector4 {
    v4(c.r, c.g, c.b, c.a)
}

/// One corner column of the town as the framing reads it: `graph.corners.GetChild(i)` (`position`, its child
/// count and its last child's `position`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CornerColumn {
    pub position: Vector3,
    pub child_count: i32,
    pub last_child_position: Vector3,
}

/// What `MaybeUpdateFraming` sets.
#[derive(Clone, Debug, PartialEq)]
pub struct Framing {
    pub camera_position: Vector3,
    pub camera_rotation: Quaternion,
    pub orthographic_size: f32,
    pub near_clip_plane: f32,
    pub far_clip_plane: f32,
    pub world_to_shadow: Matrix4x4,
    pub shadow_to_world: Matrix4x4,
    pub shadow_range: Vector4,
    pub sun_dir: Vector4,
    pub plane_position: Vector3,
    pub plane_rotation: Quaternion,
    pub plane_scale: Vector3,
}

/// `Placemaker.ShadowBaker`'s state.
#[derive(Clone, Debug, PartialEq)]
pub struct ShadowBaker {
    pub count: i32,
    pub current: Vector2,
    pub target: Vector2,
    pub framing_dirty: bool,
    pub angle_dirty: bool,
    pub snap: bool,
    pub should_bake: bool,
    pub lamps: f32,
    /// `Behaviour.enabled` of the baker.
    pub enabled: bool,
    /// `cam.enabled`: the camera renders the bake this frame.
    pub camera_enabled: bool,
}

impl Default for ShadowBaker {
    /// `.ctor` 0x976000.
    fn default() -> Self {
        ShadowBaker {
            count: 0,
            current: Vector2::ZERO,
            target: Vector2::ZERO,
            framing_dirty: false,
            angle_dirty: false,
            snap: false,
            should_bake: false,
            lamps: 0.0,
            enabled: true,
            camera_enabled: true,
        }
    }
}

/// `ShadowBaker.OnStart`'s `lampTex` (32 x 32, clamped): row `i` (y) has a per-row random byte `b` (an xorshift
/// from 0x1E36FFC, `b = (state * 255) >> 32` taken before each step) and a step at `c = Clamp(i, 1, 28)`: texels
/// left of `c` are `(0, 0, b, 1)`, at `c` `(170, 20, b, 1)`, at `c + 1` `(20, 100, b, 1)`, past it `(255, 255, b,
/// 1)`. Row-major from the bottom row, as `SetPixels32` takes it.
pub fn lamp_texture() -> Vec<Color32> {
    let mut out = Vec::with_capacity(1024);
    let mut s: u32 = 0x1E36FFC;
    for i in 0..WINDOW_SIZE {
        let c = mathf::clamp_i32(i, 1, 28);
        let b = ((s as u64 * 0xFF) >> 32) as u8;
        s ^= s << 13;
        s ^= s >> 17;
        s ^= s << 5;
        for j in -c..WINDOW_SIZE - c {
            let (r, g) = match j {
                j if j < 0 => (0, 0),
                0 => (0xAA, 0x14),
                1 => (0x14, 0x64),
                _ => (0xFF, 0xFF),
            };
            out.push(Color32 { r, g, b, a: 1 });
        }
    }
    out
}

/// `OnStart`'s `_ShadowTex_TexelSize`: `(texelSize.x, texelSize.y, width, height)` of `tex0`.
pub fn shadow_tex_texel_size(resolution: i32) -> Vector4 {
    let r = resolution as f32;
    v4(1.0 / r, 1.0 / r, r, r)
}

impl ShadowBaker {
    /// `Placemaker.ShadowBaker.GetQuaternion` 0x974520: `Euler(Lerp(2, 90, a.y), a.x * 360, 0)`.
    pub fn get_quaternion(a: Vector2) -> Quaternion {
        Quaternion::euler(mathf::lerp(2.0, 90.0, a.y), a.x * 360.0, 0.0)
    }

    /// `Placemaker.ShadowBaker.BakeShadow` 0x974500.
    pub fn bake_shadow(&mut self) {
        self.enabled = true;
        self.should_bake = true;
    }

    /// `Placemaker.ShadowBaker.SetFramingDirty` 0x975CD0.
    pub fn set_framing_dirty(&mut self) {
        self.framing_dirty = true;
    }

    /// `Placemaker.ShadowBaker.OnBuilt` 0x975830: jump to the target, everything dirty.
    pub fn on_built(&mut self) {
        self.angle_dirty = true;
        self.snap = true;
        self.current = self.target;
        self.framing_dirty = true;
        self.should_bake = true;
    }

    fn changed(&mut self, host: &mut dyn SunHost) {
        self.angle_dirty = true;
        host.set_world_dirty();
        self.enabled = true;
        self.should_bake = true;
    }

    /// `Placemaker.ShadowBaker.RotateAngle` 0x975BB0: `target += delta`, y clamped to 0..1, x wrapped with
    /// `fmod((double)(x + 10f), 1.0)`.
    pub fn rotate_angle(&mut self, delta: Vector2, host: &mut dyn SunHost) {
        self.target.x += delta.x;
        self.target.y += delta.y;
        self.target.y = mathf::clamp01(self.target.y);
        self.target.x = (((self.target.x + 10.0) as f64) % 1.0) as f32;
        self.changed(host);
    }

    /// `Placemaker.ShadowBaker.SetAngle` 0x975C90.
    pub fn set_angle(&mut self, target: Vector2, host: &mut dyn SunHost) {
        self.target = target;
        self.changed(host);
    }

    /// `Placemaker.ShadowBaker.SetX` 0x975CE0.
    pub fn set_x(&mut self, x: f32, host: &mut dyn SunHost) {
        self.target.x = x;
        self.changed(host);
    }

    /// `Placemaker.ShadowBaker.SetY` 0x975D20.
    pub fn set_y(&mut self, y: f32, host: &mut dyn SunHost) {
        self.target.y = y;
        self.changed(host);
    }

    /// `Placemaker.ShadowBaker.Load` 0x974860: the angle from the save (`byte * 0.003921569f`, current and
    /// target), lamps off, everything dirty, baker and camera on, `onNewSunAngle`.
    pub fn load(&mut self, sun: SaveSun, host: &mut dyn SunHost) {
        let x = sun.r as f32 * 0.003921569;
        self.current.x = x;
        self.target.x = x;
        let y = sun.h as f32 * 0.003921569;
        self.current.y = y;
        self.target.y = y;
        self.lamps = 0.0;
        self.framing_dirty = true;
        self.angle_dirty = true;
        self.enabled = true;
        self.camera_enabled = true;
        host.on_new_sun_angle();
    }

    /// `Placemaker.ShadowBaker.Save` 0x975C50: `(byte)(int)(target * 255)`.
    pub fn save(&self) -> SaveSun {
        SaveSun { h: (self.target.y * 255.0) as i32 as u8, r: (self.target.x * 255.0) as i32 as u8 }
    }

    /// `Placemaker.ShadowBaker.MaybeUpdateAngle` 0x974900: when the angle changed, the framing is dirty, the
    /// camera turns to `current` ([`get_quaternion`](Self::get_quaternion), inlined), the three colours are
    /// evaluated at `current.y`, and a bake is due.
    pub fn maybe_update_angle(&mut self, host: &mut dyn SunHost) {
        if !self.angle_dirty {
            return;
        }
        self.framing_dirty = true;
        self.angle_dirty = false;
        host.set_camera_rotation(Self::get_quaternion(self.current));
        let t = self.current.y;
        let c = host.sun_color(t);
        host.set_global_vector(ids::SUN_COLOR, color_v4(c));
        let c = host.shadow_color(t);
        host.set_global_vector(ids::SHADOW_COLOR, color_v4(c));
        let c = host.bg_color(t);
        host.set_global_vector(ids::BG_COLOR, color_v4(c));
        self.should_bake = true;
    }

    /// The lamps part of `LateUpdate` and `Placemaker.ShadowBaker.UpdateLamps` 0x975D60: `lamps` moves towards 1
    /// at night (`target.y == 0`), else -1, at `0.7` per second; when it moved, `_Lamp = (lamps, night, 1 -
    /// night, 0)`.
    pub fn update_lamps(&mut self, delta_time: f32, host: &mut dyn SunHost) {
        let night = if self.target.y == 0.0 { 1.0 } else { 0.0 };
        let want = mathf::lerp(-1.0, 1.0, night);
        if self.lamps != want {
            self.lamps = mathf::move_towards(self.lamps, want, delta_time * 0.7);
            host.set_global_vector(ids::LAMP, v4(self.lamps, night, 1.0 - night, 0.0));
        }
    }

    /// `Placemaker.ShadowBaker.LateUpdate` 0x9745A0: `current` eases towards `target` (`WorldMaster.expTFast`,
    /// or at once after `OnBuilt`; the turn by `LerpAngle` in degrees; equal once closer than 1e-5 squared),
    /// then the angle, the framing (`frame`: [`maybe_update_framing`](Self::maybe_update_framing) with the town),
    /// the lamps; a due bake turns the camera on for a frame.
    pub fn late_update(&mut self, exp_t_fast: f32, delta_time: f32, host: &mut dyn SunHost, frame: &mut dyn FnMut(&mut ShadowBaker, &mut dyn SunHost)) {
        let dx = self.target.x - self.current.x;
        let dy = self.target.y - self.current.y;
        // `Vector2 !=`: not within 1e-5 (squared 9.9999994e-11)
        if 9.9999994e-11 <= dy * dy + dx * dx {
            if (Vector2 { x: dx, y: dy }).sqr_magnitude() < 1e-5 {
                self.current = self.target;
            }
            let t = if self.snap { 0.0 } else { exp_t_fast };
            // (`LerpAngle` returns on the x87 stack; the `* (1 / 360f)` is done there, rounded to float once)
            self.current.x = (mathf::lerp_angle_x87(self.target.x * 360.0, self.current.x * 360.0, t) * 0.0027777778f32 as f64) as f32;
            self.current.y = mathf::lerp(self.target.y, self.current.y, t);
            self.angle_dirty = true;
        }
        self.snap = false;
        self.maybe_update_angle(host);
        frame(self, host);
        self.update_lamps(delta_time, host);
        if self.should_bake {
            self.camera_enabled = true;
            self.should_bake = false;
        }
    }

    /// `Placemaker.ShadowBaker.MaybeUpdateFraming` 0x974B30, when the framing is dirty: the camera (at its
    /// rotation, [`SunHost::camera_rotation`]) is fitted around the town in its own frame — a box from the centre of `graph.bounds` (on the
    /// ground) grown by each corner column's bottom and top, the last column first, then 3 bigger each way — and
    /// the shadow matrices, range, sun direction and the ground shadow plane follow. `world_to_local` is the
    /// camera transform's `worldToLocalMatrix` once moved (the engine's). Returns `None` when not dirty, and
    /// `count = 0` / `Some(None)` when the town has no corners (only a bake is due).
    pub fn maybe_update_framing(
        &mut self,
        graph_bounds: Int3x2,
        corners: &[CornerColumn],
        world_to_local: &mut dyn FnMut(Vector3, Quaternion) -> Matrix4x4,
        host: &mut dyn SunHost,
    ) -> Option<Option<Framing>> {
        if !self.framing_dirty {
            return None;
        }
        self.framing_dirty = false;
        let rot = host.camera_rotation();
        let sun = Quaternion::rotate(rot, Vector3::FORWARD);
        let inv = Quaternion::inverse(rot);
        self.count = 0;
        self.should_bake = true;
        if corners.is_empty() {
            return Some(None);
        }
        let p = hex::plane_to_world(grid_generator::hex_to_plane_i3(extra_math::center(graph_bounds)), 0.0);
        let mut b = Bounds::new(Quaternion::rotate(inv, v3(p.x, p.y, p.z)), Vector3::ZERO);
        for c in corners.iter().rev() {
            if c.child_count > 0 {
                b.encapsulate(Quaternion::rotate(inv, c.position));
                b.encapsulate(Quaternion::rotate(inv, c.last_child_position));
            }
            self.count = self.count.wrapping_add(c.child_count);
        }
        b.extents = Vector3::ONE.mul(3.0).add(b.extents);
        let camera_position = Quaternion::rotate(rot, b.center);
        let size = mathf::max(b.extents.y, b.extents.x);
        let near = -b.extents.z;
        let far = b.extents.z;
        let bs = b.size();
        let inv_z = 1.0 / bs.z;
        let s = Matrix4x4::scale(v3(0.5 / size, 0.5 / size, inv_z));
        let t = Matrix4x4::translate(v3(size, size, b.extents.z));
        let m = Matrix4x4::mul(&s, &t);
        let world_to_shadow = Matrix4x4::mul(&m, &world_to_local(camera_position, rot));
        let shadow_to_world = world_to_shadow.inverse();
        host.set_global_matrix(ids::WORLD_TO_SHADOW, world_to_shadow);
        host.set_global_matrix(ids::SHADOW_TO_WORLD, shadow_to_world);
        let shadow_range = v4(bs.z, inv_z, 0.0, 0.0);
        host.set_global_vector(ids::SHADOW_RANGE, shadow_range);
        let sun_dir = v4(sun.x, sun.y, sun.z, 0.0);
        host.set_global_vector(ids::SUN_DIR, sun_dir);
        // where the sun ray through the camera meets the ground
        let k = 1.0 / sun.y;
        let cp = camera_position;
        let plane_position = v3(cp.x - sun.x * k * cp.y, cp.y - sun.y * k * cp.y, cp.z - sun.z * k * cp.y);
        // the camera was turned to `rot` again before this; `transform.up` reads the engine's copy back
        host.set_camera_rotation(rot);
        let up = Quaternion::rotate(host.camera_rotation(), Vector3::UP);
        let flat = extra_math::set_y_f3(crate::unity::mathematics::f3(up.x, up.y, up.z), 0.0);
        let plane_rotation = Quaternion::mul(Quaternion::look_rotation(v3(flat.x, flat.y, flat.z)), Quaternion::euler(0.0, 180.0, 0.0));
        let plane_scale = v3(bs.x * 0.1, 1.0, bs.y * k * 0.1);
        Some(Some(Framing {
            camera_position,
            camera_rotation: rot,
            orthographic_size: size,
            near_clip_plane: near,
            far_clip_plane: far,
            world_to_shadow,
            shadow_to_world,
            shadow_range,
            sun_dir,
            plane_position,
            plane_rotation,
            plane_scale,
        }))
    }
}

/// `Placemaker.ShadowBakerCamera.OnPostRender` 0x9744D0: the camera turns itself off after the bake.
pub fn shadow_baker_camera_on_post_render(baker: &mut ShadowBaker) {
    baker.camera_enabled = false;
}
