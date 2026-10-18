//! `Placemaker.Ui.UndoRedoButtons` and `UndoRedoFeedback` (row G4): the undo and redo buttons. A click undoes
//! or redoes one step (unless a hold already did); holding a button for a second repeats through
//! `Maker.HoldUndo` / `HoldRedo` every frame; each step squashes the button (its scale states times 0.95); the
//! buttons show the queue lengths (`-` when empty) and are disabled at zero. The gamepad stepper on the same
//! object steps them too. The buttons, labels and scale states are the UI's (row G7e); the maker is the
//! world's (G1b), behind [`UndoRedoHost`].

use crate::unity::dotnet;

/// What the buttons reach.
pub trait UndoRedoHost {
    /// `Time.unscaledTime`.
    fn unscaled_time(&self) -> f32;
    /// `maker.UndoCommand()` / `RedoCommand()`.
    fn undo_command(&mut self) -> bool;
    fn redo_command(&mut self) -> bool;
    /// `maker.HoldUndo()` / `HoldRedo()`.
    fn hold_undo(&mut self) -> bool;
    fn hold_redo(&mut self) -> bool;
    /// The button's `disabled`.
    fn button_disabled(&self, redo: bool) -> bool;
    fn set_button_disabled(&mut self, redo: bool, disabled: bool);
    /// The step label's text.
    fn set_steps_text(&mut self, redo: bool, text: &str);
    /// The button's `StandardButton.scaleXY` / `scaleZ` currents multiplied (the press squash).
    fn squash(&mut self, redo: bool, factor: f32);
    /// `enabled` (the component runs `Update` while a button is held).
    fn set_enabled(&mut self, enabled: bool);
}

/// `UndoRedoButtons.initialDelay`: how long a press waits before repeating.
pub const INITIAL_DELAY: f32 = 1.0;

/// The component's state.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct UndoRedoButtons {
    pub is_undo_down: bool,
    pub is_redo_down: bool,
    pub undo_down_time: f32,
    pub redo_down_time: f32,
    pub has_undone: bool,
    pub has_redone: bool,
}

/// `NumberStrings.Get(n)`: the number as text.
fn number_string(n: i32) -> String {
    dotnet::int_to_string(n as i64)
}

impl UndoRedoButtons {
    /// `UndoRedoButtons.IUiSetup.OnStart` 0x3CCB40: both disabled, both labels `-`.
    pub fn on_start(&mut self, host: &mut dyn UndoRedoHost) {
        host.set_button_disabled(false, true);
        host.set_button_disabled(true, true);
        host.set_steps_text(false, "-");
        host.set_button_disabled(false, true);
        host.set_steps_text(true, "-");
        host.set_button_disabled(true, true);
    }

    /// `UndoRedoButtons.OnQueueChange(undoCount, redoCount)` 0x3CC5D0 (the maker's `onQueueChange`).
    pub fn on_queue_change(&mut self, host: &mut dyn UndoRedoHost, undo_count: i32, redo_count: i32) {
        let t = if 0 < undo_count { number_string(undo_count) } else { "-".into() };
        host.set_steps_text(false, &t);
        host.set_button_disabled(false, undo_count < 1);
        let t = if 0 < redo_count { number_string(redo_count) } else { "-".into() };
        host.set_steps_text(true, &t);
        host.set_button_disabled(true, redo_count < 1);
    }

    /// `undoDown` / `redoDown` (`<OnSetup>b__0` 0x502D20, `b__1` 0x502DA0): an enabled button squashes.
    fn button_down(host: &mut dyn UndoRedoHost, redo: bool) {
        if host.button_disabled(redo) {
            return;
        }
        host.squash(redo, 0.95);
    }

    /// The buttons' `onButtonDownChange` (`<OnSetup>b__2` 0x502E20, `b__3` 0x502E90): a press starts the delay.
    pub fn on_button_down_change(&mut self, host: &mut dyn UndoRedoHost, redo: bool, down: bool) {
        let (is_down, has_done, time) = if redo {
            (&mut self.is_redo_down, &mut self.has_redone, &mut self.redo_down_time)
        } else {
            (&mut self.is_undo_down, &mut self.has_undone, &mut self.undo_down_time)
        };
        if *is_down == down {
            return;
        }
        *is_down = down;
        host.set_enabled(true);
        if !down {
            return;
        }
        *has_done = false;
        *time = host.unscaled_time() + INITIAL_DELAY;
    }

    /// The buttons' `onClickAction` (`<OnSetup>b__4` 0x502F00, `b__5` 0x502F70): one step, unless a hold of this
    /// press already stepped.
    pub fn on_click(&mut self, host: &mut dyn UndoRedoHost, redo: bool) {
        let (down, done) = if redo { (self.is_redo_down, self.has_redone) } else { (self.is_undo_down, self.has_undone) };
        if down && done {
            return;
        }
        let ok = if redo { host.redo_command() } else { host.undo_command() };
        if ok {
            Self::button_down(host, redo);
        }
    }

    /// The gamepad stepper's step (`<OnSetup>b__6` 0x502FE0): forward redoes, back undoes (held).
    pub fn on_stepper_step(&mut self, host: &mut dyn UndoRedoHost, delta: f32) {
        if 0.0 < delta {
            host.hold_redo();
        } else {
            host.hold_undo();
        }
    }

    /// The gamepad stepper's press (`<OnSetup>b__7` 0x503040): one undo.
    pub fn on_stepper_press(&mut self, host: &mut dyn UndoRedoHost) {
        host.undo_command();
    }

    /// `UndoRedoButtons.Update` 0x3CCC20: nothing held: switched off; a button held past its delay repeats
    /// (the undo one first).
    pub fn update(&mut self, host: &mut dyn UndoRedoHost) {
        if self.is_undo_down {
            let t = host.unscaled_time();
            if self.undo_down_time < t {
                self.has_undone = true;
                if host.hold_undo() {
                    Self::button_down(host, false);
                }
            }
        } else if self.is_redo_down {
            let t = host.unscaled_time();
            if self.redo_down_time < t {
                self.has_redone = true;
                if host.hold_redo() {
                    Self::button_down(host, true);
                }
            }
        } else {
            host.set_enabled(false);
        }
    }
}

/// `UndoRedoFeedback.OnSetup` 0x3CCD10: its rect follows the screen's safe area (`UiMaster.SubscribeToSafeArea`,
/// G7a); `OnStart` 0x1D4430 does nothing.
pub fn undo_redo_feedback_on_setup(subscribe_to_safe_area: &mut dyn FnMut()) {
    subscribe_to_safe_area();
}
