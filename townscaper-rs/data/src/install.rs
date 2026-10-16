//! The player's own Townscaper install: where it is and whether it is the reference build (AGENTS.md section 2).

use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use md5::{Digest, Md5};

use crate::serialized;

/// The reference build every address in this repo belongs to (GOG Windows, AGENTS.md section 2).
pub mod reference {
    pub const UNITY_VERSION: &str = "2020.3.18f1";
    pub const METADATA_VERSION: i32 = 27;
    pub const GAME_ASSEMBLY_MD5: &str = "4470ec364697ebe141925da367a2c147";
    pub const METADATA_MD5: &str = "23b32bc8b632646f179b3e9d0c17c23d";
}

const IL2CPP_METADATA_MAGIC: u32 = 0xFAB1_1BAF;

/// Where the game is: `TOWNSCAPER_DIR`, else the GOG default.
pub fn find_dir() -> Option<PathBuf> {
    let candidates = std::env::var_os("TOWNSCAPER_DIR")
        .map(PathBuf::from)
        .into_iter()
        .chain([PathBuf::from("C:/GOG Games/Townscaper")]);
    candidates.into_iter().find(|d| d.join("Townscaper_Data").is_dir())
}

#[derive(Debug, Clone)]
pub struct Install {
    pub dir: PathBuf,
    pub unity_version: String,
    pub target_platform: u32,
    pub metadata_version: i32,
    pub game_assembly_md5: String,
    pub metadata_md5: String,
}

impl Install {
    pub fn data_dir(&self) -> PathBuf {
        self.dir.join("Townscaper_Data")
    }

    /// True when this is the build the repo's addresses belong to.
    pub fn is_reference(&self) -> bool {
        self.unity_version == reference::UNITY_VERSION
            && self.metadata_version == reference::METADATA_VERSION
            && self.game_assembly_md5 == reference::GAME_ASSEMBLY_MD5
            && self.metadata_md5 == reference::METADATA_MD5
    }

    /// What differs from the reference build, one line each (empty when it is the reference).
    pub fn differences(&self) -> Vec<String> {
        let mut d = Vec::new();
        let mut check = |what: &str, ours: &str, want: &str| {
            if ours != want {
                d.push(format!("{what}: {ours}, reference {want}"));
            }
        };
        check("unity", &self.unity_version, reference::UNITY_VERSION);
        check("metadata", &self.metadata_version.to_string(), &reference::METADATA_VERSION.to_string());
        check("GameAssembly.dll md5", &self.game_assembly_md5, reference::GAME_ASSEMBLY_MD5);
        check("global-metadata.dat md5", &self.metadata_md5, reference::METADATA_MD5);
        d
    }
}

fn md5_file(path: &Path) -> io::Result<String> {
    let mut f = fs::File::open(path)?;
    let mut h = Md5::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

fn invalid(msg: String) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg)
}

/// Read the identity of the install at `dir` (hashes ~25 MB, a fraction of a second).
pub fn identify(dir: &Path) -> io::Result<Install> {
    let data = dir.join("Townscaper_Data");

    let mut head = vec![0u8; 512];
    let n = fs::File::open(data.join("globalgamemanagers"))?.read(&mut head)?;
    let header = serialized::Header::parse(&head[..n]).map_err(|e| invalid(format!("globalgamemanagers: {e}")))?;

    let meta_path = data.join("il2cpp_data").join("Metadata").join("global-metadata.dat");
    let mut m = [0u8; 8];
    fs::File::open(&meta_path)?.read_exact(&mut m)?;
    let magic = u32::from_le_bytes(m[0..4].try_into().unwrap());
    if magic != IL2CPP_METADATA_MAGIC {
        return Err(invalid(format!("global-metadata.dat: bad magic {magic:#x}")));
    }

    Ok(Install {
        dir: dir.to_path_buf(),
        unity_version: header.unity_version,
        target_platform: header.target_platform,
        metadata_version: i32::from_le_bytes(m[4..8].try_into().unwrap()),
        game_assembly_md5: md5_file(&dir.join("GameAssembly.dll"))?,
        metadata_md5: md5_file(&meta_path)?,
    })
}
