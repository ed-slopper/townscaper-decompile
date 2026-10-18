//! `Placemaker.AmbientSound` (row G6b): the town's ambience fades to half volume while the window has focus and
//! to silence without it.

use crate::unity::mathf;

/// The component's state: whether it is enabled (it switches itself off once the volume is reached).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AmbientSound {
    pub enabled: bool,
}

impl AmbientSound {
    /// `AmbientSound.OnApplicationFocus(focus)` 0x7B2D50: back to work.
    pub fn on_application_focus(&mut self, _focus: bool) {
        self.enabled = true;
    }

    /// `AmbientSound.Update` 0x7B2D70: the source's volume (`volume`) moves towards 0.5 (focused,
    /// `Application.isFocused`) or 0 by a quarter a second of unscaled time; reached, it stops updating.
    pub fn update(&mut self, is_focused: bool, volume: &mut f32, unscaled_delta_time: f32) {
        let target = if is_focused { 0.5 } else { 0.0 };
        if *volume == target {
            self.enabled = false;
            return;
        }
        *volume = mathf::move_towards(*volume, target, unscaled_delta_time * 0.25);
    }
}
