"""Derive the US course subset from the parkland-atlas list + cache.

Rule (verified 2026-07-16 against the 200-course cache):
- candidates: cached keys whose courses.py geocode string's last comma token
  is exactly "USA" (token test — substring tests trip on e.g. "Western
  Australia") -> 117 keys.
- reference courses carry no geocode string; membership is curated below and
  asserted to exactly partition the cached reference keys -> 18 US keys.

Total: 135. Runtime code reads only the committed courses_us.txt; this module
regenerates it (`python3 -m dtm_atlas courses --write`) and diffs otherwise.
"""

from __future__ import annotations

import os
import sys

from . import config

REFERENCE_US = {
    "augusta", "bayhill", "bethpage", "brookline", "cherryhills", "colonial",
    "congressional", "doral", "eastlake", "firestone", "greenbrier", "oakmont",
    "olympia", "quail", "riviera", "sawgrass", "valhalla", "wingedfoot",
}
REFERENCE_NON_US = {
    "capilano", "chapultepec", "crans", "golfnational", "jasper",
    "valderrama", "wentworth",
}


def _load_parkland_courses():
    sys.path.insert(0, config.PARKLAND)
    try:
        import courses  # type: ignore
        return courses.CANDIDATES
    finally:
        sys.path.remove(config.PARKLAND)


def cached_keys() -> set[str]:
    cache = os.path.join(config.PARKLAND, "out", "cache")
    if not os.path.isdir(cache):
        raise SystemExit(
            f"parkland cache not found at {cache} — run the parkland_atlas "
            "collector first (its out/ is gitignored, so a fresh clone must "
            "re-collect or copy it)."
        )
    return {f[:-5] for f in os.listdir(cache) if f.endswith(".json")}


def derive() -> list[str]:
    cached = cached_keys()
    cand_geo = {k: q for (k, _l, q, _a) in _load_parkland_courses()}
    us = {k for k in cached
          if k in cand_geo and cand_geo[k].split(",")[-1].strip() == "USA"}
    ref_cached = {k for k in cached if k not in cand_geo}
    known = REFERENCE_US | REFERENCE_NON_US
    unknown = ref_cached - known
    if unknown:
        raise SystemExit(
            f"reference keys not classified US/non-US: {sorted(unknown)} — "
            "update uscourses.REFERENCE_US / REFERENCE_NON_US."
        )
    us |= ref_cached & REFERENCE_US
    return sorted(us)


def read_list() -> list[str]:
    with open(config.COURSES_US) as f:
        return [ln.strip() for ln in f
                if ln.strip() and not ln.lstrip().startswith("#")]


def run(write: bool) -> int:
    keys = derive()
    header = (
        "# US course subset for the dtm_atlas store (Stage 0, US-3DEP-first).\n"
        "# Derived from tools/parkland_atlas: cached keys whose geocode string's\n"
        "# last comma token == \"USA\", plus the curated US reference keys\n"
        "# (see dtm_atlas/uscourses.py). Regenerate: python3 -m dtm_atlas courses --write\n"
    )
    body = header + "\n".join(keys) + "\n"
    if write:
        with open(config.COURSES_US, "w") as f:
            f.write(body)
        print(f"wrote {config.COURSES_US} ({len(keys)} courses)")
        return 0
    if not os.path.exists(config.COURSES_US):
        print("courses_us.txt missing — run with --write")
        return 1
    current = open(config.COURSES_US).read()
    if current == body:
        print(f"courses_us.txt up to date ({len(keys)} courses)")
        return 0
    print("courses_us.txt DIFFERS from derivation — rerun with --write")
    return 1
