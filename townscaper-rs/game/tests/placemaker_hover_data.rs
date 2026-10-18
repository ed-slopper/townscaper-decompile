//! `ts_game::placemaker::hover_data` (G1a) against the original game. Needs `TS_HARNESS=1` and the install.
//! The graph tests' stand-in town (`graph_common`: random adds, paints and removes, each settled on both sides
//! and compared) with the scene's own town switched off, so the physics scene holds only the stand-in's voxels; a
//! fresh game `HoverData` on the stand-in master and ours are hovered through the scene's main camera at random
//! screen points (every action id twice, so the flood is kept and restarted): every field must match, the
//! source and destination verts byte for byte. Then the flood is iterated to its end on both sides.

mod graph_common;

use std::collections::HashMap;

use graph_common::*;
use serde_json::{json, Value};
use ts_check::{arg, harness, ret};
use ts_game::placemaker::ao_baker::AoBaker;
use ts_game::placemaker::graphs::{Action, Graph, GraphHost, QubeId, VoxelId, VoxelType};
use ts_game::placemaker::grid_generator::{GridBitState, GridGenerator};
use ts_game::placemaker::hover_data::{CameraView, HoverData, VoxelColliders};
use ts_game::placemaker::maker::Maker;
use ts_game::placemaker::modules::ModuleLibrary;
use ts_game::unity::camera::Camera;
use ts_game::unity::geometry::Rect;
use ts_game::unity::quaternion::Matrix4x4;
use ts_game::unity::sse::X;
use ts_game::unity::transform::{Node, Transforms};
use ts_game::unity::mathematics::{f2, Int2};
use ts_game::unity::vector::{Vector2, Vector3};

const T: &str = "UnityEngine.Transform";
const HD: &str = "Placemaker.HoverData";

#[derive(Default)]
struct LoopHost {
    ao: AoBaker,
    maker: Maker,
    propagation_counter: u16,
    last_started_propagation: u16,
}
impl GraphHost for LoopHost {
    fn ao_baker(&mut self) -> Option<&mut AoBaker> {
        Some(&mut self.ao)
    }
    fn maker(&mut self) -> Option<&mut Maker> {
        Some(&mut self.maker)
    }
    fn wfc_qube_updated(&mut self, graph: &mut Graph, qube: QubeId) {
        graph.qube_mut(qube).last_propagation = self.propagation_counter;
    }
    fn wfc_iterate_propagation_count(&mut self) {
        if self.propagation_counter == self.last_started_propagation {
            self.propagation_counter += 4;
        }
    }
    fn qube_removed(&mut self, graph: &mut Graph, qube: QubeId, side_mask: u8) {
        let q = graph.qube_mut(qube);
        q.awaiting_rebuilding = false;
        q.awaiting_grass = false;
        q.last_propagation = 0;
        q.possible_modules.clear();
        let mut mask = side_mask & q.side_variation_mask;
        let s = q.square.unwrap();
        if graph.square(s).qubes.first() == Some(&qube) {
            mask |= 0xF;
        }
        let r = graph.get_qube_relation6(qube, mask);
        for k in 0..6 {
            let rel = r.get(k);
            if rel.index_in_other != -1 {
                let n = rel.qube.unwrap();
                if !graph.qube(n).fully_covered() {
                    graph.qube_mut(n).last_propagation = self.propagation_counter;
                }
            }
        }
        graph.qube_mut(qube).side_variation_mask = 0;
    }
}

fn settle(g: &mut Game, grid: &mut GridGenerator, ours: &mut Graph, host: &mut LoopHost) {
    while ret::bytes(&g.g("Iterate", vec![]))[0] != 0 {}
    while ours.iterate(grid, host) {}
    while ret::bytes(&g.g("IterateFocus", vec![]))[0] != 0 {}
    while ours.iterate_focus(grid, host) {}
}

fn random_action(r: &mut Rng, ours: &Graph, verts: &[Int2], id: i32) -> Action {
    let existing: Vec<(Int2, u8, VoxelType)> = ours
        .corners
        .iter()
        .flat_map(|&c| ours.corner(c).voxels.iter().map(move |&v| (c, v)))
        .map(|(c, v)| (ours.corner(c).hex_pos, ours.voxel(v).height, ours.voxel(v).voxel_type))
        .collect();
    let kind = r.next() % 8;
    if existing.is_empty() || kind < 6 {
        Action::new(r.pick(verts), (r.next() % 6) as u8, VoxelType::Empty, VoxelType::from_i8((r.next() % 15) as i8), id)
    } else {
        let (hex, h, t) = r.pick(&existing);
        let out = if kind < 7 { VoxelType::from_i8((r.next() % 15) as i8) } else { VoxelType::Empty };
        Action::new(hex, h, t, out, id)
    }
}

fn f32s(b: &[u8]) -> Vec<f32> {
    b.chunks(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect()
}
fn hcall(g: &mut Game, t: u64, class: &str, m: &str, args: Vec<Value>) -> u64 {
    handle(&ok(g.h.request(Game::call(t, class, m, args, true)), m), m)
}

/// The game's voxels (by address) as our ids: corners' children in order, as `Game::snapshot` pairs them.
fn voxel_map(g: &mut Game, ours: &Graph) -> HashMap<u64, VoxelId> {
    let corners_t = g.containers["corners"];
    let kids = g.children(&[corners_t]).remove(0);
    assert_eq!(kids.len(), ours.corners.len(), "corners");
    let vk = g.children(&kids);
    let mut out = HashMap::new();
    for (k, &c) in ours.corners.iter().enumerate() {
        let ov = &ours.corner(c).voxels;
        let comps = g.components(&vk[k], "Placemaker.Voxel");
        let addrs = g.addrs(&comps);
        for (a, v) in addrs.into_iter().zip(ov) {
            out.insert(a, *v);
        }
    }
    out
}

#[test]
fn hover_matches_the_game() {
    let Some(mut h) = harness() else { return };
    let (mut g, mut grid, ao) = setup(&mut h);
    let mut ours = Graph { shadow_mesh_index_iterator: (g.ao_skip as i32) * 16383, ..Default::default() };
    ours.ground_collider.size = Vector3 { x: 1.0, y: 1.0, z: 1.0 };
    let mut host = LoopHost { ao, ..Default::default() };
    while ret::bytes(&g.g("IterateRefillPools", vec![]))[0] != 0 {}
    while ours.iterate_refill_pools(&mut || true) {}
    settle(&mut g, &mut grid, &mut ours, &mut host);
    // every other town off (the scene's, and any an earlier run left): their voxels' colliders leave the
    // physics scene
    let corner_ty = handle(&ok(g.h.request(json!({"call": "System.Type.GetType", "args": [arg::string("Placemaker.Graphs.Corner, Assembly-CSharp")], "param_types": ["System.String"], "main": true, "keep": true})), "type"), "type");
    let all = handle(&ok(g.h.request(json!({"call": "UnityEngine.Object.FindObjectsOfType", "args": [{"handle": corner_ty}], "param_types": ["System.Type"], "main": true, "keep": true})), "corners"), "arr");
    let n = ret::i32(&ok(g.h.request(Game::call(all, "System.Array", "get_Length", vec![], false)), "n"));
    let mine = g.containers["corners"];
    let mut off: Vec<u64> = vec![];
    for i in 0..n {
        let c = handle(&ok(g.h.request(json!({"call": "GetValue", "class": "System.Array", "handle": all, "args": [arg::i32(i)], "param_types": ["System.Int32"], "main": true, "keep": true})), "item"), "item");
        let t = hcall(&mut g, c, "UnityEngine.Component", "get_transform", vec![]);
        let Some(p) = opt_handle(&ok(g.h.request(Game::call(t, T, "get_parent", vec![], true)), "parent")) else { continue };
        let is_mine = ret::bytes(&ok(g.h.request(json!({"call": "UnityEngine.Object.op_Equality", "args": [{"handle": p}, {"handle": mine}], "main": true})), "eq"))[0] != 0;
        if !is_mine {
            let pg = hcall(&mut g, p, "UnityEngine.Component", "get_gameObject", vec![]);
            ok(g.h.request(Game::call(pg, "UnityEngine.GameObject", "SetActive", vec![arg::bool(false)], false)), "SetActive");
            off.push(pg);
        }
    }
    // a town round the middle of the grid
    let ps: Vec<Vector3> = grid.bits.iter().filter(|b| b.state == GridBitState::Done).map(|b| b.position).collect();
    let cx = ps.iter().map(|p| p.x).sum::<f32>() / ps.len() as f32;
    let cz = ps.iter().map(|p| p.z).sum::<f32>() / ps.len() as f32;
    let mut r = Rng(31);
    let mut verts: Vec<Int2> = Vec::new();
    for _ in 0..300 {
        let q = grid.get_quad(f2(r.f(cx - 3.0, cx + 3.0), r.f(cz - 3.0, cz + 3.0)));
        if q.is_valid() {
            for v in [q.v0, q.v1, q.v2, q.v3] {
                if !verts.contains(&v) {
                    verts.push(v);
                }
            }
        }
    }
    let steps: usize = std::env::var("TS_HOVER_STEPS").ok().and_then(|s| s.parse().ok()).unwrap_or(120);
    for id in 0..steps as i32 {
        let a = random_action(&mut r, &ours, &verts, id);
        ours.apply_action(&mut grid, &mut host, a, false);
        g.g("ApplyAction", vec![arg::raw(&bytes(&a)), arg::bool(false)]);
        settle(&mut g, &mut grid, &mut ours, &mut host);
    }
    same(&g.snapshot(&ours), &ours_snapshot(&ours), "the town");
    eprintln!("town: {} voxels", ours.total_voxel_count);
    let vmap = voxel_map(&mut g, &ours);
    // the stand-in master behind a fresh HoverData
    let master = handle(&ok(g.h.request(Game::field(g.graph, "master")), "stand-in master"), "master");
    let go = handle(&ok(g.h.request(json!({"new": "UnityEngine.GameObject", "args": [arg::string("ts_check hover")], "param_types": ["System.String"], "main": true, "keep": true})), "go"), "go");
    let hd_ty = handle(&ok(g.h.request(json!({"call": "System.Type.GetType", "args": [arg::string("Placemaker.HoverData, Assembly-CSharp")], "param_types": ["System.String"], "main": true, "keep": true})), "type"), "type");
    let hd = handle(&ok(g.h.request(json!({"call": "AddComponent", "class": "UnityEngine.GameObject", "handle": go, "args": [{"handle": hd_ty}], "param_types": ["System.Type"], "main": true, "keep": true})), "AddComponent"), "hd");
    ok(g.h.request(json!({"field": "master", "handle": hd, "set_handle": master, "main": true})), "master");
    if opt_handle(&ok(g.h.request(json!({"field": "floodVoxels", "handle": hd, "main": true})), "floodVoxels")).is_none() {
        let l = handle(&ok(g.h.request(json!({"new": list("Placemaker.Voxel, Assembly-CSharp"), "args": [], "main": true})), "list"), "list");
        ok(g.h.request(json!({"field": "floodVoxels", "handle": hd, "set_handle": l, "main": true})), "floodVoxels");
    }
    // the main camera, in ours: its transform chain as stored, its projection as the game holds it
    let cam = handle(&ok(g.h.request(json!({"call": "UnityEngine.Camera.get_main", "args": [], "main": true, "keep": true})), "main"), "cam");
    let cam_t = hcall(&mut g, cam, "UnityEngine.Component", "get_transform", vec![]);
    let mut chain = vec![cam_t];
    while let Some(p) = opt_handle(&ok(g.h.request(Game::call(*chain.last().unwrap(), T, "get_parent", vec![], true)), "parent")) {
        chain.push(p);
    }
    let mut tr = Transforms::default();
    let mut parent = None;
    let mut cam_id = 0;
    for &t in chain.iter().rev() {
        let gf = |g: &mut Game, m: &str| f32s(&ret::bytes(&ok(g.h.request(Game::call(t, T, m, vec![], false)), m)));
        let (p, q, s) = (gf(&mut g, "get_localPosition"), gf(&mut g, "get_localRotation"), gf(&mut g, "get_localScale"));
        cam_id = tr.add(Node { t: X::from_f([p[0], p[1], p[2], 0.0]), r: X::from_f([q[0], q[1], q[2], q[3]]), s: X::from_f([s[0], s[1], s[2], 0.0]), parent });
        parent = Some(cam_id);
    }
    let pm = f32s(&ret::bytes(&ok(g.h.request(Game::call(cam, "UnityEngine.Camera", "get_projectionMatrix", vec![], false)), "projection")));
    let near = f32s(&ret::bytes(&ok(g.h.request(Game::call(cam, "UnityEngine.Camera", "get_nearClipPlane", vec![], false)), "near")))[0];
    let rr = ok(g.h.request(json!({"icall": "UnityEngine.Camera::get_rect_Injected(UnityEngine.Rect&)", "args": [{"handle": cam}, {"buf": "00".repeat(16)}], "main": true})), "rect");
    let rs = rr["bufs_after"][0].as_str().unwrap();
    let rb: Vec<u8> = (0..rs.len()).step_by(2).map(|i| u8::from_str_radix(&rs[i..i + 2], 16).unwrap()).collect();
    let rf = f32s(&rb);
    let camera = Camera {
        transform: cam_id,
        field_of_view: 60.0,
        aspect: 1.0,
        near,
        far: 1000.0,
        rect: Rect { x: rf[0], y: rf[1], width: rf[2], height: rf[3] },
        projection: Some(Matrix4x4 { m: pm.try_into().unwrap() }),
    };
    let sw = ret::i32(&ok(g.h.request(json!({"call": "UnityEngine.Screen.get_width", "args": [], "main": true})), "w")) as f32;
    let sh = ret::i32(&ok(g.h.request(json!({"call": "UnityEngine.Screen.get_height", "args": [], "main": true})), "h")) as f32;
    let view = CameraView { camera: &camera, transforms: &tr, screen: Vector2 { x: sw, y: sh } };
    let colliders = VoxelColliders::build(&ours);
    let mut hover = HoverData::default();
    let lib = ModuleLibrary::default();
    let (mut on_voxel, mut on_ground, mut adds, mut sides, mut exact_hits) = (0, 0, 0, 0, 0);
    let hovers: usize = std::env::var("TS_HOVER_POINTS").ok().and_then(|s| s.parse().ok()).unwrap_or(300);
    for k in 0..hovers {
        let id = (k / 2) as i32;
        // half at random points, half at a random voxel (through our camera) with a little jitter
        let pos = if k % 2 == 0 || colliders.voxels.is_empty() {
            Vector2 { x: r.f(0.0, sw), y: r.f(0.0, sh) }
        } else {
            let c = &colliders.colliders[(r.next() as usize) % colliders.colliders.len()];
            let at = c.pose.p.add(Vector3 { x: r.f(-0.6, 0.6), y: r.f(-0.5, 0.5), z: r.f(-0.6, 0.6) });
            let s = camera.world_to_screen_point(&tr, view.screen, at);
            Vector2 { x: s.x, y: s.y }
        };
        // (the camera's pose is read in the same frame as the hover)
        let fields = ["voxel", "srcVert", "dstVert", "srcHeight", "dstHeight", "lastHash", "ray", "pointerHitPos", "pointerHitDir", "side", "occupied", "insideBorders", "valid", "validPaint", "validAdd", "validRemove", "validBucket", "floodIndex"];
        let mut reqs = vec![json!({"call": "SetHover", "class": HD, "handle": hd, "args": [arg::raw(&bytes(&pos)), arg::i32(id)], "main": true})];
        reqs.extend(fields.iter().map(|f| json!({"field": f, "handle": hd, "main": true})));
        let res = g.batch(reqs, "SetHover");
        for x in &res {
            ok(x.clone(), "SetHover batch");
        }
        hover.set_hover(&ours, &mut grid, &colliders, Some(&view), pos, id);
        let what = format!("hover {k} at {pos:?} (id {id})");
        let f = |name: &str| ret::bytes(&res[1 + fields.iter().position(|x| *x == name).unwrap()]);
        let gv = opt_handle(&res[1]).map(|v| ok(g.h.request(json!({"addr": v})), "addr")["addr"].as_u64().unwrap());
        assert_eq!(gv.map(|a| vmap.get(&a).copied().unwrap_or(VoxelId(u32::MAX))), hover.voxel, "{what}: voxel");
        assert_eq!(f("srcVert"), bytes(&hover.src_vert), "{what}: srcVert");
        assert_eq!(f("dstVert"), bytes(&hover.dst_vert), "{what}: dstVert");
        let int = |name: &str| i32s(&f(name))[0];
        assert_eq!(int("srcHeight"), hover.src_height, "{what}: srcHeight");
        assert_eq!(int("dstHeight"), hover.dst_height, "{what}: dstHeight");
        assert_eq!(int("lastHash"), hover.last_hash, "{what}: lastHash");
        assert_eq!(f("ray"), bytes(&hover.ray), "{what}: ray");
        // (the hit point and normal through our stand-in hull planes: the last ulp may differ, see unity_physics.rs)
        let (gp, gn) = (f32s(&f("pointerHitPos")), f32s(&f("pointerHitDir")));
        let (op, on) = (hover.pointer_hit_pos, hover.pointer_hit_dir);
        let close = |a: &[f32], b: Vector3| (a[0] - b.x).abs().max((a[1] - b.y).abs()).max((a[2] - b.z).abs()) <= 1e-5;
        assert!(close(&gp, op) && close(&gn, on), "{what}: pointerHitPos {gp:?} vs {op:?}, pointerHitDir {gn:?} vs {on:?}");
        if f("pointerHitPos") == bytes(&op) && f("pointerHitDir") == bytes(&on) {
            exact_hits += 1;
        }
        assert_eq!(f("side")[0] as i8, hover.side, "{what}: side");
        let flags = ["occupied", "insideBorders", "valid", "validPaint", "validAdd", "validRemove", "validBucket"].map(|n| f(n)[0] != 0);
        let ours_flags = [hover.occupied, hover.inside_borders, hover.valid, hover.valid_paint, hover.valid_add, hover.valid_remove, hover.valid_bucket];
        assert_eq!(flags, ours_flags, "{what}: occupied, insideBorders, valid, validPaint, validAdd, validRemove, validBucket");
        assert_eq!(int("floodIndex"), hover.flood_index, "{what}: floodIndex");
        if !hover.vertical() {
            sides += 1;
        }
        if hover.voxel.is_some() {
            on_voxel += 1;
        } else if hover.src_vert.valid_position() {
            on_ground += 1;
        }
        if hover.valid_add {
            adds += 1;
        }
    }
    eprintln!("{hovers} hovers: {on_voxel} on voxels ({sides} on a side), {on_ground} on the ground, {adds} could add; hit point and normal bit-exact on {exact_hits}");
    // the flood (the stand-in's qubes are uncollapsed: the voxel alone)
    let mut n = 0;
    while ret::bytes(&ok(g.h.request(json!({"call": "Iterate", "class": HD, "handle": hd, "args": [], "main": true})), "Iterate"))[0] != 0 {
        n += 1;
    }
    let mut m = 0;
    while hover.iterate(&ours, &lib) {
        m += 1;
    }
    assert_eq!(n, m, "flood steps");
    // the stand-in town off for later tests, the others back
    let mg = hcall(&mut g, mine, "UnityEngine.Component", "get_gameObject", vec![]);
    ok(g.h.request(Game::call(mg, "UnityEngine.GameObject", "SetActive", vec![arg::bool(false)], false)), "SetActive");
    for pg in off {
        ok(g.h.request(Game::call(pg, "UnityEngine.GameObject", "SetActive", vec![arg::bool(true)], false)), "SetActive");
    }
}
