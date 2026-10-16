//! Unpatched Xiph libvorbis 1.3.4's header writer (see README.md): the identification and setup headers the
//! encoder produces for a channel count, sample rate and VBR quality.

use std::os::raw::{c_float, c_int, c_long};

extern "C" {
    fn ts_vorbis_headers(
        channels: c_int,
        rate: c_long,
        quality: c_float,
        ident: *mut u8,
        ident_cap: c_long,
        ident_len: *mut c_long,
        setup: *mut u8,
        setup_cap: c_long,
        setup_len: *mut c_long,
    ) -> c_int;
}

/// The identification and setup header packets, or `None` if libvorbis refuses the settings.
pub fn headers(channels: u32, rate: u32, quality: f32) -> Option<(Vec<u8>, Vec<u8>)> {
    let mut ident = vec![0u8; 64];
    let mut setup = vec![0u8; 1 << 16];
    let (mut il, mut sl): (c_long, c_long) = (0, 0);
    let r = unsafe {
        ts_vorbis_headers(
            channels as c_int,
            rate as c_long,
            quality,
            ident.as_mut_ptr(),
            ident.len() as c_long,
            &mut il,
            setup.as_mut_ptr(),
            setup.len() as c_long,
            &mut sl,
        )
    };
    if r != 0 {
        return None;
    }
    ident.truncate(il as usize);
    setup.truncate(sl as usize);
    Some((ident, setup))
}

#[cfg(test)]
mod tests {
    #[test]
    fn writes_vorbis_headers() {
        let (ident, setup) = super::headers(2, 44100, 0.5).unwrap();
        assert_eq!(&ident[..7], b"\x01vorbis");
        assert_eq!(ident[11], 2, "channels");
        assert_eq!(&setup[..7], b"\x05vorbis");
    }
}
