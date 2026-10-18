//! `Placemaker.AudioSourcePool` (row G6a): a ring of sound effect sources, its object's children.

use crate::placemaker::audio::many_sounds_manager::{AudioSources, SourceId};

/// The component's state: the ring position, its size and the children's sources.
#[derive(Clone, Debug, PartialEq)]
pub struct AudioSourcePool {
    pub index: i32,
    pub count: i32,
    pub sources: Vec<SourceId>,
}

impl AudioSourcePool {
    /// `AudioSourcePool..ctor` 0x7B5250: 16 sources (the scene may set another count) around the template child.
    pub fn new(count: i32, template: SourceId) -> AudioSourcePool {
        AudioSourcePool { index: 0, count, sources: vec![template] }
    }

    /// `AudioSourcePool.Start` 0x7B51A0: the first child copied `count - 1` times (`Instantiate(child0, transform)`,
    /// `instantiate` giving each copy's source).
    pub fn start(&mut self, instantiate: &mut dyn FnMut() -> SourceId) {
        for _ in 1..self.count {
            let s = instantiate();
            self.sources.push(s);
        }
    }

    /// `AudioSourcePool.GetAudioSource` 0x7B50E0: the next source round the ring, stopped and reset (centred,
    /// pitch 1, full volume).
    pub fn get_audio_source(&mut self, audio: &mut dyn AudioSources) -> SourceId {
        self.index = (self.index + 1) % self.count;
        let s = self.sources[self.index as usize];
        audio.set_pan_stereo(s, 0.0);
        audio.set_pitch(s, 1.0);
        audio.set_volume(s, 1.0);
        audio.stop(s);
        s
    }
}
