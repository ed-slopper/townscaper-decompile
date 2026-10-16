//! `GridGenerator`'s patches against the original game (E3c). Needs `TS_HARNESS=1` and the player's install
//! (the patch library is read with `ts_data` and compared with the game's own).

use serde_json::{json, Value};
use ts_check::{arg, harness, ret, Harness};
use ts_game::placemaker::grid_generator::{self as gg, GridGenerator};
use ts_game::placemaker::quads::grid_generation::{CustomGridLibrary, CustomGridPatch, HexPatch, HexPatchState};
use ts_game::unity::mathematics::{i3, Int2};

const G: &str = "Placemaker.GridGenerator";
const BYTES: &str = "System.Collections.Generic.List`1[[System.Byte]]";
const INT2S: &str = "System.Collections.Generic.List`1[[Unity.Mathematics.int2, Unity.Mathematics]]";
const PATCHES: &str = "System.Collections.Generic.List`1[[Placemaker.Quads.GridGeneration.HexPatch, Assembly-CSharp]]";
const LIB_PATCHES: &str =
    "System.Collections.Generic.List`1[[Placemaker.Quads.GridGeneration.CustomGridPatch, Assembly-CSharp]]";
const DICT: &str = "System.Collections.Generic.Dictionary`2[[Unity.Mathematics.int2, Unity.Mathematics],[Placemaker.Quads.GridGeneration.HexPatch, Assembly-CSharp]]";

fn ok(r: Value, what: &str) -> Value {
    assert_eq!(r["ok"], json!(true), "{what}: {r}");
    r
}
fn handle_of(r: &Value, what: &str) -> u64 {
    r["handle"].as_u64().unwrap_or_else(|| panic!("{what}: no handle in {r}"))
}
fn field_req(handle: u64, name: &str) -> Value {
    json!({"field": name, "handle": handle, "main": true})
}
fn field_handle(h: &mut Harness, handle: u64, name: &str) -> u64 {
    let r = ok(h.request(field_req(handle, name)), name);
    handle_of(&r, name)
}
fn to_array_req(list: u64, class: &str) -> Value {
    json!({"call": "ToArray", "class": class, "handle": list, "args": [], "main": true})
}
fn hexes(b: &[u8]) -> Vec<Int2> {
    b.chunks(8).map(|c| Int2 { x: i32::from_le_bytes(c[..4].try_into().unwrap()), y: i32::from_le_bytes(c[4..].try_into().unwrap()) }).collect()
}
fn i2_bytes(v: Int2) -> Vec<u8> {
    [v.x.to_le_bytes(), v.y.to_le_bytes()].concat()
}

/// The library as `ts_data` reads it from the player's files.
fn our_library() -> CustomGridLibrary {
    let dir = ts_data::install::find_dir().expect("the player's install (TOWNSCAPER_DIR)");
    let a = ts_data::objects::AssetSet::load(&dir.join("Townscaper_Data")).unwrap();
    let d = ts_data::grid::custom_grid_library(&a).expect("CustomGridLibrary");
    CustomGridLibrary {
        all_custom_patches: d
            .all_custom_patches
            .into_iter()
            .map(|p| CustomGridPatch {
                verts: p.verts.into_iter().map(|[x, y]| Int2 { x, y }).collect(),
                quads: p.quads,
                quad_tri_cutoff: p.quad_tri_cutoff,
                subdivisions: p.subdivisions,
            })
            .collect(),
        generic_patches: d.generic_patches,
        straight_patches: d.straight_patches,
        centroid_patches: d.centroid_patches,
        centroid_curve_patches: d.centroid_curve_patches,
    }
}

/// The scene's `GridGenerator.customGridLibrary` (a handle).
fn game_library(h: &mut Harness) -> u64 {
    let r = ok(h.request(json!({"field": "customGridLibrary", "on": G, "main": true})), "customGridLibrary");
    handle_of(&r, "customGridLibrary")
}

#[test]
fn the_game_uses_the_library_we_read() {
    let Some(mut h) = harness() else { return };
    let ours = our_library();
    let lib = game_library(&mut h);
    for (name, list) in [
        ("genericPatches", &ours.generic_patches),
        ("straightPatches", &ours.straight_patches),
        ("centroidPatches", &ours.centroid_patches),
        ("centroidCurvePatches", &ours.centroid_curve_patches),
    ] {
        let l = field_handle(&mut h, lib, name);
        assert_eq!(&ret::bytes(&ok(h.request(to_array_req(l, BYTES)), name)), list, "{name}");
    }
    let all = field_handle(&mut h, lib, "allCustomPatches");
    let n = ret::i32(&ok(h.request(json!({"call": "get_Count", "class": LIB_PATCHES, "handle": all, "args": [], "main": true})), "Count"));
    assert_eq!(n as usize, ours.all_custom_patches.len(), "allCustomPatches.Count");
    for (i, p) in ours.all_custom_patches.iter().enumerate() {
        let r = ok(h.request(json!({"call": "get_Item", "class": LIB_PATCHES, "handle": all, "args": [arg::i32(i as i32)], "main": true, "keep": true})), "get_Item");
        let ph = handle_of(&r, "patch");
        let verts = field_handle(&mut h, ph, "verts");
        let quads = field_handle(&mut h, ph, "quads");
        assert_eq!(hexes(&ret::bytes(&ok(h.request(to_array_req(verts, INT2S)), "verts"))), p.verts, "patch {i} verts");
        assert_eq!(ret::bytes(&ok(h.request(to_array_req(quads, BYTES)), "quads")), p.quads, "patch {i} quads");
        let cut = ret::bytes(&ok(h.request(field_req(ph, "quadTriCutoff")), "cutoff"));
        assert_eq!(u16::from_le_bytes([cut[0], cut[1]]), p.quad_tri_cutoff, "patch {i} quadTriCutoff");
        assert_eq!(ret::bytes(&ok(h.request(field_req(ph, "subdivisions")), "subdivisions")), vec![p.subdivisions], "patch {i} subdivisions");
    }
}

struct Rng(u32);
impl Rng {
    fn next(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0
    }
    fn i(&mut self, n: u32) -> i32 {
        (self.next() % (2 * n + 1)) as i32 - n as i32
    }
}

/// Patch positions that reach every branch of `IteratePatch`'s `Verts` step: a grid centroid itself, the three
/// lines through it (72 out: the centroid-curve patches; further: the straight ones), the new-grid noise, and
/// plain procedural hexagons; plus the patch centres the game would use.
fn patch_positions(r: &mut Rng) -> Vec<Int2> {
    let mut out = Vec::new();
    for k in 0..48 {
        let h = i3(r.i(6000), r.i(6000), 0);
        let h = i3(h.x, h.y, -(h.x + h.y));
        let c = gg::hex_to_grid_centroid(h);
        let t = [72, -72, 144, -144, 216, -216, 36][(r.next() % 7) as usize];
        let d = match k % 6 {
            0 => i3(0, 0, 0),
            1 => i3(t, t, -2 * t),
            2 => i3(-2 * t, t, t),
            3 => i3(t, -2 * t, t),
            _ => i3(r.i(1200), r.i(1200), 0),
        };
        out.push(Int2 { x: c.x + d.x, y: c.y + d.y });
        out.push(gg::hex_to_patch_hex(Int2 { x: r.i(20000), y: r.i(20000) }));
    }
    out
}

#[test]
fn iterate_patch_matches_the_game() {
    let Some(mut h) = harness() else { return };
    let library = our_library();
    let mut r = Rng(51);
    let (mut custom, mut procedural, mut steps) = (0, 0, 0);
    for pos in patch_positions(&mut r) {
        let p = handle_of(&ok(h.request(json!({"new": "Placemaker.Quads.GridGeneration.HexPatch", "args": [], "main": true})), "new HexPatch"), "HexPatch");
        ok(h.request(json!({"field": "hexPos", "handle": p, "set_raw": ret_hex(&i2_bytes(pos)), "main": true})), "set hexPos");
        let (verts, quads) = (field_handle(&mut h, p, "verts"), field_handle(&mut h, p, "quads"));
        let mut ours = HexPatch { hex_pos: pos, ..Default::default() };
        for step in 0..2000 {
            let before = ours.state;
            gg::iterate_patch(&library, &mut ours);
            let res = h.batch(vec![
                json!({"call": format!("{G}.IteratePatch"), "on": G, "args": [{"handle": p}]}),
                field_req(p, "state"),
                field_req(p, "quadTriCutoff"),
                field_req(p, "subdivisions"),
                to_array_req(verts, INT2S),
                to_array_req(quads, BYTES),
            ]);
            for (i, x) in res.iter().enumerate() {
                assert_eq!(x["ok"], json!(true), "patch {pos:?} step {step} request {i}: {x}");
            }
            let what = format!("patch {pos:?} step {step} ({before:?} -> {:?})", ours.state);
            assert_eq!(ret::bytes(&res[1]), vec![ours.state as u8], "{what}: state");
            assert_eq!(ret::bytes(&res[2]), ours.quad_tri_cutoff.to_le_bytes().to_vec(), "{what}: quadTriCutoff");
            assert_eq!(ret::bytes(&res[3]), vec![ours.subdivisions], "{what}: subdivisions");
            assert_eq!(hexes(&ret::bytes(&res[4])), ours.verts, "{what}: verts");
            assert_eq!(ret::bytes(&res[5]), ours.quads, "{what}: quads");
            steps += 1;
            if before == HexPatchState::Verts {
                if ours.state == HexPatchState::Quadrangulate { custom += 1 } else { procedural += 1 }
            }
            if ours.state == HexPatchState::Done {
                break;
            }
        }
        assert_eq!(ours.state, HexPatchState::Done, "patch {pos:?} never finished");
    }
    eprintln!("custom patches {custom}, procedural {procedural}, {steps} steps");
    assert!(custom > 10 && procedural > 10, "both kinds of patch should occur");
}

fn ret_hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Random `MotivatePatch` / `UnmotivatePatch` / `IteratePatch` on a fresh `GridGenerator` (made with `new`:
/// these touch only its lists), comparing every patch after each step.
#[test]
fn patch_motivation_matches_the_game() {
    let Some(mut h) = harness() else { return };
    let library = our_library();
    let gen = handle_of(&ok(h.request(json!({"new": G, "args": [], "main": true})), "new GridGenerator"), G);
    let lib = game_library(&mut h);
    ok(h.request(json!({"field": "customGridLibrary", "handle": gen, "set_handle": lib, "main": true})), "set library");
    let (list, dict) = (field_handle(&mut h, gen, "patches"), field_handle(&mut h, gen, "patchDict"));
    let mut ours = GridGenerator { custom_grid_library: library, ..Default::default() };
    let positions: Vec<Int2> = (0..40).map(|i| gg::hex_to_patch_hex(Int2 { x: 216 * (i % 7), y: -144 * (i / 7) + 37 })).collect();
    let mut r = Rng(52);
    let mut reused = 0;
    for step in 0..400 {
        let pos = positions[(r.next() % 40) as usize];
        let src = Int2 { x: r.i(100), y: r.i(100) };
        match r.next() % 4 {
            0 | 1 => {
                if !ours.patch_dict.contains_key(&pos) && ours.patches.iter().any(|p| p.motivations.count == 0) {
                    reused += 1;
                }
                ours.motivate_patch(pos, src);
                ok(h.request(json!({"call": format!("{G}.MotivatePatch"), "handle": gen, "main": true, "args": [arg::raw(&i2_bytes(pos)), arg::raw(&i2_bytes(src))]})), "MotivatePatch");
            }
            2 => {
                // a patch that still has a reason to exist (now and then one that has none: the byte wraps)
                let keep = r.next() % 10 != 0;
                match ours.patch_dict.get(&pos) {
                    Some(&i) if ours.patches[i].motivations.count > 0 || !keep => {}
                    _ => continue,
                }
                ours.unmotivate_patch(pos, src);
                ok(h.request(json!({"call": format!("{G}.UnmotivatePatch"), "handle": gen, "main": true, "args": [arg::raw(&i2_bytes(pos)), arg::raw(&i2_bytes(src))]})), "UnmotivatePatch");
            }
            _ => {
                if ours.patches.is_empty() {
                    continue;
                }
                let i = (r.next() as usize) % ours.patches.len();
                for _ in 0..1 + r.next() % 40 {
                    ours.iterate_patch(i);
                    let item = ok(h.request(json!({"call": "get_Item", "class": PATCHES, "handle": list, "args": [arg::i32(i as i32)], "main": true, "keep": true})), "get_Item");
                    ok(h.request(json!({"call": format!("{G}.IteratePatch"), "handle": gen, "main": true, "args": [{"handle": handle_of(&item, "patch")}]})), "IteratePatch");
                }
            }
        }
        // every patch, and the dictionary's size
        let n = ret::i32(&ok(h.request(json!({"call": "get_Count", "class": PATCHES, "handle": list, "args": [], "main": true})), "Count"));
        assert_eq!(n as usize, ours.patches.len(), "step {step}: patches.Count");
        let dn = ret::i32(&ok(h.request(json!({"call": "get_Count", "class": DICT, "handle": dict, "args": [], "main": true})), "dict Count"));
        assert_eq!(dn as usize, ours.patch_dict.len(), "step {step}: patchDict.Count");
        for (i, p) in ours.patches.iter().enumerate() {
            let item = ok(h.request(json!({"call": "get_Item", "class": PATCHES, "handle": list, "args": [arg::i32(i as i32)], "main": true, "keep": true})), "get_Item");
            let ph = handle_of(&item, "patch");
            let (pv, pq) = (field_handle(&mut h, ph, "verts"), field_handle(&mut h, ph, "quads"));
            let res = h.batch(vec![
                field_req(ph, "hexPos"),
                field_req(ph, "motivations"),
                field_req(ph, "state"),
                field_req(ph, "quadTriCutoff"),
                to_array_req(pv, INT2S),
                to_array_req(pq, BYTES),
            ]);
            let what = format!("step {step} patch {i}");
            assert_eq!(hexes(&ret::bytes(&res[0])), vec![p.hex_pos], "{what}: hexPos");
            assert_eq!(ret::bytes(&res[1]), vec![p.motivations.count], "{what}: motivations");
            assert_eq!(ret::bytes(&res[2]), vec![p.state as u8], "{what}: state");
            assert_eq!(ret::bytes(&res[3]), p.quad_tri_cutoff.to_le_bytes().to_vec(), "{what}: quadTriCutoff");
            assert_eq!(hexes(&ret::bytes(&res[4])), p.verts, "{what}: verts");
            assert_eq!(ret::bytes(&res[5]), p.quads, "{what}: quads");
        }
    }
    eprintln!("patches reused {reused} times");
    assert!(reused > 3, "the reuse path should run");
}
