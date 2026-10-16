"""Repository guard (row F10). Run before pushing; CI runs it on every push.

    python -I tools/guard.py

Fails when:
  1. a tracked file is a game file or decompiler output (by name, size, magic bytes or content markers);
  2. docs/function-index.csv is malformed (columns, status values, RVAs, duplicates);
  3. a doc comment in townscaper-rs names a game method (`Placemaker.*` / `Os.*`) with an RVA that is not that
     method's row in function-index.csv.
"""
import csv
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
STATUSES = {"dumped", "read", "ported", "checked"}

FORBIDDEN_NAMES = re.compile(
    r"(^|/)(GameAssembly\.dll|global-metadata\.dat|globalgamemanagers(\.assets)?|level\d+|"
    r"dump\.cs|il2cpp\.h|script\.json|stringliteral\.json|Townscaper\.exe|UnityPlayer\.dll)$"
    r"|\.(pdb|assets|resS|resource|gpr|rgba|fsb|bank)$|(^|/)work/|\.rep/",
    re.IGNORECASE,
)
# Content that only decompiler output or the game's own files contain.
MARKERS = [
    (b"il2cpp_codegen_initialize_runtime_metadata", "Ghidra/il2cpp pseudo-C"),
    (b"_il2cpp_TypeInfo_var", "Ghidra/il2cpp pseudo-C"),
    (b"// TypeDefIndex: ", "Il2CppDumper dump.cs"),
    (b"[Address(RVA = ", "Cpp2IL output"),
]
# Markers are looked for in source and data files only (notes may name them), not in tools/ (which parse them).
MARKER_EXTS = {".rs", ".cs", ".c", ".h", ".txt", ".json", ".csv", ".java", ".ps1"}
MAX_BYTES = 2 * 1024 * 1024  # the vendored C sources and layouts.json are far below this
SIZE_EXEMPT = {"docs/function-index.csv"}

errors: list[str] = []


def tracked() -> list[str]:
    out = subprocess.run(["git", "ls-files", "-z"], cwd=ROOT, capture_output=True, check=True).stdout
    return [p for p in out.decode().split("\0") if p]


def check_files(files: list[str]) -> None:
    for rel in files:
        p = ROOT / rel
        if FORBIDDEN_NAMES.search(rel):
            errors.append(f"{rel}: game file or tool output by name")
            continue
        if not p.is_file():
            continue
        data = p.read_bytes()
        if len(data) > MAX_BYTES and rel not in SIZE_EXEMPT:
            errors.append(f"{rel}: {len(data)} bytes (over {MAX_BYTES}): game data?")
        head = data[:64]
        if head[:4] == b"FSB5":
            errors.append(f"{rel}: an FMOD sound bank")
        if head[:2] == b"MZ":
            errors.append(f"{rel}: a Windows executable")
        if len(head) >= 0x30 and head[8:12] in (b"\0\0\0\x16", b"\0\0\0\x15") and b"2020.3" in data[0x30:0x40]:
            errors.append(f"{rel}: a Unity SerializedFile")
        if rel.startswith("tools/") or p.suffix.lower() not in MARKER_EXTS:
            continue
        for marker, what in MARKERS:
            if marker in data:
                errors.append(f"{rel}: contains {marker.decode()!r} ({what})")


def check_index() -> dict[tuple[str, str], set[str]]:
    path = ROOT / "docs" / "function-index.csv"
    by_rva: dict[tuple[str, str], set[str]] = {}
    with path.open(encoding="utf-8") as f:
        r = csv.DictReader(f)
        want = ["rva", "va", "namespace", "type", "method", "signature", "status", "notes"]
        if r.fieldnames != want:
            errors.append(f"function-index.csv: columns {r.fieldnames}, want {want}")
            return by_rva
        seen = set()
        for i, row in enumerate(r, start=2):
            if not re.fullmatch(r"0x[0-9A-F]+", row["rva"]):
                errors.append(f"function-index.csv:{i}: bad rva {row['rva']!r}")
            if row["status"] not in STATUSES:
                errors.append(f"function-index.csv:{i}: status {row['status']!r} not in {sorted(STATUSES)}")
            key = (row["rva"], row["type"], row["method"])
            if key in seen:
                errors.append(f"function-index.csv:{i}: duplicate {key}")
            seen.add(key)
            full = f"{row['namespace']}.{row['type']}" if row["namespace"] else row["type"]
            by_rva.setdefault((int(row["rva"], 16), full), set()).add(row["method"])
    return by_rva


# `Ns.Type.Method` 0x..., optionally with a parameter list: `Ns.Type.Method(int, float)` 0x...
DOC_REF = re.compile(r"`((?:Placemaker|Os)(?:\.\w+)+)\.([\w.<>]+)(?:\([^`]*\))?`\s+(0x[0-9A-Fa-f]+)")


def check_doc_refs(files: list[str], index: dict[tuple[str, str], set[str]]) -> int:
    n = 0
    for rel in files:
        if not (rel.startswith("townscaper-rs/") and rel.endswith(".rs")) or not (ROOT / rel).is_file():
            continue
        for lineno, line in enumerate((ROOT / rel).read_text(encoding="utf-8").splitlines(), start=1):
            if "//" not in line:
                continue
            for m in DOC_REF.finditer(line):
                typ, method, rva = m.group(1), m.group(2), int(m.group(3), 16)
                n += 1
                if method not in index.get((rva, typ), set()):
                    errors.append(f"{rel}:{lineno}: `{typ}.{method}` {m.group(3)} is not a row of function-index.csv")
    return n


def main() -> None:
    files = tracked()
    check_files(files)
    index = check_index()
    refs = check_doc_refs(files, index)
    if errors:
        print("guard: FAILED")
        for e in errors:
            print("  " + e)
        sys.exit(1)
    print(f"guard: ok ({len(files)} tracked files, {sum(len(v) for v in index.values())} index rows, {refs} address refs)")


if __name__ == "__main__":
    main()
