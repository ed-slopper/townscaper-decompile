//! `ts_game::placemaker::props` (E10a) against the original game. Needs `TS_HARNESS=1`.
//! `PropPlacement`'s rotation and matrix on random placements; `PropNode`'s relation runs under random adds,
//! sorted overlap inserts and removes, on fresh nodes on an inactive object.

mod graph_common;

use graph_common::*;
use serde_json::{json, Value};
use ts_check::{arg, harness, ret, Harness};
use ts_game::os::utils::{ByteFloat3, SbyteFloat3};
use ts_game::placemaker::modules::{AssetRef, PropPlacement};
use ts_game::placemaker::props::*;
use ts_game::unity::vector::v3;

const PP: &str = "Placemaker.PropPlacement";
const PN: &str = "Placemaker.Props.PropNode";

fn f32s(b: &[u8]) -> Vec<f32> {
    b.chunks(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect()
}
fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}
/// The struct's 20 bytes (no prop).
fn raw(p: &PropPlacement) -> Vec<u8> {
    let mut b = vec![0u8; 20];
    b[0..3].copy_from_slice(&[p.pos.x as u8, p.pos.y as u8, p.pos.z as u8]);
    b[3..6].copy_from_slice(&[p.scale.x, p.scale.y, p.scale.z]);
    b[6..9].copy_from_slice(&[p.rotation_byte.x, p.rotation_byte.y, p.rotation_byte.z]);
    b[16] = p.corner as u8;
    b[17] = p.prio_add;
    b
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

#[test]
fn prop_placement_matches_the_game() {
    let Some(mut guard) = harness() else { return };
    let h: &mut Harness = &mut guard;
    let mut r = Rng(7);
    for k in 0..500 {
        let b = |r: &mut Rng| r.next() as u8;
        let p = PropPlacement {
            pos: SbyteFloat3 { x: b(&mut r) as i8, y: b(&mut r) as i8, z: b(&mut r) as i8 },
            scale: ByteFloat3 { x: b(&mut r), y: b(&mut r), z: b(&mut r) },
            rotation_byte: ByteFloat3 { x: b(&mut r), y: b(&mut r), z: b(&mut r) },
            prop: AssetRef::default(),
            corner: b(&mut r) as i8,
            prio_add: b(&mut r),
        };
        let this = json!({"class": PP, "raw": hex(&raw(&p))});
        let rot = f32s(&ret::bytes(&ok(h.request(json!({"call": "get_rotation", "class": PP, "this_raw": this, "args": [], "main": true})), "get_rotation")));
        let ours = p.rotation();
        assert_eq!(rot.iter().map(|f| f.to_bits()).collect::<Vec<_>>(), [ours.x, ours.y, ours.z].map(f32::to_bits), "{k}: rotation");
        let m = f32s(&ret::bytes(&ok(h.request(json!({"call": "GetMatrix", "class": PP, "this_raw": this, "args": [], "main": true})), "GetMatrix")));
        assert_eq!(m.iter().map(|f| f.to_bits()).collect::<Vec<_>>(), p.get_matrix().m.map(f32::to_bits), "{k}: GetMatrix of {p:?}");
        // set_rotation: angles round the circle, both signs, past a turn
        let f = |r: &mut Rng| (r.next() >> 8) as f32 / 16_777_216.0 * 1000.0 - 500.0;
        let v = if k % 5 == 0 { v3(360.0, -360.0, 359.9) } else { v3(f(&mut r), f(&mut r), f(&mut r)) };
        let raw_v: Vec<u8> = [v.x, v.y, v.z].iter().flat_map(|f| f.to_le_bytes()).collect();
        let got = ok(h.request(json!({"call": "set_rotation", "class": PP, "this_raw": this, "args": [arg::raw(&raw_v)], "main": true})), "set_rotation");
        let after = hex_bytes(got["this_after"].as_str().expect("this_after"));
        let mut q = p;
        q.set_rotation(v);
        assert_eq!(&after[6..9], &[q.rotation_byte.x, q.rotation_byte.y, q.rotation_byte.z], "{k}: set_rotation {v:?}");
    }
}

fn hex_bytes(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
}

#[test]
fn prop_node_relations_match_the_game() {
    let Some(mut guard) = harness() else { return };
    let h: &mut Harness = &mut guard;
    let go = handle(&ok(h.request(json!({"new": "UnityEngine.GameObject", "args": [arg::string("ts_check prop nodes")], "param_types": ["System.String"], "main": true, "keep": true})), "go"), "go");
    ok(h.request(json!({"call": "SetActive", "class": "UnityEngine.GameObject", "handle": go, "args": [arg::bool(false)], "main": true})), "SetActive");
    let tt = handle(&ok(h.request(json!({"call": "System.Type.GetType", "args": [arg::string("Placemaker.Props.PropNode, Assembly-CSharp")], "param_types": ["System.String"], "main": true, "keep": true})), "type"), "type");
    let mut r = Rng(11);
    const N: usize = 9;
    let mut handles = vec![];
    let mut addrs = vec![];
    let mut prio = vec![];
    for i in 0..N {
        let n = handle(&ok(h.request(json!({"call": "AddComponent", "class": "UnityEngine.GameObject", "handle": go, "args": [{"handle": tt}], "param_types": ["System.Type"], "main": true, "keep": true})), "node"), "node");
        // a few equal priorities, so the binary search meets ties
        let p = (r.next() % 6) as f32 * 0.5 - if i == 3 { f32::NAN } else { 0.0 };
        ok(h.request(json!({"field": "priority", "handle": n, "set_raw": hex(&p.to_le_bytes()), "main": true})), "priority");
        addrs.push(ok(h.request(json!({"addr": n})), "addr")["addr"].as_u64().unwrap());
        handles.push(n);
        prio.push(p);
    }
    let mut ours = PropNode::default();
    let me = handles[0];
    let ops = ["AddParent", "AddChild", "AddOverlap", "AddMotevee", "AddAnchor", "InserOverlap", "RemoveParent", "RemoveChild", "RemoveOverlap", "RemoveMotevee", "RemoveAnchor"];
    for k in 0..400 {
        let op = ops[(r.next() % ops.len() as u32) as usize];
        let other = 1 + (r.next() as usize % (N - 1));
        let g = h.request(json!({"call": op, "class": PN, "handle": me, "args": [{"handle": handles[other]}], "main": true}));
        let done = match op {
            "AddParent" => ours.add_parent(other),
            "AddChild" => ours.add_child(other),
            "AddOverlap" => ours.add_overlap(other),
            "AddMotevee" => ours.add_motevee(other),
            "AddAnchor" => ours.add_anchor(other),
            "InserOverlap" => ours.inser_overlap(other, &|n| prio[n]),
            "RemoveParent" => (ours.remove_parent(other), true).1,
            "RemoveChild" => (ours.remove_child(other), true).1,
            "RemoveOverlap" => (ours.remove_overlap(other), true).1,
            "RemoveMotevee" => (ours.remove_motevee(other), true).1,
            _ => (ours.remove_anchor(other), true).1,
        };
        assert_eq!(g["ok"] == json!(true), done, "{k}: {op}: the game {g}");
        let counts: Vec<u8> = ["parentCount", "childCount", "overlapCount", "moteveeCount", "anchorCount"]
            .iter()
            .map(|f| ret::bytes(&ok(h.request(json!({"field": f, "handle": me, "main": true})), f))[0])
            .collect();
        assert_eq!(counts, [ours.parent_count, ours.child_count, ours.overlap_count, ours.motevee_count, ours.anchor_count], "{k}: {op} counts");
        let l = handle(&ok(h.request(json!({"field": "relations", "handle": me, "main": true})), "relations"), "relations");
        let lc = list("Placemaker.Props.PropNode, Assembly-CSharp");
        let n = ret::i32(&ok(h.request(json!({"call": "get_Count", "class": lc, "handle": l, "args": [], "main": true})), "Count"));
        let reqs: Vec<Value> = (0..n).map(|i| json!({"call": "get_Item", "class": lc, "handle": l, "args": [arg::i32(i)], "main": true, "keep": true})).collect();
        let items: Vec<u64> = h.batch(reqs).into_iter().map(|v| handle(&ok(v, "item"), "item")).collect();
        let game: Vec<usize> = items
            .iter()
            .map(|&it| {
                let a = ok(h.request(json!({"addr": it})), "addr")["addr"].as_u64().unwrap();
                addrs.iter().position(|&x| x == a).expect("a known node")
            })
            .collect();
        assert_eq!(game, ours.relations, "{k}: {op} relations");
        // the index getters
        for (g, want) in [("get_childIndex", ours.child_index()), ("get_overlapIndex", ours.overlap_index()), ("get_moteveeIndex", ours.motevee_index()), ("get_anchorIndex", ours.anchor_index()), ("get_parentIndex", ours.parent_index())] {
            assert_eq!(ret::i32(&ok(h.request(json!({"call": g, "class": PN, "handle": me, "args": [], "main": true})), g)), want, "{k}: {g}");
        }
    }
    // the comparers
    for a in 0..N {
        for b in 0..N {
            let c = ret::i32(&ok(h.request(json!({"call": "System.IComparable<Placemaker.Props.PropNode>.CompareTo", "class": PN, "handle": handles[a], "args": [{"handle": handles[b]}], "main": true})), "CompareTo"));
            assert_eq!(c, single_compare_to(prio[a], prio[b]), "CompareTo {a} {b}");
        }
    }
    ok(h.request(json!({"call": "UnityEngine.Object.Destroy", "args": [{"handle": go}], "param_types": ["UnityEngine.Object"], "main": true})), "Destroy");
}
