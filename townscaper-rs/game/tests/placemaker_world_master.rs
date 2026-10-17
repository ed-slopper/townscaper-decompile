//! `ts_game::placemaker::world_master` against the original game (E8b). Needs `TS_HARNESS=1` and the install.
//! The graph tests' stand-in master (its own grid, graph, collapse, maker and module builder; the scene's other
//! systems) is walked by the game's `WorldMaster.Iterate` from `StartLoad` (an empty save) through loading and
//! the normal round to `Done`, one call per request; our state machine replays each step, its systems
//! answering as the game's did (a loop state that stayed was busy), and must land on the same state with the same
//! result. Then `ResetState`, `OnOnEnable`'s restart and `SetDirty`; `Update`'s loop and save timing on a fake
//! world.

mod graph_common;

use graph_common::*;
use serde_json::json;
use ts_check::{arg, harness, ret};
use ts_game::placemaker::world_master::{FrameTime, State, WorldMaster, WorldSystems};

/// Records the calls; loops answer `busy`.
#[derive(Default)]
struct Rec {
    busy: bool,
    calls: Vec<&'static str>,
    keep: usize,
}

macro_rules! rec {
    ($($name:ident),* ; $($loop_name:ident),*) => {
        $(fn $name(&mut self) { self.calls.push(stringify!($name)); })*
        $(fn $loop_name(&mut self) -> bool { self.calls.push(stringify!($loop_name)); self.busy })*
    };
}

impl WorldSystems for Rec {
    fn keep_going(&mut self) -> bool {
        if self.keep == 0 {
            return false;
        }
        self.keep -= 1;
        true
    }
    rec!(sandbanker_clear, flock_on_reset, butterfly_flock_on_reset, prop_placer_on_reset, shadow_baker_load, maker_clear_undo_queue,
         prop_mesh_animator_on_reset, graph_begin_load, on_camera_load, wfc_reset_propagation_counter, shadow_baker_set_framing_dirty,
         shadow_baker_bake_shadow, shadow_baker_on_built, prop_achievements_trigger, prop_placer_on_done_resetting, wfc_later_update,
         save_system_maybe_save_current, flock_on_update, butterfly_flock_on_update, many_sounds_on_update,
         prop_mesh_animator_on_late_update, big_mesh_master_on_late_update;
         module_builder_iterate_deconstruct_qubes, prop_placer_iterate_deconstruct_props, graph_iterate_load, graph_iterate_focus,
         grid_iterate_focus, graph_iterate, grid_iterate_generate_center, wfc_iterate0, wfc_iterate_apply_modules, wfc_iterate1,
         wfc_iterate_decor, sandbanker_iterate, graph_iterate_refill_pools, module_builder_iterate, flock_iterate_bird_creation,
         world_meshes_iterate_refill_pools, grid_iterate_bits, save_system_iterate_refresh);
    fn save_system_maybe_save_settings(&mut self, _seconds: f32) {
        self.calls.push("save_system_maybe_save_settings");
    }
}

/// States whose system is a loop (stays while it reports work).
fn is_loop(s: u8) -> bool {
    matches!(s, 3 | 4 | 8 | 9 | 11 | 12 | 13 | 14 | 15 | 16 | 17 | 20 | 21 | 22 | 24 | 25 | 27 | 30 | 31 | 32 | 35 | 36 | 37 | 39 | 41 | 42)
}

#[test]
fn world_master_iterate_matches_the_game() {
    let Some(mut h) = harness() else { return };
    let (mut g, _grid, _ao) = setup(&mut h);
    let master = handle(&ok(g.h.request(Game::field(g.graph, "master")), "master"), "master");
    let scene = handle(&ok(g.h.request(json!({"field": "master", "on": "Placemaker.GridGenerator", "main": true})), "scene master"), "scene master");
    // the scene's systems the stand-in lacks
    for f in ["shadowBaker", "flock", "butterflyFlock", "propPlacer", "propMeshAnimator", "propAchievements", "saveSystem", "manySoundsManager", "bigMeshMaster"] {
        let v = ok(g.h.request(Game::field(scene, f)), f);
        if let Some(hd) = v["handle"].as_u64() {
            ok(g.h.request(json!({"field": f, "handle": master, "set_handle": hd, "main": true})), f);
        }
    }
    // the fresh module builder's two containers
    let mb = handle(&ok(g.h.request(Game::field(master, "moduleBuilder")), "moduleBuilder"), "moduleBuilder");
    for f in ["moduleContainerPool", "moduleContainersToDeconstruct"] {
        let go = handle(&ok(g.h.request(json!({"new": "UnityEngine.GameObject", "args": [arg::string(f)], "param_types": ["System.String"], "main": true})), "go"), "go");
        let t = handle(&ok(g.h.request(json!({"call": "get_transform", "class": "UnityEngine.GameObject", "handle": go, "args": [], "main": true, "keep": true})), "t"), "t");
        ok(g.h.request(json!({"field": f, "handle": mb, "set_handle": t, "main": true})), f);
    }
    let set_u8 = |g: &mut Game, f: &str, v: u8| {
        ok(g.h.request(json!({"field": f, "handle": master, "set_raw": format!("{v:02x}"), "main": true})), f);
    };
    let state = |g: &mut Game| ret::bytes(&ok(g.h.request(Game::field(master, "state")), "state"))[0];
    set_u8(&mut g, "awaitingDim", 0);
    let mut seen = std::collections::BTreeMap::<u8, usize>::new();
    let mut steps = 0;
    // the load from an empty save, then the normal round from NormalStart
    for start in [State::StartLoad, State::NormalStart] {
    set_u8(&mut g, "state", start as u8);
    let mut ours = WorldMaster { awaiting_dim: false, state: start as u8, ..Default::default() };
    loop {
        let before = state(&mut g);
        assert_eq!(before, ours.state, "step {steps}: state");
        let r = ok(g.h.request(json!({"call": "Placemaker.WorldMaster.Iterate", "handle": master, "args": [], "main": true})), &format!("Iterate in state {before}"));
        let gr = ret::bytes(&r)[0] != 0;
        let after = state(&mut g);
        let mut rec = Rec { busy: is_loop(before) && after == before, ..Default::default() };
        let or = ours.iterate(&mut rec);
        assert_eq!((or, ours.state), (gr, after), "step {steps}: from state {before} (calls {:?})", rec.calls);
        *seen.entry(before).or_default() += 1;
        steps += 1;
        if !gr {
            break;
        }
        assert!(steps < 100_000, "the game's master never finished");
    }
    assert_eq!(ours.state, State::Done as u8);
    }
    eprintln!("{steps} steps; rounds per state: {seen:?}");
    for s in 1..=43u8 {
        assert!(seen.contains_key(&s), "state {s} never visited");
    }
    // Done stays done; DoNothing stays; past the table stays
    for s in [43u8, 0, 44, 200] {
        set_u8(&mut g, "state", s);
        let r = ret::bytes(&ok(g.h.request(json!({"call": "Placemaker.WorldMaster.Iterate", "handle": master, "args": [], "main": true})), "Iterate"))[0] != 0;
        let mut o = WorldMaster { state: s, ..Default::default() };
        let or = o.iterate(&mut Rec::default());
        assert_eq!((r, state(&mut g)), (or, o.state), "state {s}");
    }
    // AwaitDim waits for awaitingDim
    for d in [1u8, 0] {
        set_u8(&mut g, "awaitingDim", d);
        set_u8(&mut g, "state", 6);
        ok(g.h.request(json!({"call": "Placemaker.WorldMaster.Iterate", "handle": master, "args": [], "main": true})), "Iterate");
        let mut o = WorldMaster { state: 6, awaiting_dim: d != 0, ..Default::default() };
        o.iterate(&mut Rec::default());
        assert_eq!(state(&mut g), o.state, "AwaitDim with awaitingDim {d}");
    }
    // ResetState, OnOnEnable's restart (the state part), SetDirty
    ok(g.h.request(json!({"call": "Placemaker.WorldMaster.ResetState", "handle": master, "args": [], "main": true})), "ResetState");
    assert_eq!(state(&mut g), State::NormalStart as u8, "ResetState");
    let f32_of = |g: &mut Game, f: &str| ret::f32(&ok(g.h.request(Game::field(master, f)), f));
    let mut o = WorldMaster::default();
    for k in 0..3 {
        let r = g.h.batch(vec![
            json!({"call": "Placemaker.WorldMaster.SetDirty", "handle": master, "args": [], "main": true}),
            json!({"call": "UnityEngine.Time.get_unscaledTime", "args": [], "main": true}),
        ]);
        ok(r[0].clone(), "SetDirty");
        let t = ret::f32(&r[1]);
        o.set_dirty(t);
        assert_eq!(f32_of(&mut g, "lastChangeTime").to_bits(), o.last_change_time.to_bits(), "SetDirty {k}: lastChangeTime");
        if k == 0 {
            // (the game's master may have been dirty already: take its start)
            o.last_start_change_time = f32_of(&mut g, "lastStartChangeTime");
        }
        assert_eq!(f32_of(&mut g, "lastStartChangeTime").to_bits(), o.last_start_change_time.to_bits(), "SetDirty {k}: lastStartChangeTime");
    }
}

/// `Update` on a fake world: the save timing and the budgeted loop.
#[test]
fn world_master_update_follows_the_budget() {
    let mut m = WorldMaster { state: State::NormalStart as u8, ..Default::default() };
    // 3 rounds of budget: the master steps three times (NormalStart, GraphFocus busy twice)
    let mut sys = Rec { busy: true, keep: 3, ..Default::default() };
    m.update(&mut sys, FrameTime { unscaled_delta_time: 0.016, unscaled_time: 100.0, time: 100.0 });
    assert_eq!(m.state, State::GraphFocus as u8);
    assert_eq!(sys.calls.iter().filter(|c| **c == "graph_iterate_focus").count(), 2);
    assert_eq!(sys.calls.iter().filter(|c| **c == "wfc_later_update").count(), 2, "before and after the loop");
    // saving: quiet for 2 s after the last change and 10 s after the first, or 15 s after the first
    let save = |m: &mut WorldMaster, t: f32| {
        let mut s = Rec { keep: 0, ..Default::default() };
        m.update(&mut s, FrameTime { unscaled_delta_time: 0.016, unscaled_time: t, time: t });
        s.calls.contains(&"save_system_maybe_save_current")
    };
    let mut m = WorldMaster { state: State::Done as u8, ..Default::default() };
    m.set_dirty(10.0);
    m.set_dirty(19.0);
    assert!(!save(&mut m, 20.5), "1.5 s after the last change");
    assert!(save(&mut m, 21.5), "2.5 s after the last, 11.5 s after the first");
    let mut m = WorldMaster { state: State::Done as u8, ..Default::default() };
    m.set_dirty(10.0);
    for t in [11.0, 15.0, 19.0, 23.0] {
        m.set_dirty(t);
    }
    assert!(!save(&mut m, 24.0), "still changing, 14 s");
    assert!(save(&mut m, 25.5), "15 s after the first change");
    // the smoothing factors
    assert_eq!(m.exp_t[0], ((-0.016f32 * 5.0) as f64).exp() as f32, "expTSlow");
}

/// No text system here: the game's answers are fed in.
struct Saves(bool);
impl ts_game::placemaker::world_master::WorldSaves for Saves {
    fn save_to_string(&mut self, _save: &ts_game::placemaker::save_data::SaveData) -> String {
        String::new()
    }
    fn load_from_string(&mut self, _text: &str, _save: &mut ts_game::placemaker::save_data::SaveData) -> bool {
        self.0
    }
    fn read_non_image_data_from(&mut self, _into: &mut ts_game::placemaker::save_data::SaveData, _from: &ts_game::placemaker::save_data::SaveData) {}
    fn graph_save(&mut self, _save: &mut ts_game::placemaker::save_data::SaveData) {}
}

#[test]
fn world_master_new_and_load_match_the_game() {
    use ts_game::placemaker::save_data::SaveData;
    use ts_game::unity::random::Rand;
    let Some(mut h) = harness() else { return };
    let wm = handle(&ok(h.request(json!({"new": "Placemaker.WorldMaster", "args": [], "main": true})), "new WorldMaster"), "wm");
    let wmc = "Placemaker.WorldMaster";
    // New(SaveData): one corner from the engine's random state (set and drawn in one batch)
    let mut seeds = Rand { s: [11, 22, 33, 44] };
    for k in 0..200 {
        let start = Rand { s: [seeds.next_u32(), seeds.next_u32(), seeds.next_u32(), seeds.next_u32()] };
        let sd = handle(&ok(h.request(json!({"new": "Placemaker.SaveData", "args": [], "main": true})), "new SaveData"), "sd");
        let st: String = start.s.iter().flat_map(|w| w.to_le_bytes()).map(|b| format!("{b:02x}")).collect();
        let r = h.batch(vec![
            json!({"native": "UnityPlayer.dll", "va": "0x107A74F0", "args": [{"buf": st}]}),
            json!({"call": format!("{wmc}.New"), "handle": wm, "args": [{"handle": sd}], "param_types": ["Placemaker.SaveData"], "main": true}),
        ]);
        ok(r[1].clone(), "New");
        let l = handle(&ok(h.request(json!({"field": "corners", "handle": sd, "main": true})), "corners"), "corners");
        let raw = ret::bytes(&ok(h.request(json!({"call": "ToArray", "class": "System.Collections.Generic.List`1[[Placemaker.SaveData+C, Assembly-CSharp]]", "handle": l, "args": [], "main": true})), "ToArray"));
        let mut ours = SaveData::default();
        let mut rand = start.clone();
        WorldMaster::new_town(&mut ours, &mut rand);
        assert_eq!(raw, bits_of(&ours.corners), "round {k}: New corner {:?}", ours.corners);
    }
    // shouldBeLoading
    let set_u8 = |h: &mut ts_check::Harness, f: &str, v: u8| {
        ok(h.request(json!({"field": f, "handle": wm, "set_raw": format!("{v:02x}"), "main": true})), f);
    };
    for s in 0..=45u8 {
        set_u8(&mut h, "state", s);
        let g = ret::bytes(&ok(h.request(json!({"call": format!("{wmc}.get_shouldBeLoading"), "handle": wm, "args": [], "main": true})), "shouldBeLoading"))[0] != 0;
        assert_eq!(g, WorldMaster { state: s, ..Default::default() }.should_be_loading(), "shouldBeLoading in state {s}");
    }
    // Load(SaveData) of its own save, Load(string) good and bad: state and change window
    let own = handle(&ok(h.request(json!({"field": "saveData", "handle": wm, "main": true})), "saveData"), "saveData");
    let read = |h: &mut ts_check::Harness| {
        (
            ret::bytes(&ok(h.request(json!({"field": "state", "handle": wm, "main": true})), "state"))[0],
            ret::bytes(&ok(h.request(json!({"field": "anyChange", "handle": wm, "main": true})), "anyChange"))[0] != 0,
            ret::f32(&ok(h.request(json!({"field": "lastChangeTime", "handle": wm, "main": true})), "lastChangeTime")),
        )
    };
    let prime = |h: &mut ts_check::Harness| {
        set_u8(h, "state", 25);
        set_u8(h, "anyChange", 1);
        ok(h.request(json!({"field": "lastChangeTime", "handle": wm, "set_raw": "0000a040", "main": true})), "lastChangeTime");
    };
    let primed = || WorldMaster { state: 25, any_change: true, last_change_time: 5.0, ..Default::default() };
    prime(&mut h);
    ok(h.request(json!({"call": format!("{wmc}.Load"), "handle": wm, "args": [{"handle": own}], "param_types": ["Placemaker.SaveData"], "main": true})), "Load(SaveData)");
    let mut o = primed();
    o.load_save(&mut Saves(true), &mut SaveData::default(), None);
    assert_eq!(read(&mut h), (o.state, o.any_change, o.last_change_time), "Load(own save)");
    // a town string from the game's own text save system, and a bad one
    let good = {
        let c = handle(&ok(h.request(json!({"field": "corners", "handle": own, "main": true})), "c"), "c");
        let v = handle(&ok(h.request(json!({"field": "voxels", "handle": own, "main": true})), "v"), "v");
        let r = ok(h.request(json!({"call": "Placemaker.TextSaveSystem.SaveToString", "args": [{"handle": c}, {"handle": v}], "main": true})), "SaveToString");
        r["string"].as_str().unwrap().to_string()
    };
    for (text, _) in [(good.as_str(), true), ("not a town", false)] {
        prime(&mut h);
        let g = ret::bytes(&ok(h.request(json!({"call": format!("{wmc}.Load"), "handle": wm, "args": [arg::string(text)], "param_types": ["System.String"], "main": true})), "Load(string)"))[0] != 0;
        let mut o = primed();
        let ours = o.load_string(&mut Saves(g), &mut SaveData::default(), text);
        assert_eq!(ours, g);
        assert_eq!(read(&mut h), (o.state, o.any_change, o.last_change_time), "Load({text:?}) = {g}");
    }
}
