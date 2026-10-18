//! `Placemaker.Audio.ManySoundsManager` and `ManySound` (row G6a): the town's sound effects.
//!
//! Every system asks for a sound with `PlaySound(type, position, pitch, volume)`; the manager does not play each
//! request. Per sound type (a `ManySound` child named after its `SoundType`) it sums the requests of a frame,
//! each weighted by how central it is on screen, and once a frame (`OnUpdate`) plays one blended sound per type
//! at the weighted position, pan and pitch, quieter when the town hides it from the camera and when the same
//! sound keeps being asked for. Sound effect sources come from an [`AudioSourcePool`].
//!
//! The camera, the graph, the physics scene, the clock and the audio sources are the host's ([`SoundHost`],
//! [`AudioSources`]).

use crate::placemaker::audio_source_pool::AudioSourcePool;
use crate::unity::geometry::Ray;
use crate::unity::mathf;
use crate::unity::random::Rand;
use crate::unity::vector::{v3, Vector3};

/// `ManySoundsManager.SoundType`.
pub mod sound_type {
    pub const NONE: i32 = 0;
    pub const PROP: i32 = 1;
    pub const PLANT: i32 = 2;
    pub const TILE: i32 = 3;
    pub const PROPELLER: i32 = 4;
    pub const ITALIAN_WIRE: i32 = 5;
    pub const LIGHTHOUSE: i32 = 6;
    pub const BIRD_APPEAR: i32 = 7;
    pub const BIRD_TAKEOFF: i32 = 8;
    pub const BIRD_TAKEOFF1: i32 = 9;
    pub const WINDOW: i32 = 10;
    pub const DEBRIS: i32 = 11;
    pub const DEBRIS_SPLASH: i32 = 12;
    pub const SPIRE: i32 = 13;
    /// The names, by value (`Enum.Parse` / `ToString`).
    pub const NAMES: [&str; 14] = [
        "None", "Prop", "Plant", "Tile", "Propeller", "ItalianWire", "Lighthouse", "BirdAppear", "BirdTakeoff", "BirdTakeoff1", "Window", "Debris",
        "DebrisSplash", "Spire",
    ];

    /// `Enum.Parse(typeof(SoundType), name)` for the names the scene uses (exact names or a number).
    pub fn parse(name: &str) -> Option<i32> {
        NAMES.iter().position(|&n| n == name).map(|i| i as i32).or_else(|| name.trim().parse().ok())
    }

    /// `SoundType.ToString()`: the name, or the number when it has none.
    pub fn to_string(t: i32) -> String {
        usize::try_from(t).ok().and_then(|i| NAMES.get(i)).map_or_else(|| t.to_string(), |n| n.to_string())
    }
}

/// An audio source of the host's.
pub type SourceId = usize;
/// An audio clip of the host's.
pub type ClipId = usize;

/// The `AudioSource` members the game sets.
pub trait AudioSources {
    fn set_volume(&mut self, s: SourceId, volume: f32);
    /// `AudioSource.pitch` (`SetPitch`): the engine keeps the old pitch when given one that is not finite (the
    /// blended pitch can overflow, see [`ManySoundsManager::on_update`]).
    fn set_pitch(&mut self, s: SourceId, pitch: f32);
    fn set_pan_stereo(&mut self, s: SourceId, pan: f32);
    /// `audioSource.transform.position`.
    fn set_position(&mut self, s: SourceId, p: Vector3);
    fn set_clip(&mut self, s: SourceId, clip: Option<ClipId>);
    fn play(&mut self, s: SourceId);
    fn stop(&mut self, s: SourceId);
}

/// What the manager reads outside itself.
pub trait SoundHost {
    /// `Time.time`, `Time.deltaTime`.
    fn time(&self) -> f32;
    fn delta_time(&self) -> f32;
    /// Whether `Camera.main` exists, and its `WorldToViewportPoint` / `ViewportPointToRay`.
    fn has_main_camera(&self) -> bool;
    fn world_to_viewport_point(&self, p: Vector3) -> Vector3;
    fn viewport_point_to_ray(&self, viewport: Vector3) -> Ray;
    /// `Physics.Raycast(ray, maxDistance, layerMask)`.
    fn raycast(&self, ray: Ray, max_distance: f32, layer_mask: i32) -> bool;
    /// `master.graph.SampleNormalCoverage(worldPos)`.
    fn sample_normal_coverage(&self, p: Vector3) -> (Vector3, f32);
    /// `master.isDimmed`, `master.clickEffect.lastClick`, `TimeManager.slowmoScale`.
    fn is_dimmed(&self) -> bool;
    fn last_click(&self) -> f32;
    fn slowmo_scale(&self) -> f32;
    /// `Debug.Log`.
    fn log(&mut self, message: &str);
}

/// `Placemaker.Audio.ManySound`: one sound type's clips, settings and this frame's sums.
#[derive(Clone, Debug, PartialEq)]
pub struct ManySound {
    /// `audioSourcePool`: an index into the pools the host passes.
    pub audio_source_pool: usize,
    pub clips: Vec<ClipId>,
    pub time_spacing: f32,
    pub click_sound_spacing: f32,
    pub attenuator: f32,
    pub use_prop_pitch: f32,
    /// This frame's weighted sums (`pos`, `denom`, `pan`, `pitch`), the volume and the repeat damping.
    pub pos: Vector3,
    pub denom: f32,
    pub volume: f32,
    pub pan: f32,
    pub pitch: f32,
    pub last_time_played: f32,
    pub attenuation: f32,
}

impl Default for ManySound {
    /// `ManySound..ctor` 0x329D50: spaced 0.1 s from itself and from clicks, damped × 0.9 a playing, the full
    /// Perlin pitch; no pool or clips; the damping at 0 (it recovers towards 1 from the first frame).
    fn default() -> ManySound {
        ManySound::new(0, vec![], 0.1, 0.1, 0.9, 1.0)
    }
}

impl ManySound {
    /// A sound with the scene's settings (the serialized fields), its sums and damping at the constructor's 0.
    pub fn new(audio_source_pool: usize, clips: Vec<ClipId>, time_spacing: f32, click_sound_spacing: f32, attenuator: f32, use_prop_pitch: f32) -> ManySound {
        ManySound {
            audio_source_pool,
            clips,
            time_spacing,
            click_sound_spacing,
            attenuator,
            use_prop_pitch,
            pos: Vector3::ZERO,
            denom: 0.0,
            volume: 0.0,
            pan: 0.0,
            pitch: 0.0,
            last_time_played: 0.0,
            attenuation: 0.0,
        }
    }
}

/// `Placemaker.LayerMaster.voxelMask`: what hides a sound.
const VOXEL_MASK: i32 = crate::placemaker::timing::layer_master::VOXEL_MASK;

/// The component's state.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct ManySoundsManager {
    /// `soundDict`, in the order it was filled (a `Dictionary` without removals enumerates so).
    pub sounds: Vec<(i32, ManySound)>,
    pub counter: i32,
}

impl ManySoundsManager {
    /// `ManySoundsManager.OnEnable` 0x329D90: the sounds are the `ManySound`s below it (`EnumerateHirearchy`,
    /// inactive ones too), keyed by their objects' names as `SoundType`s.
    pub fn on_enable(&mut self, children: Vec<(String, ManySound)>) {
        self.sounds.clear();
        for (name, s) in children {
            let t = sound_type::parse(&name).expect("ArgumentException: not a SoundType");
            if self.sounds.iter().any(|(k, _)| *k == t) {
                panic!("ArgumentException: an item with the same key has already been added");
            }
            self.sounds.push((t, s));
        }
    }

    /// `ManySoundsManager.PlaySound(soundType, pos, pitch = 0, volume = 1)` 0x32A730: a request, taken into this
    /// frame's sums when it is on screen, the game is not dimmed, the sound is not still spaced from its last
    /// playing or from the last click, and fewer than about 51 requests came this frame. Its weight is `(1 -
    /// 4|x - ½||y - ½|)² × volume` over its viewport point; the pitch adds unweighted.
    pub fn play_sound(&mut self, host: &mut dyn SoundHost, sound: i32, pos: Vector3, pitch: f32, volume: f32) {
        let c = self.counter;
        self.counter += 1;
        if c >= 51 || !host.has_main_camera() || host.is_dimmed() {
            return;
        }
        let vp = host.world_to_viewport_point(pos);
        if !(0.0 <= vp.z && -0.0 <= vp.x && vp.x <= 1.0 && -0.0 <= vp.y && vp.y <= 1.0) {
            return;
        }
        let Some(i) = self.sounds.iter().position(|(k, _)| *k == sound) else {
            host.log(&format!("No sound = {}", sound_type::to_string(sound)));
            return;
        };
        let s = &mut self.sounds[i].1;
        if host.time() < s.last_time_played {
            return;
        }
        if host.time() < host.last_click() + s.click_sound_spacing {
            return;
        }
        self.counter += 4;
        let fy = (vp.y - 0.5).abs();
        let fx = (vp.x - 0.5).abs();
        let w = 1.0 - (fx + fx) * (fy + fy);
        let w = w * w * volume;
        s.denom = w + s.denom;
        s.pan = vp.x * w + s.pan;
        s.pos = v3(pos.x * w + s.pos.x, pos.y * w + s.pos.y, pos.z * w + s.pos.z);
        s.pitch = pitch + s.pitch;
    }

    /// `ManySoundsManager.OnUpdate` 0x32A000: each sound asked for this frame plays once.
    /// - The sums are averaged. The volume is the summed weight (clamped to 1) times the damping, which shrinks by
    ///   `attenuator` for each playing and recovers towards 1 at `1 - e^(-dt/10)` a frame.
    /// - Ground coverage moves the point out along the normal. The pitch gets Perlin noise over the viewport point
    ///   and the clock (× `usePropPitch`) and is exponentiated.
    /// - A voxel between the camera and the point (`Physics.Raycast` along the view ray) makes it quieter (× 0.4)
    ///   and lower (× 0.8).
    /// - It plays on the next pooled source (slow-motion scaling the pitch), a random clip, at the averaged
    ///   position, panned `2 pan - 1`, and is then spaced by `timeSpacing`.
    /// - The pitches add unweighted but are divided by the summed weight, so a faint sound's pitch can overflow to
    ///   infinity; the source then keeps the pool's pitch 1.
    pub fn on_update(&mut self, host: &mut dyn SoundHost, random: &mut Rand, pools: &mut [AudioSourcePool], audio: &mut dyn AudioSources) {
        self.counter = 0;
        let dt = host.delta_time();
        let recover = ((-dt * 0.1) as f64).exp() as f32;
        for (_, s) in self.sounds.iter_mut() {
            if 0.0 < s.denom {
                let inv = 1.0 / s.denom;
                s.pos = v3(s.pos.x * inv, s.pos.y * inv, s.pos.z * inv);
                s.pan *= inv;
                s.pitch *= inv;
                s.volume = mathf::clamp01(s.denom) * s.attenuation;
                s.attenuation *= s.attenuator;
                let (n, c) = host.sample_normal_coverage(s.pos);
                let p = v3(n.x * c + s.pos.x, s.pos.y + n.y * c, s.pos.z + n.z * c);
                let vp = host.world_to_viewport_point(p);
                let pitch = s.pitch;
                let t1 = host.time();
                let t2 = host.time();
                let noise = mathf::perlin_noise(t1 + vp.x, t2 + vp.y);
                s.pitch = (((noise - 0.5) * s.use_prop_pitch + pitch) as f64).exp() as f32;
                let ray = host.viewport_point_to_ray(vp);
                let o = ray.origin;
                let dist = v3(o.x - p.x, o.y - p.y, o.z - p.z).magnitude();
                if host.raycast(ray, dist, VOXEL_MASK) {
                    s.volume *= 0.4;
                    s.pitch *= 0.8;
                }
                let src = pools[s.audio_source_pool].get_audio_source(audio);
                audio.set_volume(src, s.volume);
                audio.set_pitch(src, host.slowmo_scale() * s.pitch);
                audio.set_pan_stereo(src, (s.pan + s.pan) - 1.0);
                audio.set_position(src, s.pos);
                let k = random.range_int(0, s.clips.len() as i32);
                audio.set_clip(src, Some(s.clips[k as usize]));
                audio.play(src);
                s.last_time_played = host.time() + s.time_spacing;
            }
            s.pos = Vector3::ZERO;
            s.denom = 0.0;
            s.pan = 0.0;
            s.volume = 0.0;
            s.pitch = 0.0;
            s.attenuation = mathf::lerp(1.0, s.attenuation, recover);
        }
    }
}
