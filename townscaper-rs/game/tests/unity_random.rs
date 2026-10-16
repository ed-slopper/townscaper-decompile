//! `ts_game::unity::random` against the original game's `UnityEngine.Random` (F11c). Needs `TS_HARNESS=1`.
//! The game draws from the same generator every frame (birds), so each round is one batch on the main thread:
//! set the state, draw, read the state, with no frame in between.

use serde_json::{json, Value};
use ts_check::{arg, harness, ret};
use ts_game::unity::random::Rand;

/// The managed `Random.state` / `InitState` are stripped from the build: the engine's icalls, called directly.
const INIT_STATE: u32 = 0x107A_74A0;
const GET_STATE: u32 = 0x107A_74D0;
const SET_STATE: u32 = 0x107A_74F0;

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn state_bytes(r: &Rand) -> Vec<u8> {
    r.s.iter().flat_map(|w| w.to_le_bytes()).collect()
}

fn native(va: u32, args: Vec<Value>) -> Value {
    json!({"native": "UnityPlayer.dll", "va": format!("0x{va:X}"), "args": args})
}

fn call(m: &str, args: Vec<Value>, types: &[&str]) -> Value {
    json!({"call": m, "args": args, "param_types": types})
}

#[derive(Debug)]
enum Draw {
    Value,
    Range(f32, f32),
    RangeInt(i32, i32),
    Sphere,
}

#[test]
fn random_matches_the_game() {
    let Some(mut h) = harness() else { return };
    let mut seeds = Rand { s: [1, 2, 3, 4] };
    let mut checked = 0;
    for round in 0..300 {
        // InitState, then read the state
        let seed = seeds.next_u32() as i32;
        let r = h.batch(vec![native(INIT_STATE, vec![json!({"u32": seed as u32})]), native(GET_STATE, vec![json!({"buf": "00".repeat(16)})])]);
        assert_eq!(ret::hex_field(&r[1]["bufs_after"][0]), state_bytes(&Rand::new(seed)), "InitState({seed})");

        // a mix of draws from a random state
        let start = Rand { s: [seeds.next_u32(), seeds.next_u32(), seeds.next_u32(), seeds.next_u32()] };
        let draws: Vec<Draw> = (0..24)
            .map(|i| match (round + i + seeds.next_u32() as usize) % 4 {
                0 => Draw::Value,
                1 => Draw::Range((seeds.next_u32() % 2000) as f32 / 7.0 - 100.0, (seeds.next_u32() % 2000) as f32 / 3.0 - 300.0),
                2 => Draw::RangeInt(seeds.next_u32() as i32 >> (seeds.next_u32() % 32), seeds.next_u32() as i32 >> (seeds.next_u32() % 32)),
                _ => Draw::Sphere,
            })
            .collect();
        let mut reqs = vec![native(SET_STATE, vec![json!({"buf": hex(&state_bytes(&start))})])];
        for d in &draws {
            reqs.push(match *d {
                Draw::Value => call("UnityEngine.Random.get_value", vec![], &[]),
                Draw::Range(a, b) => call("UnityEngine.Random.Range", vec![arg::f32(a), arg::f32(b)], &["System.Single", "System.Single"]),
                Draw::RangeInt(a, b) => call("UnityEngine.Random.Range", vec![arg::i32(a), arg::i32(b)], &["System.Int32", "System.Int32"]),
                Draw::Sphere => call("UnityEngine.Random.get_insideUnitSphere", vec![], &[]),
            });
        }
        reqs.push(native(GET_STATE, vec![json!({"buf": "00".repeat(16)})]));
        let replies = h.batch(reqs);
        let mut ours = start;
        for (d, g) in draws.iter().zip(&replies[1..]) {
            assert_eq!(g["ok"], json!(true), "{d:?}: {g}");
            let o: Vec<u8> = match *d {
                Draw::Value => ours.value().to_le_bytes().to_vec(),
                Draw::Range(a, b) => ours.range(a, b).to_le_bytes().to_vec(),
                Draw::RangeInt(a, b) => ours.range_int(a, b).to_le_bytes().to_vec(),
                Draw::Sphere => {
                    let v = ours.inside_unit_sphere();
                    [v.x, v.y, v.z].iter().flat_map(|f| f.to_le_bytes()).collect()
                }
            };
            assert_eq!(ret::bytes(g), o, "{d:?} in round {round}");
            checked += 1;
        }
        assert_eq!(ret::hex_field(&replies.last().unwrap()["bufs_after"][0]), state_bytes(&ours), "state after round {round}");
    }
    eprintln!("Random: {checked} draws equal to the game");
}
