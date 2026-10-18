//! The town and settings files as the game writes and reads them (row G3): `SaveSystem.saveXmlSerializer` /
//! `settingsXmlSerializer` are `XmlSerializer`s of `SaveData` / `SettingsData` (Mono's), writing UTF-8 without a
//! byte order mark through an indented `XmlTextWriter`: the declaration, the root with the `xsd` then `xsi`
//! namespace declarations, every public field in declaration order as an element (two spaces a level, CRLF),
//! an empty string or list as `<name />`, a null string or array left out, numbers in invariant form, booleans
//! `true` / `false`, byte arrays in base64; no newline after the root's end tag. Reading is the serializer's:
//! elements in any order, unknown ones skipped, missing ones keeping the constructor's values.

use crate::placemaker::save_data::{SaveCorner, SaveData, SaveVoxel, SettingsData};
use crate::unity::dotnet;

const DECLARATION: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>";
const NAMESPACES: &str = " xmlns:xsd=\"http://www.w3.org/2001/XMLSchema\" xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\"";

// ---------------------------------------------------------------------------------------------------------
// Writing

/// An element tree in the making (what the serializer would write, before the text writer lays it out).
enum Node {
    Leaf(&'static str, String),
    Group(&'static str, Vec<Node>),
}

/// `XmlTextWriter.WriteString`'s escaping of element text.
fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
    out
}

fn lay_out(out: &mut String, node: &Node, depth: usize) {
    out.push_str("\r\n");
    for _ in 0..depth {
        out.push_str("  ");
    }
    match node {
        Node::Leaf(name, text) if text.is_empty() => out.push_str(&format!("<{name} />")),
        Node::Leaf(name, text) => out.push_str(&format!("<{name}>{}</{name}>", escape(text))),
        Node::Group(name, children) if children.is_empty() => out.push_str(&format!("<{name} />")),
        Node::Group(name, children) => {
            out.push_str(&format!("<{name}>"));
            for c in children {
                lay_out(out, c, depth + 1);
            }
            out.push_str("\r\n");
            for _ in 0..depth {
                out.push_str("  ");
            }
            out.push_str(&format!("</{name}>"));
        }
    }
}

fn document(root: &'static str, children: &[Node]) -> Vec<u8> {
    let mut out = String::from(DECLARATION);
    out.push_str("\r\n<");
    out.push_str(root);
    out.push_str(NAMESPACES);
    out.push('>');
    for c in children {
        lay_out(&mut out, c, 1);
    }
    out.push_str("\r\n</");
    out.push_str(root);
    out.push('>');
    out.into_bytes()
}

fn int(name: &'static str, v: i64) -> Node {
    Node::Leaf(name, dotnet::int_to_string(v))
}
fn boolean(name: &'static str, v: bool) -> Node {
    Node::Leaf(name, if v { "true" } else { "false" }.into())
}
fn string(out: &mut Vec<Node>, name: &'static str, v: &Option<String>) {
    if let Some(s) = v {
        out.push(Node::Leaf(name, s.clone()));
    }
}

/// `Convert.ToBase64String`.
pub fn to_base64(b: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(b.len().div_ceil(3) * 4);
    for c in b.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        out.push(A[(n >> 18) as usize & 63] as char);
        out.push(A[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 { A[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if c.len() > 2 { A[n as usize & 63] as char } else { '=' });
    }
    out
}

/// `saveXmlSerializer.Serialize(stream, saveData)`: the bytes of a `.scape` file.
pub fn write_save(d: &SaveData) -> Vec<u8> {
    let mut nodes = vec![];
    string(&mut nodes, "saveString", &d.save_string);
    nodes.push(int("lastWriteTime", d.last_write_time));
    nodes.push(Node::Group("cam", vec![int("x", d.cam.x as i64), int("y", d.cam.y as i64), int("z", d.cam.z as i64)]));
    nodes.push(Node::Group("sun", vec![int("h", d.sun.h as i64), int("r", d.sun.r as i64)]));
    let corners = d.corners.iter().map(|c| Node::Group("C", vec![int("x", c.x as i64), int("y", c.y as i64), int("count", c.count as i64)])).collect();
    nodes.push(Node::Group("corners", corners));
    let voxels = d.voxels.iter().map(|v| Node::Group("V", vec![int("t", v.t as i64), int("h", v.h as i64)])).collect();
    nodes.push(Node::Group("voxels", voxels));
    if let Some(j) = &d.big_jpg {
        nodes.push(Node::Leaf("bigJpg", to_base64(j)));
    }
    document("SaveData", &nodes)
}

/// `settingsXmlSerializer.Serialize(stream, settingsData)`: the bytes of `Sett.ings`.
pub fn write_settings(s: &SettingsData) -> Vec<u8> {
    let mut nodes = vec![int("version", s.version as i64)];
    string(&mut nodes, "lastSave", &s.last_save);
    nodes.push(int("currentColor", s.current_color as i64));
    nodes.push(int("audioVolume", s.audio_volume as i64));
    nodes.push(boolean("antiAliasing", s.anti_aliasing));
    nodes.push(Node::Group("axis", vec![boolean("rotationXInverted", s.axis.rotation_x_inverted), boolean("rotationYInverted", s.axis.rotation_y_inverted)]));
    nodes.push(boolean("vSync", s.v_sync));
    nodes.push(boolean("fullscreen", s.fullscreen));
    string(&mut nodes, "language", &s.language);
    let mut shot = vec![];
    string(&mut shot, "path", &s.screenshot.path);
    shot.push(int("x", s.screenshot.x as i64));
    shot.push(int("y", s.screenshot.y as i64));
    nodes.push(Node::Group("screenshot", shot));
    let mut obj = vec![];
    string(&mut obj, "path", &s.obj_export.path);
    nodes.push(Node::Group("objExport", obj));
    nodes.push(int("uiSizePercentage", s.ui_size_percentage as i64));
    string(&mut nodes, "lastRunVersion", &s.last_run_version);
    document("SettingsData", &nodes)
}

// ---------------------------------------------------------------------------------------------------------
// Reading

/// Why a file does not read (the serializer throws; the game then keeps no save data: a broken save).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct XmlError(pub String);

fn err<T>(m: impl Into<String>) -> Result<T, XmlError> {
    Err(XmlError(m.into()))
}

/// A parsed element: its name, child elements and text.
#[derive(Debug, Default)]
struct Element {
    name: String,
    children: Vec<Element>,
    text: String,
}

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
}

impl<'a> Parser<'a> {
    fn peek(&self, t: &str) -> bool {
        self.s[self.i..].starts_with(t.as_bytes())
    }
    fn skip_until(&mut self, t: &str) -> Result<(), XmlError> {
        match self.s[self.i..].windows(t.len()).position(|w| w == t.as_bytes()) {
            Some(p) => {
                self.i += p + t.len();
                Ok(())
            }
            None => err(format!("unterminated, expected {t}")),
        }
    }
    fn skip_white(&mut self) {
        while self.i < self.s.len() && matches!(self.s[self.i], b' ' | b'\t' | b'\r' | b'\n') {
            self.i += 1;
        }
    }
    /// Declarations, comments, processing instructions and white space between elements.
    fn skip_misc(&mut self) -> Result<(), XmlError> {
        loop {
            self.skip_white();
            if self.peek("<?") {
                self.skip_until("?>")?;
            } else if self.peek("<!--") {
                self.skip_until("-->")?;
            } else if self.peek("<!") {
                self.skip_until(">")?;
            } else {
                return Ok(());
            }
        }
    }
    fn name(&mut self) -> String {
        let st = self.i;
        while self.i < self.s.len() && !matches!(self.s[self.i], b' ' | b'\t' | b'\r' | b'\n' | b'/' | b'>' | b'=') {
            self.i += 1;
        }
        String::from_utf8_lossy(&self.s[st..self.i]).into_owned()
    }
    fn element(&mut self) -> Result<Element, XmlError> {
        if !self.peek("<") {
            return err("expected an element");
        }
        self.i += 1;
        let mut e = Element { name: self.name(), ..Default::default() };
        // attributes (namespace declarations, xsi:nil, ...) are not fields here
        loop {
            self.skip_white();
            if self.peek("/>") {
                self.i += 2;
                return Ok(e);
            }
            if self.peek(">") {
                self.i += 1;
                break;
            }
            let _ = self.name();
            self.skip_white();
            if !self.peek("=") {
                return err("bad attribute");
            }
            self.i += 1;
            self.skip_white();
            let q = *self.s.get(self.i).ok_or(XmlError("eof".into()))?;
            if q != b'"' && q != b'\'' {
                return err("bad attribute value");
            }
            self.i += 1;
            while self.i < self.s.len() && self.s[self.i] != q {
                self.i += 1;
            }
            self.i += 1;
        }
        let mut raw = vec![];
        loop {
            if self.i >= self.s.len() {
                return err(format!("unterminated <{}>", e.name));
            }
            if self.peek("</") {
                self.i += 2;
                let n = self.name();
                if n != e.name {
                    return err(format!("</{n}> closes <{}>", e.name));
                }
                self.skip_white();
                if !self.peek(">") {
                    return err("bad end tag");
                }
                self.i += 1;
                break;
            } else if self.peek("<!--") {
                self.skip_until("-->")?;
            } else if self.peek("<![CDATA[") {
                self.i += 9;
                let st = self.i;
                self.skip_until("]]>")?;
                raw.extend_from_slice(&self.s[st..self.i - 3]);
            } else if self.peek("<?") {
                self.skip_until("?>")?;
            } else if self.peek("<") {
                e.children.push(self.element()?);
            } else if self.peek("&") {
                let st = self.i;
                self.skip_until(";")?;
                let ent = std::str::from_utf8(&self.s[st + 1..self.i - 1]).map_err(|_| XmlError("bad entity".into()))?;
                let c = match ent {
                    "lt" => '<',
                    "gt" => '>',
                    "amp" => '&',
                    "quot" => '"',
                    "apos" => '\'',
                    _ if ent.starts_with("#x") => char::from_u32(u32::from_str_radix(&ent[2..], 16).map_err(|_| XmlError("bad entity".into()))?).ok_or(XmlError("bad char".into()))?,
                    _ if ent.starts_with('#') => char::from_u32(ent[1..].parse().map_err(|_| XmlError("bad entity".into()))?).ok_or(XmlError("bad char".into()))?,
                    _ => return err(format!("unknown entity &{ent};")),
                };
                let mut b = [0u8; 4];
                raw.extend_from_slice(c.encode_utf8(&mut b).as_bytes());
            } else {
                // line ends normalised as XML readers do
                if self.s[self.i] == b'\r' {
                    raw.push(b'\n');
                    self.i += 1;
                    if self.peek("\n") {
                        self.i += 1;
                    }
                    continue;
                }
                raw.push(self.s[self.i]);
                self.i += 1;
            }
        }
        e.text = String::from_utf8(raw).map_err(|_| XmlError("not UTF-8".into()))?;
        // (the serializer's reader skips white-space-only text as insignificant)
        if e.text.chars().all(|c| matches!(c, ' ' | '\t' | '\n' | '\r')) {
            e.text.clear();
        }
        Ok(e)
    }
}

fn parse(bytes: &[u8]) -> Result<Element, XmlError> {
    let s = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    let mut p = Parser { s, i: 0 };
    p.skip_misc()?;
    let root = p.element()?;
    p.skip_misc()?;
    if p.i != s.len() {
        return err("data after the root element");
    }
    Ok(root)
}

/// `XmlConvert` white space round a value.
fn xml_trim(s: &str) -> &str {
    s.trim_matches(|c| matches!(c, ' ' | '\t' | '\r' | '\n'))
}
fn read_long(e: &Element) -> Result<i64, XmlError> {
    dotnet::parse_long(xml_trim(&e.text)).map_err(|x| XmlError(format!("<{}>: {x:?}", e.name)))
}
fn read_int(e: &Element) -> Result<i32, XmlError> {
    dotnet::parse_int(xml_trim(&e.text)).map_err(|x| XmlError(format!("<{}>: {x:?}", e.name)))
}
fn read_byte(e: &Element) -> Result<u8, XmlError> {
    let v = read_int(e)?;
    u8::try_from(v).map_err(|_| XmlError(format!("<{}>: Overflow", e.name)))
}
/// `XmlConvert.ToBoolean`: `true` / `1`, `false` / `0`.
fn read_bool(e: &Element) -> Result<bool, XmlError> {
    match xml_trim(&e.text) {
        "true" | "1" => Ok(true),
        "false" | "0" => Ok(false),
        t => err(format!("<{}>: not a boolean: {t:?}", e.name)),
    }
}

/// `Convert.FromBase64String` (white space ignored).
pub fn from_base64(s: &str) -> Result<Vec<u8>, XmlError> {
    let mut vals = vec![];
    let mut pad = 0;
    for c in s.chars() {
        let v = match c {
            'A'..='Z' => c as u32 - 'A' as u32,
            'a'..='z' => c as u32 - 'a' as u32 + 26,
            '0'..='9' => c as u32 - '0' as u32 + 52,
            '+' => 62,
            '/' => 63,
            '=' => {
                pad += 1;
                continue;
            }
            ' ' | '\t' | '\r' | '\n' => continue,
            _ => return err("bad base64"),
        };
        if pad > 0 {
            return err("bad base64 padding");
        }
        vals.push(v);
    }
    if (vals.len() + pad) % 4 != 0 || pad > 2 {
        return err("bad base64 length");
    }
    let mut out = vec![];
    for c in vals.chunks(4) {
        let n = c.iter().enumerate().fold(0u32, |a, (i, &v)| a | v << (18 - 6 * i));
        out.push((n >> 16) as u8);
        if c.len() > 2 {
            out.push((n >> 8) as u8);
        }
        if c.len() > 3 {
            out.push(n as u8);
        }
    }
    Ok(out)
}

/// `saveXmlSerializer.Deserialize(stream)`: a `.scape` file.
pub fn read_save(bytes: &[u8]) -> Result<SaveData, XmlError> {
    let root = parse(bytes)?;
    if root.name != "SaveData" {
        return err(format!("<{}> is not a SaveData", root.name));
    }
    let mut d = SaveData::default();
    for e in &root.children {
        match e.name.as_str() {
            "saveString" => d.save_string = Some(e.text.clone()),
            "lastWriteTime" => d.last_write_time = read_long(e)?,
            "cam" => {
                for f in &e.children {
                    match f.name.as_str() {
                        "x" => d.cam.x = read_byte(f)?,
                        "y" => d.cam.y = read_byte(f)?,
                        "z" => d.cam.z = read_byte(f)?,
                        _ => {}
                    }
                }
            }
            "sun" => {
                for f in &e.children {
                    match f.name.as_str() {
                        "h" => d.sun.h = read_byte(f)?,
                        "r" => d.sun.r = read_byte(f)?,
                        _ => {}
                    }
                }
            }
            "corners" => {
                for c in e.children.iter().filter(|c| c.name == "C") {
                    let mut v = SaveCorner::default();
                    for f in &c.children {
                        match f.name.as_str() {
                            "x" => v.x = read_int(f)?,
                            "y" => v.y = read_int(f)?,
                            "count" => v.count = read_int(f)?,
                            _ => {}
                        }
                    }
                    d.corners.push(v);
                }
            }
            "voxels" => {
                for c in e.children.iter().filter(|c| c.name == "V") {
                    let mut v = SaveVoxel::default();
                    for f in &c.children {
                        match f.name.as_str() {
                            "t" => v.t = read_byte(f)?,
                            "h" => v.h = read_byte(f)?,
                            _ => {}
                        }
                    }
                    d.voxels.push(v);
                }
            }
            "bigJpg" => d.big_jpg = Some(from_base64(&e.text)?),
            _ => {}
        }
    }
    Ok(d)
}

/// `settingsXmlSerializer.Deserialize(stream)`: `Sett.ings`.
pub fn read_settings(bytes: &[u8]) -> Result<SettingsData, XmlError> {
    let root = parse(bytes)?;
    if root.name != "SettingsData" {
        return err(format!("<{}> is not a SettingsData", root.name));
    }
    let mut s = SettingsData::default();
    for e in &root.children {
        match e.name.as_str() {
            "version" => s.version = read_int(e)?,
            "lastSave" => s.last_save = Some(e.text.clone()),
            "currentColor" => s.current_color = read_byte(e)?,
            "audioVolume" => s.audio_volume = read_byte(e)?,
            "antiAliasing" => s.anti_aliasing = read_bool(e)?,
            "axis" => {
                for f in &e.children {
                    match f.name.as_str() {
                        "rotationXInverted" => s.axis.rotation_x_inverted = read_bool(f)?,
                        "rotationYInverted" => s.axis.rotation_y_inverted = read_bool(f)?,
                        _ => {}
                    }
                }
            }
            "vSync" => s.v_sync = read_bool(e)?,
            "fullscreen" => s.fullscreen = read_bool(e)?,
            "language" => s.language = Some(e.text.clone()),
            "screenshot" => {
                for f in &e.children {
                    match f.name.as_str() {
                        "path" => s.screenshot.path = Some(f.text.clone()),
                        "x" => s.screenshot.x = read_int(f)?,
                        "y" => s.screenshot.y = read_int(f)?,
                        _ => {}
                    }
                }
            }
            "objExport" => {
                for f in &e.children {
                    if f.name == "path" {
                        s.obj_export.path = Some(f.text.clone());
                    }
                }
            }
            "uiSizePercentage" => s.ui_size_percentage = read_byte(e)?,
            "lastRunVersion" => s.last_run_version = Some(e.text.clone()),
            _ => {}
        }
    }
    Ok(s)
}
