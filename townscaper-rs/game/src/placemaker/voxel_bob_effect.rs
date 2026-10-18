//! `Placemaker.VoxelBobEffect` (row G1d): the pressed voxel sinking and springing back. A damped spring stepped
//! at 25 Hz of unscaled time drives `bobCurrent`; the shaders read the pressed position and the amount from
//! `_PressPosAmount` (`xyz` the position, `w` the bob).

use crate::unity::vector::{v4, Vector3, Vector4};

/// `VoxelBobEffect.pressPosAmountId` (`.cctor` 0x3D5090): `Shader.PropertyToID("_PressPosAmount")`.
pub const PRESS_POS_AMOUNT: &str = "_PressPosAmount";

/// The component's state (`enabled`: whether Unity runs its `Update`).
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct VoxelBobEffect {
    pub enabled: bool,
    pub bob_pos: Vector3,
    pub bob_current: f32,
    pub bob_velocity: f32,
    pub bob_time: f32,
}

impl VoxelBobEffect {
    /// `VoxelBobEffect.ClickBobEffect(position, up)` 0x3D4DE0: a kick of ±0.4 at `position`, running from now.
    pub fn click_bob_effect(&mut self, position: Vector3, up: bool, unscaled_time: f32) {
        self.bob_pos = position;
        self.enabled = true;
        self.bob_velocity = if up { 0.4 } else { -0.4 };
        self.bob_time = unscaled_time;
    }

    /// `VoxelBobEffect.SetPressed(position)` 0x3D4E40: held at `position` (the spring stops; the clicker sinks
    /// `bobCurrent` itself), the shader updated.
    pub fn set_pressed(&mut self, position: Vector3) -> Vector4 {
        self.enabled = false;
        self.bob_pos = position;
        self.update_shader()
    }

    /// `VoxelBobEffect.StopPressingNoVelocity` 0x3D4E70: the spring runs again from now.
    pub fn stop_pressing_no_velocity(&mut self, unscaled_time: f32) {
        self.enabled = true;
        self.bob_time = unscaled_time;
    }

    /// `VoxelBobEffect.StopPressing(velocity)` 0x3D4EA0: released with `velocity`, the spring runs from now.
    pub fn stop_pressing(&mut self, velocity: f32, unscaled_time: f32) {
        self.bob_velocity = velocity;
        self.enabled = true;
        self.bob_time = unscaled_time;
    }

    /// `VoxelBobEffect.UpdateShader` 0x3D4ED0: the global `_PressPosAmount = (bobPos, bobCurrent)` (returned for
    /// the host to set).
    pub fn update_shader(&self) -> Vector4 {
        v4(self.bob_pos.x, self.bob_pos.y, self.bob_pos.z, self.bob_current)
    }

    /// `VoxelBobEffect.Update` 0x3D4FB0 (while enabled): 25 Hz steps up to now, each
    /// `v = bobCurrent·-0.6 + bobVelocity`, `bobVelocity = v·0.4`, `bobCurrent += v`; settled below 0.001 the
    /// spring zeroes and stops. Then the shader. `now` is read again after every step (`Time.unscaledTime` does
    /// not change within a frame).
    pub fn update(&mut self, unscaled_time: f32) -> Vector4 {
        if self.bob_time < unscaled_time {
            loop {
                let v = self.bob_current * -0.6 + self.bob_velocity;
                self.bob_velocity = v * 0.4;
                self.bob_current = v + self.bob_current;
                self.bob_time += 0.04;
                if !(self.bob_time < unscaled_time) {
                    break;
                }
            }
        }
        if self.bob_velocity.abs() + self.bob_current.abs() < 0.001 {
            self.bob_current = 0.0;
            self.bob_velocity = 0.0;
            self.enabled = false;
        }
        self.update_shader()
    }
}
