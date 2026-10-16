//! Unity `SerializedFile` (the `.assets`, `globalgamemanagers` and `level*` files): the header ([`Header`]) and
//! the metadata ([`SerializedFile`]: types, object table, script refs, externals). Reading the objects
//! themselves is rows F8b–F8g. The reference build has **no type trees**, so only their absence is supported.
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
    TypeTreesUnsupported,
    BadCount(i32),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::TooShort => write!(f, "file too short for a SerializedFile header"),
            Error::UnsupportedFormat(v) => write!(f, "unsupported SerializedFile format {v}"),
            Error::NoVersionString => write!(f, "no Unity version string in the header"),
            Error::TypeTreesUnsupported => write!(f, "file has type trees (the reference build has none)"),
            Error::BadCount(n) => write!(f, "bad table count {n}"),
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

/// One entry of the type table. Objects point at it by index (formats 16+).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SerializedType {
    /// Unity class id: 1 GameObject, 4 Transform, 114 MonoBehaviour, 43 Mesh, 28 Texture2D, ...
    pub class_id: i32,
    pub is_stripped: bool,
    /// Index into [`SerializedFile::scripts`] for MonoBehaviours, else -1.
    pub script_type_index: i16,
    /// Hash of the script (MonoBehaviour types and ref types only).
    pub script_id: Option<[u8; 16]>,
    pub old_type_hash: [u8; 16],
}

/// One object: where its bytes are and what it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectInfo {
    pub path_id: i64,
    /// Absolute offset in the file (the table stores it relative to the data offset).
    pub offset: u64,
    pub size: u32,
    /// Index into [`SerializedFile::types`].
    pub type_index: i32,
}

/// A `PPtr`-style reference to a MonoScript: file index (0 = this file) and path id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScriptRef {
    pub file_index: i32,
    pub path_id: i64,
}

/// Another file this one refers to (`PPtr.m_FileID` n > 0 is `externals[n - 1]`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct External {
    pub guid: [u8; 16],
    pub kind: i32,
    /// As stored, e.g. `library/unity default resources` or `sharedassets0.assets`.
    pub path: String,
}

/// A SerializedFile's metadata.
#[derive(Debug, Clone)]
pub struct SerializedFile {
    pub header: Header,
    pub enable_type_tree: bool,
    pub types: Vec<SerializedType>,
    pub objects: Vec<ObjectInfo>,
    pub scripts: Vec<ScriptRef>,
    pub externals: Vec<External>,
    pub ref_types: Vec<SerializedType>,
    pub user_information: String,
    /// Where the metadata ended; must equal the header's metadata end (checked by the tests).
    pub metadata_end: usize,
}

/// Little cursor over the metadata, in the file's endianness.
struct Cur<'a> {
    b: &'a [u8],
    at: usize,
    big: bool,
}

impl<'a> Cur<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], Error> {
        let s = self.b.get(self.at..self.at + n).ok_or(Error::TooShort)?;
        self.at += n;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8, Error> {
        Ok(self.take(1)?[0])
    }
    fn i16(&mut self) -> Result<i16, Error> {
        let a: [u8; 2] = self.take(2)?.try_into().unwrap();
        Ok(if self.big { i16::from_be_bytes(a) } else { i16::from_le_bytes(a) })
    }
    fn i32(&mut self) -> Result<i32, Error> {
        let a: [u8; 4] = self.take(4)?.try_into().unwrap();
        Ok(if self.big { i32::from_be_bytes(a) } else { i32::from_le_bytes(a) })
    }
    fn i64(&mut self) -> Result<i64, Error> {
        let a: [u8; 8] = self.take(8)?.try_into().unwrap();
        Ok(if self.big { i64::from_be_bytes(a) } else { i64::from_le_bytes(a) })
    }
    fn hash(&mut self) -> Result<[u8; 16], Error> {
        Ok(self.take(16)?.try_into().unwrap())
    }
    fn cstr(&mut self) -> Result<String, Error> {
        let rest = self.b.get(self.at..).ok_or(Error::TooShort)?;
        let n = rest.iter().position(|&c| c == 0).ok_or(Error::TooShort)?;
        let s = String::from_utf8_lossy(&rest[..n]).into_owned();
        self.at += n + 1;
        Ok(s)
    }
    fn align4(&mut self) {
        self.at = (self.at + 3) & !3;
    }
    fn count(&mut self) -> Result<usize, Error> {
        let n = self.i32()?;
        if n < 0 || n as usize > self.b.len() {
            return Err(Error::BadCount(n));
        }
        Ok(n as usize)
    }
}

fn read_type(c: &mut Cur, format: u32, is_ref: bool) -> Result<SerializedType, Error> {
    let class_id = c.i32()?;
    let is_stripped = c.u8()? != 0;
    let script_type_index = c.i16()?;
    let has_script_id = (is_ref && script_type_index >= 0) || (format >= 16 && class_id == 114);
    let script_id = if has_script_id { Some(c.hash()?) } else { None };
    let old_type_hash = c.hash()?;
    Ok(SerializedType { class_id, is_stripped, script_type_index, script_id, old_type_hash })
}

impl SerializedFile {
    /// Parse the metadata of a whole file (the bytes of the file; objects are not read).
    /// Supports what the reference build uses: format 22, no type trees.
    pub fn parse(b: &[u8]) -> Result<SerializedFile, Error> {
        let header = Header::parse(b)?;
        let format = header.format;
        if format != 22 {
            return Err(Error::UnsupportedFormat(format));
        }
        let mut c = Cur { b, at: 0x30, big: header.big_endian };
        c.cstr()?; // Unity version, in the header already
        c.i32()?; // target platform, idem
        let enable_type_tree = c.u8()? != 0;
        if enable_type_tree {
            return Err(Error::TypeTreesUnsupported);
        }

        let n = c.count()?;
        let types = (0..n).map(|_| read_type(&mut c, format, false)).collect::<Result<Vec<_>, _>>()?;

        let n = c.count()?;
        let mut objects = Vec::with_capacity(n);
        for _ in 0..n {
            c.align4();
            let path_id = c.i64()?;
            let start = c.i64()?;
            let size = c.i32()? as u32;
            let type_index = c.i32()?;
            objects.push(ObjectInfo { path_id, offset: header.data_offset + start as u64, size, type_index });
        }

        let n = c.count()?;
        let mut scripts = Vec::with_capacity(n);
        for _ in 0..n {
            let file_index = c.i32()?;
            c.align4();
            let path_id = c.i64()?;
            scripts.push(ScriptRef { file_index, path_id });
        }

        let n = c.count()?;
        let mut externals = Vec::with_capacity(n);
        for _ in 0..n {
            c.cstr()?; // "temp empty"
            let guid = c.hash()?;
            let kind = c.i32()?;
            let path = c.cstr()?;
            externals.push(External { guid, kind, path });
        }

        let n = c.count()?;
        let ref_types = (0..n).map(|_| read_type(&mut c, format, true)).collect::<Result<Vec<_>, _>>()?;
        let user_information = c.cstr()?;

        Ok(SerializedFile {
            header,
            enable_type_tree,
            types,
            objects,
            scripts,
            externals,
            ref_types,
            user_information,
            metadata_end: c.at,
        })
    }

    /// The class id of an object.
    pub fn class_id(&self, o: &ObjectInfo) -> Option<i32> {
        self.types.get(o.type_index as usize).map(|t| t.class_id)
    }

    /// The bytes of an object, from the whole file's bytes.
    pub fn object_bytes<'a>(&self, file: &'a [u8], o: &ObjectInfo) -> Option<&'a [u8]> {
        file.get(o.offset as usize..o.offset as usize + o.size as usize)
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
