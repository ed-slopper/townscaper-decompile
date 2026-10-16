"""Print the identity of a Townscaper install: file hashes, Unity and IL2CPP versions.

    python -I tools/identify.py "C:/GOG Games/Townscaper"

Compare the output with the reference table in AGENTS.md. If GameAssembly.dll or
global-metadata.dat differ, addresses and type indices in this repo do not apply.
"""
import hashlib
import re
import struct
import sys
from pathlib import Path


def md5(path: Path) -> str:
    h = hashlib.md5()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def unity_version(ggm: Path) -> str:
    head = ggm.read_bytes()[:256]
    m = re.search(rb"\d{4}\.\d+\.\d+[abfp]\d+", head)
    return m.group().decode() if m else "?"


def pe_machine(dll: Path) -> str:
    data = dll.read_bytes()[:4096]
    pe = struct.unpack_from("<I", data, 0x3C)[0]
    machine = struct.unpack_from("<H", data, pe + 4)[0]
    return {0x14C: "x86", 0x8664: "x64", 0xAA64: "arm64"}.get(machine, hex(machine))


def main() -> None:
    game = Path(sys.argv[1] if len(sys.argv) > 1 else "C:/GOG Games/Townscaper")
    data = game / "Townscaper_Data"
    meta = data / "il2cpp_data" / "Metadata" / "global-metadata.dat"
    dll = game / "GameAssembly.dll"
    sanity, version = struct.unpack("<Ii", meta.read_bytes()[:8])

    print(f"game dir            {game}")
    print(f"unity               {unity_version(data / 'globalgamemanagers')}")
    print(f"GameAssembly.dll    {pe_machine(dll)} md5 {md5(dll)} ({dll.stat().st_size} bytes)")
    print(f"global-metadata.dat v{version} (magic {sanity:#x}) md5 {md5(meta)}")
    pdb = game / "GameAssembly.pdb"
    if pdb.exists():
        print(f"GameAssembly.pdb    md5 {md5(pdb)} ({pdb.stat().st_size} bytes)")
    for info in game.glob("goggame-*.info"):
        m = re.search(r'"buildId":\s*"(\d+)"', info.read_text(errors="replace"))
        print(f"store               GOG build {m.group(1) if m else '?'}")


if __name__ == "__main__":
    main()
