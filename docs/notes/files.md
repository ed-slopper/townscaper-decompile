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
