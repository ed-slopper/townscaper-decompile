# townscaper-rs

Townscaper rewritten 1:1 in Rust on Bevy. Rules: [`../AGENTS.md`](../AGENTS.md); board: [`../WORK.md`](../WORK.md).

| Crate | Path | What |
|---|---|---|
| `townscaper` | `.` | The app (Bevy): window, input, drawing, audio, the frame loop |
| `ts_game` | `game/` | The game's code ported from `GameAssembly.dll` (`os`, `placemaker`) + Unity semantics (`unity`); no Bevy |
| `ts_data` | `data/` | Readers of the player's own files (install check, Unity `SerializedFile`); no Bevy |

```bash
cargo +stable-x86_64-pc-windows-msvc run --release        # the app
cargo +stable-x86_64-pc-windows-msvc test -p ts_game -p ts_data
```

The game folder comes from `TOWNSCAPER_DIR`, else `C:/GOG Games/Townscaper`. Tests that need it skip without it.
First build of the app takes ~12 minutes (Bevy 0.20).
