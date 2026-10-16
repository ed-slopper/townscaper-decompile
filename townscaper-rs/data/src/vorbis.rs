//! Decode the Vorbis sounds of FSB5 banks (row F8e).
//!
//! FMOD keeps only the audio packets and a CRC32 of the setup header. We rebuild the identification and
//! setup headers with unpatched libvorbis ([`ts_vorbisref`]) for the clip's channels and rate at each VBR
//! quality from -0.1 to 1.0 (steps of 0.01) until the setup header's CRC matches, then decode with `lewton`.
//! All five setup headers of the reference build are found this way (`docs/notes/files.md`).

use std::collections::HashMap;
use std::fmt;
use std::sync::{Mutex, OnceLock};

use crate::audio::{self, Fsb5Sample};

#[derive(Debug)]
pub enum Error {
    NoVorbisCrc,
    /// No quality of libvorbis 1.3.4 writes a setup header with this CRC for these channels and rate.
    SetupNotFound { channels: u32, rate: u32, crc: u32 },
    Packets(crate::objects::Error),
    Header(lewton::header::HeaderReadError),
    Audio(lewton::audio::AudioReadError),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NoVorbisCrc => write!(f, "sample has no Vorbis setup CRC"),
            Error::SetupNotFound { channels, rate, crc } => {
                write!(f, "no libvorbis setup header with CRC {crc:08x} for {channels} ch {rate} Hz")
            }
            Error::Packets(e) => write!(f, "packets: {e}"),
            Error::Header(e) => write!(f, "header: {e:?}"),
            Error::Audio(e) => write!(f, "audio: {e:?}"),
        }
    }
}

impl std::error::Error for Error {}

type Key = (u32, u32, u32);

fn cache() -> &'static Mutex<HashMap<Key, Option<(Vec<u8>, Vec<u8>, f32)>>> {
    static C: OnceLock<Mutex<HashMap<Key, Option<(Vec<u8>, Vec<u8>, f32)>>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The identification and setup headers (and the quality that writes them) for a setup CRC.
pub fn headers_for(channels: u32, rate: u32, crc: u32) -> Option<(Vec<u8>, Vec<u8>, f32)> {
    let key = (channels, rate, crc);
    if let Some(v) = cache().lock().unwrap().get(&key) {
        return v.clone();
    }
    let found = (-10..=100).find_map(|qi| {
        let q = qi as f32 / 100.0;
        let (ident, setup) = ts_vorbisref::headers(channels, rate, q)?;
        (crc32fast::hash(&setup) == crc).then_some((ident, setup, q))
    });
    cache().lock().unwrap().insert(key, found.clone());
    found
}

/// Decoded sound: interleaved 16-bit samples.
#[derive(Debug, Clone)]
pub struct Pcm {
    pub channels: u32,
    pub rate: u32,
    pub samples: Vec<i16>,
    /// The VBR quality the clip was encoded at (from the matched setup header).
    pub quality: f32,
}

impl Pcm {
    pub fn frames(&self) -> usize {
        self.samples.len() / self.channels as usize
    }
}

/// Decode one FSB5 Vorbis sample of `bank` (the bank's bytes).
pub fn decode(bank: &[u8], s: &Fsb5Sample) -> Result<Pcm, Error> {
    let crc = s.vorbis_crc.ok_or(Error::NoVorbisCrc)?;
    let (ident, setup, quality) =
        headers_for(s.channels, s.frequency, crc).ok_or(Error::SetupNotFound { channels: s.channels, rate: s.frequency, crc })?;
    let ident = lewton::header::read_header_ident(&ident).map_err(Error::Header)?;
    let setup = lewton::header::read_header_setup(&setup, ident.audio_channels, (ident.blocksize_0, ident.blocksize_1))
        .map_err(Error::Header)?;
    let packets = audio::vorbis_packets(&bank[s.data.clone()]).map_err(Error::Packets)?;
    let mut pwr = lewton::audio::PreviousWindowRight::new();
    let ch = s.channels as usize;
    let mut out = Vec::with_capacity(s.samples as usize * ch);
    for p in packets {
        let block: Vec<Vec<i16>> = lewton::audio::read_audio_packet(&ident, &setup, p, &mut pwr).map_err(Error::Audio)?;
        let n = block.first().map_or(0, |c| c.len());
        for i in 0..n {
            for c in &block {
                out.push(c[i]);
            }
        }
    }
    out.truncate(s.samples as usize * ch);
    Ok(Pcm { channels: s.channels, rate: s.frequency, samples: out, quality })
}
