//! `ts_game::placemaker::boot` (E8d). The boot already ran in the game the harness talks to, so the coroutines
//! and the sign-in are checked for their step order on a recording host; `_AaLineBoth` (set by
//! `BootMaster.OnEnable`) is checked against the running game (`TS_HARNESS=1`).

use serde_json::{json, Value};
use ts_check::{arg, harness, ret};
use ts_game::placemaker::boot::*;

/// A host that writes down every call and answers from a small script.
#[derive(Default)]
struct Rec {
    log: Vec<String>,
    /// frames until a scene load finishes
    load_frames: u32,
    main_loaded: bool,
    ui_loaded: bool,
    iterate_boot: u32,
    boot_state: i32,
    new_version: bool,
}

impl Rec {
    fn note(&mut self, s: impl Into<String>) {
        self.log.push(s.into());
    }
}

impl BootHost for Rec {
    fn set_instance(&mut self) {
        self.note("instance");
    }
    fn scene_is_loaded(&mut self, name: &str) -> bool {
        if name == MAIN_SCENE {
            self.main_loaded
        } else {
            self.ui_loaded
        }
    }
    fn load_scene_additive(&mut self, name: &str) -> SceneOp {
        self.note(format!("load {name}"));
        if name == MAIN_SCENE {
            0
        } else {
            1
        }
    }
    fn operation_is_done(&mut self, op: SceneOp) -> bool {
        if self.load_frames > 0 {
            self.load_frames -= 1;
            return false;
        }
        if op == 0 {
            self.main_loaded = true;
        } else {
            self.ui_loaded = true;
        }
        true
    }
    fn find_ui_master(&mut self) -> bool {
        self.note("find ui");
        true
    }
    fn find_world_master(&mut self) -> bool {
        self.note("find world");
        true
    }
    fn link_masters(&mut self) {
        self.note("link");
    }
    fn activate_ui_master(&mut self) {
        self.note("ui active");
    }
    fn disable_audio_listener(&mut self) {
        self.note("listener off");
    }
    fn ui_maybe_setup(&mut self) {
        self.note("ui setup");
    }
    fn set_world_master_enabled(&mut self, enabled: bool) {
        self.note(format!("world enabled {enabled}"));
    }
    fn activate_world_master(&mut self) {
        self.note("world active");
    }
    fn world_on_on_enable(&mut self) {
        self.note("world OnOnEnable");
    }
    fn world_on_start(&mut self) {
        self.note("world OnStart");
    }
    fn set_world_awaiting_dim(&mut self, value: bool) {
        self.note(format!("awaitingDim {value}"));
    }
    fn loading_start(&mut self) {
        self.note("loading start");
    }
    fn loading_stop_now(&mut self) {
        self.note("loading stop");
    }
    fn loading_show_logo(&mut self) {
        self.note("logo");
    }
    fn dim_set_state(&mut self, state: i32) {
        self.note(format!("dim {state}"));
    }
    fn ui_update_vsync(&mut self) {
        self.note("vsync");
    }
    fn ui_update_antialiasing(&mut self) {
        self.note("aa");
    }
    fn ui_on_settings_loaded(&mut self) {
        self.note("settings loaded");
    }
    fn save_menu_reset_card_order(&mut self) {
        self.note("card order");
    }
    fn save_menu_open_without_refresh(&mut self) {
        self.note("save menu");
    }
    fn ui_start_finish_loading(&mut self, instant: bool) {
        self.note(format!("finish loading {instant}"));
    }
    fn ui_close_everything(&mut self) {
        self.note("close everything");
    }
    fn ui_reset_users_ui_choices(&mut self) {
        self.note("reset choices");
    }
    fn has_save_system(&mut self) -> bool {
        true
    }
    fn save_reset_boot(&mut self) {
        self.note("reset boot");
    }
    fn save_iterate_boot(&mut self) -> bool {
        self.note("iterate boot");
        if self.iterate_boot > 0 {
            self.iterate_boot -= 1;
            true
        } else {
            false
        }
    }
    fn save_boot_state(&mut self) -> i32 {
        self.boot_state
    }
    fn maybe_update_game_version(&mut self) -> bool {
        self.note("version");
        self.new_version
    }
    fn texture_png_reset_color_and_material(&mut self) {
        self.note("png reset");
    }
    fn world_load_current_meta_save(&mut self) {
        self.note("load meta save");
    }
    fn world_new_town_and_load(&mut self) {
        self.note("new town");
    }
}

/// Runs a routine to its end, one entry per step with the yielded value.
fn run(mut step: impl FnMut(&mut Rec) -> Step, host: &mut Rec) -> Vec<bool> {
    let mut out = Vec::new();
    while let Some(v) = step(host) {
        out.push(v);
        host.note(format!("-> {v}"));
        assert!(out.len() < 100);
    }
    out
}

#[test]
fn first_boot_steps_in_the_games_order() {
    let mut host = Rec { load_frames: 2, iterate_boot: 2, boot_state: boot_state::LOAD_LAST, ..Default::default() };
    let BootCoroutine::First(mut w) = BootMaster::default().start() else { panic!() };
    run(|h| w.move_next(h), &mut host);
    let want = [
        "loading start", "instance", "-> false",
        "load Placemaker", "load FlatscreenUi", "-> false", "-> false",
        "find ui", "find world", "link", "-> true",
        "ui active", "listener off", "ui setup", "-> true",
        "world enabled false", "world active", "world OnOnEnable", "-> true",
        "world OnStart", "-> true",
        "world enabled true", "-> true",
        // the user boot starts in the step the game boot ends
        "reset boot", "iterate boot", "-> false", "iterate boot", "-> false", "iterate boot",
        "vsync", "aa", "settings loaded", "version", "card order",
        "load meta save", "awaitingDim false", "finish loading false",
    ];
    assert_eq!(host.log, want);
}

#[test]
fn user_boot_new_town_and_version() {
    let mut host = Rec { boot_state: boot_state::NEW_TOWN, new_version: true, ..Default::default() };
    let mut u = UserBootRoutine::default();
    assert!(run(|h| u.move_next(h), &mut host).is_empty());
    let want = [
        "reset boot", "iterate boot", "vsync", "aa", "settings loaded", "version", "png reset", "card order",
        "new town", "loading stop", "save menu", "awaitingDim false",
    ];
    assert_eq!(host.log, want);
    for (state, tail) in [(boot_state::DONE, vec!["finish loading false"]), (3, vec![])] {
        let mut host = Rec { boot_state: state, ..Default::default() };
        let mut u = UserBootRoutine::default();
        run(|h| u.move_next(h), &mut host);
        assert_eq!(host.log[7..], tail[..], "bootState {state}");
    }
}

/// A platform with scripted players and requests.
#[derive(Default)]
struct Plat {
    sdk: bool,
    buttons: Vec<bool>,
    sign_ins: Vec<i32>,
    status: i32,
    user: Option<i64>,
    stored: Option<i64>,
    log: Vec<String>,
}

impl Platform for Plat {
    fn has_sdk(&mut self) -> bool {
        self.sdk
    }
    fn players_any_button_down(&mut self) -> Vec<bool> {
        self.buttons.clone()
    }
    fn sign_in(&mut self, mode: i32) -> SignInRequest {
        self.sign_ins.push(mode);
        self.sign_ins.len() as u32
    }
    fn request_status(&mut self, _request: SignInRequest) -> i32 {
        self.status
    }
    fn user_id(&mut self) -> Option<i64> {
        self.user
    }
    fn signed_in_user(&mut self) -> Option<i64> {
        self.stored
    }
    fn set_signed_in_user(&mut self, id: Option<i64>) {
        self.stored = id;
    }
    fn log(&mut self, text: &str) {
        self.log.push(text.into());
    }
}

#[test]
fn sign_in_and_sign_out() {
    let mut p = Plat { sdk: true, buttons: vec![false, true, true], ..Default::default() };
    let mut core = BootGameCore::default();
    core.start_new_sign_in(&mut p);
    assert_eq!((core.state, core.prompt), (1, true));
    // the first player pressing asks, once, with the first-sign-in mode
    assert!(!core.update(&mut p));
    assert_eq!((core.selected_player, core.user_prompted, p.sign_ins.clone()), (Some(1), true, vec![3]));
    assert!(!core.update(&mut p));
    assert_eq!(p.sign_ins, [3]);
    // it fails: asked again with mode 2
    p.status = REQUEST_FAILED;
    core.on_complete(&mut p);
    assert!(core.sign_in_failed_once && core.selected_player.is_none() && !core.user_prompted && core.state == 1);
    core.update(&mut p);
    assert_eq!(p.sign_ins, [3, 2]);
    // it succeeds
    p.status = REQUEST_SUCCEEDED;
    p.user = Some(77);
    core.on_complete(&mut p);
    assert_eq!((core.state, core.prompt, p.stored), (2, false, Some(77)));
    assert!(!core.update(&mut p), "same user");
    p.user = Some(78);
    assert!(core.update(&mut p), "another user signs out");
    assert_eq!((core.state, p.log.clone()), (3, vec![SIGNED_OUT_LOG.to_string()]));

    // the signed-out boot: waits for the new sign-in, then boots the user
    let mut host = Rec { boot_state: boot_state::DONE, ..Default::default() };
    let BootCoroutine::SignedOut(mut w) = BootMaster::default().on_signed_out() else { panic!() };
    assert_eq!(w.move_next(&mut host, &mut core, &mut p), Some(false));
    assert_eq!(host.log, ["close everything", "reset choices", "logo", "dim 2"]);
    assert_eq!((core.state, core.sign_in_failed_once), (1, false));
    assert_eq!(w.move_next(&mut host, &mut core, &mut p), Some(false));
    core.update(&mut p);
    assert_eq!(p.sign_ins, [3, 2, 2], "only the very first sign-in uses mode 3");
    core.on_complete(&mut p);
    assert_eq!(w.move_next(&mut host, &mut core, &mut p), None);
    assert_eq!(host.log[4..7], ["loading start", "reset boot", "iterate boot"]);
    assert_eq!(host.log.last().unwrap(), "finish loading false");
}

#[test]
fn fullscreen_choice() {
    assert_eq!(do_fullscreen(2, &[(800, 600)]), Fullscreen::Resolution { width: i32::MAX, height: i32::MAX });
    assert_eq!(do_fullscreen(7, &[(800, 600), (1920, 1080)]), Fullscreen::Resolution { width: 1920, height: 1080 });
    assert_eq!(do_fullscreen(7, &[]), Fullscreen::Toggle);
}

fn ok(r: Value, what: &str) -> Value {
    assert_eq!(r["ok"], json!(true), "{what}: {r}");
    r
}

#[test]
fn aa_line_both_matches_the_game() {
    let Some(mut h) = harness() else { return };
    let scale = ret::f32(&ok(h.request(json!({"call": "Placemaker.Ui.UiScaler.GetScale", "args": [], "main": true})), "GetScale"));
    let id = ret::i32(&ok(h.request(json!({"call": "UnityEngine.Shader.PropertyToID", "args": [arg::string("_AaLineBoth")], "main": true})), "id"));
    let r = ok(h.request(json!({"call": "UnityEngine.Shader.GetGlobalVector", "args": [arg::i32(id)], "param_types": ["System.Int32"], "main": true})), "_AaLineBoth");
    let got: Vec<u32> = ret::bytes(&r).chunks(4).map(|c| u32::from_le_bytes(c.try_into().unwrap())).collect();
    let want: Vec<u32> = aa_line_both(scale).iter().map(|f| f.to_bits()).collect();
    assert_eq!(got, want, "UiScaler.GetScale = {scale}");
    // and the clamp at both ends
    assert_eq!(aa_line_both(0.5), [1.0; 4]);
    assert_eq!(aa_line_both(4.0), [1.0, 2.0, 2.0, 2.0]);
}
