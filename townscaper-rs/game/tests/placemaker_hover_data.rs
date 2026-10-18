//! `ts_game::placemaker::hover_data` (G1a) against the original game. Needs `TS_HARNESS=1` and the install.
//! The graph tests' stand-in town (random adds, paints and removes, each settled on both sides and compared)
//! with every other town switched off (`hover_common`); a fresh game `HoverData` on the stand-in master and ours
//! are hovered through the scene's main camera at random screen points, half of them aimed at voxels (every
//! action id twice, so the flood is kept and restarted): every field must match, the source and destination
//! verts byte for byte. Then the flood is iterated to its end on both sides.

mod graph_common;
mod hover_common;

use graph_common::*;
use hover_common::*;
use serde_json::json;
use ts_check::{harness, ret};
use ts_game::placemaker::graphs::Graph;
use ts_game::placemaker::hover_data::{CameraView, HoverData, VoxelColliders};
use ts_game::placemaker::modules::ModuleLibrary;
use ts_game::unity::vector::{Vector2, Vector3};

#[test]
fn hover_matches_the_game() {
    let Some(mut h) = harness() else { return };
    let (mut g, mut grid, ao) = setup(&mut h);
    let mut ours = Graph { shadow_mesh_index_iterator: (g.ao_skip as i32) * 16383, ..Default::default() };
    ours.ground_collider.size = Vector3 { x: 1.0, y: 1.0, z: 1.0 };
    let mut host = LoopHost { ao, ..Default::default() };
    let mut r = Rng(31);
    let s = prepare(&mut g, &mut grid, &mut ours, &mut host, &mut r);
    let steps: usize = std::env::var("TS_HOVER_STEPS").ok().and_then(|s| s.parse().ok()).unwrap_or(120);
    grow(&mut g, &mut grid, &mut ours, &mut host, &mut r, &s.verts.clone(), steps);
    eprintln!("town: {} voxels", ours.total_voxel_count);
    let vmap = voxel_map(&mut g, &ours);
    let view = CameraView { camera: &s.camera, transforms: &s.tr, screen: s.screen };
    let colliders = VoxelColliders::build(&ours);
    let mut hover = HoverData::default();
    let lib = ModuleLibrary::default();
    let (mut on_voxel, mut on_ground, mut adds, mut sides, mut exact_hits) = (0, 0, 0, 0, 0);
    let hovers: usize = std::env::var("TS_HOVER_POINTS").ok().and_then(|s| s.parse().ok()).unwrap_or(300);
    for k in 0..hovers {
        let id = (k / 2) as i32;
        // half at random points, half at a random voxel (through our camera) with a little jitter
        let pos = if k % 2 == 0 || colliders.voxels.is_empty() {
            Vector2 { x: r.f(0.0, s.screen.x), y: r.f(0.0, s.screen.y) }
        } else {
            let c = &colliders.colliders[(r.next() as usize) % colliders.colliders.len()];
            let at = c.pose.p.add(Vector3 { x: r.f(-0.6, 0.6), y: r.f(-0.5, 0.5), z: r.f(-0.6, 0.6) });
            let p = s.camera.world_to_screen_point(&s.tr, s.screen, at);
            Vector2 { x: p.x, y: p.y }
        };
        let res = game_hover(&mut g, s.hd, pos, id);
        hover.set_hover(&ours, &mut grid, &colliders, Some(&view), pos, id);
        if same_hover(&mut g, &res, &hover, &vmap, &format!("hover {k} at {pos:?} (id {id})")) {
            exact_hits += 1;
        }
        if hover.voxel.is_some() {
            on_voxel += 1;
            if !hover.vertical() {
                sides += 1;
            }
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
    while ret::bytes(&ok(g.h.request(json!({"call": "Iterate", "class": HD, "handle": s.hd, "args": [], "main": true})), "Iterate"))[0] != 0 {
        n += 1;
    }
    let mut m = 0;
    while hover.iterate(&ours, &lib) {
        m += 1;
    }
    assert_eq!(n, m, "flood steps");
    restore(&mut g, &s);
}
