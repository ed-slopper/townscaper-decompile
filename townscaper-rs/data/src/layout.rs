//! The game's MonoBehaviour / ScriptableObject fields, read with the layouts `tools/layouts.ps1` generates from
//! the build's metadata (`data/layouts.json`: names and types only) and Unity 2020.3's serialization rules
//! (row F8g). Proven by `tests/layout.rs`: every MonoBehaviour of the install reads to its exact size.

use std::collections::HashMap;
use std::sync::OnceLock;

use serde::Deserialize;

use crate::objects::{Error, MonoBehaviourHeader, PPtr, Reader};

#[derive(Debug, Deserialize)]
struct Field {
    n: String,
    t: String,
}

#[derive(Debug, Deserialize)]
struct TypeDef {
    fields: Vec<Field>,
}

#[derive(Debug, Deserialize)]
struct Layouts {
    scripts: HashMap<String, TypeDef>,
    types: HashMap<String, Option<TypeDef>>,
}

fn layouts() -> &'static Layouts {
    static L: OnceLock<Layouts> = OnceLock::new();
    L.get_or_init(|| serde_json::from_str(include_str!("../layouts.json")).expect("layouts.json"))
}

/// A serialized value.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Bool(bool),
    Int(i64),
    UInt(u64),
    Float(f64),
    Str(String),
    PPtr(PPtr),
    Floats(Vec<f32>),
    Ints(Vec<i32>),
    Bytes(Vec<u8>),
    Array(Vec<Value>),
    Struct(Vec<(String, Value)>),
}

impl Value {
    /// A field of a struct value by name.
    pub fn get(&self, name: &str) -> Option<&Value> {
        match self {
            Value::Struct(f) => f.iter().find(|(n, _)| n == name).map(|(_, v)| v),
            _ => None,
        }
    }
}

/// Key of a script in `layouts.json`: `Assembly|Namespace|Class` (assembly without `.dll`).
pub fn script_key(assembly: &str, namespace: &str, class: &str) -> String {
    format!("{}|{}|{}", assembly.trim_end_matches(".dll"), namespace, class)
}

pub fn has_script(key: &str) -> bool {
    layouts().scripts.contains_key(key)
}

fn prim_size(t: &str) -> Option<usize> {
    Some(match t {
        "bool" | "i8" | "u8" => 1,
        "i16" | "u16" => 2,
        "i32" | "u32" | "f32" => 4,
        "i64" | "u64" | "f64" => 8,
        _ => return None,
    })
}

fn read_prim(r: &mut Reader, t: &str) -> Result<Value, Error> {
    Ok(match t {
        "bool" => Value::Bool(r.bool()?),
        "i8" => Value::Int(r.u8()? as i8 as i64),
        "u8" => Value::UInt(r.u8()? as u64),
        "i16" => Value::Int(r.u16()? as i16 as i64),
        "u16" => Value::UInt(r.u16()? as u64),
        "i32" => Value::Int(r.i32()? as i64),
        "u32" => Value::UInt(r.u32()? as u64),
        "i64" => Value::Int(r.i64()?),
        "u64" => Value::UInt(r.i64()? as u64),
        "f32" => Value::Float(r.f32()? as f64),
        "f64" => Value::Float(f64::from_le_bytes(r.take(8)?.try_into().unwrap())),
        _ => return Err(Error::BadLength(-2)),
    })
}

fn floats(r: &mut Reader, n: usize) -> Result<Value, Error> {
    (0..n).map(|_| r.f32()).collect::<Result<Vec<_>, _>>().map(Value::Floats)
}

fn ints(r: &mut Reader, n: usize) -> Result<Value, Error> {
    (0..n).map(|_| r.i32()).collect::<Result<Vec<_>, _>>().map(Value::Ints)
}

fn read_builtin(r: &mut Reader, name: &str) -> Result<Value, Error> {
    match name {
        "Vector2" => floats(r, 2),
        "Vector3" => floats(r, 3),
        "Vector4" | "Quaternion" | "Color" | "Rect" => floats(r, 4),
        "Bounds" => floats(r, 6),
        "Matrix4x4" => floats(r, 16),
        "Vector2Int" => ints(r, 2),
        "Vector3Int" => ints(r, 3),
        "RectInt" | "RectOffset" => ints(r, 4),
        "BoundsInt" => ints(r, 6),
        "Color32" | "LayerMask" => Ok(Value::UInt(r.u32()? as u64)),
        "Hash128" => Ok(Value::Bytes(r.take(16)?.to_vec())),
        "AnimationCurve" => {
            // m_Curve: Keyframe (time, value, inSlope, outSlope, weightedMode i32, inWeight, outWeight)
            let n = r.len()?;
            let mut keys = Vec::with_capacity(n);
            for _ in 0..n {
                keys.push(floats(r, 7)?);
            }
            let pre = r.i32()?;
            let post = r.i32()?;
            let order = r.i32()?;
            Ok(Value::Struct(vec![
                ("m_Curve".into(), Value::Array(keys)),
                ("m_PreInfinity".into(), Value::Int(pre as i64)),
                ("m_PostInfinity".into(), Value::Int(post as i64)),
                ("m_RotationOrder".into(), Value::Int(order as i64)),
            ]))
        }
        "Gradient" => {
            // key0..7 (ColorRGBA f32 ×4), ctime0..7 u16, atime0..7 u16, mode i32, color/alpha key counts u8
            let colors = floats(r, 32)?;
            let times = (0..16).map(|_| r.u16().map(|x| x as i32)).collect::<Result<Vec<_>, _>>()?;
            let mode = r.i32()?;
            let nc = r.u8()?;
            let na = r.u8()?;
            r.align4();
            Ok(Value::Struct(vec![
                ("keys".into(), colors),
                ("times".into(), Value::Ints(times)),
                ("m_Mode".into(), Value::Int(mode as i64)),
                ("m_NumColorKeys".into(), Value::UInt(nc as u64)),
                ("m_NumAlphaKeys".into(), Value::UInt(na as u64)),
            ]))
        }
        _ => Err(Error::BadLength(-3)),
    }
}

/// Read one value of type `t`. `as_field` = it is a field of a struct (small primitives then align to 4).
fn read_value(r: &mut Reader, t: &str, as_field: bool) -> Result<Value, Error> {
    if let Some(size) = prim_size(t) {
        let v = read_prim(r, t)?;
        if as_field && size < 4 {
            r.align4();
        }
        return Ok(v);
    }
    if t == "string" {
        return Ok(Value::Str(r.string()?));
    }
    if t == "pptr" {
        return Ok(Value::PPtr(r.pptr()?));
    }
    if let Some(u) = t.strip_prefix("enum:") {
        // Unity 2020.3 serializes every enum as a 32-bit int, whatever its underlying type.
        let _ = u;
        return Ok(Value::Int(r.i32()? as i64));
    }
    if let Some(b) = t.strip_prefix("builtin:") {
        return read_builtin(r, b);
    }
    if let Some(e) = t.strip_prefix("array:") {
        let n = r.len()?;
        let v = if e == "u8" || e == "bool" || e == "i8" {
            Value::Bytes(r.take(n)?.to_vec())
        } else {
            Value::Array((0..n).map(|_| read_value(r, e, false)).collect::<Result<Vec<_>, _>>()?)
        };
        r.align4();
        return Ok(v);
    }
    if let Some(k) = t.strip_prefix("type:") {
        let def = layouts().types.get(k).and_then(|d| d.as_ref()).ok_or(Error::BadLength(-4))?;
        return read_struct(r, &def.fields);
    }
    Err(Error::BadLength(-5))
}

fn read_struct(r: &mut Reader, fields: &[Field]) -> Result<Value, Error> {
    let mut out = Vec::with_capacity(fields.len());
    for f in fields {
        out.push((f.n.clone(), read_value(r, &f.t, true)?));
    }
    Ok(Value::Struct(out))
}

/// Read a MonoBehaviour's own fields (after its header) for the script `key`, to the end of the object.
pub fn read_mono_behaviour(bytes: &[u8], header: &MonoBehaviourHeader, key: &str) -> Result<Value, Error> {
    let def = layouts().scripts.get(key).ok_or(Error::BadLength(-6))?;
    let mut r = Reader::new(bytes);
    r.at = header.fields_at;
    let v = read_struct(&mut r, &def.fields)?;
    r.finish()?;
    Ok(v)
}
