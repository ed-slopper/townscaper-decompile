//! `Placemaker.WorldMaster` (row E8b): the frame loop that drives every system of the town. `Iterate` is a
//! state machine (`WorldMaster.State`) walked as far as the frame's time budget allows: loading, then the normal
//! round of grid, graph, wave function collapse, shadows, sandbanker, props, birds and pools, back to the start
//! whenever something changes (`ResetState`). Every system, ported or not, is reached through [`WorldSystems`].
//! Checked against the running game by `tests/placemaker_world_master.rs` (`TS_HARNESS=1`).

/// `WorldMaster.State` (a `byte`).
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum State {
    DoNothing = 0,
    StartLoad = 1,
    PreLoad = 2,
    LoadCleanupQubes0 = 3,
    LoadCleanupProps0 = 4,
    ResetPropAnimator = 5,
    AwaitDim = 6,
    BeginLoadGraph = 7,
    LoadGraph = 8,
    LoadGraphFocus = 9,
    LoadOthers = 10,
    LoadCleanupQubes1 = 11,
    LoadCleanupProps1 = 12,
    LoadGraph1 = 13,
    LoadGridFocus = 14,
    LoadCentralGridBit = 15,
    LoadWaveFunctionCollapse0 = 16,
    LoadApplyModules = 17,
    LoadBakeShadows0 = 18,
    NormalStart = 19,
    GraphFocus = 20,
    GridFocus = 21,
    Graph = 22,
    BakeShadow0 = 23,
    WaveFunctionCollapse0 = 24,
    WaveFunctionCollapse1 = 25,
    BakeShadow1 = 26,
    WaveFunctionCollapseDecor = 27,
    BakeShadow2 = 28,
    WaveFunctionCollapseReset = 29,
    Sandbanker = 30,
    GraphRefillPools = 31,
    Props = 32,
    PropAchievements = 33,
    BakeShadow3 = 34,
    SpawnBirds = 35,
    CleanupRemovedQubes = 36,
    CleanupProps = 37,
    ResetPropPlacer = 38,
    IterateRefillWorldMeshPools = 39,
    SaveToUrl = 40,
    GridBits = 41,
    RefreshSaveSystem = 42,
    Done = 43,
}

/// The systems the world master drives, in the order of the master's fields (each one a call of the game's).
/// Ported systems are wired by the game's world (`grid`, `graph`, `waveFunctionCollapse`, `moduleBuilder`,
/// `worldMeshes`, `bigMeshMaster`, `maker`); the others (shadow baker, props, birds, sandbanker, sounds, save
/// system, camera) by theirs. The loops return true while they have more to do this frame.
pub trait WorldSystems {
    /// `UniversalStopwatch.KeepGoing`.
    fn keep_going(&mut self) -> bool;

    // PreLoad
    fn sandbanker_clear(&mut self);
    fn flock_on_reset(&mut self);
    fn butterfly_flock_on_reset(&mut self);
    fn prop_placer_on_reset(&mut self);
    /// `shadowBaker.Load(saveData)`.
    fn shadow_baker_load(&mut self);
    fn maker_clear_undo_queue(&mut self);

    fn module_builder_iterate_deconstruct_qubes(&mut self) -> bool;
    fn prop_placer_iterate_deconstruct_props(&mut self) -> bool;
    fn prop_mesh_animator_on_reset(&mut self);
    /// `graph.BeginLoad(saveData)`.
    fn graph_begin_load(&mut self);
    /// `graph.IterateLoad(saveData)`.
    fn graph_iterate_load(&mut self) -> bool;
    fn graph_iterate_focus(&mut self) -> bool;
    /// `onCameraLoad(saveData)`.
    fn on_camera_load(&mut self);
    fn grid_iterate_focus(&mut self) -> bool;
    fn graph_iterate(&mut self) -> bool;
    fn grid_iterate_generate_center(&mut self) -> bool;
    fn wfc_iterate0(&mut self) -> bool;
    fn wfc_iterate_apply_modules(&mut self) -> bool;
    fn wfc_iterate1(&mut self) -> bool;
    fn wfc_iterate_decor(&mut self) -> bool;
    fn wfc_reset_propagation_counter(&mut self);
    fn shadow_baker_set_framing_dirty(&mut self);
    fn shadow_baker_bake_shadow(&mut self);
    fn shadow_baker_on_built(&mut self);
    fn sandbanker_iterate(&mut self) -> bool;
    fn graph_iterate_refill_pools(&mut self) -> bool;
    fn module_builder_iterate(&mut self) -> bool;
    /// `propAchievements.TriggerAchievements()` (skipped when there is none).
    fn prop_achievements_trigger(&mut self);
    fn flock_iterate_bird_creation(&mut self) -> bool;
    fn prop_placer_on_done_resetting(&mut self);
    /// `worldMeshes.IterateRefillPools(this)`.
    fn world_meshes_iterate_refill_pools(&mut self) -> bool;
    /// `grid.IterateBits(KeepGoing)`.
    fn grid_iterate_bits(&mut self) -> bool;
    /// `saveSystem.IterateRefresh(KeepGoing)`.
    fn save_system_iterate_refresh(&mut self) -> bool;

    // Update / LateUpdate
    fn wfc_later_update(&mut self);
    fn save_system_maybe_save_current(&mut self);
    /// `saveSystem.MaybeSaveSettings(10)`.
    fn save_system_maybe_save_settings(&mut self, seconds: f32);
    fn flock_on_update(&mut self);
    fn butterfly_flock_on_update(&mut self);
    fn many_sounds_on_update(&mut self);
    fn prop_mesh_animator_on_late_update(&mut self);
    fn big_mesh_master_on_late_update(&mut self);
    /// The `OnStart` of one of the master's systems ([`ON_START_ORDER`]), when it has it.
    fn on_start(&mut self, _system: &str) {}
}

/// The systems `WorldMaster.OnStart` 0x1F97F0 starts, in order (each skipped when missing).
pub const ON_START_ORDER: [&str; 11] = [
    "materialMaster",
    "aoBaker",
    "borderDrawer",
    "graph",
    "hoverHightlight",
    "moduleBuilder",
    "sandbanker",
    "screenshotCamera",
    "shadowBaker",
    "bigMeshMaster",
    "propAchievements",
];

/// `WorldMaster`'s frame state (the system references are the [`WorldSystems`]).
#[derive(Clone, Debug, PartialEq)]
pub struct WorldMaster {
    pub state: u8,
    pub stop_frame: bool,
    pub started_counting: bool,
    pub save_current_time: f32,
    pub do_go: bool,
    pub pause: bool,
    pub milliseconds: f32,
    pub is_dimmed: bool,
    pub awaiting_dim: bool,
    pub any_change: bool,
    pub last_change_time: f32,
    pub last_start_change_time: f32,
    pub last_save_string: String,
    /// The statics `expTSlow`, `expTMid`, `expTFast`, `expTFaster`: `exp(-dt·5)`, `exp(-dt·11)`, `exp(-dt·17)`,
    /// `exp(-dt·21)` for the frame's unscaled delta time (smoothing factors for the camera and effects).
    pub exp_t: [f32; 4],
}

impl Default for WorldMaster {
    /// `Placemaker.WorldMaster..ctor` 0x1F9EF0: an empty save string, `doGo`, 8 ms, awaiting the dim, no change
    /// times (-1); `state` 0.
    fn default() -> Self {
        WorldMaster {
            state: State::DoNothing as u8,
            stop_frame: false,
            started_counting: false,
            save_current_time: 0.0,
            do_go: true,
            pause: false,
            milliseconds: 8.0,
            is_dimmed: false,
            awaiting_dim: true,
            any_change: false,
            last_change_time: -1.0,
            last_start_change_time: -1.0,
            last_save_string: String::new(),
            exp_t: [0.0; 4],
        }
    }
}

/// The frame's clock values the master reads (`Time`).
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct FrameTime {
    pub unscaled_delta_time: f32,
    pub unscaled_time: f32,
    pub time: f32,
}

impl WorldMaster {
    /// `Placemaker.WorldMaster.Iterate` 0x1F8C00: one step of the state machine. A loop state calls its system
    /// and moves on once it reports no more work; a one-shot state acts and moves on. `DoNothing` stays (true),
    /// `AwaitDim` waits for `awaitingDim` to clear, `LoadBakeShadows0` bakes and jumps to
    /// `WaveFunctionCollapse1`, `BakeShadow0` marks the framing dirty before baking, `Done` returns false.
    pub fn iterate(&mut self, sys: &mut dyn WorldSystems) -> bool {
        let s = self.state;
        let busy = match s {
            0 => return true,
            1 | 40 => false,
            2 => {
                sys.sandbanker_clear();
                sys.flock_on_reset();
                sys.butterfly_flock_on_reset();
                sys.prop_placer_on_reset();
                sys.shadow_baker_load();
                sys.maker_clear_undo_queue();
                false
            }
            3 | 11 | 36 => sys.module_builder_iterate_deconstruct_qubes(),
            4 | 12 | 37 => sys.prop_placer_iterate_deconstruct_props(),
            5 => {
                sys.prop_mesh_animator_on_reset();
                false
            }
            6 => self.awaiting_dim,
            7 => {
                sys.graph_begin_load();
                false
            }
            8 => sys.graph_iterate_load(),
            9 | 20 => sys.graph_iterate_focus(),
            10 => {
                sys.on_camera_load();
                false
            }
            13 | 21 => sys.grid_iterate_focus(),
            14 | 22 => sys.graph_iterate(),
            15 => sys.grid_iterate_generate_center(),
            16 | 24 => sys.wfc_iterate0(),
            17 => sys.wfc_iterate_apply_modules(),
            18 => {
                sys.shadow_baker_set_framing_dirty();
                sys.shadow_baker_bake_shadow();
                self.state = State::WaveFunctionCollapse1 as u8;
                return true;
            }
            19 => {
                sys.shadow_baker_on_built();
                false
            }
            23 => {
                sys.shadow_baker_set_framing_dirty();
                sys.shadow_baker_bake_shadow();
                false
            }
            26 | 28 | 34 => {
                sys.shadow_baker_bake_shadow();
                false
            }
            25 => sys.wfc_iterate1(),
            27 => sys.wfc_iterate_decor(),
            29 => {
                sys.wfc_reset_propagation_counter();
                false
            }
            30 => sys.sandbanker_iterate(),
            31 => sys.graph_iterate_refill_pools(),
            32 => sys.module_builder_iterate(),
            33 => {
                sys.prop_achievements_trigger();
                false
            }
            35 => sys.flock_iterate_bird_creation(),
            38 => {
                sys.prop_placer_on_done_resetting();
                false
            }
            39 => sys.world_meshes_iterate_refill_pools(),
            41 => sys.grid_iterate_bits(),
            42 => sys.save_system_iterate_refresh(),
            43 => return false,
            // past the table: stays, like `DoNothing`
            _ => return true,
        };
        if !busy {
            self.state = s.wrapping_add(1);
        }
        true
    }

    /// `Placemaker.WorldMaster.OnStart` 0x1F97F0: each system's `OnStart`, in [`ON_START_ORDER`].
    pub fn on_start(&mut self, sys: &mut dyn WorldSystems) {
        for name in ON_START_ORDER {
            sys.on_start(name);
        }
    }

    /// `Placemaker.WorldMaster.ResetState` 0x1F9A50: back to `NormalStart`.
    pub fn reset_state(&mut self) {
        self.state = State::NormalStart as u8;
    }

    /// `Placemaker.WorldMaster.SetDirty` 0x1F9A60: the first change since the last save starts the change
    /// window; every change moves its end.
    pub fn set_dirty(&mut self, unscaled_time: f32) {
        if !self.any_change {
            self.any_change = true;
            self.last_start_change_time = unscaled_time;
        }
        self.last_change_time = unscaled_time;
    }

    /// `Placemaker.WorldMaster.ResetSaveCurrentTimer` 0x1F9A30: the next save in 10 s (`Time.time`).
    pub fn reset_save_current_timer(&mut self, time: f32) {
        self.save_current_time = time + 10.0;
    }

    /// The state machine part of `Placemaker.WorldMaster.OnOnEnable` 0x1F9620: a finished master starts over at
    /// `NormalStart` (the method also names the states for the profiler and calls `IOnOnEnable.OnOnEnable` on
    /// every component under the master: the world's job).
    pub fn on_on_enable(&mut self) {
        if self.state == State::Done as u8 {
            self.state = State::NormalStart as u8;
        }
    }

    /// `Placemaker.WorldMaster.Update` 0x1F9AF0. The frame starts counting (`StartCounting`, the caller's);
    /// the smoothing factors are set from the unscaled delta time; from `NormalStart` on, the collapse's
    /// `LaterUpdate` runs and the town is saved when it has been quiet (2 s after the last change and 10 s after
    /// the first, or 15 s after the first) and the settings every 10 s. Then `Iterate` runs while the frame has
    /// time and it has work; then, from `NormalStart` on, `LaterUpdate` again, and the flocks and sounds update.
    pub fn update(&mut self, sys: &mut dyn WorldSystems, t: FrameTime) {
        let d = -t.unscaled_delta_time;
        for (k, f) in [5.0f32, 11.0, 17.0, 21.0].into_iter().enumerate() {
            self.exp_t[k] = ((d * f) as f64).exp() as f32;
        }
        self.pause = false;
        if self.state >= State::NormalStart as u8 {
            sys.wfc_later_update();
            let quiet = 0.0 < self.last_change_time && self.last_change_time + 2.0 < t.unscaled_time && self.last_start_change_time + 10.0 < t.unscaled_time;
            if quiet || self.last_start_change_time + 15.0 < t.unscaled_time {
                sys.save_system_maybe_save_current();
            }
            sys.save_system_maybe_save_settings(10.0);
        }
        while sys.keep_going() {
            if !self.iterate(sys) {
                break;
            }
        }
        if self.state >= State::NormalStart as u8 {
            sys.wfc_later_update();
        }
        sys.flock_on_update();
        sys.butterfly_flock_on_update();
        sys.many_sounds_on_update();
        self.started_counting = false;
    }

    /// `Placemaker.WorldMaster.LateUpdate` 0x1F91B0.
    pub fn late_update(&mut self, sys: &mut dyn WorldSystems) {
        sys.prop_mesh_animator_on_late_update();
        sys.big_mesh_master_on_late_update();
    }
}
