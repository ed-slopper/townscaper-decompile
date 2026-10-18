//! `ts_game::placemaker::save_binary` (G3c) against the game's own `SaveSystem.binaryFormatter`
//! (`TS_HARNESS=1`): random save data (null and empty text, image and lists) built in the game with `Add`,
//! serialized there; ours must write the same bytes (the lists' capacities from their `Add`s) and read the
//! game's bytes back to the same data; and the game must read ours back to the same XML.

use serde_json::{json, Value};
use ts_check::{arg, harness, ret, Harness};
use ts_game::placemaker::save_binary;
use ts_game::placemaker::save_data::{SaveCorner, SaveData, SaveVoxel};
use ts_game::placemaker::save_xml;

struct Rng(u32);
impl Rng {
    fn next(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0
    }
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

fn random_save(r: &mut Rng) -> SaveData {
    let mut d = SaveData::default();
    d.save_string = match r.next() % 4 {
        0 => None,
        1 => Some(String::new()),
        2 => Some("Gr\u{e4}s\u{f6} \u{753a}".into()),
        _ => Some((0..(r.next() % 200)).map(|_| (b'A' + (r.next() % 26) as u8) as char).collect()),
    };
    d.last_write_time = (((r.next() as u64) << 32 | r.next() as u64) as i64) >> (r.next() % 64);
    d.cam.x = r.next() as u8;
    d.cam.y = r.next() as u8;
    d.cam.z = r.next() as u8;
    d.sun.h = r.next() as u8;
    d.sun.r = r.next() as u8;
    for _ in 0..(r.next() % 12) {
        d.corners.push(SaveCorner { x: (r.next() as i32) >> (r.next() % 32), y: -((r.next() % 500) as i32), count: (r.next() % 9) as i32 });
    }
    for _ in 0..(r.next() % 20) {
        d.voxels.push(SaveVoxel { t: r.next() as u8, h: r.next() as u8 });
    }
    d.big_jpg = match r.next() % 3 {
        0 => None,
        1 => Some(vec![]),
        _ => Some((0..(r.next() % 300)).map(|_| r.next() as u8).collect()),
    };
    d
}

#[test]
fn binary_matches_the_game() {
    let Some(mut guard) = harness() else { return };
    let h: &mut Harness = &mut guard;
    h.wait_for_object("Placemaker.SaveSystem");
    let bf = keep(h, json!({"field": "binaryFormatter", "on": "Placemaker.SaveSystem", "main": true}), "binaryFormatter");
    let xs = keep(h, json!({"field": "saveXmlSerializer", "on": "Placemaker.SaveSystem", "main": true}), "saveXmlSerializer");
    let bfc = "System.Runtime.Serialization.Formatters.Binary.BinaryFormatter";
    let mut r = Rng(0x5A5A);
    for round in 0..120 {
        let d = random_save(&mut r);
        let sd = keep(h, json!({"new": "Placemaker.SaveData", "args": [], "main": true}), "SaveData");
        let mut reqs = vec![];
        match &d.save_string {
            None => reqs.push(json!({"field": "saveString", "handle": sd, "set_null": true, "main": true})),
            Some(s) => {
                let sh = keep(h, json!({"call": "System.String.Copy", "args": [arg::string(s)], "param_types": ["System.String"], "main": true}), "string");
                reqs.push(json!({"field": "saveString", "handle": sd, "set_handle": sh, "main": true}));
            }
        }
        reqs.push(json!({"field": "lastWriteTime", "handle": sd, "set_raw": d.last_write_time.to_le_bytes().iter().map(|b| format!("{b:02x}")).collect::<String>(), "main": true}));
        reqs.push(json!({"field": "cam", "handle": sd, "set_raw": format!("{:02x}{:02x}{:02x}", d.cam.x, d.cam.y, d.cam.z), "main": true}));
        reqs.push(json!({"field": "sun", "handle": sd, "set_raw": format!("{:02x}{:02x}", d.sun.h, d.sun.r), "main": true}));
        if let Some(j) = &d.big_jpg {
            let jh = keep(h, json!({"call": "System.Convert.FromBase64String", "args": [arg::string(&save_xml::to_base64(j))], "param_types": ["System.String"], "main": true}), "bytes");
            reqs.push(json!({"field": "bigJpg", "handle": sd, "set_handle": jh, "main": true}));
        }
        let corners = keep(h, json!({"field": "corners", "handle": sd, "main": true}), "corners");
        let voxels = keep(h, json!({"field": "voxels", "handle": sd, "main": true}), "voxels");
        for c in &d.corners {
            let b: Vec<u8> = [c.x, c.y, c.count].iter().flat_map(|v| v.to_le_bytes()).collect();
            reqs.push(json!({"call": "Add", "class": LC, "handle": corners, "args": [arg::raw(&b)], "main": true}));
        }
        for v in &d.voxels {
            reqs.push(json!({"call": "Add", "class": LV, "handle": voxels, "args": [arg::raw(&[v.t, v.h])], "main": true}));
        }
        for x in h.batch(reqs) {
            ok(&x, "build");
        }
        let ms = keep(h, json!({"new": "System.IO.MemoryStream", "args": [], "main": true}), "MemoryStream");
        let res = h.batch(vec![
            json!({"call": "Serialize", "class": bfc, "handle": bf, "args": [{"handle": ms}, {"handle": sd}], "param_types": ["System.IO.Stream", "System.Object"], "main": true}),
            json!({"call": "ToArray", "class": "System.IO.MemoryStream", "handle": ms, "args": [], "main": true}),
        ]);
        ok(&res[0], "Serialize");
        let game = ret::bytes(&res[1]);
        let ours = save_binary::write_save(&d, save_binary::capacity_after_adds(d.corners.len()), save_binary::capacity_after_adds(d.voxels.len()));
        let first = game.iter().zip(&ours).position(|(a, b)| a != b);
        assert!(game == ours, "round {round}: bytes differ at {first:?} (game {} bytes, ours {})", game.len(), ours.len());
        assert_eq!(save_binary::read_save(&game).unwrap(), Some(d.clone()), "round {round}: read");
        // the game reads ours (written with other capacities) to the same data: compared as XML
        let ours = save_binary::write_save(&d, d.corners.len() + (r.next() % 5) as usize, d.voxels.len());
        let ms = keep(h, json!({"new": "System.IO.MemoryStream", "args": [arg::array("System.Byte", &ours)], "param_types": ["System.Byte[]"], "main": true}), "MemoryStream");
        let back = keep(h, json!({"call": "Deserialize", "class": bfc, "handle": bf, "args": [{"handle": ms}], "param_types": ["System.IO.Stream"], "main": true}), "Deserialize");
        let ms = keep(h, json!({"new": "System.IO.MemoryStream", "args": [], "main": true}), "MemoryStream");
        let res = h.batch(vec![
            json!({"call": "Serialize", "class": "System.Xml.Serialization.XmlSerializer", "handle": xs, "args": [{"handle": ms}, {"handle": back}], "param_types": ["System.IO.Stream", "System.Object"], "main": true}),
            json!({"call": "ToArray", "class": "System.IO.MemoryStream", "handle": ms, "args": [], "main": true}),
        ]);
        ok(&res[0], "XML");
        assert!(ret::bytes(&res[1]) == save_xml::write_save(&d), "round {round}: the game read ours differently");
    }
}
