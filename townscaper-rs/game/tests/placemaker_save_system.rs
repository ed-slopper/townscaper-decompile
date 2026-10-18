//! `ts_game::placemaker::save_system` (G3b). Our save system boots on the player's own save folder (read only:
//! a write fails the test) through a file-system storage container whose requests finish at once; against the
//! game (`TS_HARNESS=1`, the harness game shares that folder and has booted and refreshed on it): the menu's
//! order of towns, each meta's path, state, write time and flags, the counts, the boot and refresh states and
//! the settings must match. `GetUniqueSaveName` is compared with the game's in one batch with `DateTime.Now`.

use std::collections::VecDeque;
use std::path::PathBuf;

use serde_json::{json, Value};
use ts_check::{arg, harness, ret, Harness};
use ts_game::placemaker::save_data::SaveData;
use ts_game::placemaker::save_system::{MetaSaveId, RequestId, RequestState, SaveSystem, SaveSystemHost, StorageContainer};
use ts_game::unity::dotnet::{DateTime, UNIX_EPOCH_TICKS};

fn save_folder() -> Option<PathBuf> {
    let p = match std::env::var("TS_SAVES") {
        Ok(p) => PathBuf::from(p),
        Err(_) => PathBuf::from(std::env::var("USERPROFILE").ok()?).join("AppData/LocalLow/Oskar Stalberg/Townscaper"),
    };
    p.is_dir().then_some(p)
}

/// What a request gave.
#[derive(Clone, Debug)]
enum Result {
    Bytes(Option<Vec<u8>>),
    Bool(bool),
    List(Vec<String>),
}

/// The folder as a storage container, read only; every request done at once.
struct Folder {
    root: PathBuf,
    results: Vec<Result>,
    done: VecDeque<RequestId>,
}

impl Folder {
    fn add(&mut self, r: Result) -> RequestId {
        self.results.push(r);
        let id = self.results.len() as RequestId;
        self.done.push_back(id);
        id
    }
    fn list(dir: &PathBuf, rel: &str, out: &mut Vec<String>) {
        let mut entries: Vec<_> = std::fs::read_dir(dir).unwrap().flatten().collect();
        entries.sort_by_key(|e| e.file_name().to_string_lossy().to_lowercase());
        for e in entries {
            let name = e.file_name().to_string_lossy().into_owned();
            let r = if rel.is_empty() { name.clone() } else { format!("{rel}\\{name}") };
            if e.file_type().unwrap().is_dir() {
                Self::list(&e.path(), &r, out);
            } else {
                out.push(r);
            }
        }
    }
}

impl StorageContainer for Folder {
    fn load_bytes(&mut self, f: &str) -> RequestId {
        let b = std::fs::read(self.root.join(f.replace('\\', "/"))).ok();
        self.add(Result::Bytes(b))
    }
    fn save_bytes(&mut self, f: &str, _d: Vec<u8>) -> RequestId {
        panic!("read only: SaveBytes({f})")
    }
    fn delete(&mut self, f: &str) -> RequestId {
        panic!("read only: Delete({f})")
    }
    fn file_exists(&mut self, f: &str) -> RequestId {
        let e = self.root.join(f.replace('\\', "/")).is_file();
        self.add(Result::Bool(e))
    }
    fn list_files(&mut self) -> RequestId {
        let mut out = vec![];
        Self::list(&self.root.clone(), "", &mut out);
        self.add(Result::List(out))
    }
    fn move_next(&mut self, _r: RequestId) -> bool {
        false
    }
    fn state(&self, _r: RequestId) -> RequestState {
        RequestState::Success
    }
    fn bytes_result(&self, r: RequestId) -> Option<Vec<u8>> {
        match &self.results[r as usize - 1] {
            Result::Bytes(b) => b.clone(),
            _ => None,
        }
    }
    fn bool_result(&self, r: RequestId) -> bool {
        matches!(self.results[r as usize - 1], Result::Bool(true))
    }
    fn list_result(&self, r: RequestId) -> Option<Vec<String>> {
        match &self.results[r as usize - 1] {
            Result::List(l) => Some(l.clone()),
            _ => None,
        }
    }
}

struct Host {
    folder: Folder,
    time: f32,
    next_id: i32,
    logs: Vec<String>,
}

impl SaveSystemHost for Host {
    fn storage(&mut self) -> &mut dyn StorageContainer {
        &mut self.folder
    }
    fn unscaled_time(&self) -> f32 {
        self.time
    }
    fn now_hash(&self) -> i32 {
        1
    }
    fn utc_now_ticks(&self) -> i64 {
        0
    }
    fn last_write_time_utc_ticks(&self, path: &str) -> i64 {
        let m = std::fs::metadata(self.folder.root.join(path.replace('\\', "/"))).unwrap().modified().unwrap();
        let d = m.duration_since(std::time::UNIX_EPOCH).unwrap();
        (d.as_nanos() / 100) as i64 + UNIX_EPOCH_TICKS
    }
    fn log(&mut self, m: &str) {
        self.logs.push(m.to_string());
    }
    fn log_error(&mut self, m: &str) {
        self.logs.push(format!("ERROR {m}"));
    }
    fn world_any_change(&self) -> bool {
        false
    }
    fn set_world_any_change(&mut self, _v: bool) {}
    fn world_populate_save_data(&mut self, _s: &mut SaveData) {}
    fn world_load(&mut self, _s: &SaveData) {}
    fn world_new(&mut self, _s: &mut SaveData) {}
    fn world_set_dirty(&mut self) {}
    fn default_save_data(&self) -> SaveData {
        SaveData::default()
    }
    fn new_meta_save_object(&mut self) -> (i32, u64) {
        self.next_id -= 2;
        (self.next_id, 0)
    }
    fn load_image(&mut self, _t: u64, _j: Option<&[u8]>) {}
    fn set_active(&mut self, _m: MetaSaveId, _a: bool) {}
    fn on_new_meta_save(&mut self, _m: MetaSaveId) {}
    fn on_state_change(&mut self, _m: MetaSaveId) {}
    fn on_index_change(&mut self, _m: MetaSaveId, _i: u8) {}
    fn on_save_request_count_change(&mut self, _o: i32, _n: i32) {}
    fn on_total_save_game_count_update(&mut self, _c: u8) {}
    fn set_enabled(&mut self, _e: bool) {}
    fn default_path(&self, folder: &str) -> String {
        self.folder.root.join(folder).to_string_lossy().into_owned()
    }
    fn debug_ui_append(&mut self, _s: &str) {}
    fn create_directory(&mut self, p: &str) {
        panic!("read only: CreateDirectory({p})")
    }
    fn enumerate_files(&self, _d: &str) -> Vec<String> {
        vec![]
    }
    fn file_exists_sync(&self, p: &str) -> bool {
        std::path::Path::new(p).exists()
    }
    fn file_copy(&mut self, a: &str, _b: &str) {
        panic!("read only: Copy({a})")
    }
    fn file_delete(&mut self, p: &str) {
        panic!("read only: Delete({p})")
    }
}

/// Boot and refresh on the folder, finishing each request right after the step that made it.
fn boot(root: PathBuf) -> (SaveSystem, Host) {
    let mut host = Host { folder: Folder { root, results: vec![], done: VecDeque::new() }, time: 1.0, next_id: -1000, logs: vec![] };
    let mut s = SaveSystem::new(7410);
    let mut steps = 0;
    loop {
        host.time += 0.016;
        let more = s.iterate_boot(&mut host);
        while let Some(r) = host.folder.done.pop_front() {
            s.complete(&mut host, r);
        }
        steps += 1;
        assert!(steps < 10_000, "boot never ends");
        if !more {
            break;
        }
    }
    while s.iterate_refresh(&mut host) {
        host.time += 0.016;
        while let Some(r) = host.folder.done.pop_front() {
            s.complete(&mut host, r);
        }
        steps += 1;
        assert!(steps < 20_000, "refresh never ends");
    }
    (s, host)
}

#[test]
fn boots_on_the_save_folder() {
    let Some(root) = save_folder() else {
        eprintln!("no save folder: skipped");
        return;
    };
    let (s, host) = boot(root);
    for &m in &s.container {
        let ms = &s.metas[m];
        eprintln!("{} state {} written {} on disk {}", ms.path, ms.state, ms.last_file_write_time, ms.exists_on_disk);
    }
    eprintln!("boot {} refresh {} valid {} total {}; logs {:?}", s.boot_state, s.state, s.valid_meta_save_count, s.total_save_game_count, host.logs);
    assert!(!host.logs.iter().any(|l| l.starts_with("ERROR")), "{:?}", host.logs);
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
fn string_of(h: &mut Harness, obj: u64, field: &str) -> Option<String> {
    let r = h.request(json!({"field": field, "handle": obj, "main": true, "keep": true}));
    ok(&r, field);
    let sh = r["handle"].as_u64()?;
    let c = h.request(json!({"call": "System.String.Copy", "args": [{"handle": sh}], "param_types": ["System.String"], "main": true}));
    Some(c["string"].as_str()?.to_string())
}
fn raw_of(h: &mut Harness, obj: u64, field: &str) -> Vec<u8> {
    let r = h.request(json!({"field": field, "handle": obj, "main": true}));
    ok(&r, field);
    ret::bytes(&r)
}

#[test]
fn matches_the_game_after_its_boot() {
    let Some(root) = save_folder() else { return };
    let Some(mut guard) = harness() else { return };
    let h: &mut Harness = &mut guard;
    h.wait_for_object("Placemaker.SaveSystem");
    let ss = keep(h, json!({"call": "System.Type.GetType", "args": [arg::string("Placemaker.SaveSystem, Assembly-CSharp")], "param_types": ["System.String"], "main": true}), "type");
    let ss = keep(h, json!({"call": "UnityEngine.Object.FindObjectOfType", "args": [{"handle": ss}], "param_types": ["System.Type"], "main": true}), "SaveSystem");
    let i32_of = |h: &mut Harness, f: &str| i32::from_le_bytes(raw_of(h, ss, f)[..4].try_into().unwrap());
    let game_boot = i32_of(h, "bootState");
    let game_state = i32_of(h, "state");
    if game_state != 7 {
        eprintln!("the game's refresh is not done (state {game_state}): skipped");
        return;
    }
    let (ours, _) = boot(root);
    assert_eq!((ours.boot_state, ours.state), (game_boot, game_state), "boot and refresh states");
    assert_eq!(ours.valid_meta_save_count, i32_of(h, "validMetaSaveCount"), "validMetaSaveCount");
    assert_eq!(ours.total_save_game_count, i32_of(h, "totalSaveGameCount"), "totalSaveGameCount");
    let container = keep(h, json!({"field": "metaSaveContainer", "handle": ss, "main": true}), "container");
    let n = ret::i32(&h.request(json!({"call": "get_childCount", "class": "UnityEngine.Transform", "handle": container, "args": [], "main": true})));
    assert_eq!(n as usize, ours.container.len(), "town count");
    let mty = keep(h, json!({"call": "System.Type.GetType", "args": [arg::string("Placemaker.MetaSave, Assembly-CSharp")], "param_types": ["System.String"], "main": true}), "type");
    for i in 0..n {
        let t = keep(h, json!({"call": "GetChild", "class": "UnityEngine.Transform", "handle": container, "args": [arg::i32(i)], "main": true}), "child");
        let m = keep(h, json!({"call": "GetComponent", "class": "UnityEngine.Component", "handle": t, "args": [{"handle": mty}], "param_types": ["System.Type"], "main": true}), "MetaSave");
        let o = &ours.metas[ours.container[i as usize]];
        let what = format!("town {i}");
        assert_eq!(string_of(h, m, "path").as_deref(), Some(o.path.as_str()), "{what}: path");
        assert_eq!(string_of(h, m, "fileName").as_deref(), Some(o.file_name.as_str()), "{what}: fileName");
        assert_eq!(u32::from_le_bytes(raw_of(h, m, "state")[..4].try_into().unwrap()), o.state, "{what}: state");
        assert_eq!(i64::from_le_bytes(raw_of(h, m, "lastFileWriteTime")[..8].try_into().unwrap()), o.last_file_write_time, "{what}: lastFileWriteTime");
        assert_eq!(raw_of(h, m, "existsOnDisk")[0] != 0, o.exists_on_disk, "{what}: existsOnDisk");
        assert_eq!(raw_of(h, m, "isBin")[0] != 0, o.is_bin, "{what}: isBin");
    }
    let cur = keep(h, json!({"field": "currentMetaSave", "handle": ss, "main": true}), "current");
    let cur_name = string_of(h, cur, "fileName");
    assert_eq!(cur_name.as_deref(), ours.current_meta_save.map(|c| ours.metas[c].file_name.as_str()), "current town");
    let settings = keep(h, json!({"field": "settingsData", "handle": ss, "main": true}), "settingsData");
    let s = ours.settings_data.as_ref().unwrap();
    assert_eq!(string_of(h, settings, "lastSave"), s.last_save, "settings lastSave");
    assert_eq!(i32::from_le_bytes(raw_of(h, settings, "version")[..4].try_into().unwrap()), s.version, "settings version");
    eprintln!("{n} towns match; current {cur_name:?}");
}

#[test]
fn unique_name_matches_the_game() {
    let Some(mut guard) = harness() else { return };
    let h: &mut Harness = &mut guard;
    h.wait_for_object("Placemaker.SaveSystem");
    let ss = keep(h, json!({"call": "System.Type.GetType", "args": [arg::string("Placemaker.SaveSystem, Assembly-CSharp")], "param_types": ["System.String"], "main": true}), "type");
    let ss = keep(h, json!({"call": "UnityEngine.Object.FindObjectOfType", "args": [{"handle": ss}], "param_types": ["System.Type"], "main": true}), "SaveSystem");
    let mut matched = 0;
    for _ in 0..40 {
        let res = h.batch(vec![
            json!({"call": "System.DateTime.get_Now", "args": [], "main": true}),
            json!({"call": "GetUniqueSaveName", "class": "Placemaker.SaveSystem", "handle": ss, "args": [], "main": true}),
            json!({"call": "System.DateTime.get_Now", "args": [], "main": true}),
        ]);
        for r in &res {
            ok(r, "batch");
        }
        let before = DateTime { date_data: u64::from_le_bytes(ret::bytes(&res[0])[..8].try_into().unwrap()) };
        let after = DateTime { date_data: u64::from_le_bytes(ret::bytes(&res[2])[..8].try_into().unwrap()) };
        if before.ticks() != after.ticks() {
            continue; // the clock moved between the calls
        }
        assert_eq!(SaveSystem::get_unique_save_name(before.get_hash_code()), ret::string(&res[1]), "{before:?}");
        matched += 1;
    }
    assert!(matched > 10, "only {matched} rounds had a still clock");
    eprintln!("{matched} names match");
}
