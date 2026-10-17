//! Our `ModuleLibrary`, read from the player's install with `ts_data::modules` (shared by the module tests).
#![allow(dead_code)]

use ts_game::os::utils::{ByteFloat, ByteFloat2, ByteFloat3, SbyteFloat3};
use ts_game::placemaker::graphs::MaterialType;
use ts_game::placemaker::modules::{
    AssetRef, BaseModule, ByteQube, CornerLookup, DecorModule, Module, ModuleLibrary, ModuleMesh, OrientedModule, OutlineUv, PropPlacement,
    SideProfile,
};

fn om(o: &ts_data::modules::OrientedModuleData) -> OrientedModule {
    OrientedModule { module_index: o.module_index, flags: o.flags, pad: 0 }
}

fn asset(p: ts_data::objects::PPtr) -> AssetRef {
    AssetRef { file_id: p.file_id, path_id: p.path_id }
}

fn s3(v: [i8; 3]) -> SbyteFloat3 {
    SbyteFloat3 { x: v[0], y: v[1], z: v[2] }
}

fn base(m: &ts_data::modules::ModuleData) -> BaseModule {
    BaseModule {
        cost: m.cost,
        profile_indexes: m.profile_indexes.clone(),
        oriented_module_meshes: m.oriented_module_meshes.iter().map(om).collect(),
        prop_placements: m
            .prop_placements
            .iter()
            .map(|p| PropPlacement {
                pos: s3(p.pos),
                scale: ByteFloat3 { x: p.scale[0], y: p.scale[1], z: p.scale[2] },
                rotation_byte: ByteFloat3 { x: p.rotation_byte[0], y: p.rotation_byte[1], z: p.rotation_byte[2] },
                prop: asset(p.prop),
                corner: p.corner,
                prio_add: p.prio_add,
            })
            .collect(),
    }
}

/// The library as the asset stores it (no `setup` yet).
pub fn our_module_library() -> ModuleLibrary {
    let dir = ts_data::install::find_dir().expect("the player's install (TOWNSCAPER_DIR)");
    let a = ts_data::objects::AssetSet::load(&dir.join("Townscaper_Data")).unwrap();
    let d = ts_data::modules::module_library(&a).expect("ModuleLibrary");
    let material_type = |t: u8| match t {
        0 => MaterialType::None,
        1 => MaterialType::Ground,
        2 => MaterialType::Dirt,
        3 => MaterialType::Water,
        _ => panic!("material type {t}"),
    };
    ModuleLibrary {
        water_sides: d.water_sides,
        modules: d.modules.iter().map(|m| Module { base: base(m), decor_modules: m.decor_modules.iter().map(om).collect(), corner_links: m.corner_links }).collect(),
        decor_modules: d.decor_modules.iter().map(|m| DecorModule { base: base(m) }).collect(),
        corner_lookups: d.corner_lookups.iter().map(|c| CornerLookup { values: ByteQube { v: c.values }, oriented_modules: c.oriented_modules.iter().map(om).collect() }).collect(),
        side_lookups: d
            .side_lookups
            .iter()
            .map(|v| SideProfile { v00: v[0], v01: v[1], v10: v[2], v11: v[3], v20: v[4], v21: v[5], v30: v[6], v31: v[7], v4: v[8], v5: v[9] })
            .collect(),
        module_meshes: d
            .module_meshes
            .iter()
            .map(|m| ModuleMesh {
                material: asset(m.material),
                verts: m.verts.iter().copied().map(s3).collect(),
                normals: m.normals.iter().copied().map(s3).collect(),
                tangents: m.tangents.iter().copied().map(s3).collect(),
                concavity: m.concavity.iter().map(|&value| ByteFloat { value }).collect(),
                uvs: m.uvs.iter().map(|&[x, y]| ByteFloat2 { x, y }).collect(),
                outline_uvs: m.outline_uvs.iter().map(|&b| OutlineUv { b }).collect(),
                tris: m.tris.clone(),
                material_type: material_type(m.material_type),
                corner: m.corner,
            })
            .collect(),
        decor_overlaps: d.decor_overlaps,
        ground_uv0: ByteFloat2 { x: d.ground_uv0[0], y: d.ground_uv0[1] },
        ground_uv1: ByteFloat2 { x: d.ground_uv1[0], y: d.ground_uv1[1] },
        corner_dict: Default::default(),
    }
}
