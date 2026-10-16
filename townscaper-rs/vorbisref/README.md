# ts_vorbisref

FMOD's FSB5 banks store Vorbis audio **without** its identification and setup headers, only a CRC32 of the
setup header. The headers are what libvorbis's encoder writes for the clip's channels, rate and quality, so we
rebuild them by running the encoder's header step at each quality and keeping the one whose CRC matches
(row F8e, `docs/notes/files.md`).

It must be **unpatched** Xiph libvorbis: the aoTuV-patched encoder (`vorbis_rs`) gives different codebooks at
some qualities (one clip of the game, stereo at quality 0.36, only matches plain libvorbis).

`vendor/` holds the files of the official Xiph releases that the encoder needs, unchanged:

| Library | Release | sha256 of the release tarball | Licence |
|---|---|---|---|
| libvorbis | 1.3.4, downloads.xiph.org | `eee09a0a13ec38662ff949168fe897a25d2526529bc7e805305f381c219a1ecb` | BSD-3-Clause (`vendor/libvorbis-1.3.4/COPYING`) |
| libogg | 1.3.5, downloads.xiph.org | `0eb4b4b9420a0f51db142ba3f9c64b333f826532dc0f48c6410ae51f4799b664` | BSD-3-Clause (`vendor/libogg-1.3.5/COPYING`) |
