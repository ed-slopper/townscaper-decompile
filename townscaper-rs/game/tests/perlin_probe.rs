//! F11a working probe: save the game's PerlinNoise on fixed points (TS_HARNESS=1), then try candidate
//! formulas offline against them. Output goes to work/ (the game's numbers stay local).
use ts_check::{arg, harness, ret};

fn points() -> Vec<(f32, f32)> {
    let mut s = 16u32;
    let mut next = || { s ^= s << 13; s ^= s >> 17; s ^= s << 5; s };
    let mut v = vec![(0.0, 1.5), (0.5, 0.5), (1.25, 3.75), (-2.5, 7.125)];
    for _ in 0..3000 {
        v.push(((next() % 100000) as f32 / 977.0 - 30.0, (next() % 100000) as f32 / 613.0 - 50.0));
    }
    v
}

#[test]
fn save_game_perlin() {
    let Some(mut h) = harness() else { return };
    let mut out = String::new();
    for (x, y) in points() {
        let g = ret::f32(&h.call("UnityEngine.Mathf.PerlinNoise", vec![arg::f32(x), arg::f32(y)], &[], false).unwrap());
        out += &format!("{} {} {}\n", x.to_bits(), y.to_bits(), g.to_bits());
    }
    std::fs::write(concat!(env!("CARGO_MANIFEST_DIR"), "/../../work/perlin_game.txt"), out).unwrap();
}

const PERM: [u8; 256] = [151,160,137,91,90,15,131,13,201,95,96,53,194,233,7,225,140,36,103,30,69,142,8,99,37,240,21,10,23,190,6,148,247,120,234,75,0,26,197,62,94,252,219,203,117,35,11,32,57,177,33,88,237,149,56,87,174,20,125,136,171,168,68,175,74,165,71,134,139,48,27,166,77,146,158,231,83,111,229,122,60,211,133,230,220,105,92,41,55,46,245,40,244,102,143,54,65,25,63,161,1,216,80,73,209,76,132,187,208,89,18,169,200,196,135,130,116,188,159,86,164,100,109,198,173,186,3,64,52,217,226,250,124,123,5,202,38,147,118,126,255,82,85,212,207,206,59,227,47,16,58,17,182,189,28,42,223,183,170,213,119,248,152,2,44,154,163,70,221,153,101,155,167,43,172,9,129,22,39,253,19,98,108,110,79,113,224,232,178,185,112,104,218,246,97,228,251,34,242,193,238,210,144,12,191,179,162,241,81,51,145,235,249,14,239,107,49,192,214,31,181,199,106,157,184,84,204,176,115,121,50,45,127,4,150,254,138,236,205,93,222,114,67,29,24,72,243,141,128,195,78,66,215,61,156,180];
fn p(i: i32) -> i32 { PERM[(i & 255) as usize] as i32 }
fn grad<T: Copy + std::ops::Neg<Output = T> + std::ops::Add<Output = T> + From<f32>>(hash: i32, x: T, y: T) -> T {
    let h = hash & 15;
    let u = if h < 8 { x } else { y };
    let v = if h < 4 { y } else if h == 12 || h == 14 { x } else { T::from(0.0) };
    (if h & 1 == 0 { u } else { -u }) + (if h & 2 == 0 { v } else { -v })
}

fn variant(x: f32, y: f32, lerp: u8, fade: u8, fin: u8) -> f32 {
    let xi = (x as f64).floor() as i32;
    let yi = (y as f64).floor() as i32;
    let (x, y) = (x - xi as f32, y - yi as f32);
    let fd = |t: f32| match fade { 0 => t * t * t * (t * (t * 6.0 - 15.0) + 10.0), 1 => ((t * 6.0 - 15.0) * t + 10.0) * t * t * t, _ => t * t * t * (t * t * 6.0 - t * 15.0 + 10.0) };
    let lp = |t: f32, a: f32, b: f32| match lerp { 0 => a + t * (b - a), 1 => (1.0 - t) * a + t * b, _ => a * (1.0 - t) + b * t };
    let (u, v) = (fd(x), fd(y));
    let (xi, yi) = (xi & 255, yi & 255);
    let a = p(xi) + yi; let aa = p(a); let ab = p(a + 1);
    let b = p(xi + 1) + yi; let ba = p(b); let bb = p(b + 1);
    let n = lp(v, lp(u, grad(p(aa), x, y), grad(p(ba), x - 1.0, y)), lp(u, grad(p(ab), x, y - 1.0), grad(p(bb), x - 1.0, y - 1.0)));
    match fin {
        0 => (n + 0.69) / (0.793 + 0.69),
        1 => (n + 0.69) / 1.483,
        2 => (n + 0.69) * (1.0 / 1.483),
        3 => ((n as f64 + 0.69) / (0.793 + 0.69)) as f32,
        _ => ((n + 0.69) as f64 / (0.793f32 as f64 + 0.69f32 as f64)) as f32,
    }
}

fn variant_f64(x: f32, y: f32) -> f32 {
    let (x, y) = (x as f64, y as f64);
    let (xi, yi) = (x.floor() as i32, y.floor() as i32);
    let (x, y) = (x - xi as f64, y - yi as f64);
    let fd = |t: f64| t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
    let lp = |t: f64, a: f64, b: f64| a + t * (b - a);
    let (u, v) = (fd(x), fd(y));
    let (xi, yi) = (xi & 255, yi & 255);
    let a = p(xi) + yi; let aa = p(a); let ab = p(a + 1);
    let b = p(xi + 1) + yi; let ba = p(b); let bb = p(b + 1);
    let n = lp(v, lp(u, grad(p(aa), x, y), grad(p(ba), x - 1.0, y)), lp(u, grad(p(ab), x, y - 1.0), grad(p(bb), x - 1.0, y - 1.0)));
    ((n + 0.69) / (0.793 + 0.69)) as f32
}

#[test]
fn try_perlin_variants() {
    let Ok(text) = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../work/perlin_game.txt")) else { return };
    let rows: Vec<(f32, f32, f32)> = text.lines().map(|l| {
        let v: Vec<u32> = l.split(' ').map(|x| x.parse().unwrap()).collect();
        (f32::from_bits(v[0]), f32::from_bits(v[1]), f32::from_bits(v[2]))
    }).collect();
    for lerp in 0..3 { for fade in 0..3 { for fin in 0..5 {
        let ok = rows.iter().filter(|&&(x, y, g)| variant(x, y, lerp, fade, fin).to_bits() == g.to_bits()).count();
        println!("lerp {lerp} fade {fade} fin {fin}: {ok}/{}", rows.len());
    }}}
    let ok = rows.iter().filter(|&&(x, y, g)| variant_f64(x, y).to_bits() == g.to_bits()).count();
    println!("all f64: {ok}/{}", rows.len());
}
