//! F8c: every Mesh of the reference install (skips without it).

use ts_data::install;
use ts_data::mesh::{self, Mesh};
use ts_data::objects::AssetSet;

#[test]
fn every_mesh_reads_and_decodes() {
    let Some(dir) = install::find_dir() else { return };
    let data_dir = dir.join("Townscaper_Data");
    let a = AssetSet::load(&data_dir).unwrap();
    let (mut n, mut streamed, mut verts) = (0, 0, 0usize);
    let mut formats = std::collections::BTreeSet::new();
    for f in &a.files {
        for o in &f.meta.objects {
            if f.class_id(o) != mesh::CLASS_ID {
                continue;
            }
            let m = Mesh::read(f.bytes_of(o)).unwrap_or_else(|e| panic!("{} mesh {}: {e}", f.name, o.path_id));
            assert_eq!(m.mesh_compression, 0, "{}: compressed meshes not handled", m.name);
            let idx = m.indices();
            assert!(idx.iter().all(|&i| i < m.vertex_count.max(1)), "{}: index past vertex count", m.name);
            for s in &m.sub_meshes {
                let isz = if m.index_format == 1 { 4 } else { 2 };
                assert!((s.first_byte + s.index_count * isz) as usize <= m.index_buffer.len(), "{}: submesh past index buffer", m.name);
            }
            // The vertex bytes: inline, or the streamed range of the .resS next to the file.
            let resS;
            let bytes: &[u8] = if m.vertex_data.is_empty() && m.stream_data.size > 0 {
                let file = m.stream_data.path.rsplit('/').next().unwrap();
                let all = std::fs::read(data_dir.join(file)).unwrap();
                resS = all[m.stream_data.offset as usize..][..m.stream_data.size as usize].to_vec();
                streamed += 1;
                &resS
            } else {
                &m.vertex_data
            };
            let (last_start, last_stride) = *m.stream_layout().last().unwrap_or(&(0, 0));
            assert!(last_start + last_stride * m.vertex_count as usize <= bytes.len(), "{}: streams past data", m.name);
            for c in &m.channels {
                if c.components() > 0 {
                    formats.insert((c.format, c.components()));
                }
            }
            if m.vertex_count > 0 {
                let pos = m.channel_f32(bytes, 0).expect("positions");
                let tol = 1e-3 + 1e-4 * m.aabb_extent.iter().fold(0f32, |a, b| a.max(b.abs()));
                for p in &pos {
                    for k in 0..3 {
                        assert!((p[k] - m.aabb_center[k]).abs() <= m.aabb_extent[k] + tol, "{}: vertex outside its bounds", m.name);
                    }
                }
            }
            n += 1;
            verts += m.vertex_count as usize;
        }
    }
    eprintln!("{n} meshes ({streamed} streamed), {verts} vertices, channel formats (format, comps): {formats:?}");
    assert!(n > 50, "only {n} meshes");
}
