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

/// F8a: every SerializedFile's metadata reads to exactly its stated end, and every object lies in the data.
#[test]
fn every_serialized_file_metadata_reads_exactly() {
    use ts_data::serialized::SerializedFile;
    let Some(dir) = install::find_dir() else { return };
    let data = dir.join("Townscaper_Data");
    let mut total = 0;
    for e in std::fs::read_dir(&data).unwrap() {
        let p = e.unwrap().path();
        let name = p.file_name().unwrap().to_string_lossy().into_owned();
        if !(name == "globalgamemanagers" || name.ends_with(".assets") || (name.starts_with("level") && !name.contains('.'))) {
            continue;
        }
        let bytes = std::fs::read(&p).unwrap();
        let f = SerializedFile::parse(&bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(f.metadata_end, 0x30 + f.header.metadata_size as usize, "{name}: metadata end");
        // The table is in path-id order; in the data the objects must not overlap.
        let mut by_offset: Vec<_> = f.objects.iter().collect();
        by_offset.sort_by_key(|o| o.offset);
        let mut prev_end = f.header.data_offset;
        for o in by_offset {
            assert!((o.type_index as usize) < f.types.len(), "{name}: type index");
            assert!(o.offset >= prev_end, "{name}: objects overlap at {}", o.path_id);
            assert!(o.offset + o.size as u64 <= f.header.file_size, "{name}: object {} past the end", o.path_id);
            prev_end = o.offset + o.size as u64;
        }
        for s in &f.scripts {
            assert!(s.file_index >= 0 && s.file_index as usize <= f.externals.len(), "{name}: script file index");
        }
        total += f.objects.len();
        eprintln!("{name}: {} types, {} objects, {} scripts, {} externals", f.types.len(), f.objects.len(), f.scripts.len(), f.externals.len());
    }
    assert!(total > 1000, "only {total} objects");
}
