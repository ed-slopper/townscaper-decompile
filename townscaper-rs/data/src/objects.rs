//! Unity built-in objects with Unity 2020.3.18f1's serialized layouts (the build has no type trees), and a set
//! of loaded files that resolves `PPtr`s across them (row F8b).
//!
//! Layouts are proven by the tests in `tests/objects.rs`: every object of these classes in the reference install
//! is read to exactly its size, and the references between them agree (each Transform's GameObject lists it).

use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::io;
use std::path::Path;

use crate::serialized::{self, ObjectInfo, SerializedFile};

pub mod class_id {
    pub const GAME_OBJECT: i32 = 1;
    pub const TRANSFORM: i32 = 4;
    pub const MONO_BEHAVIOUR: i32 = 114;
    pub const MONO_SCRIPT: i32 = 115;
    pub const RECT_TRANSFORM: i32 = 224;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    TooShort { at: usize, want: usize },
    BadLength(i32),
    /// Bytes left over after the known fields: the layout is wrong for this object.
    Trailing { read: usize, size: usize },
    WrongClass { want: i32, got: i32 },
    Serialized(serialized::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::TooShort { at, want } => write!(f, "object too short: {want} bytes at {at}"),
            Error::BadLength(n) => write!(f, "bad length {n}"),
            Error::Trailing { read, size } => write!(f, "read {read} of {size} bytes: layout mismatch"),
            Error::WrongClass { want, got } => write!(f, "expected class {want}, got {got}"),
            Error::Serialized(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for Error {}

/// Reader over one object's bytes (little-endian: the reference build's data).
pub struct Reader<'a> {
    pub b: &'a [u8],
    pub at: usize,
}

impl<'a> Reader<'a> {
    pub fn new(b: &'a [u8]) -> Self {
        Reader { b, at: 0 }
    }
    pub fn take(&mut self, n: usize) -> Result<&'a [u8], Error> {
        let s = self.b.get(self.at..self.at + n).ok_or(Error::TooShort { at: self.at, want: n })?;
        self.at += n;
        Ok(s)
    }
    pub fn u8(&mut self) -> Result<u8, Error> {
        Ok(self.take(1)?[0])
    }
    pub fn bool(&mut self) -> Result<bool, Error> {
        Ok(self.u8()? != 0)
    }
    pub fn u16(&mut self) -> Result<u16, Error> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    pub fn i32(&mut self) -> Result<i32, Error> {
        Ok(i32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    pub fn u32(&mut self) -> Result<u32, Error> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    pub fn i64(&mut self) -> Result<i64, Error> {
        Ok(i64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
    pub fn f32(&mut self) -> Result<f32, Error> {
        Ok(f32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    pub fn align4(&mut self) {
        self.at = (self.at + 3) & !3;
    }
    /// Array length prefix.
    pub fn len(&mut self) -> Result<usize, Error> {
        let n = self.i32()?;
        if n < 0 || n as usize > self.b.len() {
            return Err(Error::BadLength(n));
        }
        Ok(n as usize)
    }
    /// `string`: i32 length, UTF-8 bytes, aligned to 4.
    pub fn string(&mut self) -> Result<String, Error> {
        let n = self.len()?;
        let s = String::from_utf8_lossy(self.take(n)?).into_owned();
        self.align4();
        Ok(s)
    }
    pub fn pptr(&mut self) -> Result<PPtr, Error> {
        Ok(PPtr { file_id: self.i32()?, path_id: self.i64()? })
    }
    pub fn pptrs(&mut self) -> Result<Vec<PPtr>, Error> {
        let n = self.len()?;
        (0..n).map(|_| self.pptr()).collect()
    }
    pub fn vec2(&mut self) -> Result<[f32; 2], Error> {
        Ok([self.f32()?, self.f32()?])
    }
    pub fn vec3(&mut self) -> Result<[f32; 3], Error> {
        Ok([self.f32()?, self.f32()?, self.f32()?])
    }
    pub fn quat(&mut self) -> Result<[f32; 4], Error> {
        Ok([self.f32()?, self.f32()?, self.f32()?, self.f32()?])
    }
    /// After the last field: the object ends here, or after alignment padding (objects are not padded at
    /// their end, so a trailing `bool` leaves the size unaligned: GameObject is 67 bytes in level0).
    pub fn finish(&mut self) -> Result<(), Error> {
        if self.at != self.b.len() {
            self.align4();
        }
        if self.at != self.b.len() {
            return Err(Error::Trailing { read: self.at, size: self.b.len() });
        }
        Ok(())
    }
}

/// A reference to an object: `file_id` 0 = the same file, n > 0 = that file's `externals[n - 1]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PPtr {
    pub file_id: i32,
    pub path_id: i64,
}

impl PPtr {
    pub fn is_null(&self) -> bool {
        self.path_id == 0
    }
}

#[derive(Debug, Clone)]
pub struct GameObject {
    pub components: Vec<PPtr>,
    pub layer: u32,
    pub name: String,
    pub tag: u16,
    pub is_active: bool,
}

impl GameObject {
    pub fn read(b: &[u8]) -> Result<Self, Error> {
        let mut r = Reader::new(b);
        let g = GameObject {
            components: r.pptrs()?,
            layer: r.u32()?,
            name: r.string()?,
            tag: r.u16()?,
            is_active: r.bool()?,
        };
        r.finish()?;
        Ok(g)
    }
}

/// `Transform`, and the extra fields of a `RectTransform` when it is one.
#[derive(Debug, Clone)]
pub struct Transform {
    pub game_object: PPtr,
    pub local_rotation: [f32; 4],
    pub local_position: [f32; 3],
    pub local_scale: [f32; 3],
    pub children: Vec<PPtr>,
    pub father: PPtr,
    pub rect: Option<RectFields>,
}

#[derive(Debug, Clone, Copy)]
pub struct RectFields {
    pub anchor_min: [f32; 2],
    pub anchor_max: [f32; 2],
    pub anchored_position: [f32; 2],
    pub size_delta: [f32; 2],
    pub pivot: [f32; 2],
}

impl Transform {
    pub fn read(b: &[u8], is_rect: bool) -> Result<Self, Error> {
        let mut r = Reader::new(b);
        let mut t = Transform {
            game_object: r.pptr()?,
            local_rotation: r.quat()?,
            local_position: r.vec3()?,
            local_scale: r.vec3()?,
            children: r.pptrs()?,
            father: r.pptr()?,
            rect: None,
        };
        if is_rect {
            t.rect = Some(RectFields {
                anchor_min: r.vec2()?,
                anchor_max: r.vec2()?,
                anchored_position: r.vec2()?,
                size_delta: r.vec2()?,
                pivot: r.vec2()?,
            });
        }
        r.finish()?;
        Ok(t)
    }
}

/// The fields every `MonoBehaviour` starts with; the script's own fields follow at `fields_at` (F8g).
#[derive(Debug, Clone)]
pub struct MonoBehaviourHeader {
    pub game_object: PPtr,
    pub enabled: bool,
    pub script: PPtr,
    pub name: String,
    /// Offset in the object's bytes where the script's serialized fields begin.
    pub fields_at: usize,
}

impl MonoBehaviourHeader {
    pub fn read(b: &[u8]) -> Result<Self, Error> {
        let mut r = Reader::new(b);
        let game_object = r.pptr()?;
        let enabled = r.bool()?;
        r.align4();
        let script = r.pptr()?;
        let name = r.string()?;
        Ok(MonoBehaviourHeader { game_object, enabled, script, name, fields_at: r.at })
    }
}

#[derive(Debug, Clone)]
pub struct MonoScript {
    pub name: String,
    pub execution_order: i32,
    pub properties_hash: [u8; 16],
    pub class_name: String,
    pub namespace: String,
    pub assembly_name: String,
}

impl MonoScript {
    pub fn read(b: &[u8]) -> Result<Self, Error> {
        let mut r = Reader::new(b);
        let s = MonoScript {
            name: r.string()?,
            execution_order: r.i32()?,
            properties_hash: r.take(16)?.try_into().unwrap(),
            class_name: r.string()?,
            namespace: r.string()?,
            assembly_name: r.string()?,
        };
        r.finish()?;
        Ok(s)
    }

    /// `Namespace.Class`, as the type is named in `docs/type-index.csv`.
    pub fn full_name(&self) -> String {
        if self.namespace.is_empty() {
            self.class_name.clone()
        } else {
            format!("{}.{}", self.namespace, self.class_name)
        }
    }
}

/// One loaded SerializedFile with its bytes.
pub struct LoadedFile {
    pub name: String,
    pub bytes: Vec<u8>,
    pub meta: SerializedFile,
    by_path_id: HashMap<i64, usize>,
}

impl LoadedFile {
    pub fn object(&self, path_id: i64) -> Option<&ObjectInfo> {
        self.by_path_id.get(&path_id).map(|&i| &self.meta.objects[i])
    }
    pub fn bytes_of(&self, o: &ObjectInfo) -> &[u8] {
        self.meta.object_bytes(&self.bytes, o).expect("object inside file (checked by F8a)")
    }
    pub fn class_id(&self, o: &ObjectInfo) -> i32 {
        self.meta.class_id(o).unwrap_or(-1)
    }
}

/// All SerializedFiles of `Townscaper_Data`, by file name; resolves `PPtr`s across them.
pub struct AssetSet {
    pub files: Vec<LoadedFile>,
    by_name: HashMap<String, usize>,
}

/// Is this a SerializedFile of the build (not `.resS`, `.resource`, `.json`, ...)?
pub fn is_serialized_name(name: &str) -> bool {
    name == "globalgamemanagers"
        || name.ends_with(".assets")
        || (name.starts_with("level") && !name.contains('.'))
}

impl AssetSet {
    pub fn load(data_dir: &Path) -> io::Result<AssetSet> {
        let mut names: Vec<String> = fs::read_dir(data_dir)?
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| is_serialized_name(n))
            .collect();
        names.sort();
        let mut files = Vec::new();
        let mut by_name = HashMap::new();
        for name in names {
            let bytes = fs::read(data_dir.join(&name))?;
            let meta = SerializedFile::parse(&bytes)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("{name}: {e}")))?;
            let by_path_id = meta.objects.iter().enumerate().map(|(i, o)| (o.path_id, i)).collect();
            by_name.insert(name.clone(), files.len());
            files.push(LoadedFile { name, bytes, meta, by_path_id });
        }
        Ok(AssetSet { files, by_name })
    }

    pub fn file(&self, name: &str) -> Option<&LoadedFile> {
        self.by_name.get(name).map(|&i| &self.files[i])
    }

    /// The file a `PPtr` read in `from` points into. `None` for Unity's built-in resources (not in the build).
    pub fn target_file<'a>(&'a self, from: &'a LoadedFile, p: PPtr) -> Option<&'a LoadedFile> {
        if p.file_id == 0 {
            return Some(from);
        }
        let ext = from.meta.externals.get(p.file_id as usize - 1)?;
        let base = ext.path.rsplit(['/', '\\']).next().unwrap_or(&ext.path);
        self.file(base)
    }

    /// Resolve a `PPtr` to its file and object.
    pub fn resolve<'a>(&'a self, from: &'a LoadedFile, p: PPtr) -> Option<(&'a LoadedFile, &'a ObjectInfo)> {
        if p.is_null() {
            return None;
        }
        let f = self.target_file(from, p)?;
        Some((f, f.object(p.path_id)?))
    }

    /// The `MonoScript` of a MonoBehaviour (by its header's script `PPtr`).
    pub fn script_of(&self, from: &LoadedFile, mb: &MonoBehaviourHeader) -> Option<MonoScript> {
        let (f, o) = self.resolve(from, mb.script)?;
        if f.class_id(o) != class_id::MONO_SCRIPT {
            return None;
        }
        MonoScript::read(f.bytes_of(o)).ok()
    }
}
