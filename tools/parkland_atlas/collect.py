"""Parkland Atlas collector — driver.

Pipeline per candidate:
  1. reuse cache if present (resumable; the reference 27 are pre-seeded)
  2. Nominatim geocode -> center
  3. OSM holes (single routing via tee-to-green continuity)
  4. terrarium DEM + ESA WorldCover (+ OSM hydrography) + profiles
  5. PARKLAND GATE: keep only if >= MIN_HOLES mapped and canopy >= MIN_TREEPCT
  6. cache the record

Then emit the HTML viewer + a merged JSON of all kept courses.

    python3 collect.py --limit 30          # validation batch
    python3 collect.py                      # full run (all candidates)
    python3 collect.py --emit-only          # just rebuild HTML/JSON from cache

Everything is resumable: re-running never re-fetches a cached course, and
failures are logged (never abort the batch).
"""

import argparse
import json
import os
import sys
import time

import requests

import courses as course_list
import sources
import viewer

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "out")
CACHE = os.path.join(OUT, "cache")
FAILLOG = os.path.join(OUT, "failures.log")
FAILSET = os.path.join(OUT, "failed.json")  # keys to skip on re-run (fast convergence)

MIN_HOLES = 12          # a real routing (allows a few unmapped holes)
MIN_TREEPCT = 12.0      # parkland canopy gate (drops links/desert mistags)
MAX_RELIEF_M = 260.0    # inland parkland ceiling (drops coastal-cliff / DEM-blowup surveys)
MAX_WATERPCT = 28.0     # drops surveys whose window is dominated by ocean/large open water
NOMINATIM = "https://nominatim.openstreetmap.org/search"
UA = sources.UA


def _geocode_query(query):
    """One Nominatim lookup with backoff; None on miss/rate-limit."""
    for attempt in range(3):
        try:
            r = requests.get(
                NOMINATIM,
                params={"q": query, "format": "json", "limit": 1,
                        "email": "davisolmsted@gmail.com"},
                headers=UA, timeout=30)
            if r.status_code == 200:
                j = r.json()
                if j:
                    return float(j[0]["lat"]), float(j[0]["lon"])
                return None  # 200 + empty = genuine miss, don't retry
            # rate-limited / server error: back off and retry
        except (requests.RequestException, ValueError, KeyError):
            pass
        time.sleep(1.5 * (attempt + 1))
    return None


def geocode(query):
    """Try the full query, then progressively looser variants (drop the
    leading club name's suffix, then just 'name, country')."""
    variants = [query]
    parts = [p.strip() for p in query.split(",")]
    if len(parts) >= 3:
        # 'Name, City, Region, Country' -> 'Name, City, Country' -> 'Name, Country'
        variants.append(f"{parts[0]}, {parts[1]}, {parts[-1]}")
        variants.append(f"{parts[0]}, {parts[-1]}")
    # NB: no bare-name variant — it lands on unrelated POIs; the fetch_holes
    # name-match guard is the backstop, but we keep geocode queries specific.
    seen = set()
    for v in variants:
        if v in seen:
            continue
        seen.add(v)
        hit = _geocode_query(v)
        time.sleep(1.3)  # Nominatim courtesy (max ~1 req/s)
        if hit is not None:
            return hit
    return None


def load_failed():
    try:
        with open(FAILSET) as f:
            return set(json.load(f))
    except (FileNotFoundError, ValueError):
        return set()


_FAILED = load_failed()


def log_fail(key, why):
    with open(FAILLOG, "a") as f:
        f.write(f"{key}\t{why}\n")
    _FAILED.add(key)
    with open(FAILSET, "w") as f:
        json.dump(sorted(_FAILED), f)
    print(f"  FAIL {key}: {why}")


def collect_one(key, label, query, arch):
    """Return the course record (from cache or freshly collected), or None."""
    if key in course_list.EXCLUDE:
        return None  # user-removed / duplicate — never collect
    cache_path = os.path.join(CACHE, f"{key}.json")
    if os.path.exists(cache_path):
        with open(cache_path) as f:
            return json.load(f)
    if key in _FAILED:
        return None  # known miss — skip fast on re-runs (delete out/failed.json to retry)

    center = geocode(query)  # geocode() already paces itself
    if center is None:
        log_fail(key, "geocode failed")
        return None
    try:
        holes = sources.fetch_holes(label, center)
    except sources.FetchError as e:
        log_fail(key, f"holes: {e}")
        return None
    if len(holes) < MIN_HOLES:
        log_fail(key, f"only {len(holes)} holes mapped")
        return None
    bbox = sources.bbox_from_holes(holes)
    try:
        rec = sources.build_course(key, label, arch, bbox, holes)
    except sources.FetchError as e:
        log_fail(key, f"layers: {e}")
        return None
    if rec["treepct"] < MIN_TREEPCT:
        log_fail(key, f"canopy {rec['treepct']}% < {MIN_TREEPCT}% (not parkland)")
        return None
    relief = rec["emax"] - rec["emin"]
    if relief > MAX_RELIEF_M:
        # Real parkland tops out well under this; larger = a coastal/ocean
        # survey window or a DEM blowup. (The pre-cached reference courses,
        # e.g. Banff, are never re-collected, so this never drops them.)
        log_fail(key, f"relief {relief:.0f} m > {MAX_RELIEF_M:.0f} (coastal/DEM error)")
        return None
    if rec["waterpct"] > MAX_WATERPCT:
        log_fail(key, f"water {rec['waterpct']}% > {MAX_WATERPCT}% (likely ocean in window)")
        return None
    with open(cache_path, "w") as f:
        json.dump(rec, f)
    print(f"  ok   {key:20s} {rec['group']:8s} relief {rec['emax']-rec['emin']:5.1f}m "
          f"canopy {rec['treepct']:4.1f}% water {rec['waterpct']:4.1f}% holes {len(holes)}")
    time.sleep(2.0)  # Overpass courtesy between courses
    return rec


def purge_excluded():
    """Delete cached entries the user removed / deduped, so they never re-emit."""
    n = 0
    for key in course_list.EXCLUDE:
        p = os.path.join(CACHE, f"{key}.json")
        if os.path.exists(p):
            os.remove(p)
            n += 1
    if n:
        print(f"purged {n} excluded courses from cache")


def load_all_cached():
    out = {}
    for fn in sorted(os.listdir(CACHE)):
        if fn.endswith(".json"):
            key = fn[:-5]
            if key in course_list.EXCLUDE:
                continue
            with open(os.path.join(CACHE, fn)) as f:
                c = json.load(f)
            out[c.get("key", key)] = c
    return out


def emit(kept):
    os.makedirs(OUT, exist_ok=True)
    html_path = os.path.join(OUT, "parkland_atlas.html")
    viewer.emit_html(kept, html_path)
    json_path = os.path.join(OUT, "parkland_atlas.json")
    with open(json_path, "w") as f:
        json.dump(kept, f, separators=(",", ":"))
    groups = {}
    for c in kept.values():
        groups[c.get("group", "?")] = groups.get(c.get("group", "?"), 0) + 1
    print(f"\nemitted {len(kept)} courses  {dict(sorted(groups.items()))}")
    print(f"  {html_path}\n  {json_path}")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--limit", type=int, default=0, help="max NEW candidates to fetch (0 = all)")
    ap.add_argument("--emit-only", action="store_true", help="rebuild HTML/JSON from cache only")
    args = ap.parse_args()
    os.makedirs(CACHE, exist_ok=True)
    purge_excluded()

    if args.emit_only:
        emit(load_all_cached())
        return

    kept = {}
    # reference courses first (already cached; never re-fetched)
    for key in course_list.REFERENCE:
        if key in course_list.EXCLUDE:
            continue
        p = os.path.join(CACHE, f"{key}.json")
        if os.path.exists(p):
            with open(p) as f:
                kept[key] = json.load(f)
        else:
            print(f"  (reference {key} not in cache — run seed_reference.py)")

    fetched = 0
    for key, label, query, arch in course_list.CANDIDATES:
        if key in course_list.EXCLUDE:
            continue
        if args.limit and fetched >= args.limit and not os.path.exists(os.path.join(CACHE, f"{key}.json")):
            continue
        pre_cached = os.path.exists(os.path.join(CACHE, f"{key}.json"))
        rec = collect_one(key, label, query, arch)
        if rec is not None:
            kept[key] = rec
            if not pre_cached:
                fetched += 1

    emit(kept)


if __name__ == "__main__":
    main()
