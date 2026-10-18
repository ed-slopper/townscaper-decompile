//! `Placemaker.UiAudio` (row G6a): the interface's sounds, played round a ring of ten sources (its children).

use crate::placemaker::audio::many_sounds_manager::{AudioSources, ClipId, SourceId};
use crate::unity::random::Rand;

/// The component's state: the ring position, the children's sources and the two button clips.
#[derive(Clone, Debug, PartialEq)]
pub struct UiAudio {
    pub index: i32,
    pub sources: Vec<SourceId>,
    pub button_down: Option<ClipId>,
    pub button_up: Option<ClipId>,
}

impl UiAudio {
    /// The constructor's state (`..ctor` is the shared empty one) over the template child.
    pub fn new(template: SourceId, button_down: Option<ClipId>, button_up: Option<ClipId>) -> UiAudio {
        UiAudio { index: 0, sources: vec![template], button_down, button_up }
    }

    /// `UiAudio.Start` 0x3C9600: the first child copied 9 times (`Instantiate(child0, transform)`).
    pub fn start(&mut self, instantiate: &mut dyn FnMut() -> SourceId) {
        for _ in 0..9 {
            let s = instantiate();
            self.sources.push(s);
        }
    }

    /// `UiAudio.GetAudioSource` 0x3C9480: the next source round the ring of children.
    pub fn get_audio_source(&mut self) -> SourceId {
        self.index = (self.index + 1) % self.sources.len() as i32;
        self.sources[self.index as usize]
    }

    /// `UiAudio.Play(clip, randomizePitch)` 0x3C9510: the clip on the next source, at pitch 1 or a random
    /// 0.8..1.2.
    pub fn play(&mut self, audio: &mut dyn AudioSources, random: &mut Rand, clip: Option<ClipId>, randomize_pitch: bool) -> SourceId {
        let s = self.get_audio_source();
        let pitch = if randomize_pitch { random.range(0.8, 1.2) } else { 1.0 };
        audio.set_pitch(s, pitch);
        audio.set_clip(s, clip);
        audio.play(s);
        s
    }
}
