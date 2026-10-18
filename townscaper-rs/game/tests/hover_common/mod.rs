//! What the picking and building tests (G1) share: the graph tests' stand-in town grown by random actions on
//! both sides, every other town in the scene switched off (so the physics scene holds only the stand-in's
//! voxels), a fresh game `HoverData` on the stand-in master, the scene's main camera mirrored in ours, and the
//! hover compared field by field.

#![allow(dead_code)]

use std::collections::HashMap;

use crate::graph_common::*;
use serde_json::{json, Value};
use ts_check::{arg, ret};
use ts_game::placemaker::ao_baker::AoBaker;
use ts_game::placemaker::graphs::{Action, Graph, GraphHost, QubeId, VoxelId, VoxelType};
use ts_game::placemaker::grid_generator::{GridBitState, GridGenerator};
use ts_game::placemaker::hover_data::HoverData;
use ts_game::placemaker::maker::Maker;
use ts_game::unity::camera::Camera;
use ts_game::unity::geometry::Rect;
use ts_game::unity::mathematics::{f2, Int2};
use ts_game::unity::quaternion::Matrix4x4;
use ts_game::unity::sse::X;
use ts_game::unity::transform::{Node, Transforms};
use ts_game::unity::vector::{Vector2, Vector3};

pub const T: &str = "UnityEngine.Transform";
pub const HD: &str = "Placemaker.HoverData";

/// Our side of the systems round the stand-in graph (as in placemaker_graph_loop.rs).
#[derive(Default)]
pub struct LoopHost {
    pub ao: AoBaker,
    pub maker: Maker,
    pub propagation_counter: u16,
    pub last_started_propagation: u16,
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

/// Both sides' `Iterate`, then `IterateFocus`, until each says it is done.
pub fn settle(g: &mut Game, grid: &mut GridGenerator, ours: &mut Graph, host: &mut LoopHost) {
    while ret::bytes(&g.g("Iterate", vec![]))[0] != 0 {}
    while ours.iterate(grid, host) {}
    while ret::bytes(&g.g("IterateFocus", vec![]))[0] != 0 {}
    while ours.iterate_focus(grid, host) {}
}

pub fn random_action(r: &mut Rng, ours: &Graph, verts: &[Int2], id: i32) -> Action {
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

pub fn f32s(b: &[u8]) -> Vec<f32> {
    b.chunks(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect()
}
pub fn hcall(g: &mut Game, t: u64, class: &str, m: &str, args: Vec<Value>) -> u64 {
    handle(&ok(g.h.request(Game::call(t, class, m, args, true)), m), m)
}

/// The game's voxels (by address) as our ids: corners' children in order, as `Game::snapshot` pairs them.
pub fn voxel_map(g: &mut Game, ours: &Graph) -> HashMap<u64, VoxelId> {
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

/// The stand-in town, the game's HoverData and the camera.
pub struct Scene {
    pub master: u64,
    pub hd: u64,
    pub camera: Camera,
    pub tr: Transforms,
    pub screen: Vector2,
    /// The other towns' corner containers switched off.
    pub off: Vec<u64>,
    pub verts: Vec<Int2>,
}

/// Sets the stand-in up: pools, an empty settle, every other town off, the verts of a patch round the grid's
/// middle, a fresh HoverData on the stand-in master (also `master.hoverData`), the camera mirrored.
pub fn prepare(g: &mut Game, grid: &mut GridGenerator, ours: &mut Graph, host: &mut LoopHost, r: &mut Rng) -> Scene {
    while ret::bytes(&g.g("IterateRefillPools", vec![]))[0] != 0 {}
    while ours.iterate_refill_pools(&mut || true) {}
    settle(g, grid, ours, host);
    let corner_ty = handle(&ok(g.h.request(json!({"call": "System.Type.GetType", "args": [arg::string("Placemaker.Graphs.Corner, Assembly-CSharp")], "param_types": ["System.String"], "main": true, "keep": true})), "type"), "type");
    let all = handle(&ok(g.h.request(json!({"call": "UnityEngine.Object.FindObjectsOfType", "args": [{"handle": corner_ty}], "param_types": ["System.Type"], "main": true, "keep": true})), "corners"), "arr");
    let n = ret::i32(&ok(g.h.request(Game::call(all, "System.Array", "get_Length", vec![], false)), "n"));
    let mine = g.containers["corners"];
    let mut off: Vec<u64> = vec![];
    for i in 0..n {
        let c = handle(&ok(g.h.request(json!({"call": "GetValue", "class": "System.Array", "handle": all, "args": [arg::i32(i)], "param_types": ["System.Int32"], "main": true, "keep": true})), "item"), "item");
        let t = hcall(g, c, "UnityEngine.Component", "get_transform", vec![]);
        let Some(p) = opt_handle(&ok(g.h.request(Game::call(t, T, "get_parent", vec![], true)), "parent")) else { continue };
        let is_mine = ret::bytes(&ok(g.h.request(json!({"call": "UnityEngine.Object.op_Equality", "args": [{"handle": p}, {"handle": mine}], "main": true})), "eq"))[0] != 0;
        if !is_mine {
            let pg = hcall(g, p, "UnityEngine.Component", "get_gameObject", vec![]);
            ok(g.h.request(Game::call(pg, "UnityEngine.GameObject", "SetActive", vec![arg::bool(false)], false)), "SetActive");
            off.push(pg);
        }
    }
    let ps: Vec<Vector3> = grid.bits.iter().filter(|b| b.state == GridBitState::Done).map(|b| b.position).collect();
    let cx = ps.iter().map(|p| p.x).sum::<f32>() / ps.len() as f32;
    let cz = ps.iter().map(|p| p.z).sum::<f32>() / ps.len() as f32;
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
    let master = handle(&ok(g.h.request(Game::field(g.graph, "master")), "stand-in master"), "master");
    let go = handle(&ok(g.h.request(json!({"new": "UnityEngine.GameObject", "args": [arg::string("ts_check hover")], "param_types": ["System.String"], "main": true, "keep": true})), "go"), "go");
    let hd_ty = handle(&ok(g.h.request(json!({"call": "System.Type.GetType", "args": [arg::string("Placemaker.HoverData, Assembly-CSharp")], "param_types": ["System.String"], "main": true, "keep": true})), "type"), "type");
    let hd = handle(&ok(g.h.request(json!({"call": "AddComponent", "class": "UnityEngine.GameObject", "handle": go, "args": [{"handle": hd_ty}], "param_types": ["System.Type"], "main": true, "keep": true})), "AddComponent"), "hd");
    ok(g.h.request(json!({"field": "master", "handle": hd, "set_handle": master, "main": true})), "master");
    ok(g.h.request(json!({"field": "hoverData", "handle": master, "set_handle": hd, "main": true})), "hoverData");
    if opt_handle(&ok(g.h.request(json!({"field": "floodVoxels", "handle": hd, "main": true})), "floodVoxels")).is_none() {
        let l = handle(&ok(g.h.request(json!({"new": list("Placemaker.Voxel, Assembly-CSharp"), "args": [], "main": true})), "list"), "list");
        ok(g.h.request(json!({"field": "floodVoxels", "handle": hd, "set_handle": l, "main": true})), "floodVoxels");
    }
    // the main camera: its transform chain as stored, its projection as the game holds it
    let cam = handle(&ok(g.h.request(json!({"call": "UnityEngine.Camera.get_main", "args": [], "main": true, "keep": true})), "main"), "cam");
    let cam_t = hcall(g, cam, "UnityEngine.Component", "get_transform", vec![]);
    let mut chain = vec![cam_t];
    while let Some(p) = opt_handle(&ok(g.h.request(Game::call(*chain.last().unwrap(), T, "get_parent", vec![], true)), "parent")) {
        chain.push(p);
    }
    let mut tr = Transforms::default();
    let mut parent = None;
    let mut cam_id = 0;
    for &t in chain.iter().rev() {
        let gf = |g: &mut Game, m: &str| f32s(&ret::bytes(&ok(g.h.request(Game::call(t, T, m, vec![], false)), m)));
        let (p, q, s) = (gf(g, "get_localPosition"), gf(g, "get_localRotation"), gf(g, "get_localScale"));
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
    Scene { master, hd, camera, tr, screen: Vector2 { x: sw, y: sh }, off, verts }
}

/// The stand-in town grown by `steps` random actions on both sides (each settled), compared.
pub fn grow(g: &mut Game, grid: &mut GridGenerator, ours: &mut Graph, host: &mut LoopHost, r: &mut Rng, verts: &[Int2], steps: usize) {
    for id in 0..steps as i32 {
        let a = random_action(r, ours, verts, id);
        ours.apply_action(grid, host, a, false);
        g.g("ApplyAction", vec![arg::raw(&bytes(&a)), arg::bool(false)]);
        settle(g, grid, ours, host);
    }
    same(&g.snapshot(ours), &ours_snapshot(ours), "the town");
}

/// The stand-in town off, the others back (for later tests in the same game).
pub fn restore(g: &mut Game, s: &Scene) {
    let mine = g.containers["corners"];
    let mg = hcall(g, mine, "UnityEngine.Component", "get_gameObject", vec![]);
    ok(g.h.request(Game::call(mg, "UnityEngine.GameObject", "SetActive", vec![arg::bool(false)], false)), "SetActive");
    for &pg in &s.off {
        ok(g.h.request(Game::call(pg, "UnityEngine.GameObject", "SetActive", vec![arg::bool(true)], false)), "SetActive");
    }
}

/// The fields a hover compares.
pub const FIELDS: [&str; 18] = ["voxel", "srcVert", "dstVert", "srcHeight", "dstHeight", "lastHash", "ray", "pointerHitPos", "pointerHitDir", "side", "occupied", "insideBorders", "valid", "validPaint", "validAdd", "validRemove", "validBucket", "floodIndex"];

/// The game's hover (the replies to `FIELDS` reads, in order) against ours. The hit point and normal go through
/// our stand-in hull planes (row E12), so they are compared to 1e-5; true when they are bit-exact too.
pub fn same_hover(g: &mut Game, res: &[Value], hover: &HoverData, vmap: &HashMap<u64, VoxelId>, what: &str) -> bool {
    let f = |name: &str| ret::bytes(&res[FIELDS.iter().position(|x| *x == name).unwrap()]);
    assert_eq!(f32s(&f("ray")), [hover.ray.origin.x, hover.ray.origin.y, hover.ray.origin.z, hover.ray.direction.x, hover.ray.direction.y, hover.ray.direction.z], "{what}: ray (did the camera move?)");
    let gv = opt_handle(&res[0]).map(|v| ok(g.h.request(json!({"addr": v})), "addr")["addr"].as_u64().unwrap());
    assert_eq!(gv.map(|a| vmap.get(&a).copied().unwrap_or(VoxelId(u32::MAX))), hover.voxel, "{what}: voxel");
    assert_eq!(f("srcVert"), bytes(&hover.src_vert), "{what}: srcVert");
    assert_eq!(f("dstVert"), bytes(&hover.dst_vert), "{what}: dstVert");
    let int = |name: &str| i32s(&f(name))[0];
    assert_eq!(int("srcHeight"), hover.src_height, "{what}: srcHeight");
    assert_eq!(int("dstHeight"), hover.dst_height, "{what}: dstHeight");
    assert_eq!(int("lastHash"), hover.last_hash, "{what}: lastHash");
    assert_eq!(f("ray"), bytes(&hover.ray), "{what}: ray");
    let (gp, gn) = (f32s(&f("pointerHitPos")), f32s(&f("pointerHitDir")));
    let (op, on) = (hover.pointer_hit_pos, hover.pointer_hit_dir);
    let close = |a: &[f32], b: Vector3| (a[0] - b.x).abs().max((a[1] - b.y).abs()).max((a[2] - b.z).abs()) <= 1e-5;
    assert!(close(&gp, op) && close(&gn, on), "{what}: pointerHitPos {gp:?} vs {op:?}, pointerHitDir {gn:?} vs {on:?}");
    assert_eq!(f("side")[0] as i8, hover.side, "{what}: side");
    let flags = ["occupied", "insideBorders", "valid", "validPaint", "validAdd", "validRemove", "validBucket"].map(|n| f(n)[0] != 0);
    let ours_flags = [hover.occupied, hover.inside_borders, hover.valid, hover.valid_paint, hover.valid_add, hover.valid_remove, hover.valid_bucket];
    assert_eq!(flags, ours_flags, "{what}: occupied, insideBorders, valid, validPaint, validAdd, validRemove, validBucket");
    assert_eq!(int("floodIndex"), hover.flood_index, "{what}: floodIndex");
    f("pointerHitPos") == bytes(&op) && f("pointerHitDir") == bytes(&on)
}

/// `SetHover(pos, id)` in the game with the fields read in the same frame.
pub fn game_hover(g: &mut Game, hd: u64, pos: Vector2, id: i32) -> Vec<Value> {
    let mut reqs = vec![json!({"call": "SetHover", "class": HD, "handle": hd, "args": [arg::raw(&bytes(&pos)), arg::i32(id)], "main": true})];
    reqs.extend(FIELDS.iter().map(|f| json!({"field": f, "handle": hd, "main": true})));
    let res = g.batch(reqs, "SetHover");
    for x in &res {
        ok(x.clone(), "SetHover batch");
    }
    res[1..].to_vec()
}
