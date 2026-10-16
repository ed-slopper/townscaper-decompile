//! Needs the player's install: set `TOWNSCAPER_DIR` (or have it at the GOG default). Skips otherwise.

use ts_data::install;

#[test]
fn install_is_the_reference_build() {
    let Some(dir) = install::find_dir() else {
        eprintln!("skipped: no Townscaper install (set TOWNSCAPER_DIR)");
        return;
    };
    let i = install::identify(&dir).expect("identify");
    assert_eq!(i.unity_version, install::reference::UNITY_VERSION);
    assert_eq!(i.target_platform, 5, "StandaloneWindows (x86)");
    assert!(i.is_reference(), "not the reference build: {:?}", i.differences());
}

#[test]
fn every_serialized_file_has_a_2020_3_header() {
    let Some(dir) = install::find_dir() else { return };
    let data = dir.join("Townscaper_Data");
    let mut n = 0;
    for e in std::fs::read_dir(&data).unwrap() {
        let p = e.unwrap().path();
        let name = p.file_name().unwrap().to_string_lossy().into_owned();
        let is_serialized = name == "globalgamemanagers"
            || name.ends_with(".assets")
            || (name.starts_with("level") && !name.contains('.'));
        if !is_serialized {
            continue;
        }
        let bytes = std::fs::read(&p).unwrap();
        let h = ts_data::serialized::Header::parse(&bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(h.unity_version, "2020.3.18f1", "{name}");
        assert_eq!(h.file_size, bytes.len() as u64, "{name}: header file size");
        assert!(h.data_offset < h.file_size, "{name}");
        n += 1;
    }
    assert!(n >= 8, "found only {n} serialized files");
}
