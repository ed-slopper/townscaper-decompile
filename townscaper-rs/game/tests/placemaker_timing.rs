//! `ts_game::placemaker::timing` against the original game (E8a). Needs `TS_HARNESS=1`.
//! The layer table and masks, the shader ids, `TimeManager`'s scales (through `Time.timeScale`, restored after)
//! and `UniversalStopwatch.StartCounting` (once per frame).

use serde_json::{json, Value};
use ts_check::{arg, harness, ret};
use ts_game::placemaker::timing::{layer_master as lm, shader_ids, Clock, TimeManager, UniversalStopwatch};

fn ok(r: Value, what: &str) -> Value {
    assert_eq!(r["ok"], json!(true), "{what}: {r}");
    r
}

#[derive(Default)]
struct NoClock;
impl Clock for NoClock {
    fn restart(&mut self) {}
    fn elapsed_ms(&self) -> i64 {
        0
    }
}

#[test]
fn timing_and_statics_match_the_game() {
    let Some(mut h) = harness() else { return };
    let st = |h: &mut ts_check::Harness, n: &str| ret::i32(&ok(h.request(json!({"static_field": n, "main": true})), n));
    for (n, v) in [
        ("defaultLayer", lm::DEFAULT_LAYER),
        ("voxelLayer", lm::VOXEL_LAYER),
        ("groundLayer", lm::GROUND_LAYER),
        ("gridLayer", lm::GRID_LAYER),
        ("aoLayer", lm::AO_LAYER),
        ("aoStackLayer", lm::AO_STACK_LAYER),
        ("onlyShadowsLayer", lm::ONLY_SHADOWS_LAYER),
        ("noShadowsLayer", lm::NO_SHADOWS_LAYER),
        ("clickMask", lm::CLICK_MASK),
        ("propMask", lm::PROP_MASK),
        ("gridMask", lm::GRID_MASK),
        ("voxelMask", lm::VOXEL_MASK),
        ("onlyShadowsMask", lm::ONLY_SHADOWS_MASK),
        ("noShadowsMask", lm::NO_SHADOWS_MASK),
    ] {
        assert_eq!(st(&mut h, &format!("Placemaker.LayerMaster.{n}")), v, "LayerMaster.{n}");
    }
    for (n, name) in [("color", shader_ids::COLOR), ("rect", shader_ids::RECT), ("aaLineBoth", shader_ids::AA_LINE_BOTH), ("dim", shader_ids::DIM), ("shadingTexLightStack", shader_ids::SHADING_TEX_LIGHT_STACK)] {
        let id = ret::i32(&ok(h.request(json!({"call": "UnityEngine.Shader.PropertyToID", "args": [arg::string(name)], "main": true})), name));
        assert_eq!(st(&mut h, &format!("Placemaker.ShaderIds.{n}")), id, "ShaderIds.{n} = {name}");
    }

    // TimeManager: start from the game's state, restore it after
    let tm = "Placemaker.TimeManager";
    let get_f = |h: &mut ts_check::Harness, m: &str| ret::f32(&ok(h.request(json!({"call": m, "args": [], "main": true})), m));
    let dim0 = get_f(&mut h, &format!("{tm}.get_dimScale"));
    let slow0 = ret::bytes(&ok(h.request(json!({"call": format!("{tm}.get_slowmo"), "args": [], "main": true})), "slowmo"))[0] != 0;
    let mut ours = TimeManager::default();
    ours.set_dim_scale(dim0);
    ours.set_slowmo(slow0);
    let mut s: u32 = 5;
    for k in 0..200 {
        s ^= s << 13;
        s ^= s >> 17;
        s ^= s << 5;
        match s % 4 {
            0 => {
                let b = s & 16 != 0;
                ok(h.request(json!({"call": format!("{tm}.set_slowmo"), "args": [arg::bool(b)], "main": true})), "set_slowmo");
                ours.set_slowmo(b);
            }
            n => {
                let v = (s >> 8) as f32 / 16_777_216.0 * 2.0;
                let m = ["", "set_dimScale", "SetDimScale", "SetRecordingScale"][n as usize];
                ok(h.request(json!({"call": format!("{tm}.{m}"), "args": [arg::f32(v)], "main": true})), m);
                ours.set_dim_scale(v);
            }
        }
        let ts = get_f(&mut h, "UnityEngine.Time.get_timeScale");
        assert_eq!(ts.to_bits(), ours.time_scale.to_bits(), "step {k}: timeScale");
        assert_eq!(get_f(&mut h, &format!("{tm}.get_slowmoScale")).to_bits(), ours.slowmo_scale().to_bits(), "step {k}: slowmoScale");
        assert_eq!(get_f(&mut h, &format!("{tm}.get_dimScale")).to_bits(), ours.dim_scale().to_bits(), "step {k}: dimScale");
    }
    ok(h.request(json!({"call": format!("{tm}.set_dimScale"), "args": [arg::f32(dim0)], "main": true})), "restore");
    ok(h.request(json!({"call": format!("{tm}.set_slowmo"), "args": [arg::bool(slow0)], "main": true})), "restore");

    // StartCounting: once per frame
    for k in 0..20 {
        let r = h.batch(vec![
            json!({"static_field": "Placemaker.UniversalStopwatch.lastCountFrame", "main": true}),
            json!({"call": "UnityEngine.Time.get_frameCount", "args": [], "main": true}),
            json!({"call": "Placemaker.UniversalStopwatch.StartCounting", "args": [], "main": true}),
            json!({"call": "Placemaker.UniversalStopwatch.StartCounting", "args": [], "main": true}),
        ]);
        let last = ret::i32(&r[0]);
        let frame = ret::i32(&r[1]);
        let mut ours = UniversalStopwatch { stopwatch: NoClock, last_count_frame: last };
        assert_eq!(ret::bytes(&r[2])[0] != 0, ours.start_counting(frame), "round {k}: first StartCounting");
        assert_eq!(ret::bytes(&r[3])[0] != 0, ours.start_counting(frame), "round {k}: second StartCounting");
    }
}

/// A clock that advances 1 ms each time it is read.
#[derive(Default)]
struct Ticks(std::cell::Cell<i64>);
impl Clock for Ticks {
    fn restart(&mut self) {
        self.0.set(0);
    }
    fn elapsed_ms(&self) -> i64 {
        let t = self.0.get();
        self.0.set(t + 1);
        t
    }
}

/// `FastCoroutines` on a fake clock (its budgets are wall-clock time, which the harness can't compare): the queue
/// runs enumerators to their end and keeps invoking actions until the time is up.
#[test]
fn fast_coroutines_follow_the_budget() {
    use std::{cell::RefCell, rc::Rc};
    use ts_game::placemaker::timing::{FastCoroutines, FastItem};
    let log = Rc::new(RefCell::new(Vec::new()));
    let mut fc: FastCoroutines<Ticks> = FastCoroutines::default();
    for k in 0..2 {
        let (l, mut n) = (log.clone(), 0);
        fc.items.push(FastItem::Enumerator(Box::new(move || {
            n += 1;
            l.borrow_mut().push(format!("e{k}.{n}"));
            n < 3
        })));
    }
    let l = log.clone();
    fc.items.push(FastItem::Action(Box::new(move || l.borrow_mut().push("a".into()))));
    assert!(fc.fast_iterate(16), "the action is never removed");
    let got = log.borrow().clone();
    assert_eq!(got[..6], ["e0.1", "e0.2", "e0.3", "e1.1", "e1.2", "e1.3"]);
    assert!(got[6..].iter().all(|s| s == "a") && got.len() > 7, "{got:?}");
    // a coroutine frame: steps until it yields false
    let mut steps = vec![Some(true), Some(true), Some(false), Some(true), None].into_iter();
    let mut fc: FastCoroutines<Ticks> = FastCoroutines::default();
    assert!(fc.fast_coroutine_frame(&mut || steps.next().unwrap(), 6));
    assert!(!fc.fast_coroutine_frame(&mut || steps.next().unwrap(), 6));
}
