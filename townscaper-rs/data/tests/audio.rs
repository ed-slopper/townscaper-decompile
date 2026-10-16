//! F8e: every AudioClip of the reference install (skips without it).

use std::collections::{BTreeMap, BTreeSet};

use ts_data::audio::{self, AudioClip, Fsb5};
use ts_data::install;
use ts_data::objects::AssetSet;

#[test]
fn every_audio_clip_and_bank_reads() {
    let Some(dir) = install::find_dir() else { return };
    let data_dir = dir.join("Townscaper_Data");
    let a = AssetSet::load(&data_dir).unwrap();
    let mut res: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let mut kinds = BTreeSet::new();
    let (mut n, mut packets, mut frames) = (0, 0usize, 0usize);
    let mut qualities = BTreeSet::new();
    let mut silent = Vec::new();
    for f in &a.files {
        for o in &f.meta.objects {
            if f.class_id(o) != audio::CLASS_ID {
                continue;
            }
            let c = AudioClip::read(f.bytes_of(o)).unwrap_or_else(|e| panic!("{} clip {}: {e}", f.name, o.path_id));
            let file = c.source.rsplit('/').next().unwrap().to_string();
            let all = res.entry(file.clone()).or_insert_with(|| std::fs::read(data_dir.join(&file)).unwrap());
            let bank = &all[c.offset as usize..][..c.size as usize];
            let fsb = Fsb5::parse(bank).unwrap_or_else(|e| panic!("{}: {e}", c.name));
            assert_eq!(fsb.samples.len(), 1, "{}: one sound per bank", c.name);
            let s = &fsb.samples[0];
            assert_eq!(s.channels as i32, c.channels, "{}: channels", c.name);
            assert_eq!(s.frequency as i32, c.frequency, "{}: frequency", c.name);
            let secs = s.samples as f32 / s.frequency as f32;
            assert!((secs - c.length).abs() < 0.01, "{}: {secs} s in the bank, {} s in the clip", c.name, c.length);
            assert_eq!(fsb.mode, audio::codec::VORBIS);
            let p = audio::vorbis_packets(&bank[s.data.clone()]).unwrap();
            assert!(!p.is_empty(), "{}: no packets", c.name);
            packets += p.len();
            kinds.insert((s.channels, s.frequency, s.vorbis_crc.expect("vorbis crc")));
            let pcm = ts_data::vorbis::decode(bank, s).unwrap_or_else(|e| panic!("{}: {e}", c.name));
            assert_eq!(pcm.frames(), s.samples as usize, "{}: decoded length", c.name);
            let peak = pcm.samples.iter().map(|&x| (x as i32).abs()).max().unwrap_or(0);
            if std::env::var_os("TS_PEAKS").is_some() { eprintln!("peak {peak:6} {} {:.2}s {} bytes {:.0} B/s", c.name, c.length, s.data.len(), s.data.len() as f32 / c.length); }
            if peak <= 100 { silent.push(c.name.clone()); }
            qualities.insert(format!("{:.2}", pcm.quality));
            frames += pcm.frames();
            if std::env::var_os("TS_LIST_CLIPS").is_some() {
                eprintln!("clip {:08x} {} ch {} {} Hz {:.2} s", s.vorbis_crc.unwrap(), c.name, s.channels, s.frequency, c.length);
            }
            n += 1;
        }
    }
    eprintln!("{n} clips, {packets} vorbis packets, {frames} frames decoded; qualities {qualities:?}; (channels, Hz, setup crc): {kinds:x?}");
    assert_eq!(n, 90);
    // "birdsappear" is effectively silent in the build: -97.6 dBFS RMS (16-bit dither level), files.md
    assert_eq!(silent, ["birdsappear"], "clips that decode to (near) silence");
}
