//! `ts_game::placemaker::ao_baker` (E9b) against the original game. Needs `TS_HARNESS=1`.
//! The ids and the uncovered colour; the scene baker's marching-squares lookup texture and `powers`; then
//! `BakeAo`'s atlas layout on a fresh `AoBaker` and `Camera` on an inactive object, its stand-in master's
//! graph given random shadow-mesh bounds and heights (fields and both globals, read in the same batch). The
//! scene baker bakes again at the end, which resets the globals.

mod graph_common;

use graph_common::*;
use serde_json::{json, Value};
use ts_check::{arg, harness, ret, Harness};
use ts_game::placemaker::ao_baker::*;
use ts_game::unity::geometry::Bounds;
use ts_game::unity::vector::{v3, Vector4};

const AO: &str = "Placemaker.AoBaker";

fn f32s(b: &[u8]) -> Vec<f32> {
    b.chunks(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect()
}
fn bits(v: &[f32]) -> Vec<u32> {
    v.iter().map(|f| f.to_bits()).collect()
}
fn field(h: &mut Harness, t: u64, f: &str) -> Vec<u8> {
    ret::bytes(&ok(h.request(json!({"field": f, "handle": t, "main": true})), f))
}
fn field_h(h: &mut Harness, t: u64, f: &str) -> u64 {
    handle(&ok(h.request(json!({"field": f, "handle": t, "main": true})), f), f)
}
fn set_h(h: &mut Harness, t: u64, f: &str, v: u64) {
    ok(h.request(json!({"field": f, "handle": t, "set_handle": v, "main": true})), f);
}
fn set_raw(h: &mut Harness, t: u64, f: &str, b: &[u8]) {
    let hex: String = b.iter().map(|x| format!("{x:02x}")).collect();
    ok(h.request(json!({"field": f, "handle": t, "set_raw": hex, "main": true})), f);
}
fn prop_id(h: &mut Harness, name: &str) -> i32 {
    ret::i32(&ok(h.request(json!({"call": "UnityEngine.Shader.PropertyToID", "args": [arg::string(name)], "main": true})), name))
}

/// Our host: the globals written down.
#[derive(Default)]
struct Rec(Vec<(String, Vector4)>);
impl AoHost for Rec {
    fn set_global_vector(&mut self, name: &str, v: Vector4) {
        self.0.push((name.into(), v));
    }
}

#[test]
fn ao_baker_matches_the_game() {
    let Some(mut guard) = harness() else { return };
    let h: &mut Harness = &mut guard;
    h.wait_for_object("Placemaker.GridGenerator");
    for (f, n) in [("tex3dUvSize", ids::TEX3D_UV_SIZE), ("tex3dWorldBounds", ids::TEX3D_WORLD_BOUNDS)] {
        let s = ret::i32(&ok(h.request(json!({"static_field": format!("{AO}.{f}"), "main": true})), f));
        assert_eq!(s, prop_id(h, n), "{f} = {n}");
    }
    let col = f32s(&ret::bytes(&ok(h.request(json!({"static_field": format!("{AO}.uncoveredCol"), "main": true})), "uncoveredCol")));
    assert_eq!(bits(&col), bits(&[UNCOVERED_COL.r, UNCOVERED_COL.g, UNCOVERED_COL.b, UNCOVERED_COL.a]));
    let col32 = ret::bytes(&ok(h.request(json!({"static_field": format!("{AO}.uncoveredCol32"), "main": true})), "uncoveredCol32"));
    assert_eq!(col32, [255, 255, 255, 255]);

    // the scene baker's lookup texture and powers
    let master = handle(&ok(h.request(json!({"field": "master", "on": "Placemaker.GridGenerator", "main": true})), "master"), "master");
    let live = field_h(h, master, "aoBaker");
    let mut ours = AoBaker::default();
    ours.on_start();
    assert_eq!(i32s(&field(h, live, "powers")), [ours.powers.x, ours.powers.y, ours.powers.z, ours.powers.w], "powers");
    let tex = field_h(h, live, "marchingSquaresTex");
    let size = |h: &mut Harness, m: &str| ret::i32(&ok(h.request(json!({"call": m, "class": "UnityEngine.Texture", "handle": tex, "args": [], "main": true})), m));
    assert_eq!((size(h, "get_width"), size(h, "get_height")), ours.marching_squares_tex_size, "lookup size");
    let px: Vec<u8> = f32s(&ret::bytes(&ok(h.request(json!({"call": "GetPixels", "class": "UnityEngine.Texture2D", "handle": tex, "args": [], "main": true})), "GetPixels")))
        .iter()
        .map(|f| (f * 255.0).round() as u8)
        .collect();
    let want: Vec<u8> = marching_squares_texture().iter().flat_map(|c| [c.r, c.g, c.b, c.a]).collect();
    assert_eq!(px.len(), want.len(), "lookup texels");
    if let Some(i) = (0..px.len()).find(|&i| px[i] != want[i]) {
        panic!("lookup texel {} (x {}, y {}): game {:?} ours {:?}", i / 4, (i / 4) % 1024, (i / 4) / 1024, &px[i / 4 * 4..i / 4 * 4 + 4], &want[i / 4 * 4..i / 4 * 4 + 4]);
    }

    // BakeAo's layout on a fresh baker
    let go = handle(&ok(h.request(json!({"new": "UnityEngine.GameObject", "args": [arg::string("ts_check ao baker")], "param_types": ["System.String"], "main": true, "keep": true})), "go"), "go");
    ok(h.request(json!({"call": "SetActive", "class": "UnityEngine.GameObject", "handle": go, "args": [arg::bool(false)], "main": true})), "SetActive");
    let add = |h: &mut Harness, t: &str| {
        let tt = handle(&ok(h.request(json!({"call": "System.Type.GetType", "args": [arg::string(t)], "param_types": ["System.String"], "main": true, "keep": true})), t), t);
        handle(&ok(h.request(json!({"call": "AddComponent", "class": "UnityEngine.GameObject", "handle": go, "args": [{"handle": tt}], "param_types": ["System.Type"], "main": true, "keep": true})), t), t)
    };
    let ao = add(h, "Placemaker.AoBaker, Assembly-CSharp");
    let cam = add(h, "UnityEngine.Camera, UnityEngine.CoreModule");
    set_h(h, ao, "cam", cam);
    let wm = handle(&ok(h.request(json!({"new": "Placemaker.WorldMaster", "args": [], "main": true, "keep": true})), "wm"), "wm");
    let graph = handle(&ok(h.request(json!({"new": "Placemaker.Graphs.Graph", "args": [], "main": true, "keep": true})), "graph"), "graph");
    set_h(h, wm, "graph", graph);
    set_h(h, ao, "master", wm);
    let ids_now = [(ids::TEX3D_UV_SIZE, prop_id(h, ids::TEX3D_UV_SIZE)), (ids::TEX3D_WORLD_BOUNDS, prop_id(h, ids::TEX3D_WORLD_BOUNDS))];
    let mut s: u32 = 21;
    let mut rnd = || {
        s ^= s << 13;
        s ^= s >> 17;
        s ^= s << 5;
        s
    };
    for k in 0..300 {
        let f = |r: u32, scale: f32| (r >> 8) as f32 / 16_777_216.0 * scale;
        let b = Bounds { center: v3(f(rnd(), 200.0) - 100.0, f(rnd(), 20.0), f(rnd(), 200.0) - 100.0), extents: v3(f(rnd(), 60.0), f(rnd(), 20.0), f(rnd(), 60.0)) };
        let height = (rnd() % 40) as i32 - if k % 17 == 0 { 3 } else { 0 };
        let raw: Vec<u8> = [b.center.x, b.center.y, b.center.z, b.extents.x, b.extents.y, b.extents.z].iter().flat_map(|x| x.to_le_bytes()).collect();
        set_raw(h, graph, "shadowMeshBounds", &raw);
        set_raw(h, graph, "boundsHeight", &height.to_le_bytes());
        let mut reqs = vec![json!({"call": "BakeAo", "class": AO, "handle": ao, "args": [], "main": true})];
        for (_, id) in ids_now {
            reqs.push(json!({"call": "UnityEngine.Shader.GetGlobalVector", "args": [arg::i32(id)], "param_types": ["System.Int32"], "main": true}));
        }
        let r: Vec<Value> = h.batch(reqs).into_iter().map(|v| ok(v, "BakeAo batch")).collect();
        let mut rec = Rec::default();
        ours.bake_ao(b, height, &mut rec);
        let game = (
            bits(&f32s(&field(h, ao, "size"))),
            bits(&f32s(&field(h, ao, "texParams"))),
            i32s(&field(h, ao, "countX"))[0],
            i32s(&field(h, ao, "countY"))[0],
            i32s(&field(h, ao, "totalCount"))[0],
        );
        let mine = (
            bits(&[ours.size.x, ours.size.y]),
            bits(&[ours.tex_params.x, ours.tex_params.y, ours.tex_params.z, ours.tex_params.w]),
            ours.count_x,
            ours.count_y,
            ours.total_count,
        );
        assert_eq!(game, mine, "round {k}: bounds {b:?} height {height}");
        for ((name, v), g) in rec.0.iter().zip(&r[1..]) {
            assert_eq!(bits(&f32s(&ret::bytes(g))), bits(&[v.x, v.y, v.z, v.w]), "round {k}: {name}");
        }
        let cam_on = ret::bytes(&ok(h.request(json!({"call": "get_enabled", "class": "UnityEngine.Behaviour", "handle": cam, "args": [], "main": true})), "enabled"))[0] != 0;
        assert_eq!(cam_on, ours.camera_enabled, "round {k}: camera");
    }
    ok(h.request(json!({"call": "UnityEngine.Object.Destroy", "args": [{"handle": go}], "param_types": ["UnityEngine.Object"], "main": true})), "Destroy");
    ok(h.request(json!({"call": "BakeAo", "class": AO, "handle": live, "args": [], "main": true})), "live BakeAo");
}
