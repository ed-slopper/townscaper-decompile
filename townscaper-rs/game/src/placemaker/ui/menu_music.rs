//! `Placemaker.Ui.MenuMusic` (row G6b): the menu's music, a random clip faded in and out.

use crate::placemaker::audio::many_sounds_manager::ClipId;
use crate::unity::mathf;
use crate::unity::random::Rand;

/// The `AudioSource` (`GetComponent<AudioSource>()` in `OnSetup` 0x32CB90) members the music uses.
pub trait MusicSource {
    fn volume(&self) -> f32;
    fn set_volume(&mut self, v: f32);
    fn is_playing(&self) -> bool;
    fn set_clip(&mut self, clip: Option<ClipId>);
    fn play(&mut self);
    fn stop(&mut self);
}

/// The component's state.
#[derive(Clone, Debug, PartialEq)]
pub struct MenuMusic {
    pub target_volume: f32,
    pub speed: f32,
    pub audio_clips: Vec<ClipId>,
    pub enabled: bool,
}

impl MenuMusic {
    /// `MenuMusic..ctor` 0x32CEE0 (speed 1) with the scene's clips.
    pub fn new(audio_clips: Vec<ClipId>) -> MenuMusic {
        MenuMusic { target_volume: 0.0, speed: 1.0, audio_clips, enabled: true }
    }

    /// `MenuMusic.StartPlaying()` 0x32CBD0: towards full volume; unless already playing, a random clip (when it
    /// has any) starts at full volume.
    pub fn start_playing(&mut self, src: &mut dyn MusicSource, random: &mut Rand) {
        self.enabled = true;
        self.target_volume = 1.0;
        self.speed = 1.0;
        if src.is_playing() {
            return;
        }
        if !self.audio_clips.is_empty() {
            let k = random.range_int(0, self.audio_clips.len() as i32);
            src.set_clip(Some(self.audio_clips[k as usize]));
        }
        src.play();
        src.set_volume(1.0);
    }

    /// `MenuMusic.StartPlaying(clip)` 0x32CCC0: the clip set, then [`start_playing`](Self::start_playing), which
    /// replaces it with a random one of its own when not already playing.
    pub fn start_playing_clip(&mut self, src: &mut dyn MusicSource, random: &mut Rand, clip: Option<ClipId>) {
        src.set_clip(clip);
        self.start_playing(src, random);
    }

    /// `MenuMusic.StopPlaying(timer)` 0x32CDC0: towards silence, `speed = 1 / timer` (which `Update` does not
    /// use).
    pub fn stop_playing(&mut self, timer: f32) {
        self.enabled = true;
        self.target_volume = 0.0;
        self.speed = 1.0 / timer;
    }

    /// `MenuMusic.Update` 0x32CDF0: the volume moves towards the target by one a second (`Time.deltaTime`);
    /// reached, it stops updating, and stops the music at silence.
    pub fn update(&mut self, src: &mut dyn MusicSource, delta_time: f32) {
        if src.volume() == self.target_volume {
            self.enabled = false;
            if self.target_volume == 0.0 {
                src.stop();
            }
            return;
        }
        let v = mathf::move_towards(src.volume(), self.target_volume, delta_time);
        src.set_volume(v);
    }
}
