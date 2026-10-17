//! E7d against the original game: `placemaker::materials` (the palette texture, the shader globals, the palette
//! holder), `grid_generator::BitMeshQuadChange` and `placemaker::world_meshes` (on a fresh `WorldMeshes`).
//! Needs `TS_HARNESS=1`.

use std::collections::HashMap;

use serde_json::{json, Value};
use ts_check::{arg, harness, ret, Harness};
use ts_game::placemaker::grid_generator::BitMeshQuadChange;
use ts_game::placemaker::materials::{MaterialHost, MaterialMaster, Palette, PaletteHolder};
use ts_game::placemaker::world_meshes::{MeshFilterId, MeshFilterParent, MeshId, WorldMeshes};
use ts_game::unity::geometry::{Color, Color32};
use ts_game::unity::mathematics::Int2;
use ts_game::unity::vector::Vector4;

fn ok(r: Value, what: &str) -> Value {
    assert_eq!(r["ok"], json!(true), "{what}: {r}");
    r
}
fn handle(r: &Value, what: &str) -> u64 {
    r["handle"].as_u64().unwrap_or_else(|| panic!("{what}: no handle in {r}"))
}
fn f32s(b: &[u8]) -> Vec<f32> {
    b.chunks(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect()
}
fn color(b: &[u8]) -> Color {
    let f = f32s(b);
    Color { r: f[0], g: f[1], b: f[2], a: f[3] }
}
fn bits(c: &Color) -> [u32; 4] {
    [c.r.to_bits(), c.g.to_bits(), c.b.to_bits(), c.a.to_bits()]
}
fn field(h: &mut Harness, obj: u64, f: &str) -> Value {
    ok(h.request(json!({"field": f, "handle": obj, "main": true})), f)
}

struct Rng(u32);
impl Rng {
    fn next(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0
    }
}

#[derive(Default)]
struct Globals(Vec<(String, [u32; 4])>);
impl MaterialHost for Globals {
    fn set_global_vector(&mut self, name: &str, v: Vector4) {
        self.0.push((name.into(), [v.x.to_bits(), v.y.to_bits(), v.z.to_bits(), v.w.to_bits()]));
    }
    fn set_global_color(&mut self, name: &str, c: Color) {
        self.0.push((name.into(), bits(&c)));
    }
}

#[test]
fn materials_and_palette_match_the_game() {
    let Some(mut h) = harness() else { return };
    let mm = handle(&ok(h.request(json!({"field": "materialMaster", "on": "Placemaker.WorldMaster", "main": true})), "materialMaster"), "materialMaster");
    let master = handle(&field(&mut h, mm, "master"), "master");
    let pal = handle(&field(&mut h, master, "palette"), "palette");
    let pc = handle(&field(&mut h, pal, "voxelColors"), "voxelColors");
    let raw = ret::bytes(&ok(h.request(json!({"array_raw": pc, "main": true})), "voxelColors"));
    let palette = Palette { voxel_colors: raw.chunks(4).map(|c| Color32 { r: c[0], g: c[1], b: c[2], a: c[3] }).collect() };
    let ours = MaterialMaster {
        background_color: color(&ret::bytes(&field(&mut h, mm, "backgroundColor"))),
        grass_color: color(&ret::bytes(&field(&mut h, mm, "grassColor"))),
        roof_color: color(&ret::bytes(&field(&mut h, mm, "roofColor"))),
        plate_color: color(&ret::bytes(&field(&mut h, mm, "plateColor"))),
        stone_color: color(&ret::bytes(&field(&mut h, mm, "stoneColor"))),
        ..Default::default()
    };
    // the palette texture
    let tex = handle(&ok(h.request(json!({"call": "Placemaker.MaterialMaster.CreatePaletteTexture", "handle": mm, "args": [], "main": true, "keep": true})), "CreatePaletteTexture"), "texture");
    // (GetPixels32 is stripped: the float pixels map back to the stored bytes one to one)
    let pixels: Vec<u8> = f32s(&ret::bytes(&ok(h.request(json!({"call": "GetPixels", "class": "UnityEngine.Texture2D", "handle": tex, "args": [], "main": true})), "GetPixels")))
        .iter()
        .map(|f| (f * 255.0).round() as u8)
        .collect();
    let pt = ours.create_palette_texture(&palette);
    let w = ret::i32(&ok(h.request(json!({"call": "get_width", "class": "UnityEngine.Texture", "handle": tex, "args": [], "main": true})), "width"));
    let ht = ret::i32(&ok(h.request(json!({"call": "get_height", "class": "UnityEngine.Texture", "handle": tex, "args": [], "main": true})), "height"));
    assert_eq!((w, ht), (pt.width, pt.height), "palette texture size");
    assert_eq!(pixels, pt.pixels.iter().flat_map(|c| [c.r, c.g, c.b, c.a]).collect::<Vec<u8>>(), "palette texture pixels");
    // the shader globals
    let mut g = Globals::default();
    ours.on_start(&mut g);
    ours.on_on_enable(&mut g);
    ok(h.request(json!({"call": "Placemaker.MaterialMaster.OnStart", "handle": mm, "args": [], "main": true})), "OnStart");
    ok(h.request(json!({"call": "Placemaker.WorldMaster.IOnOnEnable.OnOnEnable", "class": "Placemaker.MaterialMaster", "handle": mm, "args": [{"handle": master}], "main": true})), "OnOnEnable");
    for (name, want) in &g.0 {
        let call = "UnityEngine.Shader.GetGlobalVector";
        let id = ret::i32(&ok(h.request(json!({"call": "UnityEngine.Shader.PropertyToID", "args": [arg::string(name)], "main": true})), "PropertyToID"));
        let r = ok(h.request(json!({"call": call, "args": [arg::i32(id)], "param_types": ["System.Int32"], "main": true})), name);
        let b = ret::bytes(&r);
        assert_eq!(f32s(&b).iter().map(|f| f.to_bits()).collect::<Vec<_>>(), want.to_vec(), "shader global {name}");
    }
    // the palette holder (a fresh one on the scene's palette)
    let ph = handle(&ok(h.request(json!({"new": "Placemaker.PaletteHolder", "args": [], "main": true})), "new PaletteHolder"), "holder");
    ok(h.request(json!({"field": "palette", "handle": ph, "set_handle": pal, "main": true})), "palette");
    let mut holder = PaletteHolder { palette: palette.clone(), selected_color_index: 0 };
    let mut r = Rng(17);
    for k in 0..300 {
        if k % 3 == 0 {
            let i = (r.next() % 40) as i32 - 5;
            ok(h.request(json!({"call": "Placemaker.PaletteHolder.SelectPicker", "handle": ph, "args": [arg::i32(i)], "main": true})), "SelectPicker");
            holder.select_picker(i, &mut |_| {});
        } else {
            let d = (r.next() % 9) as i32 - 4 - if r.next() % 10 == 0 { 100 } else { 0 };
            ok(h.request(json!({"call": "Placemaker.PaletteHolder.SelectNextPicker", "handle": ph, "args": [arg::i32(d)], "main": true})), "SelectNextPicker");
            holder.select_next_picker(d, &mut |_| {});
        }
        assert_eq!(ret::i32(&field(&mut h, ph, "selectedColorIndex")), holder.selected_color_index, "step {k}: selectedColorIndex");
    }
    // BitMeshQuadChange.CompareTo
    let class = "Placemaker.BitMeshQuadChange, Assembly-CSharp";
    let raw = |q: &BitMeshQuadChange| {
        let mut b = vec![0u8; 16];
        b[0] = q.quad_index;
        b[4..8].copy_from_slice(&q.hex_pos.x.to_le_bytes());
        b[8..12].copy_from_slice(&q.hex_pos.y.to_le_bytes());
        b[12] = q.state as u8;
        b.iter().map(|x| format!("{x:02x}")).collect::<String>()
    };
    let mut q = || BitMeshQuadChange { quad_index: (r.next() % 4) as u8, hex_pos: Int2 { x: (r.next() % 3) as i32 - 1, y: (r.next() % 3) as i32 - 1 }, state: r.next() % 2 == 0 };
    let pairs: Vec<(BitMeshQuadChange, BitMeshQuadChange)> = (0..200).map(|_| (q(), q())).collect();
    let reqs = pairs
        .iter()
        .map(|(a, b)| json!({"call": "System.IComparable<Placemaker.BitMeshQuadChange>.CompareTo", "class": class, "this_raw": {"class": class, "raw": raw(a)}, "args": [{"raw": raw(b)}], "main": true}))
        .collect();
    for ((a, b), g) in pairs.iter().zip(h.batch(reqs)) {
        assert_eq!(ret::i32(&ok(g, "CompareTo")).signum(), a.compare_to(b).signum(), "CompareTo({a:?}, {b:?})");
    }
}

/// Game objects ↔ our ids, one to one.
struct Ids<T: Copy + Eq + std::hash::Hash + std::fmt::Debug> {
    to_ours: HashMap<u64, T>,
    to_game: HashMap<T, u64>,
}
impl<T: Copy + Eq + std::hash::Hash + std::fmt::Debug> Ids<T> {
    fn new() -> Self {
        Ids { to_ours: HashMap::new(), to_game: HashMap::new() }
    }
    fn same(&mut self, game: u64, ours: T, what: &str) {
        assert_eq!(*self.to_ours.entry(game).or_insert(ours), ours, "{what}: game object {game:#x}");
        assert_eq!(*self.to_game.entry(ours).or_insert(game), game, "{what}: our {ours:?}");
    }
}

#[test]
fn world_meshes_match_the_game() {
    let Some(mut h) = harness() else { return };
    let new_go = |h: &mut Harness, name: &str| {
        let go = handle(&ok(h.request(json!({"new": "UnityEngine.GameObject", "args": [arg::string(name)], "param_types": ["System.String"], "main": true})), "go"), "go");
        let t = handle(&ok(h.request(json!({"call": "get_transform", "class": "UnityEngine.GameObject", "handle": go, "args": [], "main": true, "keep": true})), "t"), "t");
        (go, t)
    };
    let (wgo, wt) = new_go(&mut h, "ts_check world meshes");
    let ty = handle(&ok(h.request(json!({"call": "System.Type.GetType", "args": [arg::string("Placemaker.WorldMeshes, Assembly-CSharp")], "param_types": ["System.String"], "main": true, "keep": true})), "type"), "type");
    let wm = handle(&ok(h.request(json!({"call": "AddComponent", "class": "UnityEngine.GameObject", "handle": wgo, "args": [{"handle": ty}], "param_types": ["System.Type"], "main": true, "keep": true})), "AddComponent"), "wm");
    let master = handle(&ok(h.request(json!({"field": "master", "on": "Placemaker.GridGenerator", "main": true})), "master"), "master");
    let parents: Vec<u64> = (0..2).map(|k| new_go(&mut h, &format!("ts_check parent {k}")).1).collect();
    let parent_addrs: Vec<u64> = parents.iter().map(|&p| ok(h.request(json!({"addr": p})), "addr")["addr"].as_u64().unwrap()).collect();
    let wt_addr = ok(h.request(json!({"addr": wt})), "addr")["addr"].as_u64().unwrap();
    let mut ours = WorldMeshes::default();
    let mut mf_handles: HashMap<MeshFilterId, u64> = HashMap::new();
    let mut mesh_handles: HashMap<MeshId, u64> = HashMap::new();
    let (mut mf_ids, mut mesh_ids) = (Ids::<MeshFilterId>::new(), Ids::<MeshId>::new());
    let wmc = "Placemaker.WorldMeshes";
    let addr = |h: &mut Harness, x: u64| ok(h.request(json!({"addr": x})), "addr")["addr"].as_u64().unwrap();
    let mut r = Rng(23);
    // first the refill (one request: a fresh frame, nothing runs out of time)
    let g = ok(h.request(json!({"call": format!("{wmc}.IterateRefillPools"), "handle": wm, "args": [{"handle": master}], "main": true})), "IterateRefillPools");
    assert_eq!(ret::bytes(&g)[0], 0, "the game's refill ran out of time");
    assert!(!ours.iterate_refill_pools(&mut || true));
    for step in 0..400 {
        let op = r.next() % 7;
        let p = (r.next() % 2) as usize;
        match op {
            0 | 1 => {
                let only = op == 1;
                let m = if only { "GetMeshFilterOnly" } else { "GetMeshFilter" };
                let g = handle(&ok(h.request(json!({"call": format!("{wmc}.{m}"), "handle": wm, "args": [{"handle": parents[p]}], "main": true, "keep": true})), m), m);
                let o = if only { ours.get_mesh_filter_only(MeshFilterParent::Other(p as u32)) } else { ours.get_mesh_filter(MeshFilterParent::Other(p as u32)) };
                let a = addr(&mut h, g);
                mf_ids.same(a, o, &format!("step {step}: {m}"));
                mf_handles.insert(o, g);
            }
            2 => {
                let g = handle(&ok(h.request(json!({"call": format!("{wmc}.GetMesh"), "handle": wm, "args": [], "main": true, "keep": true})), "GetMesh"), "mesh");
                let o = ours.get_mesh();
                let a = addr(&mut h, g);
                mesh_ids.same(a, o, &format!("step {step}: GetMesh"));
                mesh_handles.insert(o, g);
            }
            3 => {
                let held: Vec<MeshId> = mesh_handles.keys().copied().filter(|m| !ours.mesh_stack.contains(m) && !ours.mesh_filters.iter().any(|f| f.shared_mesh == Some(*m))).collect();
                if let Some(&m) = held.get(r.next() as usize % held.len().max(1)) {
                    ok(h.request(json!({"call": format!("{wmc}.ReturnMesh"), "handle": wm, "args": [{"handle": mesh_handles[&m]}], "main": true})), "ReturnMesh");
                    ours.return_mesh(m);
                }
            }
            _ => {
                let out: Vec<MeshFilterId> = mf_handles.keys().copied().filter(|f| ours.mesh_filters[f.0 as usize].parent != MeshFilterParent::Pool).collect();
                if out.is_empty() {
                    continue;
                }
                if op == 6 {
                    // all of one parent's
                    let kids: Vec<MeshFilterId> = (0..ours.mesh_filters.len() as u32).map(MeshFilterId).filter(|f| ours.mesh_filters[f.0 as usize].parent == MeshFilterParent::Other(p as u32)).collect();
                    // (our children order: the game's, read back)
                    let n = ret::i32(&ok(h.request(json!({"call": "get_childCount", "class": "UnityEngine.Transform", "handle": parents[p], "args": [], "main": true})), "childCount"));
                    let mut order = vec![];
                    for i in 0..n {
                        let t = handle(&ok(h.request(json!({"call": "GetChild", "class": "UnityEngine.Transform", "handle": parents[p], "args": [arg::i32(i)], "main": true, "keep": true})), "child"), "child");
                        let mft = handle(&ok(h.request(json!({"call": "System.Type.GetType", "args": [arg::string("UnityEngine.MeshFilter, UnityEngine.CoreModule")], "param_types": ["System.String"], "main": true, "keep": true})), "t"), "t");
                        let mf = handle(&ok(h.request(json!({"call": "GetComponent", "class": "UnityEngine.Component", "handle": t, "args": [{"handle": mft}], "param_types": ["System.Type"], "main": true, "keep": true})), "mf"), "mf");
                        let a = addr(&mut h, mf);
                        order.push(Some(mf_ids.to_ours[&a]));
                    }
                    assert_eq!(order.len(), kids.len(), "step {step}: parent {p} children");
                    ok(h.request(json!({"call": format!("{wmc}.ReturnMeshFilters"), "handle": wm, "args": [{"handle": parents[p]}], "main": true})), "ReturnMeshFilters");
                    ours.return_mesh_filters(&order);
                } else {
                    let f = out[r.next() as usize % out.len()];
                    let m = if op == 4 { "ReturnMeshFilter" } else { "ReturnMeshFilterOnly" };
                    ok(h.request(json!({"call": format!("{wmc}.{m}"), "handle": wm, "args": [{"handle": mf_handles[&f]}], "main": true})), m);
                    if op == 4 {
                        ours.return_mesh_filter(f);
                    } else {
                        ours.return_mesh_filter_only(f);
                    }
                }
            }
        }
        // the pool's children, the mesh stack, every handed-out object's state
        if step % 10 == 9 {
            let what = format!("step {step}");
            let n = ret::i32(&ok(h.request(json!({"call": "get_childCount", "class": "UnityEngine.Transform", "handle": wt, "args": [], "main": true})), "childCount"));
            assert_eq!(n as usize, ours.pool.len(), "{what}: pool size");
            let st = handle(&ok(h.request(json!({"field": "meshStack", "handle": wm, "main": true})), "meshStack"), "meshStack");
            let lc = "System.Collections.Generic.List`1[[UnityEngine.Mesh, UnityEngine.CoreModule]]";
            let c = ret::i32(&ok(h.request(json!({"call": "get_Count", "class": lc, "handle": st, "args": [], "main": true})), "Count"));
            assert_eq!(c as usize, ours.mesh_stack.len(), "{what}: mesh stack size");
            for i in 0..c {
                let m = handle(&ok(h.request(json!({"call": "get_Item", "class": lc, "handle": st, "args": [arg::i32(i)], "main": true, "keep": true})), "item"), "mesh");
                let a = addr(&mut h, m);
                mesh_ids.same(a, ours.mesh_stack[i as usize], &format!("{what}: meshStack[{i}]"));
            }
            for (&f, &g) in &mf_handles {
                let o = ours.mesh_filters[f.0 as usize].clone();
                let go = handle(&ok(h.request(json!({"call": "get_gameObject", "class": "UnityEngine.Component", "handle": g, "args": [], "main": true, "keep": true})), "go"), "go");
                let active = ret::bytes(&ok(h.request(json!({"call": "get_activeSelf", "class": "UnityEngine.GameObject", "handle": go, "args": [], "main": true})), "activeSelf"))[0] != 0;
                assert_eq!(active, o.active, "{what}: {f:?} active");
                let t = handle(&ok(h.request(json!({"call": "get_transform", "class": "UnityEngine.Component", "handle": g, "args": [], "main": true, "keep": true})), "t"), "t");
                let par = ok(h.request(json!({"call": "get_parent", "class": "UnityEngine.Transform", "handle": t, "args": [], "main": true, "keep": true})), "parent");
                let pa = addr(&mut h, handle(&par, "parent"));
                let want = match o.parent {
                    MeshFilterParent::Pool => wt_addr,
                    MeshFilterParent::Other(k) => parent_addrs[k as usize],
                };
                assert_eq!(pa, want, "{what}: {f:?} parent");
                let sm = ok(h.request(json!({"call": "get_sharedMesh", "class": "UnityEngine.MeshFilter", "handle": g, "args": [], "main": true, "keep": true})), "sharedMesh");
                match (sm["null"] == json!(true), o.shared_mesh) {
                    (true, None) => {}
                    (false, Some(m)) => {
                        let a = addr(&mut h, handle(&sm, "mesh"));
                        mesh_ids.same(a, m, &format!("{what}: {f:?} sharedMesh"));
                    }
                    (gn, om) => panic!("{what}: {f:?} sharedMesh: game null {gn}, ours {om:?}"),
                }
            }
        }
    }
    eprintln!("{} mesh filters, {} meshes made", ours.mesh_filters.len(), ours.mesh_count);
}
