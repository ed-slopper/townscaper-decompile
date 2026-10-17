//! The call server (row F9b): JSON lines on 127.0.0.1 (`TS_HARNESS_PORT`, default 47615).
//!
//! Request:  `{"id":1, "call":"UnityEngine.Mathf.RoundToInt", "args":[{"raw":"0000203f"}],
//!             "param_types":["System.Single"], "main":true, "on":"Placemaker.WorldMaster"}`
//!   - `call`: `Namespace.Class.Method` (the class is looked up in every loaded image);
//!   - `args`: `{"raw": hex}` value-type bytes (little-endian, as in memory), `{"string": s}`,
//!     `{"array": "System.Single", "raw": hex}` a new array of a value type, `{"null": true}`,
//!     `{"handle": n}` an object made by `new` (or kept);
//!   - `param_types` (optional): picks the overload by il2cpp's parameter type names;
//!   - `main` (optional): run on Unity's main thread (needed by most UnityEngine APIs);
//!   - `on` (optional): call on the first object of that type in the scene (`FindObjectOfType`, inactive too),
//!     else the first loaded one (`Resources.FindObjectsOfTypeAll`: assets such as `ScriptableObject`s);
//!   - `this_raw` (optional): `{"class": "UnityEngine.Vector3", "raw": hex}`: an instance method of a value type
//!     on that value (boxed); the value after the call comes back as `this_after`.
//! Raw arguments are passed by pointer, so `ref` / `out` parameters work: every raw argument's bytes after the
//! call come back in `args_after` (hex, in argument order; `null` for non-raw arguments).
//! Reply: `{"id":1, "ok":true, "type":"System.Int32", "raw":"02000000"}`, or `"string"`, or
//!   `{"array": elem type, "raw": hex, "n": len}`, `{"object": class}`, `{"null": true}`, `{"void": true}`;
//!   errors: `{"ok":false, "error": text}` (managed exceptions formatted by il2cpp).
//! `{"cmd":"ping"}` → `{"ok":true}`; `{"cmd":"quit"}` ends the game process.
//! Native engine functions (stripped from the managed side, e.g. `Random.InitState`):
//! `{"native": "UnityPlayer.dll", "va": "0x107A74A0", "args": [{"u32": 5}, {"buf": hex}]}`: a `cdecl` function at
//! its address in the PDB-based Ghidra project (image base 0x10000000, rebased to the loaded module), up to 6
//! 32-bit arguments; `buf` arguments are pointers to a copy of the bytes, returned in `bufs_after`; `eax` is the
//! 32-bit return value.
//! Generic instantiations: `"class": "System.Collections.Generic.Dictionary`2[[System.Int32],[System.Int32]]"`
//! (a `Type.GetType` name) in place of the class part of `call` (`"call": "Add"` is then just the method).
//! Objects: `{"new": <class name or Type.GetType name>, "args": [...]}` makes an object (`.ctor` with the args)
//! and returns `{"handle": n}` (a GC handle, kept alive); `"handle": n` in a call makes it the `this`.
//! Array arguments come back after the call too, in `args_after` (their element bytes).
//! `{"batch": [request, ...], "main": true}` runs the requests one after another in the same main-thread job
//! (no game frame in between: e.g. `Random` draws, which the game itself also makes every frame); the reply's
//! `results` has one reply per request.
//! `{"field": "name", "handle": n, "main": true}` reads (and with `set_raw` / `set_handle` / `set_null`, first
//! writes) an instance field: value types as `raw`, references as a kept `handle`.
//! `static_field` with `"keep": true` also returns a reference field's object as a `handle`; `{"array_raw": n}`
//! returns the element bytes of a value-type array held by handle `n`.
//! `{"icall": "Ns.Class::method(Args)", "args": [...], "main": true}` calls an engine internal call by name
//! (for getters the build strips from the managed side; see `icall`).
//! `{"addr": n}` returns the address of the object held by handle `n` (to match references in raw values).
//! `{"static_field": "Namespace.Class.field", "main": true}` runs the class's static constructor and returns
//! the field's value as `{"type", "raw"}` (value types by value, references as the 4-byte pointer).

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use std::sync::{Mutex, OnceLock};

use serde_json::{json, Value};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::Threading::GetCurrentProcessId;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, EnumWindows, GetWindowThreadProcessId, PostThreadMessageW, SetWindowsHookExW, WH_GETMESSAGE, WM_NULL,
};

use crate::il2cpp::{c, cstr, Api, Ptr};
use crate::log;

const ARRAY_DATA: usize = 16; // Il2CppArray on x86: klass, monitor, bounds, max_length

pub struct Ctx {
    pub api: Api,
    pub domain: Ptr,
}
unsafe impl Send for Ctx {}
unsafe impl Sync for Ctx {}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn unhex(s: &str) -> Result<Vec<u8>, String> {
    if s.len() % 2 != 0 {
        return Err("odd hex".into());
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|e| e.to_string())).collect()
}

impl Ctx {
    unsafe fn find_class(&self, full: &str) -> Result<Ptr, String> {
        let (ns, name) = full.rsplit_once('.').unwrap_or(("", full));
        for (_, img) in self.api.images(self.domain) {
            let k = self.api.class(img, ns, name);
            if !k.is_null() {
                return Ok(k);
            }
        }
        Err(format!("class {full} not found"))
    }

    /// A class by `Namespace.Name`, or by a `Type.GetType` name (generic instantiations, nested `A+B` types,
    /// assembly-qualified names).
    unsafe fn resolve_class(&self, name: &str) -> Result<Ptr, String> {
        if !name.contains(['`', '+', ',']) {
            return self.find_class(name);
        }
        let ty = self.find_class("System.Type")?;
        let m = self.find_method(ty, "GetType", 1, Some(&vec![json!("System.String")]))?;
        let mut args: [Ptr; 1] = [(self.api.il2cpp_string_new)(c(name).as_ptr())];
        let mut exc: Ptr = std::ptr::null_mut();
        let t = (self.api.il2cpp_runtime_invoke)(m, std::ptr::null_mut(), args.as_mut_ptr(), &mut exc);
        if !exc.is_null() {
            return Err(self.exception_text(exc));
        }
        if t.is_null() {
            return Err(format!("Type.GetType({name}) is null (not in the build?)"));
        }
        let k = (self.api.il2cpp_class_from_system_type)(t);
        if k.is_null() {
            return Err(format!("no class for {name}"));
        }
        Ok(k)
    }

    unsafe fn type_name(&self, t: Ptr) -> String {
        let p = (self.api.il2cpp_type_get_name)(t);
        let s = cstr(p);
        (self.api.il2cpp_free)(p as Ptr);
        s
    }

    unsafe fn find_method(&self, class: Ptr, name: &str, argc: usize, types: Option<&Vec<Value>>) -> Result<Ptr, String> {
        let mut seen = Vec::new();
        for (n, pc, m) in self.api.methods(class) {
            if n != name || pc as usize != argc {
                continue;
            }
            let ptypes: Vec<String> = (0..pc).map(|i| self.type_name((self.api.il2cpp_method_get_param)(m, i))).collect();
            match types {
                Some(want) if want.iter().map(|v| v.as_str().unwrap_or("")).ne(ptypes.iter().map(String::as_str)) => {
                    seen.push(ptypes.join(", "));
                }
                _ => return Ok(m),
            }
        }
        Err(format!("method {name}/{argc} not found (overloads: {seen:?})"))
    }

    unsafe fn exception_text(&self, exc: Ptr) -> String {
        let mut buf = vec![0i8; 4096];
        (self.api.il2cpp_format_exception)(exc, buf.as_mut_ptr(), buf.len() as i32);
        cstr(buf.as_ptr())
    }

    unsafe fn find_object(&self, type_name: &str) -> Result<Ptr, String> {
        let k = self.find_class(type_name)?;
        let ty = (self.api.il2cpp_type_get_object)((self.api.il2cpp_class_get_type)(k));
        let obj_class = self.find_class("UnityEngine.Object")?;
        let m = self.find_method(obj_class, "FindObjectOfType", 2, None)?;
        let mut include_inactive = 1u8;
        let mut args: [Ptr; 2] = [ty, &mut include_inactive as *mut u8 as Ptr];
        let mut exc: Ptr = std::ptr::null_mut();
        let r = (self.api.il2cpp_runtime_invoke)(m, std::ptr::null_mut(), args.as_mut_ptr(), &mut exc);
        if !exc.is_null() {
            return Err(self.exception_text(exc));
        }
        if !r.is_null() {
            return Ok(r);
        }
        // Not in a scene: a loaded asset (a `ScriptableObject`, ...), `Resources.FindObjectsOfTypeAll(type)[0]`.
        let res = self.find_class("UnityEngine.Resources")?;
        let m = self.find_method(res, "FindObjectsOfTypeAll", 1, None)?;
        let mut args: [Ptr; 1] = [ty];
        let arr = (self.api.il2cpp_runtime_invoke)(m, std::ptr::null_mut(), args.as_mut_ptr(), &mut exc);
        if !exc.is_null() {
            return Err(self.exception_text(exc));
        }
        if arr.is_null() || (self.api.il2cpp_array_length)(arr) == 0 {
            return Err(format!("no {type_name} in the scene or loaded"));
        }
        Ok(*((arr as *const u8).add(ARRAY_DATA) as *const Ptr))
    }

    unsafe fn native(&self, req: &Value) -> Result<Value, String> {
        use windows_sys::Win32::System::LibraryLoader::GetModuleHandleA;
        let module = req["native"].as_str().ok_or("native: module")?;
        let va = u32::from_str_radix(req["va"].as_str().ok_or("native: va")?.trim_start_matches("0x"), 16).map_err(|e| e.to_string())?;
        let base = GetModuleHandleA(c(module).as_ptr() as *const u8);
        if base.is_null() {
            return Err(format!("{module} not loaded"));
        }
        let f = (base as usize).wrapping_add(va as usize).wrapping_sub(0x1000_0000);
        let mut bufs: Vec<Vec<u8>> = Vec::new();
        let mut words: Vec<u32> = Vec::new();
        for a in req["args"].as_array().cloned().unwrap_or_default() {
            if let Some(n) = a.get("u32").and_then(Value::as_u64) {
                words.push(n as u32);
            } else if let Some(h) = a.get("buf").and_then(Value::as_str) {
                bufs.push(unhex(h)?);
                words.push(bufs.last_mut().unwrap().as_mut_ptr() as usize as u32);
            } else {
                return Err(format!("bad native arg {a}"));
            }
        }
        let w = |i: usize| words.get(i).copied().unwrap_or(0);
        type F6 = unsafe extern "C" fn(u32, u32, u32, u32, u32, u32) -> u32;
        // cdecl: the caller cleans the stack, so passing unused trailing words is harmless
        let fp: F6 = std::mem::transmute(f);
        let eax = fp(w(0), w(1), w(2), w(3), w(4), w(5));
        Ok(json!({"eax": eax, "bufs_after": bufs.iter().map(|b| hex(b)).collect::<Vec<_>>()}))
    }

    /// `{"icall": "UnityEngine.BoxCollider::get_size_Injected(UnityEngine.Vector3&)", "args": [{"handle": n},
    /// {"buf": hex}]}`: an engine internal call by its registered name (`il2cpp_resolve_icall`), for getters the
    /// build strips from the managed side. Arguments: objects by handle (the managed pointer), `u32`s, or buffers
    /// (pointers to a copy, returned in `bufs_after`); `eax` is the 32-bit return value.
    unsafe fn icall(&self, req: &Value) -> Result<Value, String> {
        let name = req["icall"].as_str().ok_or("icall: name")?;
        let f = (self.api.il2cpp_resolve_icall)(c(name).as_ptr());
        if f.is_null() {
            return Err(format!("icall {name} not registered"));
        }
        let mut bufs: Vec<Vec<u8>> = Vec::new();
        let mut words: Vec<u32> = Vec::new();
        for a in req["args"].as_array().cloned().unwrap_or_default() {
            if let Some(n) = a.get("u32").and_then(Value::as_u64) {
                words.push(n as u32);
            } else if let Some(h) = a.get("handle").and_then(Value::as_u64) {
                words.push((self.api.il2cpp_gchandle_get_target)(h as u32) as usize as u32);
            } else if let Some(h) = a.get("buf").and_then(Value::as_str) {
                bufs.push(unhex(h)?);
                words.push(bufs.last_mut().unwrap().as_mut_ptr() as usize as u32);
            } else {
                return Err(format!("bad icall arg {a}"));
            }
        }
        let w = |i: usize| words.get(i).copied().unwrap_or(0);
        type F6 = unsafe extern "C" fn(u32, u32, u32, u32, u32, u32) -> u32;
        let fp: F6 = std::mem::transmute(f);
        let eax = fp(w(0), w(1), w(2), w(3), w(4), w(5));
        Ok(json!({"eax": eax, "bufs_after": bufs.iter().map(|b| hex(b)).collect::<Vec<_>>()}))
    }

    /// `{"static_field": "Os.Hex.HexMath.hexAxisX"}`: the class is initialised (its static constructor runs),
    /// then the field's bytes (a value type's data, or the 4-byte object pointer).
    unsafe fn static_field(&self, req: &Value) -> Result<Value, String> {
        let full = req["static_field"].as_str().ok_or("static_field")?;
        let (class_name, field) = full.rsplit_once('.').ok_or("static_field is Namespace.Class.field")?;
        let class = match req["class"].as_str() {
            Some(k) => self.resolve_class(k)?,
            None => self.find_class(class_name)?,
        };
        (self.api.il2cpp_runtime_class_init)(class);
        let f = (self.api.il2cpp_class_get_field_from_name)(class, c(field).as_ptr());
        if f.is_null() {
            return Err(format!("no field {field}"));
        }
        let t = (self.api.il2cpp_field_get_type)(f);
        let k = (self.api.il2cpp_class_from_type)(t);
        let size = if (self.api.il2cpp_class_is_valuetype)(k) != 0 {
            (self.api.il2cpp_class_value_size)(k, std::ptr::null_mut()) as usize
        } else {
            4
        };
        let mut buf = vec![0u8; size.max(4)];
        (self.api.il2cpp_field_static_get_value)(f, buf.as_mut_ptr() as Ptr);
        buf.truncate(size);
        let mut out = json!({"type": self.type_name(t), "raw": hex(&buf)});
        if (self.api.il2cpp_class_is_valuetype)(k) == 0 && req["keep"].as_bool() == Some(true) {
            let p = u32::from_le_bytes(buf[..4].try_into().unwrap()) as usize as Ptr;
            if !p.is_null() {
                out["handle"] = json!((self.api.il2cpp_gchandle_new)(p, 0));
            }
        }
        Ok(out)
    }

    /// `{"array_raw": n}`: the element bytes of the value-type array held by handle `n`.
    unsafe fn array_raw(&self, req: &Value) -> Result<Value, String> {
        let arr = (self.api.il2cpp_gchandle_get_target)(req["array_raw"].as_u64().ok_or("array_raw")? as u32);
        if arr.is_null() {
            return Err("array_raw: null".into());
        }
        let n = (self.api.il2cpp_array_length)(arr) as usize;
        let elem = (self.api.il2cpp_class_get_element_class)((self.api.il2cpp_object_get_class)(arr));
        let es = (self.api.il2cpp_class_value_size)(elem, std::ptr::null_mut()) as usize;
        Ok(json!({"n": n, "raw": hex(std::slice::from_raw_parts((arr as *const u8).add(ARRAY_DATA), n * es))}))
    }

    /// `{"field": "name", "handle": n}` (or `"on": class`): an instance field of an object. Value types come back
    /// as `{"type", "raw"}`, references as a new kept `{"handle": n}` (or `{"null": true}`). With `"set_raw": hex`
    /// (value types) or `"set_handle": n` / `"set_null": true` (references) the field is written first.
    unsafe fn field(&self, req: &Value) -> Result<Value, String> {
        let name = req["field"].as_str().ok_or("field")?;
        let obj = match (req["handle"].as_u64(), req["on"].as_str()) {
            (Some(h), _) => (self.api.il2cpp_gchandle_get_target)(h as u32),
            (None, Some(t)) => self.find_object(t)?,
            _ => return Err("field needs handle or on".into()),
        };
        if obj.is_null() {
            return Err("field: null object".into());
        }
        let f = (self.api.il2cpp_class_get_field_from_name)((self.api.il2cpp_object_get_class)(obj), c(name).as_ptr());
        if f.is_null() {
            return Err(format!("no field {name}"));
        }
        let t = (self.api.il2cpp_field_get_type)(f);
        let k = (self.api.il2cpp_class_from_type)(t);
        let value_type = (self.api.il2cpp_class_is_valuetype)(k) != 0;
        if let Some(r) = req["set_raw"].as_str() {
            let mut raw = unhex(r)?;
            (self.api.il2cpp_field_set_value)(obj, f, raw.as_mut_ptr() as Ptr);
        } else if let Some(h) = req["set_handle"].as_u64() {
            (self.api.il2cpp_field_set_value)(obj, f, (self.api.il2cpp_gchandle_get_target)(h as u32));
        } else if req.get("set_null").is_some() {
            (self.api.il2cpp_field_set_value)(obj, f, std::ptr::null_mut());
        }
        if value_type {
            let size = (self.api.il2cpp_class_value_size)(k, std::ptr::null_mut()) as usize;
            let mut buf = vec![0u8; size.max(4)];
            (self.api.il2cpp_field_get_value)(obj, f, buf.as_mut_ptr() as Ptr);
            buf.truncate(size);
            return Ok(json!({"type": self.type_name(t), "raw": hex(&buf)}));
        }
        let mut p: Ptr = std::ptr::null_mut();
        (self.api.il2cpp_field_get_value)(obj, f, &mut p as *mut Ptr as Ptr);
        if p.is_null() {
            return Ok(json!({"type": self.type_name(t), "null": true}));
        }
        Ok(json!({"type": self.type_name(t), "handle": (self.api.il2cpp_gchandle_new)(p, 0)}))
    }

    /// Run one call request (on whatever thread this is).
    pub unsafe fn call(&self, req: &Value) -> Value {
        if req.get("static_field").is_some() {
            return match self.static_field(req) {
                Ok(mut v) => {
                    v["ok"] = json!(true);
                    v
                }
                Err(e) => json!({"ok": false, "error": e}),
            };
        }
        if let Some(h) = req.get("addr").and_then(Value::as_u64) {
            // the object's address, to compare with references inside value types
            let p = (self.api.il2cpp_gchandle_get_target)(h as u32);
            return json!({"ok": true, "addr": p as usize as u64});
        }
        if req.get("array_raw").is_some() {
            return match self.array_raw(req) {
                Ok(mut v) => {
                    v["ok"] = json!(true);
                    v
                }
                Err(e) => json!({"ok": false, "error": e}),
            };
        }
        if req.get("field").is_some() {
            return match self.field(req) {
                Ok(mut v) => {
                    v["ok"] = json!(true);
                    v
                }
                Err(e) => json!({"ok": false, "error": e}),
            };
        }
        if let Some(list) = req.get("batch").and_then(Value::as_array) {
            let results: Vec<Value> = list.iter().map(|r| self.call(r)).collect();
            return json!({"ok": true, "results": results});
        }
        if let Some(k) = req.get("new").and_then(Value::as_str) {
            let made = (|| -> Result<Value, String> {
                let class = self.resolve_class(k)?;
                let obj = (self.api.il2cpp_object_new)(class);
                let handle = (self.api.il2cpp_gchandle_new)(obj, 0);
                let mut r = req.clone();
                r["call"] = json!(".ctor");
                r["class"] = json!(k);
                r["handle"] = json!(handle);
                r.as_object_mut().unwrap().remove("new");
                let v = self.call_inner(&r)?;
                Ok(json!({"handle": handle, "ctor": v}))
            })();
            return match made {
                Ok(mut v) => {
                    v["ok"] = json!(true);
                    v
                }
                Err(e) => json!({"ok": false, "error": e}),
            };
        }
        if req.get("icall").is_some() {
            return match self.icall(req) {
                Ok(mut v) => {
                    v["ok"] = json!(true);
                    v
                }
                Err(e) => json!({"ok": false, "error": e}),
            };
        }
        if req.get("native").is_some() {
            return match self.native(req) {
                Ok(mut v) => {
                    v["ok"] = json!(true);
                    v
                }
                Err(e) => json!({"ok": false, "error": e}),
            };
        }
        match self.call_inner(req) {
            Ok(mut v) => {
                v["ok"] = json!(true);
                v
            }
            Err(e) => json!({"ok": false, "error": e}),
        }
    }

    unsafe fn call_inner(&self, req: &Value) -> Result<Value, String> {
        let call = req["call"].as_str().ok_or("no call")?;
        // constructors are `.ctor` / `.cctor`: `UnityEngine.Bounds..ctor`
        let (class_name, method) = match call.strip_suffix("..ctor").or(call.strip_suffix("..cctor")) {
            Some(c) => (c, if call.ends_with("..ctor") { ".ctor" } else { ".cctor" }),
            None => call.rsplit_once('.').unwrap_or(("", call)),
        };
        let (class, method) = match req["class"].as_str() {
            Some(k) => (self.resolve_class(k)?, call),
            None => (self.find_class(class_name)?, method),
        };
        let args = req["args"].as_array().cloned().unwrap_or_default();
        let m = self.find_method(class, method, args.len(), req["param_types"].as_array())?;
        let mut this_size = 0usize;
        let this = match (req["on"].as_str(), req.get("this_raw")) {
            _ if req.get("handle").and_then(Value::as_u64).is_some() => {
                (self.api.il2cpp_gchandle_get_target)(req["handle"].as_u64().unwrap() as u32)
            }
            (Some(t), _) => self.find_object(t)?,
            (None, Some(v)) if v.is_object() => {
                let k = self.resolve_class(v["class"].as_str().ok_or("this_raw.class")?)?;
                let raw = unhex(v["raw"].as_str().ok_or("this_raw.raw")?)?;
                this_size = raw.len();
                // value-type instance methods take the unboxed data as `this`
                (self.api.il2cpp_object_unbox)((self.api.il2cpp_value_box)(k, raw.as_ptr() as Ptr))
            }
            _ => std::ptr::null_mut(),
        };

        // Argument storage must outlive the invoke.
        let mut store: Vec<Vec<u8>> = Vec::new();
        let mut ptrs: Vec<Ptr> = Vec::new();
        let mut arrays: Vec<(usize, Ptr, usize)> = Vec::new(); // (arg index, array, bytes)
        for a in &args {
            if let Some(r) = a.get("array").and_then(Value::as_str) {
                let elem = self.find_class(r)?;
                let raw = unhex(a["raw"].as_str().unwrap_or(""))?;
                let size = (self.api.il2cpp_class_value_size)(elem, std::ptr::null_mut()) as usize;
                let n = if size == 0 { 0 } else { raw.len() / size };
                let arr = (self.api.il2cpp_array_new)(elem, n);
                std::ptr::copy_nonoverlapping(raw.as_ptr(), (arr as *mut u8).add(ARRAY_DATA), n * size);
                arrays.push((ptrs.len(), arr, n * size));
                ptrs.push(arr);
            } else if let Some(r) = a.get("raw").and_then(Value::as_str) {
                store.push(unhex(r)?);
                ptrs.push(store.last_mut().unwrap().as_mut_ptr() as Ptr);
            } else if let Some(s) = a.get("string").and_then(Value::as_str) {
                ptrs.push((self.api.il2cpp_string_new)(c(s).as_ptr()));
            } else if a.get("null").is_some() {
                ptrs.push(std::ptr::null_mut());
            } else if let Some(h) = a.get("handle").and_then(Value::as_u64) {
                ptrs.push((self.api.il2cpp_gchandle_get_target)(h as u32));
            } else {
                return Err(format!("bad arg {a}"));
            }
        }
        let mut exc: Ptr = std::ptr::null_mut();
        let r = (self.api.il2cpp_runtime_invoke)(m, this, ptrs.as_mut_ptr(), &mut exc);
        if !exc.is_null() {
            return Err(self.exception_text(exc));
        }
        let mut store_i = 0;
        let args_after: Vec<Value> = args
            .iter()
            .enumerate()
            .map(|(i, a)| {
                if let Some(&(_, arr, bytes)) = arrays.iter().find(|x| x.0 == i) {
                    json!(hex(std::slice::from_raw_parts((arr as *const u8).add(ARRAY_DATA), bytes)))
                } else if a.get("raw").is_some() {
                    store_i += 1;
                    json!(hex(&store[store_i - 1]))
                } else {
                    Value::Null
                }
            })
            .collect();
        let this_after = if this_size > 0 {
            json!(hex(std::slice::from_raw_parts(this as *const u8, this_size)))
        } else {
            Value::Null
        };
        let mut v = self.reply(m, r, &tname_of(self, m))?;
        // `"keep": true`: keep a returned object alive and return its handle
        if req["keep"].as_bool() == Some(true)
            && !r.is_null()
            && (self.api.il2cpp_class_is_valuetype)((self.api.il2cpp_object_get_class)(r)) == 0
        {
            v["handle"] = json!((self.api.il2cpp_gchandle_new)(r, 0));
        }
        v["args_after"] = json!(args_after);
        if this_size > 0 {
            v["this_after"] = this_after;
        }
        Ok(v)
    }

    unsafe fn reply(&self, m: Ptr, r: Ptr, _t: &str) -> Result<Value, String> {
        let rt = (self.api.il2cpp_method_get_return_type)(m);
        let tname = self.type_name(rt);
        let kind = (self.api.il2cpp_type_get_type)(rt);
        const VOID: i32 = 0x01;
        const STRING: i32 = 0x0e;
        const SZARRAY: i32 = 0x1d;
        if kind == VOID {
            return Ok(json!({"void": true}));
        }
        if r.is_null() {
            return Ok(json!({"type": tname, "null": true}));
        }
        let rclass = (self.api.il2cpp_class_from_type)(rt);
        if (self.api.il2cpp_class_is_valuetype)(rclass) != 0 {
            let size = (self.api.il2cpp_class_value_size)(rclass, std::ptr::null_mut()) as usize;
            let p = (self.api.il2cpp_object_unbox)(r) as *const u8;
            return Ok(json!({"type": tname, "raw": hex(std::slice::from_raw_parts(p, size))}));
        }
        if kind == STRING {
            let n = (self.api.il2cpp_string_length)(r) as usize;
            let s = String::from_utf16_lossy(std::slice::from_raw_parts((self.api.il2cpp_string_chars)(r), n));
            return Ok(json!({"type": tname, "string": s}));
        }
        if kind == SZARRAY {
            let ac = (self.api.il2cpp_object_get_class)(r); // array class; its element class gives the size
            let ec = (self.api.il2cpp_class_get_element_class)(ac);
            let n = (self.api.il2cpp_array_length)(r) as usize;
            let es = (self.api.il2cpp_class_value_size)(ec, std::ptr::null_mut()) as usize;
            let elem = format!("{}.{}", cstr((self.api.il2cpp_class_get_namespace)(ec)), cstr((self.api.il2cpp_class_get_name)(ec)));
            if (self.api.il2cpp_class_is_valuetype)(ec) == 0 {
                return Ok(json!({"type": tname, "array": elem, "n": n, "refs": true}));
            }
            let p = (r as *const u8).add(ARRAY_DATA);
            return Ok(json!({"type": tname, "array": elem, "n": n, "raw": hex(std::slice::from_raw_parts(p, n * es))}));
        }
        let oc = (self.api.il2cpp_object_get_class)(r);
        Ok(json!({"type": tname, "object": format!("{}.{}", cstr((self.api.il2cpp_class_get_namespace)(oc)), cstr((self.api.il2cpp_class_get_name)(oc)))}))
    }
}

unsafe fn tname_of(ctx: &Ctx, m: Ptr) -> String {
    ctx.type_name((ctx.api.il2cpp_method_get_return_type)(m))
}

// ---- main-thread dispatch: a WH_GETMESSAGE hook on Unity's window thread runs queued jobs ----

type Job = Box<dyn FnOnce() + Send>;

fn queue() -> &'static Mutex<VecDeque<Job>> {
    static Q: OnceLock<Mutex<VecDeque<Job>>> = OnceLock::new();
    Q.get_or_init(|| Mutex::new(VecDeque::new()))
}

unsafe extern "system" fn hook(code: i32, w: WPARAM, l: LPARAM) -> LRESULT {
    loop {
        let job = queue().lock().unwrap().pop_front();
        match job {
            Some(j) => j(),
            None => break,
        }
    }
    CallNextHookEx(std::ptr::null_mut(), code, w, l)
}

unsafe extern "system" fn find_window(hwnd: HWND, out: LPARAM) -> i32 {
    let mut pid = 0u32;
    let tid = GetWindowThreadProcessId(hwnd, &mut pid);
    if pid == GetCurrentProcessId() {
        *(out as *mut u32) = tid;
        return 0;
    }
    1
}

/// The thread of the game's window (Unity's main thread), waiting until the window exists.
fn main_thread() -> u32 {
    loop {
        let mut tid = 0u32;
        unsafe { EnumWindows(Some(find_window), &mut tid as *mut u32 as LPARAM) };
        if tid != 0 {
            return tid;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

fn on_main<T: Send + 'static>(tid: u32, f: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = mpsc::channel();
    queue().lock().unwrap().push_back(Box::new(move || {
        let _ = tx.send(f());
    }));
    loop {
        unsafe { PostThreadMessageW(tid, WM_NULL, 0, 0) };
        if let Ok(v) = rx.recv_timeout(std::time::Duration::from_millis(50)) {
            return v;
        }
    }
}

pub fn serve(ctx: &'static Ctx) {
    let tid = main_thread();
    let hk = unsafe { SetWindowsHookExW(WH_GETMESSAGE, Some(hook), std::ptr::null_mut(), tid) };
    log(&format!("harness: main thread {tid}, hook {}", if hk.is_null() { "FAILED" } else { "set" }));
    let port: u16 = std::env::var("TS_HARNESS_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(47615);
    let listener = match TcpListener::bind(("127.0.0.1", port)) {
        Ok(l) => l,
        Err(e) => {
            log(&format!("harness: cannot listen on {port}: {e}"));
            return;
        }
    };
    log(&format!("harness: listening on 127.0.0.1:{port}"));
    log("harness: ready");
    for conn in listener.incoming().flatten() {
        let mut w = conn.try_clone().unwrap();
        for line in BufReader::new(conn).lines() {
            let Ok(line) = line else { break };
            let req: Value = match serde_json::from_str(&line) {
                Ok(v) => v,
                Err(e) => {
                    let _ = writeln!(w, "{}", json!({"ok": false, "error": e.to_string()}));
                    continue;
                }
            };
            let mut reply = match req["cmd"].as_str() {
                Some("ping") => json!({"ok": true}),
                Some("quit") => {
                    let _ = writeln!(w, "{}", json!({"ok": true}));
                    std::process::exit(0);
                }
                _ if req["main"].as_bool() == Some(true) => {
                    let r = req.clone();
                    on_main(tid, move || unsafe { ctx.call(&r) })
                }
                _ => unsafe { ctx.call(&req) },
            };
            reply["id"] = req["id"].clone();
            if writeln!(w, "{reply}").is_err() {
                break;
            }
        }
    }
}
