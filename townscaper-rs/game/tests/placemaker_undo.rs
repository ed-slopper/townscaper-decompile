//! G4: `Action.ToString` against the game (`TS_HARNESS=1`, every voxel type, signs, heights), and the undo /
//! redo buttons' click, hold and stepper logic on a fake maker.

use serde_json::json;
use ts_check::{harness, ret, Harness};
use ts_game::placemaker::graphs::{Action, VoxelType};
use ts_game::placemaker::ui::undo_redo_buttons::{UndoRedoButtons, UndoRedoHost};
use ts_game::unity::mathematics::Int2;

#[test]
fn action_text_matches_the_game() {
    let Some(mut guard) = harness() else { return };
    let h: &mut Harness = &mut guard;
    let mut n = 0;
    for (k, t) in (0..19i8).enumerate() {
        let a = Action::new(Int2 { x: (k as i32 - 9) * 37, y: -(k as i32) * 1000 }, (k * 13) as u8, VoxelType::from_i8(t), VoxelType::from_i8(18 - t), k as i32);
        let raw: Vec<u8> = [a.hex_pos.x.to_le_bytes().to_vec(), a.hex_pos.y.to_le_bytes().to_vec(), vec![a.height, a.in_type as u8, a.out_type as u8, a.pad], a.action_id.to_le_bytes().to_vec()].concat();
        let hex: String = raw.iter().map(|b| format!("{b:02x}")).collect();
        let r = h.request(json!({"call": "ToString", "class": "Placemaker.Graphs.Action", "this_raw": {"class": "Placemaker.Graphs.Action", "raw": hex}, "args": [], "main": true}));
        assert_eq!(r["ok"], true, "{r}");
        assert_eq!(a.to_string(), ret::string(&r), "{a:?}");
        n += 1;
    }
    eprintln!("{n} actions");
}

#[derive(Default)]
struct Maker {
    time: f32,
    undos: i32,
    redos: i32,
    holds: i32,
    disabled: [bool; 2],
    texts: [String; 2],
    squashes: i32,
    enabled: bool,
}

impl UndoRedoHost for Maker {
    fn unscaled_time(&self) -> f32 {
        self.time
    }
    fn undo_command(&mut self) -> bool {
        self.undos += 1;
        true
    }
    fn redo_command(&mut self) -> bool {
        self.redos += 1;
        true
    }
    fn hold_undo(&mut self) -> bool {
        self.holds += 1;
        self.undos += 1;
        true
    }
    fn hold_redo(&mut self) -> bool {
        self.holds += 1;
        self.redos += 1;
        true
    }
    fn button_disabled(&self, redo: bool) -> bool {
        self.disabled[redo as usize]
    }
    fn set_button_disabled(&mut self, redo: bool, d: bool) {
        self.disabled[redo as usize] = d;
    }
    fn set_steps_text(&mut self, redo: bool, t: &str) {
        self.texts[redo as usize] = t.to_string();
    }
    fn squash(&mut self, _redo: bool, f: f32) {
        assert_eq!(f, 0.95);
        self.squashes += 1;
    }
    fn set_enabled(&mut self, e: bool) {
        self.enabled = e;
    }
}

#[test]
fn buttons_click_hold_and_label() {
    let mut m = Maker::default();
    let mut b = UndoRedoButtons::default();
    b.on_start(&mut m);
    assert_eq!(m.texts, ["-".to_string(), "-".to_string()]);
    b.on_queue_change(&mut m, 3, 0);
    assert_eq!((m.texts[0].as_str(), m.disabled), ("3", [false, true]));
    // a click: one undo, a squash
    b.on_button_down_change(&mut m, false, true);
    b.on_button_down_change(&mut m, false, false);
    b.on_click(&mut m, false);
    assert_eq!((m.undos, m.squashes), (1, 1));
    // a hold: nothing for a second, then a step a frame; the click at the release does nothing more
    m.time = 10.0;
    b.on_button_down_change(&mut m, false, true);
    m.time = 10.5;
    b.update(&mut m);
    assert_eq!(m.holds, 0);
    m.time = 11.01;
    b.update(&mut m);
    b.update(&mut m);
    assert_eq!(m.holds, 2);
    b.on_click(&mut m, false);
    assert_eq!(m.undos, 3);
    b.on_button_down_change(&mut m, false, false);
    b.update(&mut m);
    assert!(!m.enabled);
    // the stepper
    b.on_stepper_step(&mut m, 1.0);
    assert_eq!(m.redos, 1);
}
