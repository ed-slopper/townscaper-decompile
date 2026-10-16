"""Build the type and function indexes of the game's own code from Il2CppDumper's dump.cs.

    python -I tools/index.py [work/il2cppdumper/dump.cs]

Writes (names, signatures and addresses only, no code):
    docs/type-index.csv      one row per type in Assembly-CSharp and Assembly-CSharp-firstpass
    docs/function-index.csv  one row per method with a body, for the game's own code (owner = game)

The status column of function-index.csv is kept across runs: rows already marked by hand
(read / ported / checked, see AGENTS.md) keep their mark when the index is rebuilt.
"""
import csv
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DOCS = ROOT / "docs"

# TypeDefIndex ranges, from the "// Image N" header of dump.cs (reference build, see AGENTS.md).
IMAGES = [("Assembly-CSharp-firstpass", 7411, 7477), ("Assembly-CSharp", 7477, 10**9)]

# Namespaces of bundled middleware; everything else in the two assemblies is the game's own.
MIDDLEWARE = {
    "Rewired": "Rewired (input)",
    "I2": "I2 Localization",
    "CielaSpike": "Thread Ninja",
    "SFB": "StandaloneFileBrowser",
    "TMPro": "TextMeshPro",
    "Steamworks": "Steamworks.NET",
    "GooglePlayGames": "Google Play Games",
    "Crosstales": "Crosstales FileBrowser",
}

TYPE_RE = re.compile(
    r"^(?P<mods>(?:\[[^\]]*\]\s*)*(?:(?:public|private|internal|protected|sealed|abstract|static|readonly)\s+)*)"
    r"(?P<kind>class|struct|enum|interface)\s+(?P<name>[^\s:]+)(?:\s*:\s*(?P<bases>[^/]+?))?\s*"
    r"// TypeDefIndex: (?P<idx>\d+)"
)
RVA_RE = re.compile(r"// RVA: (0x[0-9A-F]+) Offset: 0x[0-9A-F]+ VA: (0x[0-9A-F]+)")
FIELD_RE = re.compile(r"^\t(?!//)[^()]*;\s*(//.*)?$")


def image_of(idx: int) -> str | None:
    for name, lo, hi in IMAGES:
        if lo <= idx < hi:
            return name
    return None


def owner_of(namespace: str) -> str:
    top = namespace.split(".")[0] if namespace else ""
    return MIDDLEWARE.get(top, "game")


def method_name(sig: str) -> str:
    head = sig.split("(")[0].split()
    return head[-1] if head else "?"


def parse(dump: Path):
    types, methods = [], []
    namespace, cur, pending_rva = "", None, None
    for line in dump.read_text(encoding="utf-8", errors="replace").splitlines():
        if line.startswith("// Namespace: "):
            namespace = line[len("// Namespace: "):].strip()
            continue
        m = TYPE_RE.match(line)
        if m:
            idx = int(m["idx"])
            image = image_of(idx)
            cur = None
            if image:
                cur = {
                    "typedef": idx,
                    "assembly": image,
                    "namespace": namespace,
                    "type": m["name"],
                    "kind": m["kind"],
                    "base": (m["bases"] or "").strip(),
                    "fields": 0,
                    "methods": 0,
                    "owner": owner_of(namespace),
                }
                types.append(cur)
            pending_rva = None
            continue
        if cur is None:
            continue
        if line == "}":
            cur, pending_rva = None, None
            continue
        r = RVA_RE.search(line)
        if r and line.startswith("\t// RVA"):
            pending_rva = (r.group(1), r.group(2))
            continue
        if line.startswith("\t// RVA: -1"):
            pending_rva = ("", "")
            continue
        stripped = line.strip()
        if pending_rva is not None and stripped and not stripped.startswith(("//", "/*", "[")):
            cur["methods"] += 1
            if pending_rva[0]:
                methods.append({
                    "_t": cur,
                    "rva": pending_rva[0],
                    "va": pending_rva[1],
                    "namespace": cur["namespace"],
                    "type": cur["type"],
                    "method": method_name(stripped),
                    "signature": stripped.rstrip(" {}").strip(),
                    "owner": cur["owner"],
                })
            pending_rva = None
            continue
        if FIELD_RE.match(line) and "(" not in stripped and not stripped.startswith("["):
            cur["fields"] += 1
    # dump.cs prints nested types (Outer.Inner) with an empty namespace: take the outer type's.
    outer_ns = {(t["assembly"], t["type"]): t["namespace"] for t in types if "." not in t["type"]}
    for t in types:
        if not t["namespace"] and "." in t["type"]:
            ns = outer_ns.get((t["assembly"], t["type"].split(".")[0]), "")
            t["namespace"], t["owner"] = ns, owner_of(ns)
    for m in methods:
        t = m.pop("_t")
        m["namespace"], m["owner"], m["assembly"] = t["namespace"], t["owner"], t["assembly"]
    return types, methods


def main() -> None:
    dump = Path(sys.argv[1]) if len(sys.argv) > 1 else ROOT / "work" / "il2cppdumper" / "dump.cs"
    types, methods = parse(dump)
    DOCS.mkdir(exist_ok=True)

    with (DOCS / "type-index.csv").open("w", newline="", encoding="utf-8") as f:
        w = csv.DictWriter(f, fieldnames=list(types[0].keys()), lineterminator="\n")
        w.writeheader()
        w.writerows(types)

    fn_path = DOCS / "function-index.csv"
    old = {}
    if fn_path.exists():
        with fn_path.open(encoding="utf-8") as f:
            old = {(r["rva"], r["type"], r["method"]): r for r in csv.DictReader(f)}

    seen, game = set(), []
    for m in methods:
        key = (m["rva"], m["type"], m["method"])
        if m["owner"] == "game" and key not in seen:
            seen.add(key)
            game.append(m)
    game.sort(key=lambda m: (m["namespace"], m["type"], int(m["rva"], 16)))
    cols = ["rva", "va", "namespace", "type", "method", "signature", "status", "notes"]
    with fn_path.open("w", newline="", encoding="utf-8") as f:
        w = csv.DictWriter(f, fieldnames=cols, lineterminator="\n")
        w.writeheader()
        for m in game:
            prev = old.get((m["rva"], m["type"], m["method"]), {})
            w.writerow({
                **{k: m[k] for k in cols[:6]},
                "status": prev.get("status") or "dumped",
                "notes": prev.get("notes", ""),
            })

    by_owner = {}
    for t in types:
        by_owner.setdefault(t["owner"], [0, 0])
        by_owner[t["owner"]][0] += 1
        by_owner[t["owner"]][1] += t["methods"]
    for owner, (nt, nm) in sorted(by_owner.items(), key=lambda kv: -kv[1][0]):
        print(f"{owner:24} {nt:5} types {nm:6} methods")
    print(f"function-index.csv: {len(game)} game methods with bodies")


if __name__ == "__main__":
    main()
