//! E5d: the `ModuleLibrary` asset reads, and its indexes point inside its own lists.

use ts_data::install;
use ts_data::modules::module_library;
use ts_data::objects::AssetSet;

#[test]
fn module_library_reads() {
    let Some(dir) = install::find_dir() else { return };
    let a = AssetSet::load(&dir.join("Townscaper_Data")).unwrap();
    let lib = module_library(&a).expect("ModuleLibrary");
    assert_eq!(
        (lib.modules.len(), lib.decor_modules.len(), lib.module_meshes.len(), lib.corner_lookups.len(), lib.side_lookups.len()),
        (2072, 128, 892, 4769, 14519)
    );
    let meshes = lib.module_meshes.len();
    let sides = lib.side_lookups.len();
    for (k, m) in lib.modules.iter().chain(&lib.decor_modules).enumerate() {
        assert!(m.oriented_module_meshes.iter().all(|o| (o.module_index as usize) < meshes), "module {k}: mesh index");
        assert!(m.profile_indexes.iter().all(|&p| (p as usize) < sides), "module {k}: profile index");
    }
    for m in &lib.modules {
        assert!(m.decor_modules.iter().all(|o| (o.module_index as usize) < lib.decor_modules.len()), "decor index");
    }
    for c in &lib.corner_lookups {
        assert!(c.oriented_modules.iter().all(|o| (o.module_index as usize) < lib.modules.len()), "corner lookup index");
    }
    for (k, m) in lib.module_meshes.iter().enumerate() {
        let n = m.verts.len();
        assert!(n <= 256 && m.tris.len() % 3 == 0, "mesh {k}: {n} verts, {} tris", m.tris.len());
        assert!(m.tris.iter().all(|&t| (t as usize) < n), "mesh {k}: tri past verts");
        for len in [m.normals.len(), m.tangents.len(), m.uvs.len()] {
            assert!(len == n || len == 0, "mesh {k}: {len} per-vertex values for {n} verts");
        }
        assert!(m.material_type <= 3, "mesh {k}: material type {}", m.material_type);
    }
    // `GetDecorOverlap` binary-searches it
    assert!(lib.decor_overlaps.windows(2).all(|w| w[0] <= w[1]), "decorOverlaps sorted");
}
