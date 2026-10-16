//! Unity 2020.3 `Mesh` (class 43): the serialized layout and the vertex/index decode (row F8c).
//! Proven by `tests/mesh.rs` on every mesh of the reference install.

use crate::objects::{Error, Reader};

pub const CLASS_ID: i32 = 43;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SubMesh {
    /// Byte offset into the index buffer.
    pub first_byte: u32,
    pub index_count: u32,
    /// 0 triangles, 2 quads, 3 lines, 4 line strip, 5 points.
    pub topology: i32,
    pub base_vertex: u32,
    pub first_vertex: u32,
    pub vertex_count: u32,
    pub aabb_center: [f32; 3],
    pub aabb_extent: [f32; 3],
}

/// Where a vertex attribute lives: `ChannelInfo`. Channels: 0 position, 1 normal, 2 tangent, 3 colour,
/// 4..=11 UV0..UV7, 12 blend weights, 13 blend indices.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Channel {
    pub stream: u8,
    pub offset: u8,
    /// `VertexFormat`: 0 Float32, 1 Float16, 2 UNorm8, 3 SNorm8, 4 UNorm16, 5 SNorm16, 6 UInt8, 7 SInt8,
    /// 8 UInt16, 9 SInt16, 10 UInt32, 11 SInt32.
    pub format: u8,
    /// Low 4 bits: component count (0 = channel unused).
    pub dimension: u8,
}

impl Channel {
    pub fn components(&self) -> usize {
        (self.dimension & 0xF) as usize
    }
    pub fn format_size(&self) -> usize {
        match self.format {
            0 | 10 | 11 => 4,
            1 | 4 | 5 | 8 | 9 => 2,
            _ => 1,
        }
    }
    pub fn byte_size(&self) -> usize {
        self.components() * self.format_size()
    }
}

/// `StreamingInfo`: vertex data kept in a `.resS` file instead of the object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamingInfo {
    pub offset: u64,
    pub size: u32,
    /// e.g. `archive:/CAB-…/CAB-….resS` or `sharedassets0.assets.resS`.
    pub path: String,
}

#[derive(Debug, Clone)]
pub struct Mesh {
    pub name: String,
    pub sub_meshes: Vec<SubMesh>,
    pub mesh_compression: u8,
    pub is_readable: bool,
    /// 0 = u16 indices, 1 = u32.
    pub index_format: i32,
    pub index_buffer: Vec<u8>,
    pub vertex_count: u32,
    pub channels: Vec<Channel>,
    /// The vertex streams; empty when they are streamed (`stream_data`).
    pub vertex_data: Vec<u8>,
    pub aabb_center: [f32; 3],
    pub aabb_extent: [f32; 3],
    pub mesh_usage_flags: i32,
    pub stream_data: StreamingInfo,
}

fn skip_bytes(r: &mut Reader) -> Result<(), Error> {
    let n = r.len()?;
    r.take(n)?;
    r.align4();
    Ok(())
}

/// `PackedBitVector` of floats (compressed meshes; the build's meshes are uncompressed: read and dropped).
fn skip_packed_floats(r: &mut Reader) -> Result<(), Error> {
    r.u32()?; // items
    r.f32()?; // range
    r.f32()?; // start
    skip_bytes(r)?;
    r.u8()?; // bit size
    r.align4();
    Ok(())
}

fn skip_packed_ints(r: &mut Reader) -> Result<(), Error> {
    r.u32()?;
    skip_bytes(r)?;
    r.u8()?;
    r.align4();
    Ok(())
}

impl Mesh {
    pub fn read(b: &[u8]) -> Result<Mesh, Error> {
        let mut r = Reader::new(b);
        let name = r.string()?;
        let n = r.len()?;
        let mut sub_meshes = Vec::with_capacity(n);
        for _ in 0..n {
            sub_meshes.push(SubMesh {
                first_byte: r.u32()?,
                index_count: r.u32()?,
                topology: r.i32()?,
                base_vertex: r.u32()?,
                first_vertex: r.u32()?,
                vertex_count: r.u32()?,
                aabb_center: r.vec3()?,
                aabb_extent: r.vec3()?,
            });
        }
        // m_Shapes (BlendShapeData)
        let n = r.len()?;
        r.take(n * 40)?; // vertices: position, normal, tangent (Vector3 each), index u32
        let n = r.len()?;
        r.take(n * 12)?; // shapes: first vertex, vertex count, has normals, has tangents (+align)
        let n = r.len()?;
        for _ in 0..n {
            r.string()?; // channel name
            r.take(12)?; // name hash, frame index, frame count
        }
        let n = r.len()?;
        r.take(n * 4)?; // full weights
        let n = r.len()?;
        r.take(n * 64)?; // bind poses
        let n = r.len()?;
        r.take(n * 4)?; // bone name hashes
        r.u32()?; // root bone name hash
        let n = r.len()?;
        r.take(n * 24)?; // bones AABB
        let n = r.len()?;
        r.take(n * 4)?; // variable bone count weights
        let mesh_compression = r.u8()?;
        let is_readable = r.bool()?;
        r.bool()?; // keep vertices
        r.bool()?; // keep indices
        r.align4();
        let index_format = r.i32()?;
        let n = r.len()?;
        let index_buffer = r.take(n)?.to_vec();
        r.align4();
        // m_VertexData
        let vertex_count = r.u32()?;
        let n = r.len()?;
        let mut channels = Vec::with_capacity(n);
        for _ in 0..n {
            channels.push(Channel { stream: r.u8()?, offset: r.u8()?, format: r.u8()?, dimension: r.u8()? });
        }
        let n = r.len()?;
        let vertex_data = r.take(n)?.to_vec();
        r.align4();
        // m_CompressedMesh
        for _ in 0..3 {
            skip_packed_floats(&mut r)?; // vertices, UV, normals
        }
        skip_packed_floats(&mut r)?; // tangents
        skip_packed_ints(&mut r)?; // weights
        skip_packed_ints(&mut r)?; // normal signs
        skip_packed_ints(&mut r)?; // tangent signs
        skip_packed_floats(&mut r)?; // float colours
        skip_packed_ints(&mut r)?; // bone indices
        skip_packed_ints(&mut r)?; // triangles
        r.u32()?; // UV info
        let aabb_center = r.vec3()?;
        let aabb_extent = r.vec3()?;
        let mesh_usage_flags = r.i32()?;
        skip_bytes(&mut r)?; // baked convex collision mesh
        skip_bytes(&mut r)?; // baked triangle collision mesh
        r.f32()?; // mesh metrics 0
        r.f32()?; // mesh metrics 1
        let stream_data = StreamingInfo { offset: r.i64()? as u64, size: r.u32()?, path: r.string()? };
        r.finish()?;
        Ok(Mesh {
            name,
            sub_meshes,
            mesh_compression,
            is_readable,
            index_format,
            index_buffer,
            vertex_count,
            channels,
            vertex_data,
            aabb_center,
            aabb_extent,
            mesh_usage_flags,
            stream_data,
        })
    }

    /// The indices as u32, whatever the index format.
    pub fn indices(&self) -> Vec<u32> {
        if self.index_format == 1 {
            self.index_buffer.chunks_exact(4).map(|c| u32::from_le_bytes(c.try_into().unwrap())).collect()
        } else {
            self.index_buffer.chunks_exact(2).map(|c| u16::from_le_bytes(c.try_into().unwrap()) as u32).collect()
        }
    }

    /// Byte stride of each stream; streams are laid out one after another, each start aligned to 16.
    pub fn stream_layout(&self) -> Vec<(usize, usize)> {
        let streams = self.channels.iter().filter(|c| c.components() > 0).map(|c| c.stream as usize + 1).max().unwrap_or(0);
        let mut out = Vec::with_capacity(streams);
        let mut offset = 0usize;
        for s in 0..streams {
            let stride: usize = self.channels.iter().filter(|c| c.stream as usize == s).map(|c| c.byte_size()).sum();
            out.push((offset, stride));
            offset += stride * self.vertex_count as usize;
            offset = (offset + 15) & !15;
        }
        out
    }

    /// Decode one channel to f32 components per vertex (normalised formats scaled to 0..1 / -1..1).
    /// `data` is `vertex_data`, or the streamed bytes from the `.resS` when `vertex_data` is empty.
    pub fn channel_f32(&self, data: &[u8], channel: usize) -> Option<Vec<Vec<f32>>> {
        let c = *self.channels.get(channel)?;
        if c.components() == 0 {
            return None;
        }
        let (start, stride) = self.stream_layout()[c.stream as usize];
        let mut out = Vec::with_capacity(self.vertex_count as usize);
        for v in 0..self.vertex_count as usize {
            let at = start + v * stride + c.offset as usize;
            let mut comps = Vec::with_capacity(c.components());
            for k in 0..c.components() {
                let p = at + k * c.format_size();
                let x = match c.format {
                    0 => f32::from_le_bytes(data.get(p..p + 4)?.try_into().ok()?),
                    1 => f16_to_f32(u16::from_le_bytes(data.get(p..p + 2)?.try_into().ok()?)),
                    2 => *data.get(p)? as f32 / 255.0,
                    3 => (*data.get(p)? as i8 as f32 / 127.0).max(-1.0),
                    4 => u16::from_le_bytes(data.get(p..p + 2)?.try_into().ok()?) as f32 / 65535.0,
                    5 => (i16::from_le_bytes(data.get(p..p + 2)?.try_into().ok()?) as f32 / 32767.0).max(-1.0),
                    6 => *data.get(p)? as f32,
                    7 => *data.get(p)? as i8 as f32,
                    8 => u16::from_le_bytes(data.get(p..p + 2)?.try_into().ok()?) as f32,
                    9 => i16::from_le_bytes(data.get(p..p + 2)?.try_into().ok()?) as f32,
                    10 => u32::from_le_bytes(data.get(p..p + 4)?.try_into().ok()?) as f32,
                    11 => i32::from_le_bytes(data.get(p..p + 4)?.try_into().ok()?) as f32,
                    _ => return None,
                };
                comps.push(x);
            }
            out.push(comps);
        }
        Some(out)
    }
}

/// IEEE half to f32.
pub fn f16_to_f32(h: u16) -> f32 {
    let sign = ((h >> 15) as u32) << 31;
    let exp = ((h >> 10) & 0x1F) as u32;
    let man = (h & 0x3FF) as u32;
    let bits = match (exp, man) {
        (0, 0) => sign,
        (0, m) => {
            // subnormal: normalise
            let mut e = 127 - 15 + 1;
            let mut m = m;
            while m & 0x400 == 0 {
                m <<= 1;
                e -= 1;
            }
            sign | (e << 23) | ((m & 0x3FF) << 13)
        }
        (0x1F, m) => sign | 0x7F80_0000 | (m << 13),
        (e, m) => sign | ((e + 127 - 15) << 23) | (m << 13),
    };
    f32::from_bits(bits)
}

#[cfg(test)]
mod tests {
    use super::f16_to_f32;

    #[test]
    fn half_floats() {
        assert_eq!(f16_to_f32(0x3C00), 1.0);
        assert_eq!(f16_to_f32(0xC000), -2.0);
        assert_eq!(f16_to_f32(0x0000), 0.0);
        assert_eq!(f16_to_f32(0x7BFF), 65504.0);
        assert_eq!(f16_to_f32(0x0001), 2f32.powi(-24));
    }
}
