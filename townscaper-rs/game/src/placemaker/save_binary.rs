//! `.binscape` files (row G3c): `SaveSystem.binaryFormatter`, a `BinaryFormatter` (the reference source's, in
//! Mono) of `SaveData`, in the .NET Remoting binary format (MS-NRBF). The game reads these (a path ending
//! `.binscape`) but never writes one (`shouldBeBin` is false); the writer here reproduces its bytes for checking.
//!
//! The stream: the header; `Assembly-CSharp`'s library record; `SaveData` with its member names and types; the
//! text as a string record; `cam` and `sun` inline as structs; the lists and the image as references, written
//! after the root in the order they were met (`List<T>` as `_items`, `_size`, `_version`; its array with one
//! full struct record and then records that point at its layout); the end. Objects are numbered in the order
//! they are met, structs with negative numbers.

use crate::placemaker::save_data::{SaveCam, SaveCorner, SaveData, SaveSun, SaveVoxel};

const LIBRARY: &str = "Assembly-CSharp, Version=0.0.0.0, Culture=neutral, PublicKeyToken=null";
const SAVE_DATA: &str = "Placemaker.SaveData";
const CAM: &str = "Placemaker.SaveData+Cam";
const SUN: &str = "Placemaker.SaveData+Sun";
const C: &str = "Placemaker.SaveData+C";
const V: &str = "Placemaker.SaveData+V";

fn list_name(elem: &str) -> String {
    format!("System.Collections.Generic.List`1[[{elem}, {LIBRARY}]]")
}

// record types
const SERIALIZED_STREAM_HEADER: u8 = 0;
const CLASS_WITH_ID: u8 = 1;
const SYSTEM_CLASS_WITH_MEMBERS: u8 = 2;
const CLASS_WITH_MEMBERS: u8 = 3;
const SYSTEM_CLASS_WITH_MEMBERS_AND_TYPES: u8 = 4;
const CLASS_WITH_MEMBERS_AND_TYPES: u8 = 5;
const BINARY_OBJECT_STRING: u8 = 6;
const BINARY_ARRAY: u8 = 7;
const MEMBER_PRIMITIVE_TYPED: u8 = 8;
const MEMBER_REFERENCE: u8 = 9;
const OBJECT_NULL: u8 = 10;
const MESSAGE_END: u8 = 11;
const BINARY_LIBRARY: u8 = 12;
const OBJECT_NULL_MULTIPLE_256: u8 = 13;
const OBJECT_NULL_MULTIPLE: u8 = 14;
const ARRAY_SINGLE_PRIMITIVE: u8 = 15;
const ARRAY_SINGLE_OBJECT: u8 = 16;
const ARRAY_SINGLE_STRING: u8 = 17;

// binary types
const BT_PRIMITIVE: u8 = 0;
const BT_STRING: u8 = 1;
const BT_OBJECT: u8 = 2;
const BT_SYSTEM_CLASS: u8 = 3;
const BT_CLASS: u8 = 4;
const BT_OBJECT_ARRAY: u8 = 5;
const BT_STRING_ARRAY: u8 = 6;
const BT_PRIMITIVE_ARRAY: u8 = 7;

// primitive types
const PT_BYTE: u8 = 2;
const PT_INT32: u8 = 8;
const PT_INT64: u8 = 9;

// ---------------------------------------------------------------------------------------------------------
// Writing

struct W {
    out: Vec<u8>,
    next_id: i32,
}

impl W {
    fn u8(&mut self, v: u8) {
        self.out.push(v);
    }
    fn i32(&mut self, v: i32) {
        self.out.extend_from_slice(&v.to_le_bytes());
    }
    fn string(&mut self, s: &str) {
        let b = s.as_bytes();
        let mut n = b.len() as u32;
        loop {
            let byte = (n & 0x7F) as u8;
            n >>= 7;
            if n == 0 {
                self.out.push(byte);
                break;
            }
            self.out.push(byte | 0x80);
        }
        self.out.extend_from_slice(b);
    }
    fn id(&mut self) -> i32 {
        let i = self.next_id;
        self.next_id += 1;
        i
    }
    /// A struct record (`ClassWithMembersAndTypes`, negative id) of byte or int fields.
    fn struct_record(&mut self, name: &str, members: &[&str], prim: u8, values: &[i32]) -> i32 {
        let id = -self.id();
        self.u8(CLASS_WITH_MEMBERS_AND_TYPES);
        self.i32(id);
        self.string(name);
        self.i32(members.len() as i32);
        for m in members {
            self.string(m);
        }
        for _ in members {
            self.u8(BT_PRIMITIVE);
        }
        for _ in members {
            self.u8(prim);
        }
        self.i32(2);
        self.values(prim, values);
        id
    }
    fn values(&mut self, prim: u8, values: &[i32]) {
        for &v in values {
            if prim == PT_BYTE {
                self.u8(v as u8);
            } else {
                self.i32(v);
            }
        }
    }
}

/// `List<T>`'s capacity after `n` `Add`s from empty (0, then 4, doubling).
pub fn capacity_after_adds(n: usize) -> usize {
    let mut c = 0;
    while c < n {
        c = if c == 0 { 4 } else { c * 2 };
    }
    c
}

/// `binaryFormatter.Serialize(stream, saveData)`, the lists' backing arrays `corners_capacity` /
/// `voxels_capacity` long (how the lists grew; see [`capacity_after_adds`]).
pub fn write_save(d: &SaveData, corners_capacity: usize, voxels_capacity: usize) -> Vec<u8> {
    let mut w = W { out: vec![], next_id: 1 };
    w.u8(SERIALIZED_STREAM_HEADER);
    let root = w.id();
    w.i32(root);
    w.i32(-1);
    w.i32(1);
    w.i32(0);
    w.u8(BINARY_LIBRARY);
    let lib = w.id();
    w.i32(lib);
    w.string(LIBRARY);
    w.u8(CLASS_WITH_MEMBERS_AND_TYPES);
    w.i32(root);
    w.string(SAVE_DATA);
    let members = ["saveString", "lastWriteTime", "cam", "sun", "corners", "voxels", "bigJpg"];
    w.i32(members.len() as i32);
    for m in members {
        w.string(m);
    }
    for t in [BT_STRING, BT_PRIMITIVE, BT_CLASS, BT_CLASS, BT_SYSTEM_CLASS, BT_SYSTEM_CLASS, BT_PRIMITIVE_ARRAY] {
        w.u8(t);
    }
    w.u8(PT_INT64);
    w.string(CAM);
    w.i32(lib);
    w.string(SUN);
    w.i32(lib);
    w.string(&list_name(C));
    w.string(&list_name(V));
    w.u8(PT_BYTE);
    w.i32(lib);
    // the values
    match &d.save_string {
        Some(s) => {
            w.u8(BINARY_OBJECT_STRING);
            let id = w.id();
            w.i32(id);
            w.string(s);
        }
        None => w.u8(OBJECT_NULL),
    }
    w.out.extend_from_slice(&d.last_write_time.to_le_bytes());
    w.struct_record(CAM, &["x", "y", "z"], PT_BYTE, &[d.cam.x as i32, d.cam.y as i32, d.cam.z as i32]);
    w.struct_record(SUN, &["h", "r"], PT_BYTE, &[d.sun.h as i32, d.sun.r as i32]);
    let corners_id = w.id();
    w.u8(MEMBER_REFERENCE);
    w.i32(corners_id);
    let voxels_id = w.id();
    w.u8(MEMBER_REFERENCE);
    w.i32(voxels_id);
    let jpg_id = match &d.big_jpg {
        Some(_) => {
            let id = w.id();
            w.u8(MEMBER_REFERENCE);
            w.i32(id);
            Some(id)
        }
        None => {
            w.u8(OBJECT_NULL);
            None
        }
    };
    // the referenced objects, in the order met; their arrays after them
    let list = |w: &mut W, id: i32, elem: &str, size: usize| -> i32 {
        w.u8(SYSTEM_CLASS_WITH_MEMBERS_AND_TYPES);
        w.i32(id);
        w.string(&list_name(elem));
        w.i32(3);
        for m in ["_items", "_size", "_version"] {
            w.string(m);
        }
        for t in [BT_CLASS, BT_PRIMITIVE, BT_PRIMITIVE] {
            w.u8(t);
        }
        w.string(&format!("{elem}[]"));
        w.i32(2);
        w.u8(PT_INT32);
        w.u8(PT_INT32);
        let items = w.id();
        w.u8(MEMBER_REFERENCE);
        w.i32(items);
        w.i32(size as i32);
        w.i32(size as i32);
        items
    };
    let c_items = list(&mut w, corners_id, C, d.corners.len());
    let v_items = list(&mut w, voxels_id, V, d.voxels.len());
    if let (Some(id), Some(j)) = (jpg_id, &d.big_jpg) {
        w.u8(ARRAY_SINGLE_PRIMITIVE);
        w.i32(id);
        w.i32(j.len() as i32);
        w.u8(PT_BYTE);
        w.out.extend_from_slice(j);
    }
    let array = |w: &mut W, id: i32, elem: &str, members: &[&str], prim: u8, rows: Vec<Vec<i32>>| {
        w.u8(BINARY_ARRAY);
        w.i32(id);
        w.u8(0);
        w.i32(1);
        w.i32(rows.len() as i32);
        w.u8(BT_CLASS);
        w.string(elem);
        w.i32(2);
        let mut layout = None;
        for row in rows {
            match layout {
                None => layout = Some(w.struct_record(elem, members, prim, &row)),
                Some(meta) => {
                    w.u8(CLASS_WITH_ID);
                    let id = -w.id();
                    w.i32(id);
                    w.i32(meta);
                    w.values(prim, &row);
                }
            }
        }
    };
    let mut rows: Vec<Vec<i32>> = d.corners.iter().map(|c| vec![c.x, c.y, c.count]).collect();
    rows.resize(corners_capacity.max(d.corners.len()), vec![0, 0, 0]);
    array(&mut w, c_items, C, &["x", "y", "count"], PT_INT32, rows);
    let mut rows: Vec<Vec<i32>> = d.voxels.iter().map(|v| vec![v.t as i32, v.h as i32]).collect();
    rows.resize(voxels_capacity.max(d.voxels.len()), vec![0, 0]);
    array(&mut w, v_items, V, &["t", "h"], PT_BYTE, rows);
    w.u8(MESSAGE_END);
    w.out
}

// ---------------------------------------------------------------------------------------------------------
// Reading

/// Why a file does not read (the formatter throws).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BinaryError(pub String);

fn err<T>(m: impl Into<String>) -> Result<T, BinaryError> {
    Err(BinaryError(m.into()))
}

/// A member's value as read.
#[derive(Clone, Debug, PartialEq)]
enum Value {
    Null,
    Int(i64),
    Float(f64),
    Ref(i32),
    Str(String),
    Object(i32),
}

/// A class's layout (for `ClassWithId` records that reuse it).
#[derive(Clone, Debug)]
struct Layout {
    name: String,
    members: Vec<String>,
    /// (binary type, primitive type) per member; `None` for `ClassWithMembers` (types not given).
    types: Option<Vec<(u8, u8)>>,
}

#[derive(Clone, Debug)]
enum Object {
    Class { layout: usize, values: Vec<Value> },
    Str(String),
    Array(Vec<Value>),
    Bytes(Vec<u8>),
}

struct R<'a> {
    b: &'a [u8],
    i: usize,
    layouts: Vec<Layout>,
    layout_of: std::collections::HashMap<i32, usize>,
    objects: std::collections::HashMap<i32, Object>,
}

impl<'a> R<'a> {
    fn u8(&mut self) -> Result<u8, BinaryError> {
        let v = *self.b.get(self.i).ok_or(BinaryError("end of stream".into()))?;
        self.i += 1;
        Ok(v)
    }
    fn bytes(&mut self, n: usize) -> Result<&'a [u8], BinaryError> {
        if self.i + n > self.b.len() {
            return err("end of stream");
        }
        let s = &self.b[self.i..self.i + n];
        self.i += n;
        Ok(s)
    }
    fn i32(&mut self) -> Result<i32, BinaryError> {
        Ok(i32::from_le_bytes(self.bytes(4)?.try_into().unwrap()))
    }
    fn string(&mut self) -> Result<String, BinaryError> {
        let mut n = 0u32;
        let mut shift = 0;
        loop {
            let b = self.u8()?;
            n |= ((b & 0x7F) as u32) << shift;
            if b & 0x80 == 0 {
                break;
            }
            shift += 7;
            if shift > 28 {
                return err("bad string length");
            }
        }
        String::from_utf8(self.bytes(n as usize)?.to_vec()).map_err(|_| BinaryError("bad UTF-8".into()))
    }
    fn primitive(&mut self, t: u8) -> Result<Value, BinaryError> {
        Ok(match t {
            1 | 2 => Value::Int(self.u8()? as i64),
            10 => Value::Int(self.u8()? as i8 as i64),
            7 => Value::Int(i16::from_le_bytes(self.bytes(2)?.try_into().unwrap()) as i64),
            14 => Value::Int(u16::from_le_bytes(self.bytes(2)?.try_into().unwrap()) as i64),
            8 => Value::Int(self.i32()? as i64),
            15 => Value::Int(u32::from_le_bytes(self.bytes(4)?.try_into().unwrap()) as i64),
            9 | 12 | 13 | 16 => Value::Int(i64::from_le_bytes(self.bytes(8)?.try_into().unwrap())),
            11 => Value::Float(f32::from_le_bytes(self.bytes(4)?.try_into().unwrap()) as f64),
            6 => Value::Float(f64::from_le_bytes(self.bytes(8)?.try_into().unwrap())),
            18 | 5 => Value::Str(self.string()?),
            3 => {
                // a UTF-8 char
                let first = self.u8()?;
                let extra = if first < 0x80 { 0 } else if first < 0xE0 { 1 } else if first < 0xF0 { 2 } else { 3 };
                let mut v = vec![first];
                v.extend_from_slice(self.bytes(extra)?);
                Value::Str(String::from_utf8(v).map_err(|_| BinaryError("bad char".into()))?)
            }
            _ => return err(format!("primitive type {t}")),
        })
    }
    fn class_types(&mut self, n: usize) -> Result<Vec<(u8, u8)>, BinaryError> {
        let kinds: Vec<u8> = (0..n).map(|_| self.u8()).collect::<Result<_, _>>()?;
        let mut out = vec![];
        for k in kinds {
            let extra = match k {
                BT_PRIMITIVE | BT_PRIMITIVE_ARRAY => self.u8()?,
                BT_SYSTEM_CLASS => {
                    self.string()?;
                    0
                }
                BT_CLASS => {
                    self.string()?;
                    self.i32()?;
                    0
                }
                BT_STRING | BT_OBJECT | BT_OBJECT_ARRAY | BT_STRING_ARRAY => 0,
                _ => return err(format!("binary type {k}")),
            };
            out.push((k, extra));
        }
        Ok(out)
    }
    /// A value of a member of binary type `t` (`p`: its primitive type).
    fn member(&mut self, t: Option<(u8, u8)>) -> Result<Value, BinaryError> {
        if let Some((BT_PRIMITIVE, p)) = t {
            return self.primitive(p);
        }
        self.record_value()
    }
    /// A record that stands for a value.
    fn record_value(&mut self) -> Result<Value, BinaryError> {
        let kind = self.u8()?;
        match kind {
            MEMBER_REFERENCE => Ok(Value::Ref(self.i32()?)),
            OBJECT_NULL => Ok(Value::Null),
            MEMBER_PRIMITIVE_TYPED => {
                let p = self.u8()?;
                self.primitive(p)
            }
            _ => {
                self.i -= 1;
                let id = self.record()?.ok_or(BinaryError("expected an object record".into()))?;
                Ok(Value::Object(id))
            }
        }
    }
    fn class_body(&mut self, id: i32, layout: usize) -> Result<i32, BinaryError> {
        let l = self.layouts[layout].clone();
        let mut values = vec![];
        for k in 0..l.members.len() {
            let t = l.types.as_ref().map(|t| t[k]);
            values.push(self.member(t)?);
        }
        self.objects.insert(id, Object::Class { layout, values });
        Ok(id)
    }
    /// One record; the id of the object it made, if any.
    fn record(&mut self) -> Result<Option<i32>, BinaryError> {
        let kind = self.u8()?;
        match kind {
            BINARY_LIBRARY => {
                self.i32()?;
                self.string()?;
                Ok(None)
            }
            CLASS_WITH_MEMBERS_AND_TYPES | SYSTEM_CLASS_WITH_MEMBERS_AND_TYPES | CLASS_WITH_MEMBERS | SYSTEM_CLASS_WITH_MEMBERS => {
                let id = self.i32()?;
                let name = self.string()?;
                let n = self.i32()? as usize;
                let members = (0..n).map(|_| self.string()).collect::<Result<Vec<_>, _>>()?;
                let typed = kind == CLASS_WITH_MEMBERS_AND_TYPES || kind == SYSTEM_CLASS_WITH_MEMBERS_AND_TYPES;
                let types = if typed { Some(self.class_types(n)?) } else { None };
                if kind == CLASS_WITH_MEMBERS_AND_TYPES || kind == CLASS_WITH_MEMBERS {
                    self.i32()?;
                }
                self.layouts.push(Layout { name, members, types });
                let layout = self.layouts.len() - 1;
                self.layout_of.insert(id, layout);
                Ok(Some(self.class_body(id, layout)?))
            }
            CLASS_WITH_ID => {
                let id = self.i32()?;
                let meta = self.i32()?;
                let layout = *self.layout_of.get(&meta).ok_or(BinaryError(format!("no layout {meta}")))?;
                self.layout_of.insert(id, layout);
                Ok(Some(self.class_body(id, layout)?))
            }
            BINARY_OBJECT_STRING => {
                let id = self.i32()?;
                let s = self.string()?;
                self.objects.insert(id, Object::Str(s));
                Ok(Some(id))
            }
            ARRAY_SINGLE_PRIMITIVE => {
                let id = self.i32()?;
                let n = self.i32()? as usize;
                let p = self.u8()?;
                if p == PT_BYTE {
                    let b = self.bytes(n)?.to_vec();
                    self.objects.insert(id, Object::Bytes(b));
                } else {
                    let v = (0..n).map(|_| self.primitive(p)).collect::<Result<Vec<_>, _>>()?;
                    self.objects.insert(id, Object::Array(v));
                }
                Ok(Some(id))
            }
            BINARY_ARRAY | ARRAY_SINGLE_OBJECT | ARRAY_SINGLE_STRING => {
                let id = self.i32()?;
                let (n, elem) = if kind == BINARY_ARRAY {
                    let shape = self.u8()?;
                    let rank = self.i32()?;
                    let lengths = (0..rank).map(|_| self.i32()).collect::<Result<Vec<_>, _>>()?;
                    if shape >= 3 {
                        (0..rank).try_for_each(|_| self.i32().map(|_| ()))?;
                    }
                    let t = self.class_types(1)?[0];
                    (lengths.iter().map(|&l| l as usize).product::<usize>(), Some(t))
                } else {
                    (self.i32()? as usize, None)
                };
                let mut v = vec![];
                while v.len() < n {
                    if let Some((BT_PRIMITIVE, p)) = elem {
                        v.push(self.primitive(p)?);
                        continue;
                    }
                    let k = self.u8()?;
                    match k {
                        OBJECT_NULL_MULTIPLE_256 => {
                            let c = self.u8()? as usize;
                            v.extend(std::iter::repeat(Value::Null).take(c));
                        }
                        OBJECT_NULL_MULTIPLE => {
                            let c = self.i32()? as usize;
                            v.extend(std::iter::repeat(Value::Null).take(c));
                        }
                        _ => {
                            self.i -= 1;
                            v.push(self.record_value()?);
                        }
                    }
                }
                self.objects.insert(id, Object::Array(v));
                Ok(Some(id))
            }
            _ => err(format!("record type {kind} at {}", self.i - 1)),
        }
    }

    fn resolve(&self, v: &Value) -> Option<&Object> {
        match v {
            Value::Ref(id) | Value::Object(id) => self.objects.get(id),
            _ => None,
        }
    }
    fn field<'b>(&'b self, o: &'b Object, name: &str) -> Option<&'b Value> {
        match o {
            Object::Class { layout, values } => {
                let i = self.layouts[*layout].members.iter().position(|m| m == name)?;
                values.get(i)
            }
            _ => None,
        }
    }
    fn int(&self, o: &Object, name: &str) -> i64 {
        match self.field(o, name) {
            Some(Value::Int(v)) => *v,
            _ => 0,
        }
    }
    /// A `List<T>`'s first `_size` items.
    fn list(&self, v: &Value) -> Result<Option<Vec<&Object>>, BinaryError> {
        let Some(l) = self.resolve(v) else { return Ok(None) };
        let size = self.int(l, "_size").max(0) as usize;
        let Some(Object::Array(items)) = self.field(l, "_items").and_then(|i| self.resolve(i)) else {
            return err("a list without items");
        };
        let mut out = vec![];
        for it in items.iter().take(size) {
            out.push(self.resolve(it).ok_or(BinaryError("a null list item".into()))?);
        }
        Ok(Some(out))
    }
}

/// `binaryFormatter.Deserialize(stream)` as `SaveData`: `None` when the root is not one (the game's `as`).
pub fn read_save(bytes: &[u8]) -> Result<Option<SaveData>, BinaryError> {
    let mut r = R { b: bytes, i: 0, layouts: vec![], layout_of: Default::default(), objects: Default::default() };
    if r.u8()? != SERIALIZED_STREAM_HEADER {
        return err("no header");
    }
    let root = r.i32()?;
    r.i32()?;
    let (major, minor) = (r.i32()?, r.i32()?);
    if (major, minor) != (1, 0) {
        return err(format!("version {major}.{minor}"));
    }
    loop {
        let k = r.u8()?;
        if k == MESSAGE_END {
            break;
        }
        r.i -= 1;
        r.record()?;
    }
    let Some(obj) = r.objects.get(&root) else { return err("no root") };
    let Object::Class { layout, .. } = obj else { return Ok(None) };
    if r.layouts[*layout].name != SAVE_DATA {
        return Ok(None);
    }
    let mut d = SaveData { save_string: None, big_jpg: None, ..SaveData::default() };
    d.save_string = match r.field(obj, "saveString") {
        Some(Value::Str(s)) => Some(s.clone()),
        Some(v) => match r.resolve(v) {
            Some(Object::Str(s)) => Some(s.clone()),
            _ => None,
        },
        None => None,
    };
    d.last_write_time = r.int(obj, "lastWriteTime");
    if let Some(cam) = r.field(obj, "cam").and_then(|v| r.resolve(v)) {
        d.cam = SaveCam { x: r.int(cam, "x") as u8, y: r.int(cam, "y") as u8, z: r.int(cam, "z") as u8 };
    }
    if let Some(sun) = r.field(obj, "sun").and_then(|v| r.resolve(v)) {
        d.sun = SaveSun { h: r.int(sun, "h") as u8, r: r.int(sun, "r") as u8 };
    }
    if let Some(v) = r.field(obj, "corners") {
        d.corners = r.list(v)?.unwrap_or_default().iter().map(|c| SaveCorner { x: r.int(c, "x") as i32, y: r.int(c, "y") as i32, count: r.int(c, "count") as i32 }).collect();
    }
    if let Some(v) = r.field(obj, "voxels") {
        d.voxels = r.list(v)?.unwrap_or_default().iter().map(|c| SaveVoxel { t: r.int(c, "t") as u8, h: r.int(c, "h") as u8 }).collect();
    }
    d.big_jpg = match r.field(obj, "bigJpg").and_then(|v| r.resolve(v)) {
        Some(Object::Bytes(b)) => Some(b.clone()),
        _ => None,
    };
    Ok(Some(d))
}
