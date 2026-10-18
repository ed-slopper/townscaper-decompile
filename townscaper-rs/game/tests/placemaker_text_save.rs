//! `ts_game::placemaker::text_save_system` (G3a). On the player's own saves (no harness): each file's
//! `saveString` must be our text of its corners and voxels, and must read back to them. Against the game
//! (`TS_HARNESS=1`): random towns written by both (`TextSaveSystem.SaveToString`), and random, damaged and
//! foreign strings read by both (`LoadFromString`: result, lists, and a throw), with the game's shared bit array
//! put in our starting state first so the leftovers past each string match.

use std::path::PathBuf;

use serde_json::{json, Value};
use ts_check::{arg, harness, ret, Harness};
use ts_game::placemaker::save_data::{SaveCorner, SaveVoxel};
use ts_game::placemaker::save_xml;
use ts_game::placemaker::text_save_system::{self as tss, BitArray, LoadError};

fn save_folder() -> Option<PathBuf> {
    let p = match std::env::var("TS_SAVES") {
        Ok(p) => PathBuf::from(p),
        Err(_) => PathBuf::from(std::env::var("USERPROFILE").ok()?).join("AppData/LocalLow/Oskar Stalberg/Townscaper"),
    };
    p.is_dir().then_some(p)
}

#[test]
fn real_saves_match_their_strings() {
    let Some(dir) = save_folder() else {
        eprintln!("no save folder: skipped");
        return;
    };
    let mut n = 0;
    let mut state = BitArray::default();
    for f in std::fs::read_dir(dir.join("Saves")).into_iter().flatten().flatten() {
        let p = f.path();
        if p.extension().and_then(|e| e.to_str()) != Some("scape") {
            continue;
        }
        let d = save_xml::read_save(&std::fs::read(&p).unwrap()).unwrap();
        let text = d.save_string.clone().unwrap();
        let ours = tss::save_to_string(&mut state, &d.corners, &d.voxels, &mut |m| panic!("{m}"));
        assert_eq!(ours, text, "{}", p.display());
        let (mut cs, mut vs) = (vec![], vec![]);
        tss::load_from_string(&mut state, &text, &mut cs, &mut vs, &mut |m| panic!("{m}")).unwrap();
        assert_eq!(cs, d.corners, "{}: corners", p.display());
        // (the ground voxel's type is not stored: it reads back as 15)
        let ground: Vec<SaveVoxel> = d.voxels.iter().map(|v| if v.h == 0 { SaveVoxel { t: 15, h: 0 } } else { *v }).collect();
        assert_eq!(vs, ground, "{}: voxels", p.display());
        eprintln!("{}: {} letters", p.file_name().unwrap().to_string_lossy(), text.len());
        n += 1;
    }
    eprintln!("{n} saves");
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

/// A town as the graph saves one: columns by x then y, each an optional ground voxel then rising heights.
fn random_town(r: &mut Rng) -> (Vec<SaveCorner>, Vec<SaveVoxel>) {
    let (mut cs, mut vs) = (vec![], vec![]);
    let n = r.next() % 40;
    let mut x = (r.next() % 600) as i32 - 300;
    let mut y = (r.next() % 600) as i32 - 300;
    let palette: Vec<u8> = (0..1 + r.next() % 6).map(|_| (r.next() % 15) as u8).collect();
    for _ in 0..n {
        if r.next() % 3 == 0 {
            x += (r.next() % 20) as i32;
            y = (r.next() % 600) as i32 - 300;
        } else {
            y += 1 + (r.next() % 30) as i32;
        }
        let mut count = 0;
        if r.next() % 3 == 0 {
            vs.push(SaveVoxel { t: (r.next() % 16) as u8, h: 0 });
            count += 1;
        }
        let mut h = 0u32;
        let tall = if r.next() % 20 == 0 { 255 } else { 12 };
        for _ in 0..(r.next() % 6) {
            h += 1 + r.next() % (if tall == 255 { 120 } else { 3 });
            if h > 255 {
                break;
            }
            vs.push(SaveVoxel { t: palette[(r.next() as usize) % palette.len()], h: h as u8 });
            count += 1;
        }
        cs.push(SaveCorner { x, y, count });
    }
    (cs, vs)
}

fn ok(r: &Value, what: &str) {
    assert_eq!(r["ok"], true, "{what}: {r}");
}
fn keep(h: &mut Harness, mut req: Value, what: &str) -> u64 {
    req["keep"] = json!(true);
    let r = h.request(req);
    ok(&r, what);
    r["handle"].as_u64().unwrap()
}
const LC: &str = "System.Collections.Generic.List`1[[Placemaker.SaveData+C, Assembly-CSharp]]";
const LV: &str = "System.Collections.Generic.List`1[[Placemaker.SaveData+V, Assembly-CSharp]]";
const PC: &str = "System.Collections.Generic.List<Placemaker.SaveData.C>";
const PV: &str = "System.Collections.Generic.List<Placemaker.SaveData.V>";

fn game_lists(h: &mut Harness, cs: &[SaveCorner], vs: &[SaveVoxel]) -> (u64, u64) {
    let lc = keep(h, json!({"new": LC, "args": [], "main": true}), "List<C>");
    let lv = keep(h, json!({"new": LV, "args": [], "main": true}), "List<V>");
    let mut reqs = vec![];
    for c in cs {
        let b: Vec<u8> = [c.x, c.y, c.count].iter().flat_map(|v| v.to_le_bytes()).collect();
        reqs.push(json!({"call": "Add", "class": LC, "handle": lc, "args": [arg::raw(&b)], "main": true}));
    }
    for v in vs {
        reqs.push(json!({"call": "Add", "class": LV, "handle": lv, "args": [arg::raw(&[v.t, v.h])], "main": true}));
    }
    for chunk in reqs.chunks(400) {
        for x in h.batch(chunk.to_vec()) {
            ok(&x, "Add");
        }
    }
    (lc, lv)
}

fn read_lists(h: &mut Harness, lc: u64, lv: u64) -> (Vec<SaveCorner>, Vec<SaveVoxel>) {
    let res = h.batch(vec![
        json!({"call": "ToArray", "class": LC, "handle": lc, "args": [], "main": true}),
        json!({"call": "ToArray", "class": LV, "handle": lv, "args": [], "main": true}),
    ]);
    let cb = ret::bytes(&res[0]);
    let vb = ret::bytes(&res[1]);
    let cs = cb.chunks(12).map(|c| {
        let i = |k: usize| i32::from_le_bytes(c[k..k + 4].try_into().unwrap());
        SaveCorner { x: i(0), y: i(4), count: i(8) }
    });
    (cs.collect(), vb.chunks(2).map(|v| SaveVoxel { t: v[0], h: v[1] }).collect())
}

#[test]
fn text_matches_the_game() {
    let Some(mut guard) = harness() else { return };
    let h: &mut Harness = &mut guard;
    // the game's shared bit array to our starting state: length 1, false
    let ba = h.request(json!({"static_field": "Placemaker.TextSaveSystem.bitArray", "main": true, "keep": true}));
    ok(&ba, "bitArray");
    let ba = ba["handle"].as_u64().unwrap();
    for x in h.batch(vec![
        json!({"call": "set_Length", "class": "System.Collections.BitArray", "handle": ba, "args": [arg::i32(1)], "main": true}),
        json!({"call": "Set", "class": "System.Collections.BitArray", "handle": ba, "args": [arg::i32(0), arg::bool(false)], "main": true}),
    ]) {
        ok(&x, "reset bitArray");
    }
    let mut state = BitArray::default();
    let mut r = Rng(0xBADC0DE);
    let mut written = vec![];
    for round in 0..150 {
        let (cs, vs) = random_town(&mut r);
        let (lc, lv) = game_lists(h, &cs, &vs);
        let res = h.request(json!({"call": "Placemaker.TextSaveSystem.SaveToString", "args": [{"handle": lc}, {"handle": lv}], "param_types": [PC, PV], "main": true}));
        ok(&res, "SaveToString");
        let ours = tss::save_to_string(&mut state, &cs, &vs, &mut |_| {});
        assert_eq!(ret::string(&res), ours, "round {round}: {cs:?} {vs:?}");
        written.push(ours);
    }
    // strings to read: the written ones, damaged ones, random letters, foreign characters, line ends
    let alphabet: Vec<char> = tss::ALPHABET.chars().collect();
    let mut texts: Vec<String> = written.clone();
    for w in &written {
        let mut c: Vec<char> = w.chars().collect();
        if c.len() > 2 {
            let i = (r.next() as usize) % c.len();
            c[i] = alphabet[(r.next() % 64) as usize];
            texts.push(c.iter().collect());
            texts.push(c[..c.len() / 2].iter().collect());
        }
    }
    for _ in 0..80 {
        let n = r.next() % 25;
        texts.push((0..n).map(|_| alphabet[(r.next() % 64) as usize]).collect());
    }
    texts.extend(["", "\r\n", "AAAA", "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA", "abc!", "A B", "\r\nAQAAAAAAAA\n", "-_-_-_", "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz"].map(String::from));
    let (mut rejected, mut threw, mut read) = (0, 0, 0);
    for (k, t) in texts.iter().enumerate() {
        let lc = keep(h, json!({"new": LC, "args": [], "main": true}), "List<C>");
        let lv = keep(h, json!({"new": LV, "args": [], "main": true}), "List<V>");
        let res = h.request(json!({"call": "Placemaker.TextSaveSystem.LoadFromString", "args": [arg::string(t), {"handle": lc}, {"handle": lv}], "param_types": ["System.String", PC, PV], "main": true}));
        let (mut cs, mut vs) = (vec![], vec![]);
        let mut logs = vec![];
        let ours = tss::load_from_string(&mut state, t, &mut cs, &mut vs, &mut |m| logs.push(m.to_string()));
        let what = format!("text {k} {t:?}: ours {ours:?} {logs:?}");
        if res["ok"] != true {
            assert!(matches!(ours, Err(LoadError::Exception(_))), "{what}: the game threw {res}");
            threw += 1;
            continue;
        }
        let game_ok = ret::bytes(&res)[0] != 0;
        assert_eq!(game_ok, ours.is_ok(), "{what}");
        if game_ok {
            read += 1;
        } else {
            rejected += 1;
        }
        let (gc, gv) = read_lists(h, lc, lv);
        assert_eq!((gc, gv), (cs, vs), "{what}: lists");
    }
    eprintln!("150 written; {} read: {read} towns, {rejected} rejected, {threw} threw", texts.len());
}
