//! `ts_game::placemaker::sandbanker` (E9c) against the original game. Needs `TS_HARNESS=1` and the install.
//! A fresh `Sandbanker` on an inactive object, its master the scene's (the town's squares, the module library,
//! the frame budget) and its meshes two fresh ones, is stepped by the game's `Iterate`, one call per request,
//! from `Reset` to `Done`. Ours replays each step on the same squares (read from the scene) with our module
//! library, `Gather` reading as many squares as the game's did; after every step the state, the square index,
//! the vertices, triangles and coverage must match bit for bit, and after `ApplyWater` / `ApplySand` the water
//! and sand meshes.

mod graph_common;
mod module_common;

use graph_common::*;
use serde_json::{json, Value};
use ts_check::{arg, harness, ret, Harness};
use ts_game::placemaker::graphs::OrientedModuleSides;
use ts_game::placemaker::modules::OrientedModule;
use ts_game::os::utils::ByteFloat2;
use ts_game::placemaker::sandbanker::*;
use ts_game::unity::quaternion::Matrix4x4;
use ts_game::unity::vector::{v3, Vector2, Vector3, Vector4};

const SB: &str = "Placemaker.Sandbanker";
const T: &str = "UnityEngine.Transform";

fn f32s(b: &[u8]) -> Vec<f32> {
    b.chunks(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect()
}
fn call(h: &mut Harness, t: u64, class: &str, m: &str, args: Vec<Value>) -> Value {
    ok(h.request(json!({"call": m, "class": class, "handle": t, "args": args, "main": true})), m)
}
fn call_keep(h: &mut Harness, t: u64, class: &str, m: &str, args: Vec<Value>) -> u64 {
    handle(&ok(h.request(json!({"call": m, "class": class, "handle": t, "args": args, "main": true, "keep": true})), m), m)
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
fn ty(h: &mut Harness, n: &str) -> u64 {
    handle(&ok(h.request(json!({"call": "System.Type.GetType", "args": [arg::string(n)], "param_types": ["System.String"], "main": true, "keep": true})), n), n)
}
/// A `List<T>` field's items as bytes.
fn list_bytes(h: &mut Harness, obj: u64, f: &str, elem: &str) -> Vec<u8> {
    let l = field_h(h, obj, f);
    ret::bytes(&call(h, l, &list(elem), "ToArray", vec![]))
}
fn bits3(v: &[Vector3]) -> Vec<u32> {
    v.iter().flat_map(|v| [v.x, v.y, v.z]).map(f32::to_bits).collect()
}
fn bits2(v: &[Vector2]) -> Vec<u32> {
    v.iter().flat_map(|v| [v.x, v.y]).map(f32::to_bits).collect()
}
fn bits4(v: &[Vector4]) -> Vec<u32> {
    v.iter().flat_map(|v| [v.x, v.y, v.z, v.w]).map(f32::to_bits).collect()
}
fn fbits(b: &[u8]) -> Vec<u32> {
    f32s(b).iter().map(|f| f.to_bits()).collect()
}

/// A mesh's data through a `Get…(List<T>)` method (the array getters are stripped in this build); `submesh`
/// adds the submesh-0 argument (`GetTriangles(List<int>, 0)`).
fn mesh_list(h: &mut Harness, mesh: u64, m: &str, elem: &str, cs_elem: &str, submesh: bool) -> Vec<u8> {
    let l = handle(&ok(h.request(json!({"new": list(elem), "args": [], "main": true, "keep": true})), m), m);
    let lt = format!("System.Collections.Generic.List<{cs_elem}>");
    let (args, types) = if submesh {
        (vec![json!({"handle": l}), arg::i32(0)], vec![lt, "System.Int32".to_string()])
    } else {
        (vec![json!({"handle": l})], vec![lt])
    };
    ok(h.request(json!({"call": m, "class": "UnityEngine.Mesh", "handle": mesh, "args": args, "param_types": types, "main": true})), m);
    ret::bytes(&call(h, l, &list(elem), "ToArray", vec![]))
}

/// Our host: the scene's squares, read up front; the frame budget is the game's step.
struct Host {
    squares: Vec<Option<(SandSquare, Vector3)>>,
    budget: i32,
    read: i32,
    water: Option<(Vec<Vector3>, Vec<Vector2>, Vec<i32>)>,
    sand: Option<(Vec<Vector3>, Vec<i32>, Vec<Vector4>)>,
}
impl SandHost for Host {
    fn keep_going(&mut self) -> bool {
        let go = self.read < self.budget;
        self.read += 1;
        go
    }
    fn square_count(&mut self) -> i32 {
        self.squares.len() as i32
    }
    fn square(&mut self, index: i32) -> Option<SandSquare> {
        self.squares[index as usize].as_ref().map(|s| s.0.clone())
    }
    fn transform_point(&mut self, index: i32, p: Vector3) -> Vector3 {
        p.add(self.squares[index as usize].as_ref().unwrap().1)
    }
    fn set_water_mesh(&mut self, verts: &[Vector3], uvs: &[Vector2], tris: &[i32]) {
        self.water = Some((verts.to_vec(), uvs.to_vec(), tris.to_vec()));
    }
    fn set_sand_mesh(&mut self, verts: &[Vector3], tris: &[i32], tangents: &[Vector4]) {
        self.sand = Some((verts.to_vec(), tris.to_vec(), tangents.to_vec()));
    }
}

/// The scene's squares as `Gather` reads them, with each square transform's position.
fn read_squares(h: &mut Harness, squares_t: u64) -> Vec<Option<(SandSquare, Vector3)>> {
    let qube_t = ty(h, "Placemaker.Graphs.Qube, Assembly-CSharp");
    let square_t = ty(h, "Placemaker.Graphs.Square, Assembly-CSharp");
    let oms = "Placemaker.Graphs.OrientedModuleSides, Assembly-CSharp";
    let n = ret::i32(&call(h, squares_t, T, "get_childCount", vec![]));
    let mut out = vec![];
    for i in 0..n {
        let t = call_keep(h, squares_t, T, "GetChild", vec![arg::i32(i)]);
        let pos = f32s(&ret::bytes(&call(h, t, T, "get_position", vec![])));
        if ret::i32(&call(h, t, T, "get_childCount", vec![])) == 0 {
            out.push(None);
            continue;
        }
        let c0 = call_keep(h, t, T, "GetChild", vec![arg::i32(0)]);
        let q = handle(&ok(h.request(json!({"call": "GetComponent", "class": "UnityEngine.Component", "handle": c0, "args": [{"handle": qube_t}], "param_types": ["System.Type"], "main": true, "keep": true})), "Qube"), "Qube");
        let s = handle(&ok(h.request(json!({"call": "GetComponent", "class": "UnityEngine.Component", "handle": t, "args": [{"handle": square_t}], "param_types": ["System.Type"], "main": true, "keep": true})), "Square"), "Square");
        let pm = list_bytes(h, q, "possibleModules", oms);
        let first_possible_module = (pm.len() >= 0x1C).then(|| OrientedModuleSides {
            oriented_module: OrientedModule { module_index: u16::from_le_bytes([pm[0x14], pm[0x15]]), flags: pm[0x16], pad: pm[0x17] },
            ..Default::default()
        });
        let m = ["m0", "m1", "m2", "m3"].map(|f| Matrix4x4 { m: f32s(&field(h, s, f)).try_into().unwrap() });
        let corners = ["c0", "c1", "c2", "c3"].map(|f| {
            let c = field_h(h, s, f);
            (field(h, c, "groundCoverage")[0], f32s(&field(h, c, "angle"))[0])
        });
        let sq = SandSquare {
            m,
            corners,
            voxel_count: field(h, q, "voxelCount")[0],
            possible_module_list_index: i32s(&field(h, q, "possibleModuleListIndex"))[0],
            first_possible_module,
        };
        out.push(Some((sq, v3(pos[0], pos[1], pos[2]))));
    }
    out
}

#[test]
fn sandbanker_matches_the_game() {
    let Some(mut guard) = harness() else { return };
    let h: &mut Harness = &mut guard;
    h.wait_for_object("Placemaker.GridGenerator");
    let master = handle(&ok(h.request(json!({"field": "master", "on": "Placemaker.GridGenerator", "main": true})), "master"), "master");
    for _ in 0..600 {
        if field(h, master, "state")[0] == 43 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    let graph = field_h(h, master, "graph");
    let squares_t = field_h(h, graph, "squares");
    let squares = read_squares(h, squares_t);
    assert!(squares.iter().any(|s| s.is_some()), "the scene has no town");
    let library = module_common::our_module_library();

    // a fresh sandbanker with two fresh meshes, all inactive
    let new_go = |h: &mut Harness, n: &str| {
        let go = handle(&ok(h.request(json!({"new": "UnityEngine.GameObject", "args": [arg::string(n)], "param_types": ["System.String"], "main": true, "keep": true})), n), n);
        call(h, go, "UnityEngine.GameObject", "SetActive", vec![arg::bool(false)]);
        go
    };
    let add = |h: &mut Harness, go: u64, t: &str| {
        let tt = ty(h, t);
        handle(&ok(h.request(json!({"call": "AddComponent", "class": "UnityEngine.GameObject", "handle": go, "args": [{"handle": tt}], "param_types": ["System.Type"], "main": true, "keep": true})), t), t)
    };
    let go = new_go(h, "ts_check sandbanker");
    let sb = add(h, go, "Placemaker.Sandbanker, Assembly-CSharp");
    set_h(h, sb, "master", master);
    let mut meshes = vec![];
    for (f, n) in [("sandMf", "ts_check sand"), ("waterMf", "ts_check water")] {
        let g = new_go(h, n);
        let mf = add(h, g, "UnityEngine.MeshFilter, UnityEngine.CoreModule");
        let mesh = handle(&ok(h.request(json!({"new": "UnityEngine.Mesh", "args": [], "main": true, "keep": true})), "mesh"), "mesh");
        call(h, mesh, "UnityEngine.Mesh", "set_indexFormat", vec![arg::i32(1)]);
        call(h, mf, "UnityEngine.MeshFilter", "set_sharedMesh", vec![json!({"handle": mesh})]);
        set_h(h, sb, f, mf);
        meshes.push((g, mesh));
    }
    let (sand_mesh, water_mesh) = (meshes[0].1, meshes[1].1);

    let mut ours = Sandbanker::default();
    let mut host = Host { squares, budget: 0, read: 0, water: None, sand: None };
    let mut steps = 0;
    loop {
        steps += 1;
        assert!(steps < 5000, "the game's sandbanker never finished");
        let r = call(h, sb, SB, "Iterate", vec![]);
        let game_more = ret::bytes(&r)[0] != 0;
        let game_state = field(h, sb, "state")[0];
        let game_index = i32s(&field(h, sb, "index"))[0];
        host.budget = game_index - ours.index;
        host.read = 0;
        let before = ours.state;
        let more = ours.iterate(&library, &mut host);
        let what = format!("step {steps} ({before:?})");
        assert_eq!((more, ours.state as u8, ours.index), (game_more, game_state, game_index), "{what}: result, state, index");
        if !more {
            break;
        }
        if before == State::Gather && ours.state == State::Gather {
            continue;
        }
        assert_eq!(fbits(&list_bytes(h, sb, "verts", "UnityEngine.Vector3, UnityEngine.CoreModule")), bits3(&ours.verts), "{what}: verts");
        assert_eq!(i32s(&list_bytes(h, sb, "tris", "System.Int32")), ours.tris, "{what}: tris");
        assert_eq!(fbits(&list_bytes(h, sb, "coverage", "System.Single")), ours.coverage.iter().map(|f| f.to_bits()).collect::<Vec<_>>(), "{what}: coverage");
        let mesh_array = |h: &mut Harness, mesh: u64, m: &str| ret::bytes(&ok(h.request(json!({"call": format!("UnityEngine.Mesh.{m}"), "handle": mesh, "args": [], "main": true})), m));
        if before == State::ApplyWater {
            let (v, uv, t) = host.water.take().expect("our water mesh");
            assert_eq!(fbits(&mesh_array(h, water_mesh, "get_vertices")), bits3(&v), "{what}: water vertices");
            // (`Mesh.uv` is stripped: `GetUVs(0, list)`)
            let l = handle(&ok(h.request(json!({"new": list("UnityEngine.Vector2, UnityEngine.CoreModule"), "args": [], "main": true, "keep": true})), "uv list"), "uv list");
            ok(h.request(json!({"call": "GetUVs", "class": "UnityEngine.Mesh", "handle": water_mesh, "args": [arg::i32(0), {"handle": l}], "param_types": ["System.Int32", "System.Collections.Generic.List<UnityEngine.Vector2>"], "main": true})), "GetUVs");
            let got = ret::bytes(&call(h, l, &list("UnityEngine.Vector2, UnityEngine.CoreModule"), "ToArray", vec![]));
            assert_eq!(fbits(&got), bits2(&uv), "{what}: water uv");
            assert_eq!(i32s(&mesh_list(h, water_mesh, "GetTriangles", "System.Int32", "System.Int32", true)), t, "{what}: water triangles");
            assert!(!t.is_empty(), "the town has no shore water");
        }
        if before == State::ApplySand {
            let (v, t, tan) = host.sand.take().expect("our sand mesh");
            assert_eq!(fbits(&mesh_array(h, sand_mesh, "get_vertices")), bits3(&v), "{what}: sand vertices");
            assert_eq!(i32s(&mesh_list(h, sand_mesh, "GetTriangles", "System.Int32", "System.Int32", true)), t, "{what}: sand triangles");
            assert_eq!(fbits(&mesh_list(h, sand_mesh, "GetTangents", "UnityEngine.Vector4, UnityEngine.CoreModule", "UnityEngine.Vector4", false)), bits4(&tan), "{what}: sand tangents");
        }
    }
    assert_eq!(ours.state, State::Done);
    // `_SandUv` from the scene sandbanker's shade rectangle (set by its OnEnable)
    let live = field_h(h, master, "sandbanker");
    let (p, z) = (field(h, live, "uvPos"), field(h, live, "uvSize"));
    let mine = Sandbanker { uv_pos: ByteFloat2 { x: p[0], y: p[1] }, uv_size: ByteFloat2 { x: z[0], y: z[1] }, ..Default::default() };
    let id = ret::i32(&ok(h.request(json!({"call": "UnityEngine.Shader.PropertyToID", "args": [arg::string(SAND_UV)], "main": true})), "id"));
    let g = ret::bytes(&ok(h.request(json!({"call": "UnityEngine.Shader.GetGlobalVector", "args": [arg::i32(id)], "param_types": ["System.Int32"], "main": true})), "_SandUv"));
    assert_eq!(fbits(&g), bits4(&[mine.shade_uvs()]), "_SandUv");
    for o in [go, meshes[0].0, meshes[1].0, sand_mesh, water_mesh] {
        ok(h.request(json!({"call": "UnityEngine.Object.Destroy", "args": [{"handle": o}], "param_types": ["UnityEngine.Object"], "main": true})), "Destroy");
    }
}
