//! Frame timing and engine statics (row E8a): `Placemaker.UniversalStopwatch` (the per-frame time budget every
//! `Iterate` loop checks through `KeepGoing`), `TimeManager` (the time scale), `FastCoroutines` /
//! `FastCoroutineUpdater` (budgeted work queues and coroutines), `LayerMaster` and `ShaderIds` (the layers and
//! shader properties by name) and `MiscReferences`. Wall-clock time comes from a [`Clock`]. Checked against the
//! running game by `tests/placemaker_timing.rs` (`TS_HARNESS=1`).

/// A restartable stopwatch (`System.Diagnostics.Stopwatch`): milliseconds since the last restart.
pub trait Clock {
    fn restart(&mut self);
    fn elapsed_ms(&self) -> i64;
}

/// A [`Clock`] on the wall clock.
#[derive(Clone, Copy, Debug)]
pub struct WallClock(std::time::Instant);

impl Default for WallClock {
    fn default() -> Self {
        WallClock(std::time::Instant::now())
    }
}

impl Clock for WallClock {
    fn restart(&mut self) {
        self.0 = std::time::Instant::now();
    }
    fn elapsed_ms(&self) -> i64 {
        self.0.elapsed().as_millis() as i64
    }
}

/// `Placemaker.UniversalStopwatch` (`.cctor` 0x3CD0D0: a new stopwatch, `lastCountFrame = 0`).
#[derive(Clone, Debug, Default)]
pub struct UniversalStopwatch<C: Clock> {
    pub stopwatch: C,
    pub last_count_frame: i32,
}

/// The frame budget of `UniversalStopwatch.KeepGoing`, in milliseconds.
pub const FRAME_BUDGET_MS: i64 = 8;

impl<C: Clock> UniversalStopwatch<C> {
    /// `Placemaker.UniversalStopwatch.StartCounting` 0x3CD020: restarts the stopwatch once per frame (`Time.frameCount`);
    /// true when it did.
    pub fn start_counting(&mut self, frame_count: i32) -> bool {
        if frame_count == self.last_count_frame {
            return false;
        }
        self.stopwatch.restart();
        self.last_count_frame = frame_count;
        true
    }

    /// `Placemaker.UniversalStopwatch.KeepGoing` 0x3CCFB0 (also `WorldMaster.KeepGoing` 0x1F9160): less than 8 ms
    /// since the frame's restart.
    pub fn keep_going(&self) -> bool {
        self.stopwatch.elapsed_ms() < FRAME_BUDGET_MS
    }
}

/// `Placemaker.TimeManager` (`.cctor` 0x320BE0: `dimScale = 1`, no slow motion). Every setter applies
/// `Time.timeScale = slowmoScale × dimScale` (`UpdateTime` 0x320B10).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TimeManager {
    dim_scale: f32,
    slowmo: bool,
    /// `Time.timeScale` as last set.
    pub time_scale: f32,
}

impl Default for TimeManager {
    fn default() -> Self {
        TimeManager { dim_scale: 1.0, slowmo: false, time_scale: 1.0 }
    }
}

impl TimeManager {
    /// `get_slowmoScale` 0x320C70: 0.25 in slow motion, else 1.
    pub fn slowmo_scale(&self) -> f32 {
        if self.slowmo {
            0.25
        } else {
            1.0
        }
    }
    /// `get_dimScale` 0x320C20.
    pub fn dim_scale(&self) -> f32 {
        self.dim_scale
    }
    /// `get_slowmo` 0x320CD0.
    pub fn slowmo(&self) -> bool {
        self.slowmo
    }
    fn update_time(&mut self) {
        self.time_scale = self.slowmo_scale() * self.dim_scale;
    }
    /// `set_dimScale` 0x320D20; `SetDimScale` 0x3209D0 and `SetRecordingScale` 0x320A70 set the same field.
    pub fn set_dim_scale(&mut self, value: f32) {
        self.dim_scale = value;
        self.update_time();
    }
    /// `set_slowmo` 0x320D80.
    pub fn set_slowmo(&mut self, value: bool) {
        self.slowmo = value;
        self.update_time();
    }
}

/// `FastCoroutines.Item`: a queued enumerator, or a queued action.
pub enum FastItem {
    /// `Queue(IEnumerator)` 0x6CE8C0: stepped (`MoveNext`) until it ends.
    Enumerator(Box<dyn FnMut() -> bool>),
    /// `Queue(Action)` 0x6CE960: invoked — and never removed (`FastIterate` only removes finished enumerators;
    /// kept).
    Action(Box<dyn FnMut()>),
}

/// `Placemaker.FastCoroutines` (`.cctor` 0x6CEAB0: an empty queue and a new stopwatch).
pub struct FastCoroutines<C: Clock> {
    pub items: Vec<FastItem>,
    pub stopwatch: C,
    pub static_milliseconds: i32,
}

impl<C: Clock + Default> Default for FastCoroutines<C> {
    fn default() -> Self {
        FastCoroutines { items: Vec::new(), stopwatch: C::default(), static_milliseconds: 0 }
    }
}

impl<C: Clock> FastCoroutines<C> {
    /// `Placemaker.FastCoroutines.FastIterate` 0x6CE5B0 (`FastCoroutineUpdater.Update` 0x6CE500 runs it with 16 ms
    /// every frame): the stopwatch restarts; the first item runs (an enumerator steps until it ends, then is
    /// removed, or until `milliseconds` pass; an action is invoked) while the time is not past `milliseconds`.
    /// True when items are left.
    pub fn fast_iterate(&mut self, milliseconds: i32) -> bool {
        self.stopwatch.restart();
        let ms = milliseconds as i64;
        loop {
            if self.items.is_empty() {
                break;
            }
            match &mut self.items[0] {
                FastItem::Enumerator(step) => loop {
                    if !step() {
                        self.items.remove(0);
                        break;
                    }
                    if ms <= self.stopwatch.elapsed_ms() {
                        break;
                    }
                },
                FastItem::Action(act) => act(),
            }
            if self.stopwatch.elapsed_ms() > ms {
                break;
            }
        }
        !self.items.is_empty()
    }

    /// `Placemaker.FastCoroutines.KeepGoing` 0x6CE830: less than `staticMilliseconds` since the coroutine's restart.
    pub fn keep_going(&self) -> bool {
        self.stopwatch.elapsed_ms() < self.static_milliseconds as i64
    }

    /// One frame of `FastCoroutines.FastCoroutine` 0x6CE550 (`<FastCoroutine>d__9.MoveNext` 0x1EFAF0, started by
    /// `StartFastCoroutine` 0x6CEA00, 6 ms by default): the stopwatch restarts with `staticMilliseconds =
    /// milliseconds`; the enumerator steps (`Some(current)` while it runs) until it ends (false: the coroutine
    /// ends), yields false, or `KeepGoing` fails. True: continue next frame.
    pub fn fast_coroutine_frame(&mut self, enumerator: &mut dyn FnMut() -> Option<bool>, milliseconds: i32) -> bool {
        loop {
            self.stopwatch.restart();
            self.static_milliseconds = milliseconds;
            match enumerator() {
                None => return false,
                Some(false) => return true,
                Some(true) => {}
            }
            if !self.keep_going() {
                return true;
            }
        }
    }
}

/// `Placemaker.LayerMaster` (`.cctor` 0x478690): layers by name (`LayerMask.NameToLayer`), masks by
/// `LayerMask.GetMask`. The numbers are the project's layer table (read from the running game).
pub mod layer_master {
    pub const DEFAULT_LAYER: i32 = 0; // "Default"
    pub const VOXEL_LAYER: i32 = 8; // "Voxels"
    pub const GROUND_LAYER: i32 = 9; // "Ground"
    pub const PROPS_LAYER: i32 = 10; // "Props" (masks only)
    pub const GRID_LAYER: i32 = 11; // "Grid"
    pub const LAST_CLICK_LAYER: i32 = 13; // "LastClick" (masks only)
    pub const AO_LAYER: i32 = 14; // "Ao"
    pub const AO_STACK_LAYER: i32 = 16; // "AoStack"
    pub const ONLY_SHADOWS_LAYER: i32 = 17; // "OnlyShadows"
    pub const NO_SHADOWS_LAYER: i32 = 18; // "NoShadows"
    /// "Voxels" and "LastClick".
    pub const CLICK_MASK: i32 = 1 << VOXEL_LAYER | 1 << LAST_CLICK_LAYER;
    pub const PROP_MASK: i32 = 1 << PROPS_LAYER;
    pub const GRID_MASK: i32 = 1 << GRID_LAYER;
    pub const VOXEL_MASK: i32 = 1 << VOXEL_LAYER;
    pub const ONLY_SHADOWS_MASK: i32 = 1 << ONLY_SHADOWS_LAYER;
    pub const NO_SHADOWS_MASK: i32 = 1 << NO_SHADOWS_LAYER;
}

/// `Placemaker.ShaderIds` (`.cctor` 0x973750): the shader properties it looks up (`Shader.PropertyToID`; the ids
/// are the engine's, so the names stand for them here).
pub mod shader_ids {
    pub const COLOR: &str = "_Color";
    pub const RECT: &str = "_Rect";
    pub const AA_LINE_BOTH: &str = "_AaLineBoth";
    pub const DIM: &str = "_Dim";
    pub const SHADING_TEX_LIGHT_STACK: &str = "_ShadingTexLightStack";
}

/// `Placemaker.MiscReferences` (a `ScriptableObject`, `..ctor` 0x3208C0): `italianMeshes`.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct MiscReferences {
    pub italian_meshes: Vec<crate::placemaker::modules::AssetRef>,
}
