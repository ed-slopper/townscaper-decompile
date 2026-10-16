//! Client of the checking harness (row F9c): tests call the original game's methods and compare its results
//! with our port's, exactly.
//!
//! Comparisons run only with `TS_HARNESS=1` (they start a copy of the game: see `townscaper-rs/harness`);
//! without it [`harness`] returns `None` and the test skips. Build the harness first:
//! `cargo +stable-x86_64-pc-windows-msvc build --release --target i686-pc-windows-msvc --manifest-path harness/Cargo.toml`.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

pub const PORT: u16 = 47615;

pub struct Harness {
    reader: BufReader<TcpStream>,
    writer: TcpStream,
    next_id: u64,
}

/// A failed call: the harness's error text (managed exceptions included).
#[derive(Debug, Clone, PartialEq)]
pub struct CallError(pub String);

fn launcher() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../harness/target/i686-pc-windows-msvc/release/ts_harness_launch.exe")
}

fn connect() -> Option<TcpStream> {
    TcpStream::connect(("127.0.0.1", PORT)).ok()
}

impl Harness {
    fn start() -> Result<Harness, String> {
        let stream = match connect() {
            Some(s) => s,
            None => {
                let exe = launcher();
                if !exe.exists() {
                    return Err(format!("harness not built: {}", exe.display()));
                }
                let st = std::process::Command::new(&exe).arg("--keep").status().map_err(|e| e.to_string())?;
                if !st.success() {
                    return Err("harness launcher failed (see work/harness/harness.log)".into());
                }
                let t = Instant::now();
                loop {
                    if let Some(s) = connect() {
                        break s;
                    }
                    if t.elapsed() > Duration::from_secs(30) {
                        return Err("harness did not listen".into());
                    }
                    std::thread::sleep(Duration::from_millis(200));
                }
            }
        };
        stream.set_read_timeout(Some(Duration::from_secs(60))).ok();
        Ok(Harness { reader: BufReader::new(stream.try_clone().unwrap()), writer: stream, next_id: 1 })
    }

    /// Send one request (see `harness/src/server.rs`) and read its reply.
    pub fn request(&mut self, mut req: Value) -> Value {
        req["id"] = json!(self.next_id);
        self.next_id += 1;
        writeln!(self.writer, "{req}").expect("harness write");
        let mut line = String::new();
        self.reader.read_line(&mut line).expect("harness read");
        serde_json::from_str(&line).expect("harness reply")
    }

    /// An instance method of a value type, on the value `this` (its bytes) of class `this_class`; the reply has
    /// `this_after` (the value after the call) and `args_after` (raw arguments after the call: `out` / `ref`).
    pub fn call_on_value(
        &mut self,
        method: &str,
        this_class: &str,
        this: &[u8],
        args: Vec<Value>,
        param_types: &[&str],
        main: bool,
    ) -> Result<Value, CallError> {
        let hex: String = this.iter().map(|x| format!("{x:02x}")).collect();
        let mut req = json!({"call": method, "args": args, "main": main, "this_raw": {"class": this_class, "raw": hex}});
        if !param_types.is_empty() {
            req["param_types"] = json!(param_types);
        }
        let r = self.request(req);
        if r["ok"] == json!(true) {
            Ok(r)
        } else {
            Err(CallError(r["error"].as_str().unwrap_or("?").to_string()))
        }
    }

    /// Call `Namespace.Class.Method` with arguments built by [`arg`]; returns the reply or the error.
    pub fn call(&mut self, method: &str, args: Vec<Value>, param_types: &[&str], main: bool) -> Result<Value, CallError> {
        let mut req = json!({"call": method, "args": args, "main": main});
        if !param_types.is_empty() {
            req["param_types"] = json!(param_types);
        }
        let r = self.request(req);
        if r["ok"] == json!(true) {
            Ok(r)
        } else {
            Err(CallError(r["error"].as_str().unwrap_or("?").to_string()))
        }
    }
}

/// The shared harness of this test binary, started on first use; `None` (skip) unless `TS_HARNESS=1`.
pub fn harness() -> Option<MutexGuard<'static, Harness>> {
    static H: OnceLock<Option<Mutex<Harness>>> = OnceLock::new();
    let h = H.get_or_init(|| {
        if std::env::var_os("TS_HARNESS").is_none() {
            eprintln!("skipped: set TS_HARNESS=1 to compare with the original game");
            return None;
        }
        Some(Mutex::new(Harness::start().unwrap_or_else(|e| panic!("harness: {e}"))))
    });
    h.as_ref().map(|m| m.lock().unwrap_or_else(|p| p.into_inner()))
}

/// Argument builders: value types as their little-endian bytes.
pub mod arg {
    use serde_json::{json, Value};

    fn hex(b: &[u8]) -> String {
        b.iter().map(|x| format!("{x:02x}")).collect()
    }
    pub fn raw(b: &[u8]) -> Value {
        json!({"raw": hex(b)})
    }
    pub fn f32(v: f32) -> Value {
        raw(&v.to_le_bytes())
    }
    pub fn i32(v: i32) -> Value {
        raw(&v.to_le_bytes())
    }
    pub fn bool(v: bool) -> Value {
        raw(&[v as u8])
    }
    pub fn string(s: &str) -> Value {
        json!({"string": s})
    }
    /// A new array of a value type, e.g. `array("System.Single", &bytes)`.
    pub fn array(elem: &str, bytes: &[u8]) -> Value {
        json!({"array": elem, "raw": hex(bytes)})
    }
}

/// Reply decoders.
pub mod ret {
    use serde_json::Value;

    pub fn bytes(r: &Value) -> Vec<u8> {
        let s = r["raw"].as_str().expect("raw value in reply");
        (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
    }
    pub fn i32(r: &Value) -> i32 {
        i32::from_le_bytes(bytes(r)[..4].try_into().unwrap())
    }
    pub fn f32(r: &Value) -> f32 {
        f32::from_le_bytes(bytes(r)[..4].try_into().unwrap())
    }
    /// A hex field of the reply (`this_after`, or `args_after[i]`) as bytes.
    pub fn hex_field(v: &Value) -> Vec<u8> {
        let s = v.as_str().expect("hex field");
        (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
    }
    pub fn string(r: &Value) -> String {
        r["string"].as_str().expect("string reply").to_string()
    }
}
