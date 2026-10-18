//! `ts_game::placemaker::hover_hightlight` (G1f) against the original game. Needs `TS_HARNESS=1` and the install.
//! On the stand-in town (`hover_common`), every qube is given the same collapsed module on both sides (a random
//! module with a house-material mesh, a random orientation: `possibleModules = [it]`, standing in for the
//! collapse, which `HoverHightlight` only reads); then both sides hover at the same screen points and a fresh
//! game `HoverHightlight` on the stand-in master and ours draw: the `_HoverPos` global and the whole mesh
//! (vertices, normals, both uv sets, triangles) must match to the bit.

mod graph_common;
mod hover_common;
mod module_common;

use std::collections::HashMap;

use graph_common::*;
use hover_common::*;
use serde_json::{json, Value};
use ts_check::{arg, harness, ret};
use ts_game::placemaker::graphs::{Graph, OrientedModuleSides};
use ts_game::placemaker::hover_data::{CameraView, HoverData, VoxelColliders};
use ts_game::placemaker::hover_hightlight::{HighlightContext, HoverHightlight};
use ts_game::placemaker::modules::{AssetRef, OrientedModule};
use ts_game::unity::vector::{Vector2, Vector3};

const V3T: &str = "UnityEngine.Vector3, UnityEngine.CoreModule";
const V2T: &str = "UnityEngine.Vector2, UnityEngine.CoreModule";
const HH: &str = "Placemaker.HoverHightlight";

fn list_items(g: &mut Game, obj: u64, f: &str, elem: &str) -> Vec<u64> {
    let l = handle(&ok(g.h.request(Game::field(obj, f)), f), f);
    let n = ret::i32(&ok(g.h.request(Game::call(l, &list(elem), "get_Count", vec![], false)), "count"));
    let reqs: Vec<Value> = (0..n).map(|i| Game::call(l, &list(elem), "get_Item", vec![arg::i32(i)], true)).collect();
    g.batch(reqs, "get_Item").iter().map(|r| handle(r, "item")).collect()
}

#[test]
fn highlight_matches_the_game() {
    let Some(mut h) = harness() else { return };
    let (mut g, mut grid, ao) = setup(&mut h);
    let mut lib = module_common::our_module_library();
    lib.setup();
    let mut ours = Graph { shadow_mesh_index_iterator: (g.ao_skip as i32) * 16383, ..Default::default() };
    ours.ground_collider.size = Vector3 { x: 1.0, y: 1.0, z: 1.0 };
    let mut host = LoopHost { ao, ..Default::default() };
    let mut r = Rng(19);
    let s = prepare(&mut g, &mut grid, &mut ours, &mut host, &mut r);
    grow(&mut g, &mut grid, &mut ours, &mut host, &mut r, &s.verts.clone(), 50);
    eprintln!("town: {} voxels", ours.total_voxel_count);
    // the house material in our asset refs (the scene's module meshes' materials, matched by address)
    let scene_lib = handle(&ok(g.h.request(json!({"field": "moduleLibrary", "on": "Placemaker.WorldMaster", "main": true})), "moduleLibrary"), "moduleLibrary");
    let meshes = list_items(&mut g, scene_lib, "moduleMeshes", "Placemaker.ModuleMesh, Assembly-CSharp");
    let mats: Vec<u64> = g.batch(meshes.iter().map(|&m| Game::field(m, "material")).collect(), "material").iter().map(|r| handle(r, "material")).collect();
    let mat_of: HashMap<u64, AssetRef> = g.addrs(&mats).into_iter().zip(&lib.module_meshes).map(|(a, m)| (a, m.material)).collect();
    let mm = handle(&ok(g.h.request(json!({"field": "materialMaster", "on": "Placemaker.WorldMaster", "main": true})), "materialMaster"), "materialMaster");
    let house_h = handle(&ok(g.h.request(Game::field(mm, "houseMaterial")), "houseMaterial"), "houseMaterial");
    let house = mat_of[&g.addrs(&[house_h])[0]];
    // modules with a house mesh
    let candidates: Vec<u16> = (0..lib.modules.len())
        .filter(|&i| lib.modules[i].base.oriented_module_meshes.iter().any(|m| lib.module_meshes[m.module_index as usize].material == house))
        .map(|i| i as u16)
        .collect();
    eprintln!("{} modules with a house mesh", candidates.len());
    // every qube gets one: ours and the game's (the squares' children in order, as Game::snapshot pairs them)
    let squares_t = g.containers["squares"];
    let sq_ts = g.children(&[squares_t]).remove(0);
    assert_eq!(sq_ts.len(), ours.squares.len(), "squares");
    let qube_ts = g.children(&sq_ts);
    let oms_list = list("Placemaker.Graphs.OrientedModuleSides, Assembly-CSharp");
    let mut set_reqs = vec![];
    let mut qube_comps = vec![];
    for (k, &sq) in ours.squares.clone().iter().enumerate() {
        let comps = g.components(&qube_ts[k], "Placemaker.Graphs.Qube");
        for (j, &q) in ours.square(sq).qubes.clone().iter().enumerate() {
            let oms = OrientedModuleSides {
                oriented_module: OrientedModule { module_index: r.pick(&candidates), flags: (r.next() % 8) as u8, pad: 0 },
                ..Default::default()
            };
            ours.qube_mut(q).possible_modules = vec![oms];
            qube_comps.push((comps[j], oms));
        }
    }
    for &(c, oms) in &qube_comps {
        let l = handle(&ok(g.h.request(Game::field(c, "possibleModules")), "possibleModules"), "list");
        set_reqs.push(Game::call(l, &oms_list, "Clear", vec![], false));
        set_reqs.push(Game::call(l, &oms_list, "Add", vec![arg::raw(&bytes(&oms))], false));
    }
    for chunk in set_reqs.chunks(400) {
        for x in g.batch(chunk.to_vec(), "possibleModules") {
            ok(x, "possibleModules");
        }
    }
    eprintln!("{} qubes given a module", qube_comps.len());
    // a fresh HoverHightlight on the stand-in master, with a mesh of its own
    let go = handle(&ok(g.h.request(json!({"new": "UnityEngine.GameObject", "args": [arg::string("ts_check highlight")], "param_types": ["System.String"], "main": true, "keep": true})), "go"), "go");
    let hh_ty = handle(&ok(g.h.request(json!({"call": "System.Type.GetType", "args": [arg::string("Placemaker.HoverHightlight, Assembly-CSharp")], "param_types": ["System.String"], "main": true, "keep": true})), "type"), "type");
    let hh = handle(&ok(g.h.request(json!({"call": "AddComponent", "class": "UnityEngine.GameObject", "handle": go, "args": [{"handle": hh_ty}], "param_types": ["System.Type"], "main": true, "keep": true})), "AddComponent"), "hh");
    ok(g.h.request(json!({"field": "master", "handle": hh, "set_handle": s.master, "main": true})), "master");
    let mesh = handle(&ok(g.h.request(json!({"new": "UnityEngine.Mesh", "args": [], "main": true})), "mesh"), "mesh");
    ok(g.h.request(json!({"field": "mesh", "handle": hh, "set_handle": mesh, "main": true})), "mesh");
    let new_list = |g: &mut Game, t: &str| handle(&ok(g.h.request(json!({"new": list(t), "args": [], "main": true})), "list"), "list");
    let (lv, ln, lu, lu2, lt) = (new_list(&mut g, V3T), new_list(&mut g, V3T), new_list(&mut g, V2T), new_list(&mut g, V2T), new_list(&mut g, "System.Int32"));
    let view = CameraView { camera: &s.camera, transforms: &s.tr, screen: s.screen };
    let colliders = VoxelColliders::build(&ours);
    let vmap = voxel_map(&mut g, &ours);
    let cx = HighlightContext { graph: &ours, lib: &lib, house_material: house };
    let mut hover = HoverData::default();
    let mut ours_hh = HoverHightlight::default();
    let (mut drawn, mut empty, mut verts_total) = (0, 0, 0usize);
    let hovers: usize = std::env::var("TS_HIGHLIGHT_POINTS").ok().and_then(|s| s.parse().ok()).unwrap_or(200);
    for k in 0..hovers {
        let pos = if k % 3 == 0 || colliders.voxels.is_empty() {
            Vector2 { x: r.f(0.0, s.screen.x), y: r.f(0.0, s.screen.y) }
        } else {
            let c = &colliders.colliders[(r.next() as usize) % colliders.colliders.len()];
            let at = c.pose.p.add(Vector3 { x: r.f(-0.6, 0.6), y: r.f(-0.5, 0.5), z: r.f(-0.6, 0.6) });
            let p = s.camera.world_to_screen_point(&s.tr, s.screen, at);
            Vector2 { x: p.x, y: p.y }
        };
        let id = k as i32;
        let res = game_hover(&mut g, s.hd, pos, id);
        hover.set_hover(&ours, &mut grid, &colliders, Some(&view), pos, id);
        let what = format!("hover {k} at {pos:?}");
        same_hover(&mut g, &res, &hover, &vmap, &what);
        let frame = ret::i32(&ok(g.h.request(json!({"call": "UnityEngine.Time.get_frameCount", "args": [], "main": true})), "frame"));
        let mc = "UnityEngine.Mesh";
        let lp = |t: &str| format!("System.Collections.Generic.List<{t}>");
        let reqs = vec![
            json!({"call": "Draw", "class": HH, "handle": hh, "args": [], "main": true}),
            json!({"call": "UnityEngine.Time.get_frameCount", "args": [], "main": true}),
            json!({"call": "GetVertices", "class": mc, "handle": mesh, "args": [{"handle": lv}], "param_types": [lp("UnityEngine.Vector3")], "main": true}),
            json!({"call": "GetNormals", "class": mc, "handle": mesh, "args": [{"handle": ln}], "param_types": [lp("UnityEngine.Vector3")], "main": true}),
            json!({"call": "GetUVs", "class": mc, "handle": mesh, "args": [arg::i32(0), {"handle": lu}], "param_types": ["System.Int32", lp("UnityEngine.Vector2")], "main": true}),
            json!({"call": "GetUVs", "class": mc, "handle": mesh, "args": [arg::i32(1), {"handle": lu2}], "param_types": ["System.Int32", lp("UnityEngine.Vector2")], "main": true}),
            json!({"call": "GetTriangles", "class": mc, "handle": mesh, "args": [{"handle": lt}, arg::i32(0)], "param_types": [lp("System.Int32"), "System.Int32"], "main": true}),
            Game::call(lv, &list(V3T), "ToArray", vec![], false),
            Game::call(ln, &list(V3T), "ToArray", vec![], false),
            Game::call(lu, &list(V2T), "ToArray", vec![], false),
            Game::call(lu2, &list(V2T), "ToArray", vec![], false),
            Game::call(lt, &list("System.Int32"), "ToArray", vec![], false),
        ];
        let out = g.batch(reqs, "Draw");
        let (global, m) = ours_hh.draw(&cx, &hover, frame);
        let m = m.unwrap_or_default();
        if m.verts.is_empty() {
            empty += 1;
        } else {
            drawn += 1;
            verts_total += m.verts.len();
        }
        let _ = (global, &out[1]);
        let fl3 = |v: &[Vector3]| v.iter().flat_map(|p| [p.x, p.y, p.z]).flat_map(|f| f.to_le_bytes()).collect::<Vec<u8>>();
        let fl2 = |v: &[ts_game::unity::mathematics::Float2]| v.iter().flat_map(|p| [p.x, p.y]).flat_map(|f| f.to_le_bytes()).collect::<Vec<u8>>();
        assert_eq!(ret::bytes(&out[7]), fl3(&m.verts), "{what}: vertices ({} ours)", m.verts.len());
        assert_eq!(ret::bytes(&out[8]), fl3(&m.normals), "{what}: normals");
        assert_eq!(ret::bytes(&out[9]), fl2(&m.uvs), "{what}: uvs");
        assert_eq!(ret::bytes(&out[10]), fl2(&m.uv2s), "{what}: uv2s");
        assert_eq!(ret::bytes(&out[11]), m.tris.iter().flat_map(|t| t.to_le_bytes()).collect::<Vec<u8>>(), "{what}: triangles");
    }
    eprintln!("{hovers} draws: {drawn} with a mesh ({verts_total} vertices), {empty} empty");
    restore(&mut g, &s);
}
