//! Unity 2020.3 `AudioClip` (class 83) and the FMOD FSB5 banks its sound lives in (`.resource`) (row F8e).
//! Every clip of the reference build is FSB5 with Vorbis (mode 15); decoding is in [`vorbis`](crate::vorbis).

use crate::objects::{Error, Reader};

pub const CLASS_ID: i32 = 83;

#[derive(Debug, Clone)]
pub struct AudioClip {
    pub name: String,
    pub load_type: i32,
    pub channels: i32,
    pub frequency: i32,
    pub bits_per_sample: i32,
    /// Seconds.
    pub length: f32,
    pub is_tracker_format: bool,
    pub ambisonic: bool,
    pub subsound_index: i32,
    pub preload_audio_data: bool,
    pub load_in_background: bool,
    pub legacy_3d: bool,
    /// The bank's file (`sharedassets1.resource`), offset and size.
    pub source: String,
    pub offset: u64,
    pub size: u64,
    /// `AudioCompressionFormat`: 0 PCM, 1 Vorbis, 2 ADPCM, 3 MP3, ...
    pub compression_format: i32,
}

impl AudioClip {
    pub fn read(b: &[u8]) -> Result<AudioClip, Error> {
        let mut r = Reader::new(b);
        let name = r.string()?;
        let load_type = r.i32()?;
        let channels = r.i32()?;
        let frequency = r.i32()?;
        let bits_per_sample = r.i32()?;
        let length = r.f32()?;
        let is_tracker_format = r.bool()?;
        let ambisonic = r.bool()?;
        r.align4();
        let subsound_index = r.i32()?;
        let preload_audio_data = r.bool()?;
        let load_in_background = r.bool()?;
        let legacy_3d = r.bool()?;
        r.align4();
        let source = r.string()?;
        let offset = r.i64()? as u64;
        let size = r.i64()? as u64;
        let compression_format = r.i32()?;
        r.finish()?;
        Ok(AudioClip {
            name,
            load_type,
            channels,
            frequency,
            bits_per_sample,
            length,
            is_tracker_format,
            ambisonic,
            subsound_index,
            preload_audio_data,
            load_in_background,
            legacy_3d,
            source,
            offset,
            size,
            compression_format,
        })
    }
}

/// FSB5 codec ids (`mode` in the bank header).
pub mod codec {
    pub const PCM16: u32 = 2;
    pub const VORBIS: u32 = 15;
}

/// One sound of an FSB5 bank.
#[derive(Debug, Clone)]
pub struct Fsb5Sample {
    pub frequency: u32,
    pub channels: u32,
    pub samples: u32,
    /// Absolute range of its data inside the bank bytes.
    pub data: std::ops::Range<usize>,
    /// Vorbis: CRC32 of the setup header FMOD left out (extra chunk 11), and the seek table after it.
    pub vorbis_crc: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct Fsb5 {
    pub version: u32,
    pub mode: u32,
    pub samples: Vec<Fsb5Sample>,
}

fn u32le(b: &[u8], at: usize) -> Result<u32, Error> {
    b.get(at..at + 4).map(|s| u32::from_le_bytes(s.try_into().unwrap())).ok_or(Error::TooShort { at, want: 4 })
}

/// FSB5 header frequency index → Hz.
fn frequency_of(index: u64) -> Option<u32> {
    Some(match index {
        1 => 8000,
        2 => 11000,
        3 => 11025,
        4 => 16000,
        5 => 22050,
        6 => 24000,
        7 => 32000,
        8 => 44100,
        9 => 48000,
        10 => 96000,
        _ => return None,
    })
}

impl Fsb5 {
    /// Parse a bank (the clip's `offset..offset + size` of its `.resource` file).
    pub fn parse(b: &[u8]) -> Result<Fsb5, Error> {
        if b.get(..4) != Some(b"FSB5") {
            return Err(Error::BadLength(-1));
        }
        let version = u32le(b, 4)?;
        let count = u32le(b, 8)? as usize;
        let headers_size = u32le(b, 12)? as usize;
        let names_size = u32le(b, 16)? as usize;
        let data_size = u32le(b, 20)? as usize;
        let mode = u32le(b, 24)?;
        let header_len = if version == 0 { 64 } else { 60 };
        let data_start = header_len + headers_size + names_size;
        if data_start + data_size > b.len() {
            return Err(Error::TooShort { at: data_start, want: data_size });
        }
        let mut at = header_len;
        let mut raw = Vec::with_capacity(count);
        for _ in 0..count {
            let bits = u64::from_le_bytes(b.get(at..at + 8).ok_or(Error::TooShort { at, want: 8 })?.try_into().unwrap());
            at += 8;
            let mut frequency = frequency_of((bits >> 1) & 0xF).ok_or(Error::BadLength(((bits >> 1) & 0xF) as i32))?;
            let mut channels = ((bits >> 5) & 1) as u32 + 1;
            let offset = ((bits >> 6) & 0x0FFF_FFFF) as usize * 32;
            let samples = ((bits >> 34) & 0x3FFF_FFFF) as u32;
            let mut vorbis_crc = None;
            let mut more = bits & 1 != 0;
            while more {
                let c = u32le(b, at)?;
                at += 4;
                more = c & 1 != 0;
                let size = ((c >> 1) & 0xFF_FFFF) as usize;
                let kind = (c >> 25) & 0x7F;
                match kind {
                    1 => channels = *b.get(at).ok_or(Error::TooShort { at, want: 1 })? as u32,
                    2 => frequency = u32le(b, at)?,
                    11 => vorbis_crc = Some(u32le(b, at)?),
                    _ => {}
                }
                at += size;
            }
            raw.push((frequency, channels, samples, offset, vorbis_crc));
        }
        let mut out = Vec::with_capacity(count);
        for (i, &(frequency, channels, samples, offset, vorbis_crc)) in raw.iter().enumerate() {
            let end = raw.get(i + 1).map(|r| r.3).unwrap_or(data_size);
            out.push(Fsb5Sample { frequency, channels, samples, data: data_start + offset..data_start + end, vorbis_crc });
        }
        Ok(Fsb5 { version, mode, samples: out })
    }
}

/// FSB5 Vorbis data: packets, each a u16 little-endian length then the packet (a 0 length ends the data).
pub fn vorbis_packets(data: &[u8]) -> Result<Vec<&[u8]>, Error> {
    let mut out = Vec::new();
    let mut at = 0;
    while at + 2 <= data.len() {
        let n = u16::from_le_bytes([data[at], data[at + 1]]) as usize;
        at += 2;
        if n == 0 {
            break;
        }
        out.push(data.get(at..at + n).ok_or(Error::TooShort { at, want: n })?);
        at += n;
    }
    Ok(out)
}
