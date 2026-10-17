//! `ts_game::placemaker::modules` against the original game (E5b records). Needs `TS_HARNESS=1`.

use serde_json::{json, Value};
use ts_check::{arg, harness, ret, Harness};
use ts_game::placemaker::modules::{ByteQube, OrientedModule, OutlineUv, SideProfile};

const BQ: &str = "Placemaker.ByteQube";
const SP: &str = "Placemaker.SideProfile";
const OM: &str = "Placemaker.Modules.OrientedModule";

fn bytes<T: Copy>(v: &T) -> Vec<u8> {
    unsafe { std::slice::from_raw_parts(v as *const T as *const u8, std::mem::size_of::<T>()) }.to_vec()
}
fn ok(r: Value, what: &str) -> Value {
    assert_eq!(r["ok"], json!(true), "{what}: {r}");
    r
}

struct Rng(u32);
impl Rng {
    fn next(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0
    }
    /// Small values most of the time (corner values and indexes are small), anything now and then.
    fn byte(&mut self) -> u8 {
        if self.next() % 4 == 0 {
            self.next() as u8
        } else {
            (self.next() % 20) as u8
        }
    }
    fn qube(&mut self) -> ByteQube {
        ByteQube { v: [0; 8].map(|_| self.byte()) }
    }
}

/// A method on a struct value; `Err` when the original throws.
fn on(h: &mut Harness, class: &str, m: &str, this: &[u8], args: Vec<Value>) -> Result<Value, String> {
    h.call_on_value(&format!("{class}.{m}"), class, this, args, &[], false).map_err(|e| e.0)
}

#[test]
fn byte_qube_matches_the_game() {
    let Some(mut h) = harness() else { return };
    for (name, ours) in [
        ("identity", ByteQube::IDENTITY),
        ("rotateForward", ByteQube::ROTATE_FORWARD),
        ("rotateBackward", ByteQube::ROTATE_BACKWARD),
        ("mirror", ByteQube::MIRROR),
        ("tipForward", ByteQube::TIP_FORWARD),
        ("tipBackward", ByteQube::TIP_BACKWARD),
        ("upsideDown", ByteQube::UPSIDE_DOWN),
    ] {
        let g = ret::bytes(&ok(h.request(json!({"static_field": format!("{BQ}.{name}"), "main": true})), name));
        assert_eq!(g, ours.v.to_vec(), "ByteQube.{name}");
    }
    let mut r = Rng(31);
    for k in 0..400 {
        let q = r.qube();
        let qb = bytes(&q);
        assert_eq!(ret::i32(&on(&mut h, BQ, "get_cost", &qb, vec![]).unwrap()), q.cost(), "cost {q:?}");
        assert_eq!(ret::i32(&on(&mut h, BQ, "GetHashCode", &qb, vec![]).unwrap()), q.get_hash_code(), "hash");
        assert_eq!(ret::string(&on(&mut h, BQ, "ToString", &qb, vec![]).unwrap()), q.to_string_cs(), "ToString");
        let key = (r.next() % 10) as i32 - 1;
        match on(&mut h, BQ, "get_Item", &qb, vec![arg::i32(key)]) {
            Ok(g) => assert_eq!(ret::bytes(&g)[0], q.get(key), "[{key}]"),
            Err(_) => assert!(!(0..8).contains(&key), "get_Item({key}) threw"),
        }
        if (0..8).contains(&key) {
            let mut o = q;
            o.set(key, 77);
            let g = on(&mut h, BQ, "set_Item", &qb, vec![arg::i32(key), arg::raw(&[77])]).unwrap();
            assert_eq!(ret::hex_field(&g["this_after"]), o.v.to_vec(), "set_Item");
        }
        // GetOriented by a permutation (and now and then by any bytes, which may throw)
        let p = if k % 5 == 0 { r.qube() } else { [ByteQube::ROTATE_FORWARD, ByteQube::MIRROR, ByteQube::TIP_BACKWARD, ByteQube::UPSIDE_DOWN][k % 4] };
        let ours = std::panic::catch_unwind(|| q.get_oriented(p));
        match on(&mut h, BQ, "GetOriented", &qb, vec![arg::raw(&p.v)]) {
            Ok(g) => assert_eq!(ret::bytes(&g), ours.unwrap().v.to_vec(), "GetOriented({p:?})"),
            Err(_) => assert!(ours.is_err(), "GetOriented({p:?}) threw"),
        }
        let o = if k % 3 == 0 { r.next() as u8 } else { (r.next() % 48) as u8 };
        assert_eq!(ret::bytes(&on(&mut h, BQ, "GetOrientedForward", &qb, vec![arg::raw(&[o])]).unwrap()), q.get_oriented_forward(o).v.to_vec(), "Forward({o})");
        assert_eq!(ret::bytes(&on(&mut h, BQ, "GetOrientedBackward", &qb, vec![arg::raw(&[o])]).unwrap()), q.get_oriented_backward(o).v.to_vec(), "Backward({o})");
        if k % 4 == 0 {
            let g = on(&mut h, BQ, "GetOptimalOrientation", &qb, vec![arg::raw(&[0; 8]), arg::raw(&[0])]).unwrap();
            let (best, o) = q.get_optimal_orientation();
            assert_eq!(ret::hex_field(&g["args_after"][0]), best.v.to_vec(), "GetOptimalOrientation qube");
            assert_eq!(ret::hex_field(&g["args_after"][1])[0], o, "GetOptimalOrientation index");
        }
        let other = if k % 2 == 0 { q } else { r.qube() };
        let eq = ret::bytes(&h.call(&format!("{BQ}.op_Equality"), vec![arg::raw(&qb), arg::raw(&other.v)], &[], false).unwrap())[0] != 0;
        assert_eq!(eq, q == other, "==");
        let ne = ret::bytes(&h.call(&format!("{BQ}.op_Inequality"), vec![arg::raw(&qb), arg::raw(&other.v)], &[], false).unwrap())[0] != 0;
        assert_eq!(ne, q != other, "!=");
        let (c, ht) = ((r.next() % 11) as i32 - 5, (r.next() % 9) as i32 - 2);
        assert_eq!(ret::i32(&h.call(&format!("{BQ}.GetWrappedIndex"), vec![arg::i32(c), arg::i32(ht)], &[], false).unwrap()), ByteQube::get_wrapped_index(c, ht), "GetWrappedIndex({c}, {ht})");
    }
    let g = h.call_on_value(&format!("{BQ}..ctor"), BQ, &[0; 8], vec![arg::raw(&[9])], &["System.Byte"], false).unwrap();
    assert_eq!(ret::hex_field(&g["this_after"]), ByteQube::splat(9).v.to_vec());
    let g = h.call_on_value(&format!("{BQ}..ctor"), BQ, &[0; 8], (1..9u8).map(|b| arg::raw(&[b])).collect(), &[], false).unwrap();
    assert_eq!(ret::hex_field(&g["this_after"]), ByteQube::new(1, 2, 3, 4, 5, 6, 7, 8).v.to_vec());
}

#[test]
fn side_profile_matches_the_game() {
    let Some(mut h) = harness() else { return };
    let mut r = Rng(32);
    let sp = |r: &mut Rng| {
        let mut s = SideProfile::default();
        for i in 0..6 {
            let z = r.next() % 3 == 0;
            s.set(i, if z { 0 } else { r.next() as u16 }, if r.next() % 2 == 0 { 0 } else { r.next() as u16 });
        }
        s
    };
    for _ in 0..300 {
        let s = sp(&mut r);
        let sb = bytes(&s);
        assert_eq!(ret::i32(&on(&mut h, SP, "GetHashCode", &sb, vec![]).unwrap()), s.get_hash_code(), "hash {s:?}");
        assert_eq!(ret::bytes(&on(&mut h, SP, "IsEmtpy", &sb, vec![]).unwrap())[0] != 0, s.is_emtpy(), "IsEmtpy {s:?}");
        assert_eq!(ret::string(&on(&mut h, SP, "ToString", &sb, vec![]).unwrap()), s.to_string_cs());
        let i = (r.next() % 8) as i32 - 1;
        for (m, ours) in [("Get0", std::panic::catch_unwind(|| s.get0(i))), ("Get1", std::panic::catch_unwind(|| s.get1(i)))] {
            match on(&mut h, SP, m, &sb, vec![arg::i32(i)]) {
                Ok(g) => assert_eq!(u16::from_le_bytes(ret::bytes(&g)[..2].try_into().unwrap()), ours.unwrap(), "{m}({i})"),
                Err(_) => assert!(ours.is_err(), "{m}({i}) threw"),
            }
        }
        let (a, b) = (r.next() as u16, r.next() as u16);
        let mut o = s;
        let ours = std::panic::catch_unwind(move || {
            o.set(i, a, b);
            o
        });
        match on(&mut h, SP, "Set", &sb, vec![arg::i32(i), arg::raw(&a.to_le_bytes()), arg::raw(&b.to_le_bytes())]) {
            Ok(g) => assert_eq!(ret::hex_field(&g["this_after"]), bytes(&ours.unwrap()), "Set({i})"),
            Err(_) => assert!(ours.is_err(), "Set({i}) threw"),
        }
        let t = sp(&mut r);
        let mut o = s;
        o.add(t);
        let g = on(&mut h, SP, "Add", &sb, vec![arg::raw(&bytes(&t))]).unwrap();
        assert_eq!(ret::hex_field(&g["this_after"]), bytes(&o), "Add");
    }
}

#[test]
fn oriented_module_and_outline_uv_match_the_game() {
    let Some(mut h) = harness() else { return };
    let e = ret::bytes(&ok(h.request(json!({"static_field": format!("{OM}.empty"), "main": true})), "empty"));
    assert_eq!(e[..3], bytes(&OrientedModule::EMPTY)[..3], "OrientedModule.empty");
    let mut r = Rng(33);
    for _ in 0..300 {
        let m = OrientedModule { module_index: r.next() as u16, flags: r.next() as u8, pad: 0 };
        let mb = bytes(&m);
        assert_eq!(ret::i32(&on(&mut h, OM, "get_orientation", &mb, vec![]).unwrap()), m.orientation());
        assert_eq!(ret::bytes(&on(&mut h, OM, "get_isDecor", &mb, vec![]).unwrap())[0] != 0, m.is_decor());
        assert_eq!(ret::bytes(&on(&mut h, OM, "get_isEmpty", &mb, vec![]).unwrap())[0] != 0, m.is_empty());
        assert_eq!(ret::i32(&on(&mut h, OM, "GetHashCode", &mb, vec![]).unwrap()), m.get_hash_code(), "hash");
        assert_eq!(ret::string(&on(&mut h, OM, "ToString", &mb, vec![]).unwrap()), m.to_string_cs(), "ToString");
        let v = (r.next() % 12) as i32;
        let mut o = m;
        o.set_orientation(v);
        assert_eq!(ret::hex_field(&on(&mut h, OM, "set_orientation", &mb, vec![arg::i32(v)]).unwrap()["this_after"])[..3], bytes(&o)[..3], "set_orientation({v})");
        let b = r.next() % 2 == 0;
        let mut o = m;
        o.set_is_decor(b);
        assert_eq!(ret::hex_field(&on(&mut h, OM, "set_isDecor", &mb, vec![arg::bool(b)]).unwrap()["this_after"])[..3], bytes(&o)[..3], "set_isDecor");
        let mut o = m;
        o.set_is_empty(b);
        assert_eq!(ret::hex_field(&on(&mut h, OM, "set_isEmpty", &mb, vec![arg::bool(b)]).unwrap()["this_after"])[..3], bytes(&o)[..3], "set_isEmpty");
        let other = if r.next() % 2 == 0 { OrientedModule { pad: 9, ..m } } else { OrientedModule { module_index: r.next() as u16 % 4, flags: r.next() as u8 % 4, pad: 0 } };
        let eq = ret::bytes(&h.call(&format!("{OM}.op_Equality"), vec![arg::raw(&mb), arg::raw(&bytes(&other))], &[], false).unwrap())[0] != 0;
        assert_eq!(eq, m.eq_cs(&other), "==");
        let ne = ret::bytes(&h.call(&format!("{OM}.op_Inequality"), vec![arg::raw(&mb), arg::raw(&bytes(&other))], &[], false).unwrap())[0] != 0;
        assert_eq!(ne, !m.eq_cs(&other), "!=");
    }
    // OutlineUv is a class: one object, its byte set and read back
    let o = h.request(json!({"new": "Placemaker.Modules.OutlineUv", "args": [], "main": true}))["handle"].as_u64().unwrap();
    for b in 0..=255u8 {
        ok(h.request(json!({"field": "b", "handle": o, "set_raw": format!("{b:02x}"), "main": true})), "b");
        let u = OutlineUv { b };
        let call = |h: &mut Harness, m: &str, args: Vec<Value>| h.request(json!({"call": m, "class": "Placemaker.Modules.OutlineUv", "handle": o, "args": args, "main": true}));
        assert_eq!(ret::i32(&call(&mut h, "get_x", vec![])), u.x(), "x {b}");
        assert_eq!(ret::i32(&call(&mut h, "get_y", vec![])), u.y(), "y {b}");
        assert_eq!(ret::bytes(&call(&mut h, "get_border", vec![]))[0] != 0, u.border(), "border {b}");
        let g = ret::bytes(&call(&mut h, "get_uv", vec![]));
        assert_eq!(g, [u.uv().x.to_le_bytes(), u.uv().y.to_le_bytes()].concat(), "uv {b}");
        for v in -2..=2 {
            for (m, set) in [("set_x", OutlineUv::set_x as fn(&mut OutlineUv, i32)), ("set_y", OutlineUv::set_y)] {
                ok(h.request(json!({"field": "b", "handle": o, "set_raw": format!("{b:02x}"), "main": true})), "b");
                let mut ours = u;
                let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| set(&mut ours, v)));
                let g = call(&mut h, m, vec![arg::i32(v)]);
                if g["ok"] == json!(true) {
                    let after = ret::bytes(&ok(h.request(json!({"field": "b", "handle": o, "main": true})), "b"))[0];
                    assert_eq!(after, ours.b, "{m}({v}) on {b}");
                } else {
                    assert!(res.is_err(), "{m}({v}) threw");
                }
            }
        }
    }
}
