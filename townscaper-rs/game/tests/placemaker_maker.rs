//! `ts_game::placemaker::maker` (G1b) against the original game. Needs `TS_HARNESS=1` and the install.
//! On the stand-in town (`hover_common`), both sides are hovered at the same screen point (compared), then the
//! same edit is made through the game's stand-in `Maker` and ours: add, paint or remove at the hover (when the
//! hover allows it), undo, redo, now and then clear. After each edit both towns settle; the whole graph, the
//! action list and the maker's counters and last position must match.

mod graph_common;
mod hover_common;

use graph_common::*;
use hover_common::*;
use serde_json::json;
use ts_check::{arg, harness, ret};
use ts_game::placemaker::graphs::{Graph, VoxelType};
use ts_game::placemaker::hover_data::{CameraView, HoverData, VoxelColliders};
use ts_game::placemaker::maker::{Maker, MakerHost};
use ts_game::unity::vector::{Vector2, Vector3};

const M: &str = "Placemaker.Maker";

/// The master's parts the maker reaches: the hover it clears.
struct MkHost<'a> {
    hover: &'a mut HoverData,
}
impl MakerHost for MkHost<'_> {
    fn clear_hover(&mut self) {
        self.hover.clear_hover();
    }
}

fn maker_call(g: &mut Game, maker: u64, m: &str, args: Vec<serde_json::Value>) -> serde_json::Value {
    ok(g.h.request(json!({"call": m, "class": M, "handle": maker, "args": args, "main": true})), m)
}

fn check_maker(g: &mut Game, maker: u64, ours: &Maker, what: &str) {
    assert_eq!(g.maker_state(), ours_maker(ours), "{what}: actions");
    let int = |g: &mut Game, f: &str| i32s(&ret::bytes(&ok(g.h.request(json!({"field": f, "handle": maker, "main": true})), f)))[0];
    let counters = [int(g, "actionIndex"), int(g, "actionCount"), int(g, "actionRedoMax"), int(g, "actionId")];
    assert_eq!(counters, [ours.action_index, ours.action_count, ours.action_redo_max, ours.action_id], "{what}: actionIndex, actionCount, actionRedoMax, actionId");
    let p = ret::bytes(&ok(g.h.request(json!({"field": "lastActionPos", "handle": maker, "main": true})), "lastActionPos"));
    assert_eq!(p, bytes(&ours.last_action_pos), "{what}: lastActionPos");
}

#[test]
fn maker_matches_the_game() {
    let Some(mut h) = harness() else { return };
    let (mut g, mut grid, ao) = setup(&mut h);
    let mut ours = Graph { shadow_mesh_index_iterator: (g.ao_skip as i32) * 16383, ..Default::default() };
    ours.ground_collider.size = Vector3 { x: 1.0, y: 1.0, z: 1.0 };
    let mut host = LoopHost { ao, ..Default::default() };
    let mut r = Rng(57);
    let s = prepare(&mut g, &mut grid, &mut ours, &mut host, &mut r);
    grow(&mut g, &mut grid, &mut ours, &mut host, &mut r, &s.verts.clone(), 40);
    let maker = g.maker;
    let mut mk = Maker::default();
    check_maker(&mut g, maker, &mk, "the fresh makers");
    let view = CameraView { camera: &s.camera, transforms: &s.tr, screen: s.screen };
    let mut hover = HoverData::default();
    let steps: usize = std::env::var("TS_MAKER_STEPS").ok().and_then(|s| s.parse().ok()).unwrap_or(80);
    let mut counts = [0usize; 6];
    for k in 0..steps {
        let colliders = VoxelColliders::build(&ours);
        let vmap = voxel_map(&mut g, &ours);
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
        let what = format!("step {k} at {pos:?}");
        same_hover(&mut g, &res, &hover, &vmap, &what);
        let t = VoxelType::from_i8((r.next() % 15) as i8);
        let pick = r.next() % 20;
        let hv = hover.clone();
        let done = if pick < 8 && hover.valid_add {
            maker_call(&mut g, maker, "AddClick", vec![json!({"handle": s.hd}), arg::raw(&[t as u8])]);
            mk.add_click(&mut ours, &mut grid, &mut host, &mut MkHost { hover: &mut hover }, &hv, t);
            0
        } else if pick < 11 && hover.valid_paint {
            maker_call(&mut g, maker, "PaintClick", vec![json!({"handle": s.hd}), arg::raw(&[t as u8])]);
            mk.paint_click(&mut ours, &mut host, &mut MkHost { hover: &mut hover }, &hv, t);
            1
        } else if pick < 14 && hover.valid_remove {
            maker_call(&mut g, maker, "RemoveClick", vec![json!({"handle": s.hd})]);
            mk.remove_click(&mut ours, &mut host, &mut MkHost { hover: &mut hover }, &hv);
            2
        } else if pick < 17 {
            let a = ret::bytes(&maker_call(&mut g, maker, "Undo", vec![]))[0] != 0;
            let b = mk.undo(&mut ours, &mut grid, &mut host, &mut MkHost { hover: &mut hover });
            assert_eq!(a, b, "{what}: Undo");
            3
        } else if pick < 19 {
            let a = ret::bytes(&maker_call(&mut g, maker, "Redo", vec![]))[0] != 0;
            let b = mk.redo(&mut ours, &mut grid, &mut host, &mut MkHost { hover: &mut hover });
            assert_eq!(a, b, "{what}: Redo");
            4
        } else if k % 40 == 39 {
            let a = ret::bytes(&maker_call(&mut g, maker, "Clear", vec![]))[0] != 0;
            let b = mk.clear(&mut ours, &mut host, &mut MkHost { hover: &mut hover });
            assert_eq!(a, b, "{what}: Clear");
            5
        } else {
            continue;
        };
        counts[done] += 1;
        settle(&mut g, &mut grid, &mut ours, &mut host);
        if counts.iter().sum::<usize>() % 5 == 0 {
            same(&g.snapshot(&ours), &ours_snapshot(&ours), &what);
        }
        check_maker(&mut g, maker, &mk, &what);
        eprintln!("{what}: {} same", ["add", "paint", "remove", "undo", "redo", "clear"][done]);
    }
    same(&g.snapshot(&ours), &ours_snapshot(&ours), "the end");
    eprintln!("adds {}, paints {}, removes {}, undos {}, redos {}, clears {}; {} voxels left, {} actions", counts[0], counts[1], counts[2], counts[3], counts[4], counts[5], ours.total_voxel_count, mk.actions.len());
    restore(&mut g, &s);
}
