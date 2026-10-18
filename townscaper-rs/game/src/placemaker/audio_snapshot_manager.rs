//! `Placemaker.AudioSnapshotManager` (row G6b): the mixer's snapshot follows the window's focus and the menu dim.

/// Which of the two mixer snapshots to go to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Snapshot {
    InFocus,
    OutOfFocus,
}

/// The component's state.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AudioSnapshotManager {
    pub window_focused: bool,
    pub alpha_dim: bool,
    pub enabled: bool,
}

impl Default for AudioSnapshotManager {
    /// `AudioSnapshotManager..ctor` 0x7B50C0: focused and dimmed.
    fn default() -> AudioSnapshotManager {
        AudioSnapshotManager { window_focused: true, alpha_dim: true, enabled: true }
    }
}

impl AudioSnapshotManager {
    /// `AudioSnapshotManager.OnApplicationFocus(focus)` 0x7B4F80.
    pub fn on_application_focus(&mut self, focus: bool) {
        self.window_focused = focus;
        self.enabled = true;
    }

    /// `<IUiSetup.OnSetup>b__4_0(alpha)` 0x7B5040: what `IUiSetup.OnSetup` 0x7B4FB0 subscribes to the UI dim's
    /// alpha target (`uiMaster.dim.alphaState.SubscribeToTarget`): dimmed while it is above 0.
    pub fn on_dim_target(&mut self, alpha: f32) {
        self.alpha_dim = 0.0 < alpha;
        self.enabled = true;
    }

    /// `AudioSnapshotManager.Update` 0x7B5070: the in-focus snapshot when focused and not dimmed, else the
    /// out-of-focus one, for `TransitionTo(0.5)` (half a second); then it stops updating.
    pub fn update(&mut self) -> Snapshot {
        self.enabled = false;
        if !self.alpha_dim && self.window_focused {
            Snapshot::InFocus
        } else {
            Snapshot::OutOfFocus
        }
    }
}
