//! Start a copy of the original game with `ts_harness.dll` loaded (row F9a).
//!   ts_harness_launch [--game <install dir>] [--work <repo>/work] [--keep]
//! Copies the install to `<work>/harness/game` once (the player's install is never changed), starts it
//! windowed and suspended, loads the DLL with `LoadLibraryW` on a remote thread, resumes it and prints the
//! harness log until `harness: ready` (or 120 s). Without `--keep` the game is closed afterwards.

use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
use windows_sys::Win32::System::Diagnostics::Debug::WriteProcessMemory;
use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleA, GetProcAddress};
use windows_sys::Win32::System::Memory::{VirtualAllocEx, MEM_COMMIT, MEM_RESERVE, PAGE_READWRITE};
use windows_sys::Win32::System::Threading::{
    CreateProcessW, CreateRemoteThread, GetExitCodeThread, ResumeThread, TerminateProcess, WaitForSingleObject,
    CREATE_SUSPENDED, PROCESS_INFORMATION, STARTUPINFOW,
};

fn wide(s: &std::ffi::OsStr) -> Vec<u16> {
    s.encode_wide().chain([0]).collect()
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<u64> {
    std::fs::create_dir_all(to)?;
    let mut n = 0;
    for e in std::fs::read_dir(from)? {
        let e = e?;
        let t = to.join(e.file_name());
        if e.file_type()?.is_dir() {
            n += copy_dir(&e.path(), &t)?;
        } else {
            n += std::fs::copy(e.path(), &t)?;
        }
    }
    Ok(n)
}

fn arg(name: &str) -> Option<String> {
    let a: Vec<String> = std::env::args().collect();
    a.iter().position(|x| x == name).and_then(|i| a.get(i + 1).cloned())
}

fn main() {
    let game_src = PathBuf::from(arg("--game").or(std::env::var("TOWNSCAPER_DIR").ok()).unwrap_or("C:/GOG Games/Townscaper".into()));
    let exe_dir = std::env::current_exe().unwrap().parent().unwrap().to_path_buf();
    // target/i686-pc-windows-msvc/release → repo root is 5 levels up from the exe's folder
    let repo = exe_dir.ancestors().nth(5).unwrap().to_path_buf();
    let work = PathBuf::from(arg("--work").unwrap_or_else(|| repo.join("work").to_string_lossy().into_owned()));
    let game = work.join("harness").join("game");
    let dll = exe_dir.join("ts_harness.dll");
    let log = work.join("harness").join("harness.log");
    assert!(dll.exists(), "build ts_harness.dll first: {}", dll.display());

    if !game.join("Townscaper.exe").exists() {
        eprintln!("copying {} to {} (once)", game_src.display(), game.display());
        let n = copy_dir(&game_src, &game).expect("copy game");
        eprintln!("copied {} MB", n >> 20);
    }
    let _ = std::fs::remove_file(&log);
    std::env::set_var("TS_HARNESS_LOG", &log);

    unsafe {
        let exe = wide(game.join("Townscaper.exe").as_os_str());
        let mut cmd = wide(std::ffi::OsStr::new(
            "\"Townscaper.exe\" -screen-fullscreen 0 -screen-width 640 -screen-height 360 -logFile harness_player.log",
        ));
        let cwd = wide(game.as_os_str());
        let mut si: STARTUPINFOW = std::mem::zeroed();
        si.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
        let mut pi: PROCESS_INFORMATION = std::mem::zeroed();
        let ok = CreateProcessW(
            exe.as_ptr(),
            cmd.as_mut_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            0,
            CREATE_SUSPENDED,
            std::ptr::null(),
            cwd.as_ptr(),
            &si,
            &mut pi,
        );
        assert!(ok != 0, "CreateProcessW failed");

        // LoadLibraryW(dll path) on a thread in the game
        let path = wide(dll.as_os_str());
        let bytes = path.len() * 2;
        let remote = VirtualAllocEx(pi.hProcess, std::ptr::null(), bytes, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE);
        assert!(!remote.is_null(), "VirtualAllocEx failed");
        let ok = WriteProcessMemory(pi.hProcess, remote, path.as_ptr() as *const _, bytes, std::ptr::null_mut());
        assert!(ok != 0, "WriteProcessMemory failed");
        let k32 = GetModuleHandleA(c"kernel32.dll".as_ptr() as *const u8);
        let load = GetProcAddress(k32, c"LoadLibraryW".as_ptr() as *const u8).expect("LoadLibraryW");
        let th = CreateRemoteThread(pi.hProcess, std::ptr::null(), 0, Some(std::mem::transmute(load)), remote, 0, std::ptr::null_mut());
        assert!(!th.is_null(), "CreateRemoteThread failed");
        assert_eq!(WaitForSingleObject(th, 30_000), WAIT_OBJECT_0, "LoadLibraryW did not return");
        let mut module = 0u32;
        GetExitCodeThread(th, &mut module);
        assert!(module != 0, "LoadLibraryW failed in the game (module handle 0)");
        CloseHandle(th);
        ResumeThread(pi.hThread);
        eprintln!("game started (pid {}), harness loaded", pi.dwProcessId);

        let start = Instant::now();
        let mut printed = 0;
        let mut ready = false;
        while start.elapsed() < Duration::from_secs(120) && !ready {
            std::thread::sleep(Duration::from_millis(200));
            let text = std::fs::read_to_string(&log).unwrap_or_default();
            for l in text.lines().skip(printed) {
                println!("{l}");
                printed += 1;
                ready |= l == "harness: ready";
            }
        }
        if std::env::args().all(|a| a != "--keep") {
            TerminateProcess(pi.hProcess, 0);
            eprintln!("game closed");
        }
        CloseHandle(pi.hThread);
        CloseHandle(pi.hProcess);
        std::process::exit(if ready { 0 } else { 1 });
    }
}
