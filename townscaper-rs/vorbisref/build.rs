fn main() {
    let v = "vendor/libvorbis-1.3.4";
    let o = "vendor/libogg-1.3.5";
    let mut b = cc::Build::new();
    b.include(format!("{v}/include")).include(format!("{v}/lib")).include(format!("{o}/include")).include("include").warnings(false);
    for f in [
        "analysis", "bitrate", "block", "codebook", "envelope", "floor0", "floor1", "info", "lookup", "lpc", "lsp",
        "mapping0", "mdct", "psy", "registry", "res0", "sharedbook", "smallft", "synthesis", "vorbisenc", "window",
    ] {
        b.file(format!("{v}/lib/{f}.c"));
    }
    b.file(format!("{o}/src/bitwise.c")).file(format!("{o}/src/framing.c")).file("shim.c");
    b.compile("ts_vorbisref");
    println!("cargo:rerun-if-changed=shim.c");
    println!("cargo:rerun-if-changed=vendor");
}
