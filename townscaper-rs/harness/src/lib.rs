//! `ts_harness.dll`: loaded into a copy of the original game by `ts_harness_launch` (row F9).
//! A thread waits for IL2CPP, attaches to its domain and reports what it finds to `TS_HARNESS_LOG`.

pub mod il2cpp;
pub mod server;

use std::fs::OpenOptions;
use std::io::Write;
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::HINSTANCE;
use windows_sys::Win32::System::SystemServices::DLL_PROCESS_ATTACH;

/// Append a line to the log the launcher named (`TS_HARNESS_LOG`).
pub fn log(line: &str) {
    if let Some(p) = std::env::var_os("TS_HARNESS_LOG") {
        if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(p) {
            let _ = writeln!(f, "{line}");
        }
    }
}

fn probe() {
    log("harness: loaded");
    let start = Instant::now();
    // GameAssembly.dll is loaded by UnityPlayer after start-up; then the domain fills with assemblies.
    let (api, domain) = loop {
        if start.elapsed() > Duration::from_secs(120) {
            log("harness: timeout waiting for il2cpp");
            return;
        }
        if let Some(api) = il2cpp::Api::load() {
            let d = unsafe { (api.il2cpp_domain_get)() };
            if !d.is_null() {
                let mut n = 0usize;
                unsafe { (api.il2cpp_domain_get_assemblies)(d, &mut n) };
                if n > 50 {
                    break (api, d);
                }
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    unsafe {
        (api.il2cpp_thread_attach)(domain);
        let images = api.images(domain);
        log(&format!("harness: il2cpp up after {:.1} s, {} assemblies", start.elapsed().as_secs_f32(), images.len()));
        let Some(&(_, game)) = images.iter().find(|(n, _)| n == "Assembly-CSharp.dll") else {
            log("harness: no Assembly-CSharp.dll");
            return;
        };
        let class = api.class(game, "Placemaker", "TextSaveSystem");
        if class.is_null() {
            log("harness: Placemaker.TextSaveSystem not found");
            return;
        }
        let methods = api.methods(class);
        let names: Vec<String> = methods.iter().map(|(n, p, _)| format!("{n}/{p}")).collect();
        log(&format!("harness: Placemaker.TextSaveSystem: {} methods: {}", methods.len(), names.join(", ")));
        let ctx: &'static server::Ctx = Box::leak(Box::new(server::Ctx { api, domain }));
        server::serve(ctx);
    }
}

#[no_mangle]
#[allow(non_snake_case)]
pub extern "system" fn DllMain(_module: HINSTANCE, reason: u32, _reserved: *mut core::ffi::c_void) -> i32 {
    if reason == DLL_PROCESS_ATTACH {
        // Don't work under the loader lock: start a thread and return.
        std::thread::spawn(probe);
    }
    1
}
