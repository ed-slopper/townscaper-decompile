//! `Placemaker.ClickEffect` (row G1d): what a click looks and sounds like. Particles at the voxel (a splash on
//! the water at height 0, dust above), the voxel bobbing, the birds nearby taking off, and two sounds whose
//! pitch walks up for quick adds and down for quick removes. The clips come from Unity's global `Random`, drawn
//! in the game's order. The particles, audio sources, bob effect and flock are the host's; this module says
//! what to do with them, as [`FeelEvent`]s in the original's order.

use crate::os::hex;
use crate::placemaker::graphs::{Graph, VoxelId, VoxelType};
use crate::placemaker::hover_data::HoverData;
use crate::unity::mathematics::f2;
use crate::unity::mathf;
use crate::unity::random::Rand;
use crate::unity::vector::{v3, Vector3};

/// The effect's particle systems.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Particles {
    Particle,
    VoxelSplash,
    Dust,
    ColorPickEffect,
}

/// The effect's clip lists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Clips {
    Add,
    Remove,
    Add1,
    Remove1,
    Splash,
    ColorPick,
}

/// One thing the effect does, in order.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FeelEvent {
    /// The particle system's start colour gets `r = u` (the voxel type's column in the palette texture).
    ParticleColor { particles: Particles, u: f32 },
    /// `particle`'s shape module: scale, rotation and position.
    ParticleShape { scale: Vector3, rotation: Vector3, position: Vector3 },
    /// The system moved to `position` and played.
    Play { particles: Particles, position: Vector3 },
    /// An audio source from the pool plays `clips[index]` at `position`.
    Sound { clips: Clips, index: i32, volume: f32, pitch: f32, position: Vector3 },
    /// `voxelBobEffect.ClickBobEffect(position, up)`.
    Bob { position: Vector3, up: bool },
}

/// What the effect reads from the world.
pub struct FeelContext<'a> {
    pub random: &'a mut Rand,
    /// `master.flock.UprootRadius(position, radius, force)` (row G5), called where the original calls it: the
    /// birds draw from the same `Random`, between the effect's draws.
    pub uproot: &'a mut dyn FnMut(&mut Rand, Vector3, f32, f32),
    /// `Time.time`.
    pub time: f32,
    /// `TimeManager.slowmoScale`.
    pub slowmo_scale: f32,
    /// `master.texturePngMaster.paletteTex.srcTex.width`.
    pub palette_width: i32,
    /// The clip lists' lengths, by [`Clips`] order.
    pub clip_counts: [i32; 6],
}

/// The effect's state.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClickEffect {
    pub last_voxel_type: VoxelType,
    pub last_color_pick_type: VoxelType,
    pub last_click: f32,
    pub last_pitch: f32,
}

impl Default for ClickEffect {
    /// `Placemaker.ClickEffect..ctor` 0x201840: every field 0.
    fn default() -> ClickEffect {
        ClickEffect { last_voxel_type: VoxelType::from_i8(0), last_color_pick_type: VoxelType::from_i8(0), last_click: 0.0, last_pitch: 0.0 }
    }
}

fn pick(cx: &mut FeelContext, clips: Clips) -> i32 {
    let n = cx.clip_counts[clips as usize];
    let i = cx.random.range_int(0, n);
    assert!((i as u32) < n as u32, "ClickEffect: ArgumentOutOfRangeException");
    i
}

impl ClickEffect {
    /// `ClickEffect.SetColor(voxelType)` 0x201610: on a new type, the particle and dust systems' start colour
    /// red becomes `type / paletteWidth`.
    fn set_color(&mut self, cx: &FeelContext, t: VoxelType, out: &mut Vec<FeelEvent>) {
        if t == self.last_voxel_type {
            return;
        }
        self.last_voxel_type = t;
        let u = (t as i8 as i32) as f32 / cx.palette_width as f32;
        out.push(FeelEvent::ParticleColor { particles: Particles::Particle, u });
        out.push(FeelEvent::ParticleColor { particles: Particles::Dust, u });
    }

    /// `ClickEffect.Click(add, planePos, height, voxelType)` 0x2008A0, at `PlaneToWorld(planePos, height +
    /// 0.5)`: the particle burst (shape scale ½, coloured by the type); at height 0 the water splash (on the
    /// water, with a splash sound: volume 1, pitch `Range(0.5, 0.8)`), else dust; the bob; the add / remove
    /// sound (volume 0.4) whose pitch walks: 100 after two quiet seconds, else ±0.2 from the last, played as
    /// `Lerp(0.4, 2, |fmod(p - 0.5, 2) - 1|)`; the birds within 1 take off; then the second add / remove sound
    /// (volume 0.25, pitch `Range(1, 1.2)` times the first's).
    pub fn click(&mut self, cx: &mut FeelContext, add: bool, plane_pos: crate::unity::mathematics::Float2, height: i32, voxel_type: VoxelType) -> Vec<FeelEvent> {
        let mut out = vec![];
        let w = hex::plane_to_world(plane_pos, height as f32 + 0.5);
        let pos = v3(w.x, w.y, w.z);
        let one = Vector3 { x: 1.0, y: 1.0, z: 1.0 };
        out.push(FeelEvent::ParticleShape { scale: v3(one.x * 0.5, one.y * 0.5, one.z * 0.5), rotation: Vector3::ZERO, position: Vector3::ZERO });
        self.set_color(cx, voxel_type, &mut out);
        out.push(FeelEvent::Play { particles: Particles::Particle, position: pos });
        if height == 0 {
            let water = v3(pos.x, 0.0, pos.z);
            out.push(FeelEvent::Play { particles: Particles::VoxelSplash, position: water });
            let index = pick(cx, Clips::Splash);
            let r = cx.random.range(0.5, 0.8);
            out.push(FeelEvent::Sound { clips: Clips::Splash, index, volume: 1.0, pitch: cx.slowmo_scale * r, position: water });
        } else {
            out.push(FeelEvent::Play { particles: Particles::Dust, position: pos });
        }
        out.push(FeelEvent::Bob { position: pos, up: add });
        let index = pick(cx, if add { Clips::Add } else { Clips::Remove });
        let p = if 2.0 < cx.time - self.last_click {
            100.0
        } else if add {
            self.last_pitch + 0.2
        } else {
            self.last_pitch + -0.2
        };
        self.last_pitch = p;
        let folded = (((p - 0.5) as f64) % 2.0) as f32;
        let mul = mathf::lerp(0.4, 2.0, (folded - 1.0).abs());
        self.last_click = cx.time;
        (cx.uproot)(cx.random, pos, 1.0, 1.0);
        let first = cx.slowmo_scale * mul;
        out.push(FeelEvent::Sound { clips: if add { Clips::Add } else { Clips::Remove }, index, volume: 0.4, pitch: first, position: pos });
        let index = pick(cx, if add { Clips::Add1 } else { Clips::Remove1 });
        let r = cx.random.range(1.0, 1.2);
        out.push(FeelEvent::Sound { clips: if add { Clips::Add1 } else { Clips::Remove1 }, index, volume: 0.25, pitch: r * first * cx.slowmo_scale, position: pos });
        out
    }

    /// `ClickEffect.Click(hover, add, voxel)` 0x201070: at the hover's destination (adding) or source
    /// (removing), with the voxel's type; nothing without a voxel.
    pub fn click_hover(&mut self, cx: &mut FeelContext, graph: &Graph, hover: &HoverData, add: bool, voxel: Option<VoxelId>) -> Vec<FeelEvent> {
        let (pp, h) = if add { (hover.dst_vert.plane_pos, hover.dst_height) } else { (hover.src_vert.plane_pos, hover.src_height) };
        let p = hex::get_hex2_f(crate::unity::mathematics::f3(pp.x, pp.y, 0.0));
        match voxel {
            Some(v) => self.click(cx, add, f2(p.x, p.y), h, graph.voxel(v).voxel_type),
            None => vec![],
        }
    }

    /// `ClickEffect.ColorPick(hover)` 0x201100: at the hovered voxel: the birds within 1 (force 0.5) take off,
    /// an upward bob, the pick particles (coloured on a new type) and the pick sound (volume 1, pitch
    /// `Range(0.8, 1.2)`).
    pub fn color_pick(&mut self, cx: &mut FeelContext, graph: &Graph, hover: &HoverData) -> Vec<FeelEvent> {
        let mut out = vec![];
        let Some(v) = hover.voxel else { return out };
        let pos = graph.voxel_position(v);
        self.last_click = cx.time;
        (cx.uproot)(cx.random, pos, 1.0, 0.5);
        out.push(FeelEvent::Bob { position: pos, up: true });
        let t = graph.voxel(v).voxel_type;
        if t != self.last_color_pick_type {
            self.last_color_pick_type = t;
            let u = (t as i8 as i32) as f32 / cx.palette_width as f32;
            out.push(FeelEvent::ParticleColor { particles: Particles::ColorPickEffect, u });
        }
        out.push(FeelEvent::Play { particles: Particles::ColorPickEffect, position: pos });
        let index = pick(cx, Clips::ColorPick);
        let r = cx.random.range(0.8, 1.2);
        out.push(FeelEvent::Sound { clips: Clips::ColorPick, index, volume: 1.0, pitch: cx.slowmo_scale * r, position: pos });
        out
    }
}
