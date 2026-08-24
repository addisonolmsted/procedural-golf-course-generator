#!/usr/bin/env python3
"""Fetch REFERENCE tiles along the Sandhills' allogenic river corridors.

These are river-catena reference tiles, NOT corpus tiles: they are kept under
their own key (`sandhills_river`), never added to V2_BIOMES, review_v2 or the
texture packs, so they cannot move any published band. Purpose: the reviewer
judges which tiles truly carry a river, and the confirmed set shapes the creek
cross-section (every current river dial is `guess` provenance -- the clean-tile
screen deliberately selected upland away from these corridors, so the kept
corpus cannot answer).

Reaches: Dismal (through and west of Bessey NF), Middle Loup (Mullen-Halsey),
Snake (McKelvie NF), North Loup.
"""
import pathlib, sys
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent.parent / "macro_campaign"))
from macro_campaign import fetch, regions

CENTERS = [
    # Dismal River
    (41.745, -101.20), (41.760, -101.00), (41.788, -100.75), (41.790, -100.60),
    (41.808, -100.45), (41.830, -100.25), (41.840, -100.14),
    # Middle Loup
    (42.044, -101.04), (42.010, -100.85), (41.995, -100.70), (41.970, -100.45),
    (41.900, -100.31),
    # Snake River
    (42.580, -101.05), (42.630, -101.20), (42.660, -100.95),
    # North Loup
    (42.080, -100.20), (42.000, -100.05), (41.930, -99.90),
]

def main():
    out = pathlib.Path(fetch.__file__).resolve().parent.parent / "out" / "tiles" / "sandhills_river"
    out.mkdir(parents=True, exist_ok=True)
    ok = 0
    for lat, lon in CENTERS:
        tid = regions.tile_id(lat, lon)
        if (out / f"{tid}.cgrid").exists() and (out / f"{tid}.json").exists():
            print(f"  [skip] {tid}")
            ok += 1
            continue
        try:
            # dest is the FILE path; the sidecar is written by us, as run() does
            r = fetch.fetch_tile(lat, lon, 26914, 14, out / f"{tid}.cgrid")
            import json
            (out / f"{tid}.json").write_text(json.dumps(r, indent=1, sort_keys=True))
            print(f"  [ok] {tid}  valid {r['valid_frac']*100:.0f}%")
            ok += 1
        except Exception as e:
            print(f"  [FAIL] {tid}: {e}")
        import time; time.sleep(1.0)
    print(f"{ok}/{len(CENTERS)}")

if __name__ == "__main__":
    main()
