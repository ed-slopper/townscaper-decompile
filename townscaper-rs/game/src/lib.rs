//! `ts_game`: Townscaper's own game code, ported 1:1 from `GameAssembly.dll` (AGENTS.md section 2). No Bevy.
//!
//! The module tree follows the original's namespaces: `Os.*` → [`os`], `Placemaker.*` → [`placemaker`], the
//! global namespace's classes at the top ([`beatbox`]).
//! Every ported function names its original in its doc comment: ``/// `Os.Hex.HexMath.GetHex3` 0x3DC0C0``.
//! [`unity`] holds the Unity / .NET behaviour the game's results depend on (board row F11).

pub mod beatbox;
pub mod os;
pub mod placemaker;
pub mod unity;
