# Saving (G3)

The save folder is `%USERPROFILE%/AppData/LocalLow/Oskar Stalberg/Townscaper` on Windows: `Sett.ings`, `Saves/`
(one file per town, `Town<20 chars>.scape`), `Screenshots/`, `Textures/` (the thumbnails' PNG cache).

## The files (G3, proven)

`placemaker::save_data` (the types) and `placemaker::save_xml` (the files), checked on the player's own saves
(`tests/placemaker_save_files.rs`: every town and `Sett.ings` read and written back to the same bytes) and on
the game's own serializers (240 random objects serialized in the game, ours must write the same bytes and read
them back as the game's reader does).

- `SaveSystem.saveXmlSerializer` / `settingsXmlSerializer` are `XmlSerializer`s of `SaveData` / `SettingsData`.
  A path ending `.binscape` (`MetaSave.isBin`) goes through `binaryFormatter` instead (row G3c); the game only
  makes `.scape` (`shouldBeBin` is false).
- Writing: UTF-8 **without** a byte order mark; `<?xml version="1.0" encoding="utf-8"?>`, CRLF, the root with
  `xmlns:xsd` then `xmlns:xsi`; every field in declaration order as an element, two spaces a level; an empty
  string or an empty list as `<name />`; a null string or array left out; numbers as plain invariant integers;
  booleans `true` / `false`; `bigJpg` in base64 on one line; no newline after the root's end tag. Text escapes
  `&`, `<`, `>` only (CR and quotes are written raw).
- Reading: elements in any order, unknown ones skipped, a missing one keeps the constructor's value; line ends
  in text come back as LF; text that is only white space comes back empty (the reader drops it as
  insignificant).
- `SaveData` (`..ctor` 0x967AE0): `saveString` (the town as text, G3a), `lastWriteTime` (UTC ticks), `cam`
  (tilt, turn, zoom bytes; `(127, 0, 127)` new), `sun` (height, turn bytes; `(127, 127)`), `corners` (hex
  position and voxel count), `voxels` (type and height, in corner order), `bigJpg` (the 512² thumbnail, G2's
  `SaveCamera`). `Clear` 0x967850 resets the camera, sun and lists; `ReadNonImageDataFrom` 0x967A10 copies
  those; `CopyFrom` 0x9678D0 also the image (into a new array when the lengths differ).
- `SettingsData` (`..ctor` 0x973460): `version`, `lastSave` (a file name), `currentColor`, `audioVolume` (255),
  `antiAliasing` (true), `axis` (two inversions), `vSync`, `fullscreen` (true), `language` (empty),
  `screenshot` (path, 2048 × 2048), `objExport` (path), `uiSizePercentage` (100), `lastRunVersion` ("1.0.0").

## The town as text (G3a, proven)

`placemaker::text_save_system`, checked on the player's saves (each file's `saveString` is our text of its
corners and voxels, and reads back to them) and against the game (`tests/placemaker_text_save.rs`: 150 random
towns written the same; 539 random, damaged and foreign strings read with the same result, lists, rejection or
throw).

- `TextSaveSystem.SaveToString` 0x31E2F0 / `LoadFromString` 0x31D450. Values are packed least significant bit
  first into one static `BitArray`, six bits per letter of `TextSaveAlphabet.alphabet`
  (`ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvxyzw0123456789_-`: note `uvxyzw`).
- Header: three 5-bit widths (positions, x steps, y offsets); the first corner's x and the lowest y, each plus
  half the position range (`1 << (posBits - 1)`, the range being twice the bigger of `|x first|` and
  `|y lowest|`). Then the types used (4-bit count, 4 bits each, in type order 0..14; only voxels above the ground
  count). A voxel is stored as its type's index + 1 in `GetBitCount(typeCount + 1)` bits; 0 is an empty height;
  all ones ends a column.
- Columns in save order: from the second on, 1 bit "x moved" and then the step; y above the lowest; 1 bit for a
  ground voxel (height 0; its type is not kept and reads back as 15); then heights from 1 up (gaps as zeros)
  and the end marker, which is left out when a column reaches height 256 (and reading stops a column at 255).
- Reading trims `\r` and `\n` from both ends, needs at least 20 bits, refuses letters outside the alphabet,
  ignores the zero bits after the last set one (a string of only `A`s makes the game throw: `BitArray.Get(-1)`),
  and fails on a type index out of range. Reads are bounded by the bit array's whole length, which only grows
  and keeps bits from earlier calls: our `BitArray` carries that state, so damaged strings read the same
  leftovers. What a failed read appended stays in the lists. Messages as the game logs them.
- `GetSpacedString` and `LogBitArray` have no callers.

## The save system (G3b, proven on the boot)

`placemaker::save_system`, checked by booting ours on the player's own save folder (read only, through a
file-system storage container) and comparing with the harness game, which booted and refreshed on the same
folder (`tests/placemaker_save_system.rs`): the menu's order of towns, each meta's path, file name, state, write
time and flags, the visible and total counts, boot and refresh states, the current town and the settings.
`GetUniqueSaveName` matches the game's (35 rounds with a still clock).

- Storage is the Fury SDK's `IStorageContainer` (`Platform.SDK.GetStorage("storage")`, middleware): relative
  paths under the save folder (`Saves\Town….scape`, `Sett.ings`), asynchronous requests polled with `MoveNext`
  and finishing through `OnComplete` delegates (ours: the host calls `SaveSystem::complete`).
- `MetaSave.State` flags: unclear 1, needs updating 2, up to date 4, not yet saved 8, new from duplicate 16,
  failed to load 32, did not exist on disk 64, deleted 128, returned to pool 256; broken = 480, invisible = 483,
  belongs in the pool = 448. `ChangeState` tells the card, clears the data of a newly broken meta, shows or hides
  it (keeping `validMetaSaveCount`) and moves pool-bound ones to the pool.
- Metas live in `metaSaveContainer` (the menu order) and `metaSavePool` (child 0 is the scene's template, state
  unclear, copied with a new 512² texture when the pool is otherwise empty; else the last pooled one is reset).
- Boot (`IterateBoot`): every container meta back to the pool; `Sett.ings` exists? read it (its `lastSave` gets
  `\` for `/`, settings dirty, old version-0 towns moved from the root into `Saves`, version 2) : new settings
  (version 2); the last save exists? a meta for it, loaded; whole → valid last save, broken → the list. The
  list refresh runs; with no files a new town from `WorldMaster.defaultSaveData`.
- Refresh (`IterateRefresh`): list files; keep `*.scape` / `*.binscape` directly in `Saves` (counted as the total);
  metas whose file is gone are dropped (write time 0, did not exist on disk), the others reloaded (all but the
  current one); a meta for each new file; the current one on top; then each one after it, once loaded, inserted
  by `CompareTo` (whole ones first, newest write time, then instance id) with `ExtraMath.BinarySearchComponent`
  (whose range stops one short of the meta in `SortAll`, as compiled).
- A load (`<LoadMetaSaveFromDisk>b__0`): the bytes deserialized; no data → fresh data, failed to load (and the meta
  stays "loading"); else the file's `GetLastWriteTimeUtc`, the texture, up to date, shown, on disk, not loading.
  An unreadable XML file throws out of the callback in the game: the meta stays loading.
- Saving writes the meta's bytes (`SaveSave`: sync and write times stamped); a meta without a file first draws
  names (`Unity.Mathematics.Random` from `DateTime.Now.GetHashCode()`, 16 letters filled from the end) until one
  is free. `saveRequest` is never set in the build, so `isSaving` is always false and nothing waits on it.
  The current town's file name becomes the settings' `lastSave` when it saves.
- `StorageUtils` and `SaveCardsHandler` are an older save path in no scene or loaded asset of this build: read,
  not ported. `TexturePngMaster` (the palette PNG cache) is row G3d.

## `.binscape` files (G3c, proven)

`placemaker::save_binary`, checked against the game's `SaveSystem.binaryFormatter` (`tests/placemaker_save_binary.rs`:
120 random save data built in the game with `Add`: our bytes equal the game's, we read the game's back, the game
reads ours, written with other list capacities, to the same data).

- The game reads a `.binscape` file (`MetaSave.isBin`) with `BinaryFormatter.Deserialize` and writes one only for
  a meta that came from such a file; on PC it makes `.scape` files (`shouldBeBin` false).
- MS-NRBF as the reference source writes it: header (root 1, version 1.0); `Assembly-CSharp`'s library; `SaveData`
  as `ClassWithMembersAndTypes` with its 7 members (string, Int64, two `Class` structs, two `SystemClass` lists, a
  byte array); `saveString` as a string record (null: `ObjectNull`); `cam`, `sun` inline with negative ids; the
  lists and `bigJpg` as references, written after the root in the order met. A `List<T>` is `_items`, `_size`,
  `_version` (= the size for lists built by `Add`); its array a `BinaryArray` as long as the capacity (0, 4, 8, …
  for `Add`s), the first element a full struct record, the rest `ClassWithId` pointing at its layout, unused slots
  as zero structs; the image an `ArraySinglePrimitive` of bytes. Ids count every object in the order met.
- Reading is general (any record kind, `ClassWithId`, nulls, references, any capacity) and takes `_size` items; a
  root that is not `SaveData` reads as null (the game's `as`), an unreadable stream throws (the meta stays loading).
- Writing takes the capacities as grown by `Add`s; the game's could differ after other list operations (a gap
  only for a `.binscape` written back, which PC saves never are).
