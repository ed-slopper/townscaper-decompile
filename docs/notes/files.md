# The game's files

All in `Townscaper_Data/` of the player's install. Read by `townscaper-rs/data` (`ts_data`).

## SerializedFile (F8a, proven 2026-10-15)

`globalgamemanagers`, `globalgamemanagers.assets`, `level0..2`, `resources.assets`, `sharedassets0..2.assets`:
Unity SerializedFile **format 22**, little-endian data, target platform 5 (StandaloneWindows), **no type
trees** (`enableTypeTree` = 0 in all nine). Layout of the header: `ts_data::serialized` doc comment.

Metadata, in order (format 22, no type trees):

| Part | Per entry |
|---|---|
| Unity version (cstring), target platform (i32), enable type tree (u8) | |
| types: count (i32) | class id i32, stripped u8, script type index i16, script id [16] (class 114 only), old type hash [16] |
| objects: count | align 4, path id i64, byte start i64 (relative to the data offset), byte size u32, type index i32 |
| script refs: count | file index i32, align 4, path id i64 |
| externals: count | empty cstring, guid [16], type i32, path cstring |
| ref types: count | as types; script id present when script type index >= 0 |
| user information (cstring) | |

Proven by `data/tests/install.rs`: each file's metadata ends exactly at `0x30 + metadata size`, no two objects
overlap, every object lies inside the file.

| File | Types | Objects | Script refs | Externals |
|---|---:|---:|---:|---:|
| globalgamemanagers | 18 | 18 | 0 | 7 |
| globalgamemanagers.assets | 6 | 1,115 | 0 | 2 |
| level0 | 32 | 144 | 19 | 3 |
| level1 | 64 | 375 | 47 | 4 |
| level2 | 99 | 4,772 | 85 | 5 |
| resources.assets | 13 | 52 | 7 | 2 |
| sharedassets0.assets | 23 | 229 | 11 | 2 |
| sharedassets1.assets | 35 | 1,688 | 21 | 3 |
| sharedassets2.assets | 11 | 85 | 3 | 4 |

The object table is sorted by path id, not by offset.

## Built-in objects (F8b, proven 2026-10-15)

Unity 2020.3.18f1 layouts, little-endian; `string` = i32 length + bytes + align 4; `PPtr` = file id i32 +
path id i64; arrays = i32 count + items. Objects are **not padded at their end** (GameObject in level0 is 67
bytes, ending on a `bool`). Read by `ts_data::objects`, proven by `data/tests/objects.rs` (exact size for every
object of the class in the install; Transform ↔ GameObject ↔ children ↔ father references agree).

| Class (id) | Fields | Count in the build |
|---|---|---:|
| GameObject (1) | components `PPtr[]`, layer u32, name, tag u16, is active bool | 1,468 |
| Transform (4) | game object, local rotation (x,y,z,w), local position, local scale, children `PPtr[]`, father | 381 |
| RectTransform (224) | Transform + anchor min, anchor max, anchored position, size delta, pivot (all `Vector2`) | 1,087 |
| MonoBehaviour (114) | game object, enabled bool (align 4), script `PPtr<MonoScript>`, name, *then the script's fields* (F8g) | 2,612 |
| MonoScript (115) | name, execution order i32, properties hash [16], class name, namespace, assembly name | 1,109 |

- `PPtr` with file id n > 0 points into `externals[n - 1]`; the external's path is matched by file name.
- Every MonoBehaviour's script resolves to a MonoScript except one with a null script:
  `globalgamemanagers.assets` path id 1115 `InputSystem.inputsettings` (its package is not in the build).

## Mesh (F8c, proven 2026-10-15)

`ts_data::mesh::Mesh`, Unity 2020.3 layout (field order in the doc comments of `Mesh::read`): name, sub-meshes
(48 bytes each), blend shapes, bind poses, bone data, compression u8, readable / keep vertices / keep indices,
index format, index buffer, vertex data (count, `ChannelInfo[]` of 4 bytes, bytes), compressed mesh (11
packed bit vectors + UV info), local AABB, usage flags, two baked collision meshes, mesh metrics ×2,
`StreamingInfo` (offset **u64**, size u32, path).

Vertex streams follow each other, each start aligned to 16; a stream's stride is the sum of its channels'
sizes. In the build: **114 meshes, 6,974 vertices**, none compressed, none streamed to `.resS`, all channels
Float32 (2, 3 or 4 components). Inferred: so few vertices means the building modules are not Unity `Mesh`
objects; they are likely in the `ModuleLibrary` asset's own fields (F8g, C2).

## Texture2D (F8d, proven 2026-10-15)

`ts_data::texture::Texture2D`, Unity 2020.3 layout: name, forced fallback format, downscale fallback, alpha
optional (align), width, height, complete image size, mips stripped, format, mip count, readable,
pre-processed, ignore master limit, streaming mipmaps (align), streaming priority, image count, dimension,
texture settings (filter, aniso, mip bias, wrap U/V/W), lightmap format, colour space, platform blob,
image data, `StreamingInfo`. Rows are stored bottom-up.

In the build: **36 textures**, formats Alpha8 ×12, RGB24 ×7, RGBA32 ×8, DXT1 ×3, DXT5 ×4, BC7 ×2; all decode
to RGBA8 (BC7 through `texture2ddecoder`), one is empty (no data). Checked by eye on the BC7 sky clouds and the
DXT5 logo sheet (`cargo run -p ts_data --example dump_textures -- ../work/textures`: output stays in `work/`).
Inferred: so few textures means the town's look is mostly shaders and vertex colours, not textures (C3).

## AudioClip and FSB5 (F8e, proven 2026-10-15)

`ts_data::audio::AudioClip`, Unity 2020.3 layout: name, load type, channels, frequency, bits per sample,
length (f32 s), tracker format, ambisonic (align), subsound index, preload, load in background, legacy 3D
(align), resource (source path, offset **u64**, size **u64**), compression format.

The sound is an **FMOD FSB5** bank (version 1, 60-byte header) in `sharedassets0..2.resource`, one sound per
bank. Sample header: u64 bit field (extra chunks 1 bit, frequency index 4, stereo 1, data offset 28 ×32,
sample count 30), then extra chunks (u32: more 1 bit, size 24, type 7; type 11 = Vorbis: setup CRC32 + seek
table). Vorbis data = packets each prefixed by a u16 length.

- **90 clips**, all Vorbis (FSB5 mode 15), mono or stereo, 44,100 or 48,000 Hz, 17,174 packets; bank and
  clip agree on channels, rate and length for all 90.
- FMOD strips the Vorbis identification and setup headers. `ts_vorbisref` (unpatched Xiph libvorbis 1.3.4,
  vendored) rebuilds them: the setup header written at the right VBR quality has the stored CRC. The build
  uses **five** setup headers: mono q 0.51 (`d6e0bbd4`), stereo q 0.36 (`b6b3868b`), 0.6 (`be82e3b1`),
  0.7 (`a4f666fc`), 0.9 (`6d39bf3e`). The aoTuV-patched encoder (`vorbis_rs`) reproduces four but not
  stereo q 0.36 (used only by the music `soft-piano-vibes`), so it must be plain libvorbis.
- `ts_data::vorbis::decode` (lewton) gives exactly the bank's sample count for all 90 clips.
- `birdsappear` (0.36 s, stereo 48 kHz, full-size packets) decodes to -97.6 dBFS RMS, the level of 16-bit
  dither: the clip is effectively silent in the build (same decoder and quality give normal levels elsewhere).
