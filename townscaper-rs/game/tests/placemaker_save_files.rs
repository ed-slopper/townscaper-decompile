//! `ts_game::placemaker::save_xml` (G3) on real files: every town and the settings file in the player's save
//! folder (`%USERPROFILE%/AppData/LocalLow/Oskar Stalberg/Townscaper`, or `TS_SAVES`) must read and write back
//! to the same bytes. Skipped when there is no save folder.

use std::path::PathBuf;
use ts_game::placemaker::save_xml;

fn save_folder() -> Option<PathBuf> {
    let p = match std::env::var("TS_SAVES") {
        Ok(p) => PathBuf::from(p),
        Err(_) => PathBuf::from(std::env::var("USERPROFILE").ok()?).join("AppData/LocalLow/Oskar Stalberg/Townscaper"),
    };
    p.is_dir().then_some(p)
}

#[test]
fn real_files_round_trip() {
    let Some(dir) = save_folder() else {
        eprintln!("no save folder: skipped");
        return;
    };
    let mut towns = 0;
    if let Ok(rd) = std::fs::read_dir(dir.join("Saves")) {
        for f in rd.flatten() {
            let p = f.path();
            if p.extension().and_then(|e| e.to_str()) != Some("scape") {
                continue;
            }
            let bytes = std::fs::read(&p).unwrap();
            let d = save_xml::read_save(&bytes).unwrap_or_else(|e| panic!("{}: {e:?}", p.display()));
            let again = save_xml::write_save(&d);
            assert!(again == bytes, "{}: written back differs (first difference at byte {:?})", p.display(), again.iter().zip(&bytes).position(|(a, b)| a != b));
            eprintln!("{}: {} corners, {} voxels, {} jpg bytes", p.file_name().unwrap().to_string_lossy(), d.corners.len(), d.voxels.len(), d.big_jpg.as_ref().map_or(0, |j| j.len()));
            towns += 1;
        }
    }
    let settings = dir.join("Sett.ings");
    if let Ok(bytes) = std::fs::read(&settings) {
        let s = save_xml::read_settings(&bytes).unwrap();
        assert!(save_xml::write_settings(&s) == bytes, "Sett.ings written back differs");
        eprintln!("Sett.ings: {s:?}");
    }
    eprintln!("{towns} towns");
}

/// Against the game's own serializers (`TS_HARNESS=1`): random save data and settings (escaping, null and empty
/// strings, empty lists, null and empty images, signs) built in the game and serialized there; ours must write
/// the same bytes and read them back to the same values.
mod harness_checks {
    use serde_json::{json, Value};
    use ts_check::{arg, harness, ret, Harness};
    use ts_game::placemaker::save_data::{SaveCorner, SaveData, SaveVoxel, SettingsData};
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
    fn hex(b: &[u8]) -> String {
        b.iter().map(|x| format!("{x:02x}")).collect()
    }

    /// Strings that test the writer's escaping.
    const STRINGS: [&str; 10] = ["", "plain", "a&b<c>d\"e'f", "line\r\nbreak\nand\rcr", "\ttab  spaces ", "Gr\u{e4}s\u{f6} \u{753a} \u{1f3e0}", "]]> <!-- -->", "  ", "&amp;", "x/y\\z"];

    fn pick(r: &mut Rng) -> Option<String> {
        match r.next() % 3 {
            0 => None,
            _ => Some(STRINGS[(r.next() % STRINGS.len() as u32) as usize].into()),
        }
    }

    fn random_save(r: &mut Rng) -> SaveData {
        let mut d = SaveData::default();
        d.save_string = match r.next() % 4 {
            0 => None,
            1 => Some(STRINGS[(r.next() % STRINGS.len() as u32) as usize].into()),
            _ => Some((0..(r.next() % 40)).map(|_| (b'A' + (r.next() % 26) as u8) as char).collect()),
        };
        d.last_write_time = (((r.next() as u64) << 32 | r.next() as u64) as i64) >> (r.next() % 64);
        d.cam.x = r.next() as u8;
        d.cam.y = r.next() as u8;
        d.cam.z = r.next() as u8;
        d.sun.h = r.next() as u8;
        d.sun.r = r.next() as u8;
        for _ in 0..(r.next() % 6) {
            d.corners.push(SaveCorner { x: (r.next() as i32) >> (r.next() % 32), y: -((r.next() % 500) as i32), count: (r.next() % 9) as i32 });
        }
        for _ in 0..(r.next() % 6) {
            d.voxels.push(SaveVoxel { t: r.next() as u8, h: r.next() as u8 });
        }
        d.big_jpg = match r.next() % 3 {
            0 => None,
            1 => Some(vec![]),
            _ => Some((0..(r.next() % 50)).map(|_| r.next() as u8).collect()),
        };
        d
    }

    fn random_settings(r: &mut Rng) -> SettingsData {
        let mut s = SettingsData::default();
        s.version = (r.next() as i32) >> (r.next() % 32);
        s.last_save = pick(r);
        s.current_color = r.next() as u8;
        s.audio_volume = r.next() as u8;
        s.anti_aliasing = r.next() % 2 == 0;
        s.axis.rotation_x_inverted = r.next() % 2 == 0;
        s.axis.rotation_y_inverted = r.next() % 2 == 0;
        s.v_sync = r.next() % 2 == 0;
        s.fullscreen = r.next() % 2 == 0;
        s.language = pick(r);
        s.screenshot.path = pick(r);
        s.screenshot.x = (r.next() as i32) >> (r.next() % 32);
        s.screenshot.y = -((r.next() % 9000) as i32);
        s.obj_export.path = pick(r);
        s.ui_size_percentage = r.next() as u8;
        s.last_run_version = pick(r);
        s
    }

    /// A kept game string (`None`: null) and its address (0 for null).
    fn game_string(h: &mut Harness, s: &Option<String>) -> Option<(u64, u32)> {
        let s = s.as_ref()?;
        let hd = keep(h, json!({"call": "System.String.Copy", "args": [arg::string(s)], "param_types": ["System.String"], "main": true}), "string");
        let a = h.request(json!({"addr": hd}))["addr"].as_u64().unwrap() as u32;
        Some((hd, a))
    }
    fn set_string(obj: u64, f: &str, v: Option<(u64, u32)>) -> Value {
        match v {
            None => json!({"field": f, "handle": obj, "set_null": true, "main": true}),
            Some((hd, _)) => json!({"field": f, "handle": obj, "set_handle": hd, "main": true}),
        }
    }

    fn serialize(h: &mut Harness, serializer: u64, obj: u64) -> Vec<u8> {
        let ms = keep(h, json!({"new": "System.IO.MemoryStream", "args": [], "main": true}), "MemoryStream");
        let res = h.batch(vec![
            json!({"call": "Serialize", "class": "System.Xml.Serialization.XmlSerializer", "handle": serializer, "args": [{"handle": ms}, {"handle": obj}], "param_types": ["System.IO.Stream", "System.Object"], "main": true}),
            json!({"call": "ToArray", "class": "System.IO.MemoryStream", "handle": ms, "args": [], "main": true}),
        ]);
        ok(&res[0], "Serialize");
        ok(&res[1], "ToArray");
        ret::bytes(&res[1])
    }

    #[test]
    fn writer_matches_the_game() {
        let Some(mut guard) = harness() else { return };
        let h: &mut Harness = &mut guard;
        h.wait_for_object("Placemaker.SaveSystem");
        let save_ser = keep(h, json!({"field": "saveXmlSerializer", "on": "Placemaker.SaveSystem", "main": true}), "saveXmlSerializer");
        let settings_ser = keep(h, json!({"field": "settingsXmlSerializer", "on": "Placemaker.SaveSystem", "main": true}), "settingsXmlSerializer");
        let mut r = Rng(0xC0FFEE);
        for round in 0..120 {
            let d = random_save(&mut r);
            let sd = keep(h, json!({"new": "Placemaker.SaveData", "args": [], "main": true}), "SaveData");
            let sh = game_string(h, &d.save_string);
            let mut reqs = vec![set_string(sd, "saveString", sh)];
            reqs.push(json!({"field": "lastWriteTime", "handle": sd, "set_raw": hex(&d.last_write_time.to_le_bytes()), "main": true}));
            reqs.push(json!({"field": "cam", "handle": sd, "set_raw": hex(&[d.cam.x, d.cam.y, d.cam.z]), "main": true}));
            reqs.push(json!({"field": "sun", "handle": sd, "set_raw": hex(&[d.sun.h, d.sun.r]), "main": true}));
            if let Some(j) = &d.big_jpg {
                let jh = keep(h, json!({"call": "System.Convert.FromBase64String", "args": [arg::string(&save_xml::to_base64(j))], "param_types": ["System.String"], "main": true}), "bytes");
                reqs.push(json!({"field": "bigJpg", "handle": sd, "set_handle": jh, "main": true}));
            }
            for x in h.batch(reqs) {
                ok(&x, "set");
            }
            let corners = keep(h, json!({"field": "corners", "handle": sd, "main": true}), "corners");
            let voxels = keep(h, json!({"field": "voxels", "handle": sd, "main": true}), "voxels");
            let lc = "System.Collections.Generic.List`1[[Placemaker.SaveData+C, Assembly-CSharp]]";
            let lv = "System.Collections.Generic.List`1[[Placemaker.SaveData+V, Assembly-CSharp]]";
            let mut reqs = vec![];
            for c in &d.corners {
                let b: Vec<u8> = [c.x, c.y, c.count].iter().flat_map(|v| v.to_le_bytes()).collect();
                reqs.push(json!({"call": "Add", "class": lc, "handle": corners, "args": [arg::raw(&b)], "main": true}));
            }
            for v in &d.voxels {
                reqs.push(json!({"call": "Add", "class": lv, "handle": voxels, "args": [arg::raw(&[v.t, v.h])], "main": true}));
            }
            for x in h.batch(reqs) {
                ok(&x, "Add");
            }
            let game = serialize(h, save_ser, sd);
            let ours = save_xml::write_save(&d);
            assert!(game == ours, "round {round}: {d:?}\ngame {}\nours {}", String::from_utf8_lossy(&game), String::from_utf8_lossy(&ours));
            // read back: a null string was left out, so it keeps the constructor's value; the game's own
            // reader gives the text
            let ms = keep(h, json!({"new": "System.IO.MemoryStream", "args": [arg::array("System.Byte", &game)], "param_types": ["System.Byte[]"], "main": true}), "MemoryStream");
            let back = keep(h, json!({"call": "Deserialize", "class": "System.Xml.Serialization.XmlSerializer", "handle": save_ser, "args": [{"handle": ms}], "param_types": ["System.IO.Stream"], "main": true}), "Deserialize");
            let gs = h.request(json!({"field": "saveString", "handle": back, "main": true, "keep": true}));
            ok(&gs, "saveString");
            let text = match gs["handle"].as_u64() {
                Some(sh) => {
                    let c = h.request(json!({"call": "System.String.Copy", "args": [{"handle": sh}], "param_types": ["System.String"], "main": true}));
                    ok(&c, "Copy");
                    Some(c["string"].as_str().unwrap().to_string())
                }
                None => None,
            };
            let mut expect = d.clone();
            expect.save_string = text;
            assert_eq!(save_xml::read_save(&game).unwrap(), expect, "round {round}: read back");
        }
        for round in 0..120 {
            let s = random_settings(&mut r);
            let so = keep(h, json!({"new": "Placemaker.SettingsData", "args": [], "main": true}), "SettingsData");
            let mut reqs = vec![json!({"field": "version", "handle": so, "set_raw": hex(&s.version.to_le_bytes()), "main": true})];
            let v = game_string(h, &s.last_save);
            reqs.push(set_string(so, "lastSave", v));
            reqs.push(json!({"field": "currentColor", "handle": so, "set_raw": hex(&[s.current_color]), "main": true}));
            reqs.push(json!({"field": "audioVolume", "handle": so, "set_raw": hex(&[s.audio_volume]), "main": true}));
            reqs.push(json!({"field": "antiAliasing", "handle": so, "set_raw": hex(&[s.anti_aliasing as u8]), "main": true}));
            reqs.push(json!({"field": "axis", "handle": so, "set_raw": hex(&[s.axis.rotation_x_inverted as u8, s.axis.rotation_y_inverted as u8]), "main": true}));
            reqs.push(json!({"field": "vSync", "handle": so, "set_raw": hex(&[s.v_sync as u8]), "main": true}));
            reqs.push(json!({"field": "fullscreen", "handle": so, "set_raw": hex(&[s.fullscreen as u8]), "main": true}));
            let v = game_string(h, &s.language);
            reqs.push(set_string(so, "language", v));
            reqs.push(json!({"field": "uiSizePercentage", "handle": so, "set_raw": hex(&[s.ui_size_percentage]), "main": true}));
            let v = game_string(h, &s.last_run_version);
            reqs.push(set_string(so, "lastRunVersion", v));
            // the structs holding a string: raw (the string's address, then the ints); the strings stay kept
            let p = game_string(h, &s.screenshot.path).map_or(0, |x| x.1);
            let raw: Vec<u8> = [p, s.screenshot.x as u32, s.screenshot.y as u32].iter().flat_map(|v| v.to_le_bytes()).collect();
            reqs.push(json!({"field": "screenshot", "handle": so, "set_raw": hex(&raw), "main": true}));
            let p = game_string(h, &s.obj_export.path).map_or(0, |x| x.1);
            reqs.push(json!({"field": "objExport", "handle": so, "set_raw": hex(&p.to_le_bytes()), "main": true}));
            for x in h.batch(reqs) {
                ok(&x, "set");
            }
            let game = serialize(h, settings_ser, so);
            let ours = save_xml::write_settings(&s);
            assert!(game == ours, "round {round}: {s:?}\ngame {}\nours {}", String::from_utf8_lossy(&game), String::from_utf8_lossy(&ours));
            // read back as the game's reader gives text (seen on the save data above): line ends folded to
            // LF, white space only as empty; left-out strings keep the constructor's values
            let norm = |v: &Option<String>| {
                v.as_ref().map(|t| {
                    let t = t.replace("\r\n", "\n").replace('\r', "\n");
                    if t.chars().all(|c| matches!(c, ' ' | '\t' | '\n')) {
                        String::new()
                    } else {
                        t
                    }
                })
            };
            let def = SettingsData::default();
            let mut expect = s.clone();
            expect.language = norm(&s.language);
            expect.last_save = norm(&s.last_save);
            expect.screenshot.path = norm(&s.screenshot.path);
            expect.obj_export.path = norm(&s.obj_export.path);
            expect.last_run_version = norm(&s.last_run_version);
            let s = expect.clone();
            expect.last_save = s.last_save.clone().or(def.last_save);
            expect.language = s.language.clone().or(def.language);
            expect.screenshot.path = s.screenshot.path.clone().or(def.screenshot.path);
            expect.obj_export.path = s.obj_export.path.clone().or(def.obj_export.path);
            expect.last_run_version = s.last_run_version.clone().or(def.last_run_version);
            assert_eq!(save_xml::read_settings(&game).unwrap(), expect, "round {round}: read back");
        }
    }
}
