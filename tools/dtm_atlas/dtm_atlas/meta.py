"""meta.json writing (deterministic: sorted keys, fixed rounding, trailing
newline, no timestamps — fetch-time facts live in fetch_cache provenance.json)
and the store MANIFEST.sha256 verify machinery (the idempotency gate)."""

from __future__ import annotations

import hashlib
import json
import os

from . import config


def write_meta(path: str, obj: dict) -> None:
    body = json.dumps(obj, sort_keys=True, indent=1, allow_nan=False) + "\n"
    tmp = path + ".tmp"
    with open(tmp, "w") as f:
        f.write(body)
    os.replace(tmp, path)


def read_meta(path: str) -> dict:
    with open(path) as f:
        return json.load(f)


def _iter_store_files():
    for root, _dirs, files in os.walk(config.STORE):
        for fn in files:
            if fn == "MANIFEST.sha256":
                continue
            yield os.path.join(root, fn)


def manifest_lines() -> list[str]:
    lines = []
    for path in sorted(_iter_store_files()):
        h = hashlib.sha256()
        with open(path, "rb") as f:
            for chunk in iter(lambda: f.read(1 << 20), b""):
                h.update(chunk)
        rel = os.path.relpath(path, config.STORE)
        lines.append(f"{h.hexdigest()}  {rel}")
    return lines


def verify(write: bool = False) -> int:
    mpath = os.path.join(config.STORE, "MANIFEST.sha256")
    lines = manifest_lines()
    if write:
        with open(mpath, "w") as f:
            f.write("\n".join(lines) + "\n")
        total = hashlib.sha256("\n".join(lines).encode()).hexdigest()[:16]
        print(f"wrote MANIFEST.sha256 ({len(lines)} files, store hash {total})")
        return 0
    if not os.path.exists(mpath):
        print("MANIFEST.sha256 missing — run verify --write first")
        return 1
    want = open(mpath).read().splitlines()
    if want == lines:
        total = hashlib.sha256("\n".join(lines).encode()).hexdigest()[:16]
        print(f"store matches manifest ({len(lines)} files, store hash {total})")
        return 0
    got, exp = set(lines), set(want)
    for miss in sorted(exp - got)[:5]:
        print(f"CHANGED/MISSING: {miss.split('  ', 1)[1]}")
    for new in sorted(got - exp)[:5]:
        print(f"NEW/CHANGED: {new.split('  ', 1)[1]}")
    print(f"store DRIFTED from manifest ({len(exp - got)} missing/changed, "
          f"{len(got - exp)} new/changed)")
    return 1
