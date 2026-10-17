//! `ts_game::placemaker::module_builder` against the original game (E7a). Needs `TS_HARNESS=1`.
//! The vertex transforms on a fresh game `Square` whose four corner matrices are set at random.

use serde_json::{json, Value};
use ts_check::{arg, harness, ret};
use ts_game::placemaker::graphs::Square;
use ts_game::placemaker::module_builder;
use ts_game::unity::quaternion::Matrix4x4;
use ts_game::unity::vector::{Vector3, Vector4};

const MB: &str = "Placemaker.ModuleBuilder";

fn ok(r: Value, what: &str) -> Value {
    assert_eq!(r["ok"], json!(true), "{what}: {r}");
    r
}
fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}
fn f32s(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|f| f.to_le_bytes()).collect()
}
fn words(b: &[u8]) -> Vec<u32> {
    b.chunks(4).map(|c| u32::from_le_bytes(c.try_into().unwrap())).collect()
}

struct Rng(u32);
impl Rng {
    fn next(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0
    }
    fn f(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (self.next() as f32 / u32::MAX as f32) * (hi - lo)
    }
    /// Mostly 0..1 (vertices in the unit qube), now and then outside.
    fn unit(&mut self) -> f32 {
        if self.next() % 6 == 0 {
            self.f(-2.0, 3.0)
        } else {
            self.f(0.0, 1.0)
        }
    }
}

#[test]
fn module_geometry_matches_the_game() {
    let Some(mut h) = harness() else { return };
    let sq = ok(h.request(json!({"new": "Placemaker.Graphs.Square", "args": [], "main": true})), "new Square")["handle"].as_u64().unwrap();
    let mut r = Rng(81);
    for round in 0..40 {
        // corner matrices: a rotation-ish frame with a translation, as the graph makes them, or anything
        let mut square = Square::default();
        for k in 0..4 {
            let m: [f32; 16] = std::array::from_fn(|i| if round % 4 == 0 { r.f(-3.0, 3.0) } else if i % 5 == 0 { 1.0 + r.f(-0.3, 0.3) } else if i >= 12 && i < 15 { r.f(-20.0, 20.0) } else if i % 4 == 3 { 0.0 } else { r.f(-0.4, 0.4) });
            square.m[k] = Matrix4x4 { m };
            ok(h.request(json!({"field": format!("m{k}"), "handle": sq, "set_raw": hex(&f32s(&m)), "main": true})), "set m");
        }
        let mut reqs = vec![];
        let mut ours = vec![];
        for _ in 0..100 {
            let v = Vector3 { x: r.unit(), y: r.unit(), z: r.unit() };
            let ts: Vector4 = module_builder::get_ts(v);
            let vb = f32s(&[v.x, v.y, v.z]);
            let tb = f32s(&[ts.x, ts.y, ts.z, ts.w]);
            reqs.push(json!({"call": format!("{MB}.GetTs"), "args": [arg::raw(&vb)], "main": true}));
            ours.push(("GetTs", v, f32s(&[ts.x, ts.y, ts.z, ts.w])));
            let p = module_builder::multiply_point(&square, ts, v);
            reqs.push(json!({"call": format!("{MB}.MultiplyPoint"), "args": [{"handle": sq}, arg::raw(&tb), arg::raw(&vb)], "main": true}));
            ours.push(("MultiplyPoint", v, f32s(&[p.x, p.y, p.z])));
            let d = module_builder::multiply_vector(&square, ts, v);
            reqs.push(json!({"call": format!("{MB}.MultiplyVector"), "args": [{"handle": sq}, arg::raw(&tb), arg::raw(&vb)], "main": true}));
            ours.push(("MultiplyVector", v, f32s(&[d.x, d.y, d.z])));
        }
        let got = h.batch(reqs);
        for (g, (what, v, o)) in got.into_iter().zip(ours) {
            let g = ok(g, what);
            assert_eq!(words(&ret::bytes(&g)), words(&o), "round {round}: {what}({v:?})");
        }
    }
}
