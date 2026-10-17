//! `ts_data`: readers of the player's own Townscaper files. No Bevy (AGENTS.md section 15).
//!
//! Nothing read here is ever written into the repo: the files are the player's.

pub mod audio;
pub mod install;
pub mod grid;
pub mod layout;
pub mod mesh;
pub mod modules;
pub mod objects;
pub mod props;
pub mod scene;
pub mod serialized;
pub mod texture;
pub mod vorbis;
