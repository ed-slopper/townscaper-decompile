//! The call server (row F9b): JSON lines on 127.0.0.1 (`TS_HARNESS_PORT`, default 47615).
//!
//! Request:  `{"id":1, "call":"UnityEngine.Mathf.RoundToInt", "args":[{"raw":"0000203f"}],
//!             "param_types":["System.Single"], "main":true, "on":"Placemaker.WorldMaster"}`
//!   - `call`: `Namespace.Class.Method` (the class is looked up in every loaded image);
//!   - `args`: `{"raw": hex}` value-type bytes (little-endian, as in memory), `{"string": s}`,
//!     `{"array": "System.Single", "raw": hex}` a new array of a value type, `{"null": true}`;
//!   - `param_types` (optional): picks the overload by il2cpp's parameter type names;
//!   - `main` (optional): run on Unity's main thread (needed by most UnityEngine APIs);
//!   - `on` (optional): call on the first object of that type in the scene (`FindObjectOfType`, inactive too);
//!   - `this_raw` (optional): `{"class": "UnityEngine.Vector3", "raw": hex}`: an instance method of a value type
//!     on that value (boxed); the value after the call comes back as `this_after`.
//! Raw arguments are passed by pointer, so `ref` / `out` parameters work: every raw argument's bytes after the
//! call come back in `args_after` (hex, in argument order; `null` for non-raw arguments).
//! Reply: `{"id":1, "ok":true, "type":"System.Int32", "raw":"02000000"}`, or `"string"`, or
//!   `{"array": elem type, "raw": hex, "n": len}`, `{"object": class}`, `{"null": true}`, `{"void": true}`;
//!   errors: `{"ok":false, "error": text}` (managed exceptions formatted by il2cpp).
//! `{"cmd":"ping"}` → `{"ok":true}`; `{"cmd":"quit"}` ends the game process.

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
        if r.is_null() {
            return Err(format!("no {type_name} in the scene"));
        }
        Ok(r)
    }

    /// Run one call request (on whatever thread this is).
    pub unsafe fn call(&self, req: &Value) -> Value {
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
            None => call.rsplit_once('.').ok_or("call is Namespace.Class.Method")?,
        };
        let class = self.find_class(class_name)?;
        let args = req["args"].as_array().cloned().unwrap_or_default();
        let m = self.find_method(class, method, args.len(), req["param_types"].as_array())?;
        let mut this_size = 0usize;
        let this = match (req["on"].as_str(), req.get("this_raw")) {
            (Some(t), _) => self.find_object(t)?,
            (None, Some(v)) if v.is_object() => {
                let k = self.find_class(v["class"].as_str().ok_or("this_raw.class")?)?;
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
        for a in &args {
            if let Some(r) = a.get("array").and_then(Value::as_str) {
                let elem = self.find_class(r)?;
                let raw = unhex(a["raw"].as_str().unwrap_or(""))?;
                let size = (self.api.il2cpp_class_value_size)(elem, std::ptr::null_mut()) as usize;
                let n = if size == 0 { 0 } else { raw.len() / size };
                let arr = (self.api.il2cpp_array_new)(elem, n);
                std::ptr::copy_nonoverlapping(raw.as_ptr(), (arr as *mut u8).add(ARRAY_DATA), n * size);
                ptrs.push(arr);
            } else if let Some(r) = a.get("raw").and_then(Value::as_str) {
                store.push(unhex(r)?);
                ptrs.push(store.last_mut().unwrap().as_mut_ptr() as Ptr);
            } else if let Some(s) = a.get("string").and_then(Value::as_str) {
                ptrs.push((self.api.il2cpp_string_new)(c(s).as_ptr()));
            } else if a.get("null").is_some() {
                ptrs.push(std::ptr::null_mut());
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
            .map(|a| {
                if a.get("raw").is_some() && a.get("array").is_none() {
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
