//! The grid generator's hand-made patches: the `CustomGridLibrary` asset (row E3a), as plain data for
//! `ts_game::placemaker::quads::grid_generation::CustomGridLibrary`.

use crate::layout::{self, Value};
use crate::objects::{class_id, AssetSet, MonoBehaviourHeader};

/// One `CustomGridPatch` as stored: corner positions (hex × `GridConstants.hexScale`) and corner indexes:
/// triangles (three each) up to `quad_tri_cutoff`, quads (four each) after it.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct CustomGridPatchData {
    pub verts: Vec<[i32; 2]>,
    pub quads: Vec<u8>,
    pub quad_tri_cutoff: u16,
    pub subdivisions: u8,
}

/// The `CustomGridLibrary` fields as stored.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct CustomGridLibraryData {
    pub all_custom_patches: Vec<CustomGridPatchData>,
    pub generic_patches: Vec<u8>,
    pub straight_patches: Vec<u8>,
    pub centroid_patches: Vec<u8>,
    pub centroid_curve_patches: Vec<u8>,
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

fn patch(v: &Value) -> Option<CustomGridPatchData> {
    let Value::Array(verts) = v.get("verts")? else { return None };
    let verts = verts.iter().map(|p| Some([int(p.get("x"))? as i32, int(p.get("y"))? as i32])).collect::<Option<Vec<_>>>()?;
    Some(CustomGridPatchData {
        verts,
        quads: bytes(v.get("quads"))?,
        quad_tri_cutoff: int(v.get("quadTriCutoff"))? as u16,
        subdivisions: int(v.get("subdivisions"))? as u8,
    })
}

/// The build's one `CustomGridLibrary` (`sharedassets1.assets`), or `None` if it is missing or unreadable.
pub fn custom_grid_library(assets: &AssetSet) -> Option<CustomGridLibraryData> {
    for f in &assets.files {
        for o in &f.meta.objects {
            if f.class_id(o) != class_id::MONO_BEHAVIOUR {
                continue;
            }
            let b = f.bytes_of(o);
            let mb = MonoBehaviourHeader::read(b).ok()?;
            let Some(s) = assets.script_of(f, &mb) else { continue };
            if s.full_name() != "Placemaker.Quads.GridGeneration.CustomGridLibrary" {
                continue;
            }
            let v = layout::read_mono_behaviour(b, &mb, &layout::script_key(&s.assembly_name, &s.namespace, &s.class_name)).ok()?;
            let Value::Array(all) = v.get("allCustomPatches")? else { return None };
            return Some(CustomGridLibraryData {
                all_custom_patches: all.iter().map(patch).collect::<Option<Vec<_>>>()?,
                generic_patches: bytes(v.get("genericPatches"))?,
                straight_patches: bytes(v.get("straightPatches"))?,
                centroid_patches: bytes(v.get("centroidPatches"))?,
                centroid_curve_patches: bytes(v.get("centroidCurvePatches"))?,
            });
        }
    }
    None
}
