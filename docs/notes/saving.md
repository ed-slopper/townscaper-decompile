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
