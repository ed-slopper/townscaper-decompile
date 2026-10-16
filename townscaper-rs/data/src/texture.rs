//! Unity 2020.3 `Texture2D` (class 28): the serialized layout and the pixel decode (row F8d).
//! Proven by `tests/texture.rs` on every texture of the reference install.

use crate::mesh::StreamingInfo;
use crate::objects::{Error, Reader};

pub const CLASS_ID: i32 = 28;

#[derive(Debug, Clone)]
pub struct Texture2D {
    pub name: String,
    pub width: i32,
    pub height: i32,
    pub complete_image_size: u32,
    /// Unity `TextureFormat`: 1 Alpha8, 3 RGB24, 4 RGBA32, 5 ARGB32, 10 DXT1, 12 DXT5, 17 RGBAHalf, ...
    pub format: i32,
    pub mip_count: i32,
    pub is_readable: bool,
    pub image_count: i32,
    pub dimension: i32,
    pub filter_mode: i32,
    pub aniso: i32,
    pub mip_bias: f32,
    pub wrap: [i32; 3],
    pub color_space: i32,
    /// The pixels when stored in the object; empty when streamed (`stream_data`).
    pub image_data: Vec<u8>,
    pub stream_data: StreamingInfo,
}

impl Texture2D {
    pub fn read(b: &[u8]) -> Result<Texture2D, Error> {
        let mut r = Reader::new(b);
        let name = r.string()?;
        r.i32()?; // forced fallback format
        r.bool()?; // downscale fallback
        r.bool()?; // is alpha channel optional
        r.align4();
        let width = r.i32()?;
        let height = r.i32()?;
        let complete_image_size = r.u32()?;
        r.i32()?; // mips stripped
        let format = r.i32()?;
        let mip_count = r.i32()?;
        let is_readable = r.bool()?;
        r.bool()?; // is pre-processed
        r.bool()?; // ignore master texture limit
        r.bool()?; // streaming mipmaps
        r.align4();
        r.i32()?; // streaming mipmaps priority
        let image_count = r.i32()?;
        let dimension = r.i32()?;
        let filter_mode = r.i32()?;
        let aniso = r.i32()?;
        let mip_bias = r.f32()?;
        let wrap = [r.i32()?, r.i32()?, r.i32()?];
        r.i32()?; // lightmap format
        let color_space = r.i32()?;
        let n = r.len()?;
        r.take(n)?; // platform blob
        r.align4();
        let n = r.len()?;
        let image_data = r.take(n)?.to_vec();
        r.align4();
        let stream_data = StreamingInfo { offset: r.i64()? as u64, size: r.u32()?, path: r.string()? };
        r.finish()?;
        Ok(Texture2D {
            name,
            width,
            height,
            complete_image_size,
            format,
            mip_count,
            is_readable,
            image_count,
            dimension,
            filter_mode,
            aniso,
            mip_bias,
            wrap,
            color_space,
            image_data,
            stream_data,
        })
    }

    /// Bytes of mip 0 for this format, or `None` for a format we don't decode yet.
    pub fn mip0_size(&self) -> Option<usize> {
        let (w, h) = (self.width as usize, self.height as usize);
        let blocks = w.div_ceil(4) * h.div_ceil(4);
        Some(match self.format {
            1 => w * h,               // Alpha8
            3 => w * h * 3,           // RGB24
            4 | 5 | 14 => w * h * 4,  // RGBA32, ARGB32, BGRA32
            17 => w * h * 8,          // RGBAHalf
            20 => w * h * 16,         // RGBAFloat
            10 => blocks * 8,         // DXT1
            12 => blocks * 16,        // DXT5
            25 => blocks * 16,        // BC7
            _ => return None,
        })
    }

    /// Mip 0 as RGBA8, top row first (Unity stores bottom row first). `data` is `image_data` or the streamed
    /// bytes from the `.resS`.
    pub fn decode_rgba8(&self, data: &[u8]) -> Option<Vec<u8>> {
        let (w, h) = (self.width as usize, self.height as usize);
        let src = data.get(..self.mip0_size()?)?;
        let mut out = vec![0u8; w * h * 4];
        match self.format {
            1 => {
                for (i, &a) in src.iter().enumerate() {
                    out[i * 4..i * 4 + 4].copy_from_slice(&[255, 255, 255, a]);
                }
            }
            3 => {
                for i in 0..w * h {
                    out[i * 4..i * 4 + 4].copy_from_slice(&[src[i * 3], src[i * 3 + 1], src[i * 3 + 2], 255]);
                }
            }
            4 => out.copy_from_slice(src),
            5 => {
                for i in 0..w * h {
                    let p = &src[i * 4..i * 4 + 4];
                    out[i * 4..i * 4 + 4].copy_from_slice(&[p[1], p[2], p[3], p[0]]);
                }
            }
            14 => {
                for i in 0..w * h {
                    let p = &src[i * 4..i * 4 + 4];
                    out[i * 4..i * 4 + 4].copy_from_slice(&[p[2], p[1], p[0], p[3]]);
                }
            }
            10 | 12 => decode_dxt(src, w, h, self.format == 12, &mut out),
            25 => {
                // BC7 through texture2ddecoder (MIT OR Apache-2.0): pixels come out as BGRA u32
                let mut px = vec![0u32; w * h];
                texture2ddecoder::decode_bc7(src, w, h, &mut px).ok()?;
                for (i, p) in px.iter().enumerate() {
                    let [b, g, r, a] = p.to_le_bytes();
                    out[i * 4..i * 4 + 4].copy_from_slice(&[r, g, b, a]);
                }
            }
            _ => return None,
        }
        // flip to top-down
        let row = w * 4;
        for y in 0..h / 2 {
            let (a, b) = out.split_at_mut((h - 1 - y) * row);
            a[y * row..y * row + row].swap_with_slice(&mut b[..row]);
        }
        Some(out)
    }
}

fn rgb565(c: u16) -> [u8; 3] {
    let r = ((c >> 11) & 31) as u32;
    let g = ((c >> 5) & 63) as u32;
    let b = (c & 31) as u32;
    [((r * 527 + 23) >> 6) as u8, ((g * 259 + 33) >> 6) as u8, ((b * 527 + 23) >> 6) as u8]
}

/// BC1 (DXT1) / BC3 (DXT5) blocks to RGBA8, rows in stored order.
fn decode_dxt(src: &[u8], w: usize, h: usize, dxt5: bool, out: &mut [u8]) {
    let bsize = if dxt5 { 16 } else { 8 };
    let bw = w.div_ceil(4);
    for (bi, blk) in src.chunks_exact(bsize).enumerate() {
        let (bx, by) = (bi % bw, bi / bw);
        let (alpha, col) = if dxt5 { blk.split_at(8) } else { (&[][..], blk) };
        let c0 = u16::from_le_bytes([col[0], col[1]]);
        let c1 = u16::from_le_bytes([col[2], col[3]]);
        let (p0, p1) = (rgb565(c0), rgb565(c1));
        let mut pal = [[0u8; 4]; 4];
        pal[0] = [p0[0], p0[1], p0[2], 255];
        pal[1] = [p1[0], p1[1], p1[2], 255];
        for k in 0..3 {
            if c0 > c1 || dxt5 {
                pal[2][k] = ((2 * p0[k] as u32 + p1[k] as u32) / 3) as u8;
                pal[3][k] = ((p0[k] as u32 + 2 * p1[k] as u32) / 3) as u8;
            } else {
                pal[2][k] = ((p0[k] as u32 + p1[k] as u32) / 2) as u8;
                pal[3][k] = 0;
            }
        }
        pal[2][3] = 255;
        pal[3][3] = if c0 > c1 || dxt5 { 255 } else { 0 };
        let bits = u32::from_le_bytes([col[4], col[5], col[6], col[7]]);
        let mut alphas = [255u8; 16];
        if dxt5 {
            let (a0, a1) = (alpha[0] as u32, alpha[1] as u32);
            let mut ap = [0u8; 8];
            ap[0] = a0 as u8;
            ap[1] = a1 as u8;
            for i in 1..7u32 {
                ap[i as usize + 1] = if a0 > a1 {
                    (((7 - i) * a0 + i * a1) / 7) as u8
                } else if i < 5 {
                    (((5 - i) * a0 + i * a1) / 5) as u8
                } else if i == 5 {
                    0
                } else {
                    255
                };
            }
            let ab = alpha[2..8].iter().rev().fold(0u64, |acc, &x| (acc << 8) | x as u64);
            for (i, a) in alphas.iter_mut().enumerate() {
                *a = ap[((ab >> (3 * i)) & 7) as usize];
            }
        }
        for i in 0..16 {
            let (x, y) = (bx * 4 + i % 4, by * 4 + i / 4);
            if x >= w || y >= h {
                continue;
            }
            let mut px = pal[((bits >> (2 * i)) & 3) as usize];
            if dxt5 {
                px[3] = alphas[i];
            }
            out[(y * w + x) * 4..(y * w + x) * 4 + 4].copy_from_slice(&px);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dxt1_solid_block() {
        // c0 = pure red 0xF800, c1 = 0, all indices 0 → every pixel red
        let blk = [0x00, 0xF8, 0x00, 0x00, 0, 0, 0, 0];
        let mut out = vec![0u8; 16 * 4];
        decode_dxt(&blk, 4, 4, false, &mut out);
        assert!(out.chunks(4).all(|p| p == [255, 0, 0, 255]));
    }
}
