//! Unity `SerializedFile` (the `.assets`, `globalgamemanagers` and `level*` files): the header only, so far.
//! The object table, type trees and objects are row F8.
//!
//! Layout (format 22, Unity 2020.3; read from the player's `globalgamemanagers`):
//!
//! | off  | size | field                                          |
//! |------|------|------------------------------------------------|
//! | 0x00 | 4+4  | legacy metadata size, legacy file size (0)     |
//! | 0x08 | 4    | format version, big-endian (22)                |
//! | 0x0c | 4    | legacy data offset (0)                         |
//! | 0x10 | 1+3  | endianness of the data (0 = little), reserved  |
//! | 0x14 | 4    | metadata size, big-endian                      |
//! | 0x18 | 8    | file size, big-endian                          |
//! | 0x20 | 8    | data offset, big-endian                        |
//! | 0x28 | 8    | unknown (0)                                    |
//! | 0x30 | ..   | metadata, in the data's endianness: Unity version string (NUL-terminated), target platform u32, ... |
//!
//! Formats before 22 keep the 32-bit fields at 0x00..0x10 and start the metadata at 0x14.

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    pub format: u32,
    pub metadata_size: u32,
    pub file_size: u64,
    pub data_offset: u64,
    pub big_endian: bool,
    /// The Unity version that wrote the file, e.g. `2020.3.18f1`.
    pub unity_version: String,
    /// `BuildTarget`: 5 = StandaloneWindows (x86), 19 = StandaloneWindows64.
    pub target_platform: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    TooShort,
    UnsupportedFormat(u32),
    NoVersionString,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::TooShort => write!(f, "file too short for a SerializedFile header"),
            Error::UnsupportedFormat(v) => write!(f, "unsupported SerializedFile format {v}"),
            Error::NoVersionString => write!(f, "no Unity version string in the header"),
        }
    }
}

impl std::error::Error for Error {}

fn be32(b: &[u8], at: usize) -> Result<u32, Error> {
    b.get(at..at + 4).map(|s| u32::from_be_bytes(s.try_into().unwrap())).ok_or(Error::TooShort)
}

fn be64(b: &[u8], at: usize) -> Result<u64, Error> {
    b.get(at..at + 8).map(|s| u64::from_be_bytes(s.try_into().unwrap())).ok_or(Error::TooShort)
}

impl Header {
    /// Parse the header from the start of a file (the first few hundred bytes are enough).
    pub fn parse(b: &[u8]) -> Result<Header, Error> {
        let format = be32(b, 0x08)?;
        let (metadata_size, file_size, data_offset, endian_at, meta_at) = match format {
            9..=21 => (be32(b, 0x00)?, be32(b, 0x04)? as u64, be32(b, 0x0c)? as u64, 0x10, 0x14),
            22.. => (be32(b, 0x14)?, be64(b, 0x18)?, be64(b, 0x20)?, 0x10, 0x30),
            v => return Err(Error::UnsupportedFormat(v)),
        };
        let big_endian = *b.get(endian_at).ok_or(Error::TooShort)? != 0;
        let rest = b.get(meta_at..).ok_or(Error::TooShort)?;
        let nul = rest.iter().position(|&c| c == 0).ok_or(Error::NoVersionString)?;
        let unity_version = String::from_utf8_lossy(&rest[..nul]).into_owned();
        let p = rest.get(nul + 1..nul + 5).ok_or(Error::TooShort)?;
        let p: [u8; 4] = p.try_into().unwrap();
        let target_platform = if big_endian { u32::from_be_bytes(p) } else { u32::from_le_bytes(p) };
        Ok(Header { format, metadata_size, file_size, data_offset, big_endian, unity_version, target_platform })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A format-22 header built by hand with the values the reference `globalgamemanagers` has.
    #[test]
    fn parses_format_22() {
        let mut b = vec![0u8; 0x30];
        b[0x08..0x0c].copy_from_slice(&22u32.to_be_bytes());
        b[0x14..0x18].copy_from_slice(&0x4b0u32.to_be_bytes());
        b[0x18..0x20].copy_from_slice(&0x1f094u64.to_be_bytes());
        b[0x20..0x28].copy_from_slice(&0x1000u64.to_be_bytes());
        b.extend_from_slice(b"2020.3.18f1\0");
        b.extend_from_slice(&5u32.to_le_bytes());
        let h = Header::parse(&b).unwrap();
        assert_eq!(h.format, 22);
        assert_eq!(h.metadata_size, 0x4b0);
        assert_eq!(h.file_size, 0x1f094);
        assert_eq!(h.data_offset, 0x1000);
        assert!(!h.big_endian);
        assert_eq!(h.unity_version, "2020.3.18f1");
        assert_eq!(h.target_platform, 5);
    }

    #[test]
    fn rejects_short_input() {
        assert_eq!(Header::parse(&[0u8; 4]), Err(Error::TooShort));
    }
}
