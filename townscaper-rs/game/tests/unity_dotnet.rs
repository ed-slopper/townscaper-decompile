//! `ts_game::unity::dotnet` (E13) against the original game. The formatter table is compared with the one in
//! the player's `GameAssembly.dll` (when the install is there); the rest needs `TS_HARNESS=1`: `float`
//! `ToString("F<n>", en-US)`, `int` / `long` `ToString` and `Parse`, `String.Format`, `DateTime` `ToString`
//! (the game's formats), `GetHashCode` and the save system's `GetDateTimeLong`, on the game's own BCL.

use serde_json::{json, Value};
use ts_check::{arg, harness, ret, Harness};
use ts_game::unity::dotnet::{self, DateTime, ParseError};

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 16) as u32
    }
    fn u64(&mut self) -> u64 {
        (self.next() as u64) << 32 | self.next() as u64
    }
}

#[test]
fn formatter_table_matches_the_runtime() {
    let dll = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../work/harness/game/GameAssembly.dll");
    let Ok(d) = std::fs::read(&dll) else {
        eprintln!("no {}: skipped", dll.display());
        return;
    };
    // find the table by our first two entries
    let pat: Vec<u8> = [dotnet::mantissa_entry(0).0, dotnet::mantissa_entry(1).0].iter().flat_map(|v| v.to_le_bytes()).collect();
    let base = d.windows(16).position(|w| w == pat.as_slice()).expect("table not found in GameAssembly.dll");
    for e in 0..2048 {
        let m = u64::from_le_bytes(d[base + e * 8..base + e * 8 + 8].try_into().unwrap());
        let t = i32::from_le_bytes(d[base + 2048 * 8 + e * 4..base + 2048 * 8 + e * 4 + 4].try_into().unwrap());
        assert_eq!(dotnet::mantissa_entry(e as i32), (m, t), "entry {e}");
    }
}

fn ok(r: &Value, what: &str) {
    assert_eq!(r["ok"], true, "{what}: {r}");
}

/// Interesting floats: random bits, integers, decimal ties, tiny and huge values.
fn sample_floats(r: &mut Rng, n: usize) -> Vec<f32> {
    let mut v = vec![0.0, -0.0, 1.0, -1.0, 0.5, 16777217.0, 10000005.0, 1.0000005, 0.0000005, -0.0000004, f32::MAX, f32::MIN_POSITIVE, 1e-45, f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 123456789.0, 0.1, 9999999.5, 0.9999995, 999999.95];
    while v.len() < n {
        let x = match r.next() % 6 {
            0 => f32::from_bits(r.next()),
            1 => (r.next() % 100_000_000) as f32 * if r.next() % 2 == 0 { 1.0 } else { -1.0 },
            2 => ((r.next() % 10_000_000) as f32 + 0.5) * 10f32.powi((r.next() % 20) as i32 - 12),
            3 => (r.next() as f32 / u32::MAX as f32 - 0.5) * 200.0,
            4 => ((r.next() % 2_000_000) as f32 - 1_000_000.0) / 1_000_000.0 * 10f32.powi((r.next() % 8) as i32),
            _ => f32::from_bits(r.next() % 0x0100_0000 + 0x3F00_0000),
        };
        v.push(x);
    }
    v
}

#[test]
fn single_fixed_matches_the_game() {
    let Some(mut guard) = harness() else { return };
    let h: &mut Harness = &mut guard;
    let culture = h.request(json!({"new": "System.Globalization.CultureInfo", "args": [arg::string("en-US")], "param_types": ["System.String"], "main": true, "keep": true}));
    ok(&culture, "CultureInfo");
    let culture = culture["handle"].as_u64().unwrap();
    let mut r = Rng(0x1234_5678_9abc);
    let floats = sample_floats(&mut r, 20_000);
    let mut checked = 0;
    for precision in [6, 0, 2, 9] {
        let fmt = format!("F{precision}");
        let list: &[f32] = if precision == 6 { &floats } else { &floats[..3000] };
        for chunk in list.chunks(500) {
            let reqs: Vec<Value> = chunk
                .iter()
                .map(|x| {
                    json!({"call": "ToString", "class": "System.Single", "this_raw": {"class": "System.Single", "raw": hex(&x.to_le_bytes())}, "args": [arg::string(&fmt), {"handle": culture}], "param_types": ["System.String", "System.IFormatProvider"], "main": true})
                })
                .collect();
            for (x, res) in chunk.iter().zip(h.batch(reqs)) {
                ok(&res, "ToString");
                assert_eq!(dotnet::single_to_string_fixed(*x, precision), ret::string(&res), "{x:e} ({:08x}) {fmt}", x.to_bits());
                checked += 1;
            }
        }
    }
    eprintln!("{checked} float formats");
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

#[test]
fn integers_match_the_game() {
    let Some(mut guard) = harness() else { return };
    let h: &mut Harness = &mut guard;
    let mut r = Rng(0xfeed);
    // ToString
    let ints: Vec<i32> = (0..2000).map(|i| if i < 4 { [0, -1, i32::MIN, i32::MAX][i] } else { r.next() as i32 >> (r.next() % 31) }).collect();
    let reqs: Vec<Value> = ints.iter().map(|v| json!({"call": "ToString", "class": "System.Int32", "this_raw": {"class": "System.Int32", "raw": hex(&v.to_le_bytes())}, "args": [], "main": true})).collect();
    for (v, res) in ints.iter().zip(h.batch(reqs)) {
        ok(&res, "Int32.ToString");
        assert_eq!(dotnet::int_to_string(*v as i64), ret::string(&res), "{v}");
    }
    // Parse
    let mut strings: Vec<String> = vec![
        "0", "-0", "+5", " 12 ", "\t-7\r\n", "", " ", "-", "+-1", "1 2", "12a", "0x10", "2147483647", "2147483648", "-2147483648", "-2147483649",
        "9223372036854775807", "9223372036854775808", "-9223372036854775808", "-9223372036854775809", "00000000000000000000000001", "1.0", "１２",
        "20201018123456", "99999999999999999999",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    for _ in 0..500 {
        let v = r.u64() as i64 >> (r.next() % 63);
        strings.push(match r.next() % 4 {
            0 => v.to_string(),
            1 => format!("  {v}"),
            2 => format!("{v} "),
            _ => format!("{}", v as i32),
        });
    }
    for (class, int) in [("System.Int64", false), ("System.Int32", true)] {
        let reqs: Vec<Value> = strings.iter().map(|s| json!({"call": format!("{class}.Parse"), "args": [arg::string(s)], "param_types": ["System.String"], "main": true})).collect();
        for (s, res) in strings.iter().zip(h.batch(reqs)) {
            let ours: Result<i64, ParseError> = if int { dotnet::parse_int(s).map(|v| v as i64) } else { dotnet::parse_long(s) };
            if res["ok"] == true {
                let b = ret::bytes(&res);
                let game = if int { i32::from_le_bytes(b[..4].try_into().unwrap()) as i64 } else { i64::from_le_bytes(b[..8].try_into().unwrap()) };
                assert_eq!(ours, Ok(game), "{class}.Parse({s:?})");
            } else {
                let e = res["error"].as_str().unwrap_or("");
                let kind = if e.contains("Overflow") { ParseError::Overflow } else { ParseError::Format };
                assert!(e.contains("Overflow") || e.contains("Format"), "{class}.Parse({s:?}): {e}");
                assert_eq!(ours, Err(kind), "{class}.Parse({s:?}): {e}");
            }
        }
    }
    // String.Format with plain items
    let fmts = ["{0}{1}{2}", "{0}{1}{2}.png", "x = {0}, y = {1}, count = {2}", "{{{0}}} {2}{1}", "v {0} {1} {2}\n"];
    for f in fmts {
        let args = ["Town", "ab9Z", ".scape"];
        let res = h.request(json!({"call": "System.String.Format", "args": [arg::string(f), arg::string(args[0]), arg::string(args[1]), arg::string(args[2])], "param_types": ["System.String", "System.Object", "System.Object", "System.Object"], "main": true}));
        ok(&res, f);
        assert_eq!(dotnet::format(f, &args), ret::string(&res), "{f}");
    }
}

#[test]
fn date_time_matches_the_game() {
    let Some(mut guard) = harness() else { return };
    let h: &mut Harness = &mut guard;
    let mut r = Rng(0x5eed);
    let max = 3_155_378_975_999_999_999u64;
    let mut dates: Vec<DateTime> = vec![DateTime::from_ticks(0), DateTime::from_ticks(max as i64), DateTime::from_ticks(dotnet::UNIX_EPOCH_TICKS)];
    for i in 0..1500 {
        let ticks = if i % 3 == 0 { r.u64() % max } else { 630_000_000_000_000_000 + r.u64() % 20_000_000_000_000_000 };
        dates.push(DateTime::with_kind(ticks as i64, [dotnet::DateTimeKind::Unspecified, dotnet::DateTimeKind::Utc, dotnet::DateTimeKind::Local][(r.next() % 3) as usize]));
    }
    // (2000-02-29 and the years round the 100 / 400 rules)
    for y in [1, 4, 100, 400, 1600, 1900, 2000, 2004, 2100, 9999] {
        let days: i64 = (1..y).map(|yy: i64| if yy % 4 == 0 && (yy % 100 != 0 || yy % 400 == 0) { 366 } else { 365 }).sum();
        for d in [0, 58, 59, 60, 364, 365] {
            let t = (days + d) * 864_000_000_000;
            if (t as u64) < max {
                dates.push(DateTime::from_ticks(t));
            }
        }
    }
    for fmt in ["yyyyMMddHHmmss", "yyyy-MM-dd HH-mm-ss"] {
        for chunk in dates.chunks(500) {
            let reqs: Vec<Value> = chunk.iter().map(|d| json!({"call": "ToString", "class": "System.DateTime", "this_raw": {"class": "System.DateTime", "raw": hex(&d.date_data.to_le_bytes())}, "args": [arg::string(fmt)], "param_types": ["System.String"], "main": true})).collect();
            for (d, res) in chunk.iter().zip(h.batch(reqs)) {
                ok(&res, "DateTime.ToString");
                assert_eq!(d.to_string_custom(fmt), ret::string(&res), "{d:?} {fmt}");
            }
        }
    }
    for chunk in dates.chunks(500) {
        let mut reqs: Vec<Value> = chunk.iter().map(|d| json!({"call": "GetHashCode", "class": "System.DateTime", "this_raw": {"class": "System.DateTime", "raw": hex(&d.date_data.to_le_bytes())}, "args": [], "main": true})).collect();
        reqs.extend(chunk.iter().map(|d| json!({"call": "Placemaker.SaveSystem.GetDateTimeLong", "args": [arg::raw(&d.date_data.to_le_bytes())], "param_types": ["System.DateTime"], "main": true})));
        let res = h.batch(reqs);
        for (i, d) in chunk.iter().enumerate() {
            ok(&res[i], "GetHashCode");
            assert_eq!(d.get_hash_code(), ret::i32(&res[i]), "{d:?} GetHashCode");
            ok(&res[chunk.len() + i], "GetDateTimeLong");
            let b = ret::bytes(&res[chunk.len() + i]);
            let ours = dotnet::parse_long(&d.to_string_custom("yyyyMMddHHmmss")).unwrap();
            assert_eq!(i64::from_le_bytes(b[..8].try_into().unwrap()), ours, "{d:?} GetDateTimeLong");
        }
    }
    eprintln!("{} dates", dates.len());
}
