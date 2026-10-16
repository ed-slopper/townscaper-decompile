# ts_harness

The checking harness (board rows F9a–F9c): runs code **inside the original game** so our port can be compared
with it. 32-bit like the game, built on its own:

```bash
cargo +stable-x86_64-pc-windows-msvc build --release --target i686-pc-windows-msvc --manifest-path townscaper-rs/harness/Cargo.toml
townscaper-rs/harness/target/i686-pc-windows-msvc/release/ts_harness_launch.exe
```

- `ts_harness_launch` copies the player's game to `work/harness/game/` once (the install is never changed),
  starts the copy windowed and loads `ts_harness.dll` into it.
- `ts_harness.dll` waits until IL2CPP is up, attaches its thread and uses the C API `GameAssembly.dll`
  exports (`il2cpp_domain_get`, `il2cpp_class_from_name`, `il2cpp_runtime_invoke`, ...). Log:
  `work/harness/harness.log`.

- With `--keep` the game stays up and the DLL serves calls on `127.0.0.1:47615` (JSON lines; protocol in
  `src/server.rs`): `{"call":"UnityEngine.Mathf.RoundToInt","args":[{"raw":"00002040"}],"main":true}`.
  `{"cmd":"quit"}` closes the game.

No C# mod loader is used (no .NET SDK needed). Everything it reads stays on the player's PC.
