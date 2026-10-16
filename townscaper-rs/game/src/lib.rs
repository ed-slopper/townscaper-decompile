//! `ts_game`: Townscaper's own game code, ported 1:1 from `GameAssembly.dll` (AGENTS.md section 2). No Bevy.
//!
//! The module tree follows the original's namespaces: `Os.*` → [`os`], `Placemaker.*` → [`placemaker`].
//! Every ported function names its original in its doc comment: ``/// `Os.Hex.HexMath.Foo` 0x123456``.
//! [`unity`] holds the Unity / .NET behaviour the game's results depend on (board row F11).

pub mod os;
pub mod placemaker;
pub mod unity;
