//! The building pieces: the `ModuleLibrary` asset (row E5d), as plain data for
//! `ts_game::placemaker::modules::ModuleLibrary`. Every list is read whole, in stored order.

use crate::layout::{self, Value};
use crate::objects::{class_id, AssetSet, MonoBehaviourHeader, PPtr};

/// `Placemaker.Modules.OrientedModule` as stored: module index and flags.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct OrientedModuleData {
    pub module_index: u16,
    pub flags: u8,
}

/// `Placemaker.PropPlacement` as stored.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct PropPlacementData {
    pub pos: [i8; 3],
    pub scale: [u8; 3],
    pub rotation_byte: [u8; 3],
    /// A `Placemaker.Prop` component.
    pub prop: PPtr,
    pub corner: i8,
    pub prio_add: u8,
}

/// `Placemaker.BaseModule` / `Module` / `DecorModule` as stored (`decor_modules` and `corner_links` are
/// `Module`'s; a `DecorModule` has neither).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct ModuleData {
    pub cost: i16,
    pub profile_indexes: Vec<u16>,
    pub oriented_module_meshes: Vec<OrientedModuleData>,
    pub prop_placements: Vec<PropPlacementData>,
    pub decor_modules: Vec<OrientedModuleData>,
    pub corner_links: i32,
}

/// `Placemaker.ModuleMesh` as stored: positions, normals and tangents in sbytes, concavity and uvs in bytes.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct ModuleMeshData {
    pub material: PPtr,
    pub verts: Vec<[i8; 3]>,
    pub normals: Vec<[i8; 3]>,
    pub tangents: Vec<[i8; 3]>,
    pub concavity: Vec<u8>,
    pub uvs: Vec<[u8; 2]>,
    pub outline_uvs: Vec<u8>,
    pub tris: Vec<u8>,
    pub material_type: u8,
    pub corner: u8,
}

/// `Placemaker.Modules.CornerLookup` as stored.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct CornerLookupData {
    pub values: [u8; 8],
    pub oriented_modules: Vec<OrientedModuleData>,
}

/// The `ModuleLibrary` fields as stored (`cornerDict` is not serialized: `Setup` builds it).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct ModuleLibraryData {
    pub water_sides: Vec<u32>,
    pub modules: Vec<ModuleData>,
    pub decor_modules: Vec<ModuleData>,
    pub corner_lookups: Vec<CornerLookupData>,
    /// `SideProfile`: `v00, v01, v10, v11, v20, v21, v30, v31, v4, v5`.
    pub side_lookups: Vec<[u16; 10]>,
    pub module_meshes: Vec<ModuleMeshData>,
    pub decor_overlaps: Vec<i32>,
    pub ground_uv0: [u8; 2],
    pub ground_uv1: [u8; 2],
}

fn int(v: Option<&Value>) -> Option<i64> {
    match v? {
        Value::Int(i) => Some(*i),
        Value::UInt(u) => Some(*u as i64),
        _ => None,
    }
}

fn bytes(v: Option<&Value>) -> Option<Vec<u8>> {
    match v? {
        Value::Bytes(b) => Some(b.clone()),
        Value::Array(a) if a.is_empty() => Some(Vec::new()),
        _ => None,
    }
}

fn pptr(v: Option<&Value>) -> Option<PPtr> {
    match v? {
        Value::PPtr(p) => Some(*p),
        _ => None,
    }
}

fn list<T>(v: Option<&Value>, f: impl Fn(&Value) -> Option<T>) -> Option<Vec<T>> {
    match v? {
        Value::Array(a) => a.iter().map(f).collect(),
        Value::Bytes(b) if b.is_empty() => Some(Vec::new()),
        _ => None,
    }
}

fn ints<const N: usize>(v: &Value, names: [&str; N]) -> Option<[i64; N]> {
    let mut out = [0; N];
    for (o, n) in out.iter_mut().zip(names) {
        *o = int(v.get(n))?;
    }
    Some(out)
}

fn sbyte3(v: &Value) -> Option<[i8; 3]> {
    Some(ints(v, ["x", "y", "z"])?.map(|x| x as i8))
}

fn byte3(v: &Value) -> Option<[u8; 3]> {
    Some(ints(v, ["x", "y", "z"])?.map(|x| x as u8))
}

fn byte2(v: &Value) -> Option<[u8; 2]> {
    Some(ints(v, ["x", "y"])?.map(|x| x as u8))
}

fn oriented(v: &Value) -> Option<OrientedModuleData> {
    Some(OrientedModuleData { module_index: int(v.get("moduleIndex"))? as u16, flags: int(v.get("flags"))? as u8 })
}

fn prop_placement(v: &Value) -> Option<PropPlacementData> {
    Some(PropPlacementData {
        pos: sbyte3(v.get("pos")?)?,
        scale: byte3(v.get("scale")?)?,
        rotation_byte: byte3(v.get("rotationByte")?)?,
        prop: pptr(v.get("prop"))?,
        corner: int(v.get("corner"))? as i8,
        prio_add: int(v.get("prioAdd"))? as u8,
    })
}

fn module(v: &Value) -> Option<ModuleData> {
    Some(ModuleData {
        cost: int(v.get("cost"))? as i16,
        profile_indexes: list(v.get("profileIndexes"), |x| int(Some(x)).map(|i| i as u16))?,
        oriented_module_meshes: list(v.get("orientedModuleMeshes"), oriented)?,
        prop_placements: list(v.get("propPlacements"), prop_placement)?,
        decor_modules: match v.get("decorModules") {
            Some(d) => list(Some(d), oriented)?,
            None => Vec::new(),
        },
        corner_links: int(v.get("cornerLinks")).unwrap_or(0) as i32,
    })
}

fn module_mesh(v: &Value) -> Option<ModuleMeshData> {
    Some(ModuleMeshData {
        material: pptr(v.get("material"))?,
        verts: list(v.get("verts"), sbyte3)?,
        normals: list(v.get("normals"), sbyte3)?,
        tangents: list(v.get("tangents"), sbyte3)?,
        concavity: list(v.get("concavity"), |x| int(x.get("value")).map(|i| i as u8))?,
        uvs: list(v.get("uvs"), byte2)?,
        outline_uvs: list(v.get("outlineUvs"), |x| int(x.get("b")).map(|i| i as u8))?,
        tris: bytes(v.get("tris"))?,
        material_type: int(v.get("materialType"))? as u8,
        corner: int(v.get("corner"))? as u8,
    })
}

fn corner_lookup(v: &Value) -> Option<CornerLookupData> {
    let q = v.get("values")?;
    Some(CornerLookupData {
        values: ints(q, ["v0", "v1", "v2", "v3", "v4", "v5", "v6", "v7"])?.map(|x| x as u8),
        oriented_modules: list(v.get("orientedModules"), oriented)?,
    })
}

fn side_profile(v: &Value) -> Option<[u16; 10]> {
    Some(ints(v, ["v00", "v01", "v10", "v11", "v20", "v21", "v30", "v31", "v4", "v5"])?.map(|x| x as u16))
}

/// The build's one `ModuleLibrary` (`sharedassets1.assets`), or `None` if it is missing or unreadable.
pub fn module_library(assets: &AssetSet) -> Option<ModuleLibraryData> {
    for f in &assets.files {
        for o in &f.meta.objects {
            if f.class_id(o) != class_id::MONO_BEHAVIOUR {
                continue;
            }
            let b = f.bytes_of(o);
            let mb = MonoBehaviourHeader::read(b).ok()?;
            let Some(s) = assets.script_of(f, &mb) else { continue };
            if s.full_name() != "Placemaker.Modules.ModuleLibrary" {
                continue;
            }
            let v = layout::read_mono_behaviour(b, &mb, &layout::script_key(&s.assembly_name, &s.namespace, &s.class_name)).ok()?;
            return Some(ModuleLibraryData {
                water_sides: list(v.get("waterSides"), |x| int(Some(x)).map(|i| i as u32))?,
                modules: list(v.get("modules"), module)?,
                decor_modules: list(v.get("decorModules"), module)?,
                corner_lookups: list(v.get("cornerLookups"), corner_lookup)?,
                side_lookups: list(v.get("sideLookups"), side_profile)?,
                module_meshes: list(v.get("moduleMeshes"), module_mesh)?,
                decor_overlaps: list(v.get("decorOverlaps"), |x| int(Some(x)).map(|i| i as i32))?,
                ground_uv0: byte2(v.get("groundUv0")?)?,
                ground_uv1: byte2(v.get("groundUv1")?)?,
            });
        }
    }
    None
}
