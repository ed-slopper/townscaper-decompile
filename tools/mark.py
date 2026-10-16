"""Set the status (and notes) of rows in docs/function-index.csv (AGENTS.md section 13).

    python -I tools/mark.py checked "tests/os_utils.rs" Os.Utils.Bitwise Os.Utils.ByteFloat3.get_normal
    python -I tools/mark.py ported  "no harness test: boxed object" Os.Utils.ByteFloat.Equals

A target is `Namespace.Type` (every method of the type) or `Namespace.Type.Method` (every overload).
Prints how many rows changed; fails if a target matches nothing.
"""
import csv
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
STATUSES = ("dumped", "read", "ported", "checked")


def main() -> None:
    status, note, targets = sys.argv[1], sys.argv[2], sys.argv[3:]
    if status not in STATUSES:
        sys.exit(f"status must be one of {STATUSES}")
    path = ROOT / "docs" / "function-index.csv"
    with path.open(encoding="utf-8") as f:
        reader = csv.DictReader(f)
        fields = reader.fieldnames
        rows = list(reader)
    hits = {t: 0 for t in targets}
    for r in rows:
        full = f"{r['namespace']}.{r['type']}" if r["namespace"] else r["type"]
        for t in targets:
            if t == full or t == f"{full}.{r['method']}":
                r["status"] = status
                r["notes"] = note
                hits[t] += 1
    missing = [t for t, n in hits.items() if n == 0]
    if missing:
        sys.exit(f"no rows for: {missing}")
    with path.open("w", newline="", encoding="utf-8") as f:
        w = csv.DictWriter(f, fieldnames=fields, lineterminator="\n")
        w.writeheader()
        w.writerows(rows)
    print(f"{sum(hits.values())} rows -> {status}")


if __name__ == "__main__":
    main()
