//! `ts_game::unity::collections` against the original game's BCL (F11e). Needs `TS_HARNESS=1`.
//! Sorts use `float` keys whose ties are visible: +0 and -0 compare equal, NaNs compare equal, but their bits
//! differ, so the exact permutation the introsort leaves among equal keys is checked.

use serde_json::{json, Value};
use ts_check::{harness, ret, Harness};
use ts_game::unity::collections::*;

struct Rng(u32);
impl Rng {
    fn next(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0
    }
}

/// `float.CompareTo` (the default comparer): NaN below everything and equal to NaN, ±0 equal.
fn fcmp(a: &f32, b: &f32) -> i32 {
    if a < b {
        -1
    } else if a > b {
        1
    } else if a == b {
        0
    } else if a.is_nan() {
        if b.is_nan() { 0 } else { -1 }
    } else {
        1
    }
}

const POOL: [u32; 9] = [0x0000_0000, 0x8000_0000, 0x7FC0_0000, 0x7FC0_0001, 0xFFC0_0002, 0x3F80_0000, 0x4000_0000, 0xBF80_0000, 0x4040_0000];

fn tie_array(r: &mut Rng, n: usize) -> Vec<f32> {
    let spread = 2 + r.next() % 8; // 2..=9 pool entries
    (0..n).map(|_| f32::from_bits(POOL[(r.next() % spread) as usize])).collect()
}

fn bytes(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|f| f.to_bits().to_le_bytes()).collect()
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn i32raw(v: i32) -> Value {
    json!({"raw": hex(&v.to_le_bytes())})
}

const HELPER: &str = "System.Collections.Generic.ArraySortHelper`1[[System.Single, mscorlib]]";

/// The game sorts `keys[index..index + length]` of the whole array; returns the array after.
fn game_sort(h: &mut Harness, keys: &[f32], index: usize, length: usize) -> Vec<u8> {
    let r = h.request(json!({"class": HELPER, "call": "Sort", "main": true,
        "args": [{"array": "System.Single", "raw": hex(&bytes(keys))}, i32raw(index as i32), i32raw(length as i32), {"null": true}],
        "param_types": ["System.Single[]", "System.Int32", "System.Int32", "System.Collections.Generic.IComparer<System.Single>"]}));
    assert_eq!(r["ok"], json!(true), "{r}");
    ret::hex_field(&r["args_after"][0])
}

/// McIlroy's adversary ("A Killer Adversary for Quicksort"), run on our port: values that drive median-of-three
/// quicksort deep, so the introsort falls back to heapsort.
fn killer(n: usize) -> Vec<f32> {
    let gas = n as i64;
    let mut val = vec![gas; n];
    let (mut nsolid, mut candidate) = (0i64, 0usize);
    let mut idx: Vec<usize> = (0..n).collect();
    let mut cmp = |x: &usize, y: &usize| -> i32 {
        let (x, y) = (*x, *y);
        if val[x] == gas && val[y] == gas {
            if x == candidate {
                val[x] = nsolid;
            } else {
                val[y] = nsolid;
            }
            nsolid += 1;
        }
        if val[x] == gas {
            candidate = x;
        } else if val[y] == gas {
            candidate = y;
        }
        (val[x] - val[y]).signum() as i32
    };
    sort_by(&mut idx, 0, n, n, &mut cmp);
    val.iter().map(|&v| v as f32).collect()
}

#[test]
fn sort_matches_the_game() {
    let Some(mut h) = harness() else { return };
    let mut r = Rng(51);
    let mut checked = 0;
    let mut heaps = 0;
    let mut lens: Vec<usize> = (0..=80).collect();
    lens.extend([100, 257, 1000, 3000]);
    for &n in &lens {
        for round in 0..6 {
            let keys = tie_array(&mut r, n);
            // whole array, or a sub-range (the depth limit still comes from the whole array's length)
            let (index, length) = if round % 2 == 0 || n < 4 {
                (0, n)
            } else {
                let i = (r.next() as usize) % (n / 2);
                (i, (r.next() as usize) % (n - i) + 1)
            };
            let mut ours = keys.clone();
            sort_by(&mut ours, index, length, n, &mut fcmp);
            assert_eq!(game_sort(&mut h, &keys, index, length), bytes(&ours), "sort n={n} index={index} length={length}");
            checked += 1;
        }
    }
    for n in [64, 200, 1000, 4000] {
        let keys = killer(n);
        let mut ours = keys.clone();
        heaps += sort_by(&mut ours, 0, n, n, &mut fcmp) as usize;
        assert_eq!(game_sort(&mut h, &keys, 0, n), bytes(&ours), "killer sort n={n}");
        checked += 1;
    }
    assert!(heaps >= 3, "the adversarial inputs should reach the heapsort fallback ({heaps})");
    eprintln!("Sort: {checked} sorts equal to the game ({heaps} through heapsort)");
}

#[test]
fn binary_search_matches_the_game() {
    let Some(mut h) = harness() else { return };
    let mut r = Rng(52);
    for n in 0..120usize {
        let mut a = tie_array(&mut r, n);
        sort_by(&mut a, 0, n, n, &mut fcmp);
        for _ in 0..8 {
            let v = f32::from_bits(POOL[(r.next() % 9) as usize]);
            let (index, length) = if n > 2 && r.next() % 2 == 0 { (1, n - 2) } else { (0, n) };
            let g = h.request(json!({"class": HELPER, "call": "BinarySearch", "main": true,
                "args": [{"array": "System.Single", "raw": hex(&bytes(&a))}, i32raw(index as i32), i32raw(length as i32), {"raw": hex(&v.to_bits().to_le_bytes())}, {"null": true}],
                "param_types": ["System.Single[]", "System.Int32", "System.Int32", "System.Single", "System.Collections.Generic.IComparer<System.Single>"]}));
            assert_eq!(g["ok"], json!(true), "{g}");
            assert_eq!(ret::i32(&g), binary_search(&a, index, length, &v, &mut fcmp), "BinarySearch n={n} {v:?}");
        }
    }
}

const DICT: &str = "System.Collections.Generic.Dictionary`2[[System.Int32, mscorlib],[System.Int32, mscorlib]]";
const SET: &str = "System.Collections.Generic.HashSet`1[[System.Int32, mscorlib]], System.Core";

fn new_obj(h: &mut Harness, class: &str) -> u64 {
    let r = h.request(json!({"new": class, "args": [], "main": true}));
    r["handle"].as_u64().unwrap_or_else(|| panic!("new {class}: {r}"))
}

fn on(h: &mut Harness, handle: u64, class: &str, m: &str, args: Vec<Value>, types: &[&str]) -> Value {
    let r = h.request(json!({"class": class, "call": m, "handle": handle, "args": args, "param_types": types, "main": true, "keep": true}));
    assert_eq!(r["ok"], json!(true), "{m}: {r}");
    r
}

fn ints(b: &[u8]) -> Vec<i32> {
    b.chunks(4).map(|c| i32::from_le_bytes(c.try_into().unwrap())).collect()
}

#[test]
fn dictionary_and_hashset_order_match_the_game() {
    let Some(mut h) = harness() else { return };
    let mut r = Rng(53);
    for round in 0..60 {
        let (dh, sh) = (new_obj(&mut h, DICT), new_obj(&mut h, SET));
        let mut dict: SlotMap<i32, i32> = SlotMap::new();
        let mut set: SlotMap<i32, ()> = SlotMap::new();
        for _ in 0..(20 + round * 4) {
            let k = (r.next() % 40) as i32 - 5;
            match r.next() % 10 {
                0..=3 => {
                    if !dict.contains_key(&k) {
                        on(&mut h, dh, DICT, "Add", vec![i32raw(k), i32raw(k * 3)], &["System.Int32", "System.Int32"]);
                        dict.add(k, k * 3);
                    }
                    on(&mut h, sh, SET, "Add", vec![i32raw(k)], &["System.Int32"]);
                    set.add(k, ());
                }
                4 => {
                    on(&mut h, dh, DICT, "set_Item", vec![i32raw(k), i32raw(-k)], &["System.Int32", "System.Int32"]);
                    dict.set(k, -k);
                }
                5..=8 => {
                    on(&mut h, dh, DICT, "Remove", vec![i32raw(k)], &["System.Int32"]);
                    dict.remove(&k);
                    on(&mut h, sh, SET, "Remove", vec![i32raw(k)], &["System.Int32"]);
                    set.remove(&k);
                }
                _ if r.next() % 8 == 0 => {
                    on(&mut h, dh, DICT, "Clear", vec![], &[]);
                    dict.clear();
                    on(&mut h, sh, SET, "Clear", vec![], &[]);
                    set.clear();
                }
                _ => {}
            }
        }
        // the game's iteration order: Keys / Values copied out, HashSet copied out
        let keys = on(&mut h, dh, DICT, "get_Keys", vec![], &[]);
        let kh = keys["handle"].as_u64().unwrap();
        let n = dict.len();
        let kc = on(&mut h, kh, &format!("System.Collections.Generic.Dictionary`2+KeyCollection[[System.Int32, mscorlib],[System.Int32, mscorlib]]"), "CopyTo",
            vec![json!({"array": "System.Int32", "raw": "00".repeat(4 * n)}), i32raw(0)], &["System.Int32[]", "System.Int32"]);
        assert_eq!(ints(&ret::hex_field(&kc["args_after"][0])), dict.keys().copied().collect::<Vec<_>>(), "Dictionary order, round {round}");
        let vals = on(&mut h, dh, DICT, "get_Values", vec![], &[]);
        let vh = vals["handle"].as_u64().unwrap();
        let vc = on(&mut h, vh, &format!("System.Collections.Generic.Dictionary`2+ValueCollection[[System.Int32, mscorlib],[System.Int32, mscorlib]]"), "CopyTo",
            vec![json!({"array": "System.Int32", "raw": "00".repeat(4 * n)}), i32raw(0)], &["System.Int32[]", "System.Int32"]);
        assert_eq!(ints(&ret::hex_field(&vc["args_after"][0])), dict.iter().map(|(_, v)| *v).collect::<Vec<_>>(), "Dictionary values, round {round}");
        let m = set.len();
        let sc = on(&mut h, sh, SET, "CopyTo", vec![json!({"array": "System.Int32", "raw": "00".repeat(4 * m)})], &["System.Int32[]"]);
        assert_eq!(ints(&ret::hex_field(&sc["args_after"][0])), set.keys().copied().collect::<Vec<_>>(), "HashSet order, round {round}");
    }
}
