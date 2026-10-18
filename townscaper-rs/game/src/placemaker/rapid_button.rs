//! `Placemaker.RapidButton` (row G1c): a held button that repeats, slowly for its first presses and then fast
//! (undo / redo held down, the palette, UI navigation).

/// `Placemaker.RapidButton` (a struct: copied into the components that use it).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RapidButton {
    pub last_frame: i32,
    pub count: i32,
    pub t: f32,
    pub count_threshold: i32,
    pub slow_speed: f32,
    pub fast_speed: f32,
}

impl RapidButton {
    /// `Placemaker.RapidButton..ctor(slowSpeed, fastSpeed, countThreshold)` 0x846BE0.
    pub const fn new(slow_speed: f32, fast_speed: f32, count_threshold: i32) -> RapidButton {
        RapidButton { last_frame: 0, count: 0, t: 0.0, count_threshold, slow_speed, fast_speed }
    }

    /// `RapidButton.undoRedo` (`.cctor` 0x846AF0): 2 a second, then 10 after two repeats.
    pub const UNDO_REDO: RapidButton = RapidButton::new(2.0, 10.0, 2);
    /// `RapidButton.palette`: 3, then 6.
    pub const PALETTE: RapidButton = RapidButton::new(3.0, 6.0, 2);
    /// `RapidButton.uiNavigation`: 2, then 6.
    pub const UI_NAVIGATION: RapidButton = RapidButton::new(2.0, 6.0, 2);

    /// `Placemaker.RapidButton.Press(intensity = 1)` 0x846A20, called every frame the button is held: true on
    /// the first frame of a press (a frame gap restarts it), then each time `t` passes 1 as it grows by
    /// `(unscaledDeltaTime · speed) · intensity` (the slow speed below `countThreshold` repeats).
    pub fn press(&mut self, frame: i32, unscaled_delta_time: f32, intensity: f32) -> bool {
        if frame == self.last_frame {
            return false;
        }
        if frame != self.last_frame.wrapping_add(1) {
            self.last_frame = frame;
            self.t = 0.0;
            self.count = 0;
            return true;
        }
        let speed = if self.count < self.count_threshold { self.slow_speed } else { self.fast_speed };
        self.last_frame = frame;
        self.t = (unscaled_delta_time * speed) * intensity + self.t;
        if 1.0 < self.t {
            loop {
                self.t -= 1.0;
                if !(1.0 < self.t) {
                    break;
                }
            }
            self.count += 1;
            return true;
        }
        false
    }
}
