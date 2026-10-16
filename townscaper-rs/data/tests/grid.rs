//! E3a: the `CustomGridLibrary` asset reads, and its index lists point at patches.

use ts_data::grid::custom_grid_library;
use ts_data::install;
use ts_data::objects::AssetSet;

#[test]
fn custom_grid_library_reads() {
    let Some(dir) = install::find_dir() else { return };
    let a = AssetSet::load(&dir.join("Townscaper_Data")).unwrap();
    let lib = custom_grid_library(&a).expect("CustomGridLibrary");
    assert_eq!(lib.all_custom_patches.len(), 40);
    let n = lib.all_custom_patches.len();
    for list in [&lib.generic_patches, &lib.straight_patches, &lib.centroid_patches, &lib.centroid_curve_patches] {
        assert!(!list.is_empty());
        assert!(list.iter().all(|&i| i == 0xFF || (i as usize) < n), "{list:?}");
    }
    for (k, p) in lib.all_custom_patches.iter().enumerate() {
        // triangles (3 corners) up to `quadTriCutoff`, quads (4) after it
        let cut = p.quad_tri_cutoff as usize;
        assert!(cut % 3 == 0 && (p.quads.len() - cut) % 4 == 0, "patch {k}: {} corners, cutoff {cut}", p.quads.len());
        assert!(cut <= p.quads.len(), "patch {k}: cutoff {cut}");
        assert!(p.quads.iter().all(|&i| (i as usize) < p.verts.len()), "patch {k}: corner index past verts");
    }
}
