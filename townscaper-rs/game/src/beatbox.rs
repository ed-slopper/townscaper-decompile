//! `Beatbox` (global namespace, row G6b): a 100 bpm clock. Nothing in the build calls it.

use crate::unity::mathf;

/// `Beatbox.beatScale`: beats per second at 100 bpm, `1 / beatLength` (0.6 s).
pub const BEAT_SCALE: f32 = 1.6666666;

/// `Beatbox.GetBeat(time, scale, offset)` 0x7B61A0: the nearest beat number, `RoundToInt(time × beatScale ×
/// scale + offset)`.
pub fn get_beat(time: f32, scale: f32, offset: f32) -> i32 {
    mathf::round_to_int(time * BEAT_SCALE * scale + offset)
}

/// `Beatbox.IsBeat(scale, offset)` 0x7B61D0: whether the nearest beat changes within this frame
/// (`Time.time` → `Time.time + Time.deltaTime`).
pub fn is_beat(time: f32, delta_time: f32, scale: f32, offset: f32) -> bool {
    let a = mathf::round_to_int(time * BEAT_SCALE * scale + offset);
    let b = mathf::round_to_int((delta_time + time) * BEAT_SCALE * scale + offset);
    a != b
}
