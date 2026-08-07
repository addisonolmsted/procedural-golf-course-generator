"""Per-course data acquisition for the Parkland Atlas collector.

Reproduces the reference methodology:
  * holes       — OpenStreetMap ``golf=hole`` ways (Overpass)
  * elevation   — AWS terrarium DEM tiles, zoom 15
  * tree/water  — ESA WorldCover 2021 v200 (class 10 = tree, 80 = water),
                  water unioned with OSM hydrography polygons

Network calls throttle + retry politely; the DRIVER owns per-course caching.
"""

import io
import math
import os
import time

import numpy as np
import requests
from PIL import Image, ImageDraw

from common import (N, decode_mask, encode_elev, encode_mask, hole_length_m,
                    lat_lon_metres, lonlat_to_tile_frac, point_along, relief_group)

UA = {"User-Agent": "ParklandAtlas/1.0 (procedural-golf terrain research; davisolmsted@gmail.com)"}
OVERPASS_EPS = [
    "https://overpass-api.de/api/interpreter",
    "https://overpass.kumi.systems/api/interpreter",
    "https://maps.mail.ru/osm/tools/overpass/api/interpreter",
]
TERRARIUM = "https://s3.amazonaws.com/elevation-tiles-prod/terrarium/{z}/{x}/{y}.png"
WORLDCOVER = ("/vsicurl/https://esa-worldcover.s3.eu-central-1.amazonaws.com"
              "/v200/2021/map/ESA_WorldCover_10m_2021_v200_{name}_Map.tif")
DEM_ZOOM = 15


class FetchError(RuntimeError):
    pass


# ---------------------------------------------------------------------------
# Overpass
# ---------------------------------------------------------------------------

def overpass(query, tries=4):
    last = None
    for attempt in range(tries):
        ep = OVERPASS_EPS[attempt % len(OVERPASS_EPS)]
        try:
            r = requests.post(ep, data={"data": query}, headers=UA, timeout=90)
            if r.status_code == 200:
                return r.json()
            last = f"{r.status_code} @ {ep}"
        except requests.RequestException as e:
            last = f"{e} @ {ep}"
        time.sleep(3.0 * (attempt + 1))  # backoff
    raise FetchError(f"overpass failed: {last}")


# Minimum name-token Jaccard for a golf_course to be accepted as the candidate.
NAME_MATCH_MIN = 0.34

_STOP = {"the", "golf", "club", "course", "country", "at", "and", "of", "links",
         "national", "resort", "gc", "cc", "&", "de", "la", "le", "el", "no"}


def _norm_tokens(name):
    keep = []
    for tok in "".join(c if c.isalnum() else " " for c in (name or "").lower()).split():
        if tok not in _STOP:
            keep.append(tok)
    return set(keep)


def _name_score(cand, osm):
    """Token overlap of two course names, 0..1 (Jaccard over significant tokens)."""
    a, b = _norm_tokens(cand), _norm_tokens(osm)
    if not a or not b:
        return 0.0
    return len(a & b) / len(a | b)


def fetch_holes(name, center, search_half_m=1700.0):
    """The holes of the ONE named course at ``center`` (lat, lon).

    Multi-course clubs are separated by matching ``name`` against the
    ``leisure=golf_course`` elements near the center, then asking Overpass for
    the holes INSIDE that element's area (``map_to_area``). Falls back to the
    nearest course, then to a bbox query, so sparsely-tagged clubs still resolve.
    """
    from common import DEG_M
    clat, clon = center
    hlat = search_half_m / DEG_M
    hlon = search_half_m / (DEG_M * math.cos(math.radians(clat)))
    bbox = (clat - hlat, clon - hlon, clat + hlat, clon + hlon)
    la0, lo0, la1, lo1 = bbox

    # 1) enumerate candidate golf_course elements near the center
    q1 = (f"[out:json][timeout:60];("
          f"way[\"leisure\"=\"golf_course\"]({la0},{lo0},{la1},{lo1});"
          f"rel[\"leisure\"=\"golf_course\"]({la0},{lo0},{la1},{lo1});"
          f");out tags center;")
    cands = []
    for el in overpass(q1).get("elements", []):
        c = el.get("center") or {"lat": clat, "lon": clon}
        nm = el.get("tags", {}).get("name", "")
        dist2 = (c["lat"] - clat) ** 2 + (c["lon"] - clon) ** 2
        cands.append((el["type"], el["id"], nm, _name_score(name, nm), dist2))
    # best by name score, then nearest
    cands.sort(key=lambda t: (-t[3], t[4]))

    # NAME-MATCH GUARD: only collect when a golf_course near the geocoded
    # center actually matches the candidate name. A wrong geocode (Nominatim
    # occasionally returns an unrelated POI) then yields no course match and
    # the candidate is skipped — never collected as the wrong course's data.
    if not cands or cands[0][3] < NAME_MATCH_MIN:
        return []

    etype, eid = cands[0][0], cands[0][1]
    area_id = (3600000000 + eid) if etype == "relation" else (2400000000 + eid)
    q2 = (f"[out:json][timeout:60];way[\"golf\"=\"hole\"]"
          f"(area:{area_id});out geom tags;")
    try:
        holes = _parse_hole_ways(overpass(q2))
    except FetchError:
        holes = []

    # fallback: bbox holes around the center (some clubs' holes fall outside
    # the mapped area polygon). Safe here — we've already name-confirmed the
    # course is at this location — and select_route keeps the routing nearest
    # the center.
    if len(holes) < 9:
        q3 = (f"[out:json][timeout:60];way[\"golf\"=\"hole\"]"
              f"({la0},{lo0},{la1},{lo1});out geom tags;")
        try:
            raw = _parse_hole_ways(overpass(q3))
            if len(raw) >= len(holes):
                holes = raw
        except FetchError:
            pass

    # multi-course clubs return every course's holes — keep the one routing
    holes = select_route(holes, center)

    holes.sort(key=lambda h: (h["ref"] is None, h["ref"] if h["ref"] is not None else 0))
    for i, h in enumerate(holes, 1):
        if h["ref"] is None:
            h["ref"] = i
    return holes


def _mdist(a, b):
    """Metres between two [lat,lon] points (equirectangular)."""
    latc = math.radians(0.5 * (a[0] + b[0]))
    dy = (a[0] - b[0]) * DEG_M
    dx = (a[1] - b[1]) * DEG_M * math.cos(latc)
    return math.hypot(dx, dy)


def select_route(holes, center):
    """Extract ONE ~18-hole routing from a possibly multi-course hole set.

    Multi-course clubs (Medinah's three, Oak Hill's two) map as a single
    ``golf_course`` area, so the hole query returns every course's holes. We
    rebuild the intended routing by tee-to-green continuity (the reference's
    method): seed at the ref-1 tee nearest the geocoded course center, then
    greedily chain the next hole whose tee is closest to the current green,
    biased toward the incrementing ``ref``.
    """
    if len(holes) <= 18:
        return holes
    clat, clon = center
    items = [{"h": h, "tee": h["pts"][0], "grn": h["pts"][-1],
              "ref": h["ref"]} for h in holes]

    # seed: the ref==1 hole (else any) whose tee is nearest the center
    seeds = [it for it in items if it["ref"] == 1] or items
    seed = min(seeds, key=lambda it: _mdist((clat, clon), it["tee"]))

    used = {id(seed)}
    chain = [seed]
    cur = seed
    # forward chain
    while len(chain) < 18:
        cand = [it for it in items if id(it) not in used]
        if not cand:
            break
        def cost(it):
            # nearest tee to the current green, with a bonus for ref = cur+1
            gap = _mdist(cur["grn"], it["tee"])
            ref_bonus = 0.0 if (cur["ref"] and it["ref"] == cur["ref"] + 1) else 140.0
            return gap + ref_bonus
        nxt = min(cand, key=cost)
        if _mdist(cur["grn"], nxt["tee"]) > 400.0 and \
           not (cur["ref"] and nxt["ref"] == cur["ref"] + 1):
            break  # no plausible continuation
        used.add(id(nxt))
        chain.append(nxt)
        cur = nxt

    ordered = sorted((it["h"] for it in chain),
                     key=lambda h: (h["ref"] is None, h["ref"] or 0))
    return ordered


DEG_M = 111320.0


def _parse_hole_ways(data):
    holes = []
    for el in data.get("elements", []):
        if el.get("type") != "way" or "geometry" not in el:
            continue
        tags = el.get("tags", {})
        if tags.get("golf") != "hole":
            continue
        pts = [[g["lat"], g["lon"]] for g in el["geometry"]]
        if len(pts) < 2:
            continue
        ref, par = tags.get("ref"), tags.get("par")
        holes.append({
            "ref": int(ref) if _int_ok(ref) else None,
            "par": int(par) if _int_ok(par) else None,
            "name": tags.get("name"), "pts": pts})
    return holes


def _int_ok(v):
    try:
        int(v)
        return True
    except (TypeError, ValueError):
        return False


def fetch_osm_water_mask(bbox):
    """natural=water / waterway=riverbank polygons -> 256x256 {0,1} mask."""
    la0, lo0, la1, lo1 = bbox
    q = (f"[out:json][timeout:60];("
         f"way[\"natural\"=\"water\"]({la0},{lo0},{la1},{lo1});"
         f"way[\"waterway\"=\"riverbank\"]({la0},{lo0},{la1},{lo1});"
         f"way[\"landuse\"=\"reservoir\"]({la0},{lo0},{la1},{lo1});"
         f");out geom;")
    try:
        data = overpass(q)
    except FetchError:
        return np.zeros((N, N), np.uint8)
    img = Image.new("1", (N, N), 0)
    drw = ImageDraw.Draw(img)
    dlat = (la1 - la0) or 1e-9
    dlon = (lo1 - lo0) or 1e-9
    for el in data.get("elements", []):
        geom = el.get("geometry")
        if not geom or len(geom) < 3:
            continue
        poly = [((g["lon"] - lo0) / dlon * N, (la1 - g["lat"]) / dlat * N) for g in geom]
        drw.polygon(poly, fill=1)
    return np.array(img, dtype=np.uint8)


# ---------------------------------------------------------------------------
# Elevation (terrarium tiles)
# ---------------------------------------------------------------------------

def _tile_png(z, x, y, tries=4):
    url = TERRARIUM.format(z=z, x=x, y=y)
    last = None
    for attempt in range(tries):
        try:
            r = requests.get(url, headers=UA, timeout=60)
            if r.status_code == 200:
                return np.asarray(Image.open(io.BytesIO(r.content)).convert("RGB"),
                                  dtype=np.float64)
            last = r.status_code
        except requests.RequestException as e:
            last = e
        time.sleep(1.5 * (attempt + 1))
    raise FetchError(f"terrarium {z}/{x}/{y} failed: {last}")


def fetch_dem(bbox):
    """256x256 elevation grid (metres), ROW 0 = NORTH, from terrarium tiles."""
    la0, lo0, la1, lo1 = bbox
    z = DEM_ZOOM
    # tile range covering the bbox corners
    xs, ys = [], []
    for lat in (la0, la1):
        for lon in (lo0, lo1):
            xf, yf = lonlat_to_tile_frac(lon, lat, z)
            xs.append(xf)
            ys.append(yf)
    x0, x1 = math.floor(min(xs)), math.floor(max(xs))
    y0, y1 = math.floor(min(ys)), math.floor(max(ys))
    tw, th = (x1 - x0 + 1), (y1 - y0 + 1)
    stitched = np.zeros((th * 256, tw * 256), np.float64)
    for tx in range(x0, x1 + 1):
        for ty in range(y0, y1 + 1):
            px = _tile_png(z, tx, ty)
            e = px[:, :, 0] * 256.0 + px[:, :, 1] + px[:, :, 2] / 256.0 - 32768.0
            stitched[(ty - y0) * 256:(ty - y0 + 1) * 256,
                     (tx - x0) * 256:(tx - x0 + 1) * 256] = e
    # Nodata / decode spikes (coastal tiles, partial fetches) read as absurd
    # elevations — replace them with the median of the valid pixels so a single
    # bad cell can't fabricate kilometres of relief. Beyond this the whole
    # survey is untrustworthy, so bail.
    valid = (stitched > -500.0) & (stitched < 9000.0)
    if not valid.any():
        raise FetchError("DEM: no valid elevation pixels")
    if not valid.all():
        med = float(np.median(stitched[valid]))
        if valid.mean() < 0.85:
            raise FetchError(f"DEM: {(1-valid.mean())*100:.0f}% nodata — untrustworthy")
        stitched[~valid] = med
    # sample the 256x256 output grid (row 0 = north)
    gy = (np.arange(N) + 0.5) / N
    gx = (np.arange(N) + 0.5) / N
    lats = la1 - gy * (la1 - la0)          # north -> south
    lons = lo0 + gx * (lo1 - lo0)          # west -> east
    LON, LAT = np.meshgrid(lons, lats)
    # fractional tile coords -> pixel in the stitched array
    n = 2.0 ** z
    XF = (LON + 180.0) / 360.0 * n
    latr = np.radians(LAT)
    YF = (1.0 - np.log(np.tan(latr) + 1.0 / np.cos(latr)) / np.pi) / 2.0 * n
    PX = (XF - x0) * 256.0
    PY = (YF - y0) * 256.0
    return _bilinear(stitched, PX, PY)


def _bilinear(arr, px, py):
    h, w = arr.shape
    x0 = np.clip(np.floor(px).astype(int), 0, w - 2)
    y0 = np.clip(np.floor(py).astype(int), 0, h - 2)
    fx = np.clip(px - x0, 0.0, 1.0)
    fy = np.clip(py - y0, 0.0, 1.0)
    a = arr[y0, x0]
    b = arr[y0, x0 + 1]
    c = arr[y0 + 1, x0]
    d = arr[y0 + 1, x0 + 1]
    return (a * (1 - fx) * (1 - fy) + b * fx * (1 - fy)
            + c * (1 - fx) * fy + d * fx * fy)


# ---------------------------------------------------------------------------
# WorldCover (tree + water classes)
# ---------------------------------------------------------------------------

def _worldcover_tile_name(lat, lon):
    lat_sw = math.floor(lat / 3.0) * 3
    lon_sw = math.floor(lon / 3.0) * 3
    return (f"{'N' if lat_sw >= 0 else 'S'}{abs(lat_sw):02d}"
            f"{'E' if lon_sw >= 0 else 'W'}{abs(lon_sw):03d}")


def fetch_worldcover(bbox):
    """256x256 tree mask and water mask from ESA WorldCover COGs.

    Returns (tree, water) uint8 arrays, ROW 0 = NORTH. A course bbox can straddle
    two 3-degree tiles; each covering tile is windowed and merged.
    """
    import rasterio
    from rasterio.enums import Resampling
    from rasterio.windows import from_bounds
    os.environ.setdefault("GDAL_DISABLE_READDIR_ON_OPEN", "EMPTY_DIR")
    os.environ.setdefault("CPL_VSIL_CURL_ALLOWED_EXTENSIONS", ".tif")

    la0, lo0, la1, lo1 = bbox
    names = set()
    for lat in (la0, la1):
        for lon in (lo0, lo1):
            names.add(_worldcover_tile_name(lat, lon))
    cls = np.zeros((N, N), np.uint8)
    got = False
    for name in names:
        url = WORLDCOVER.format(name=name)
        try:
            with rasterio.open(url) as ds:
                win = from_bounds(lo0, la0, lo1, la1, ds.transform)
                arr = ds.read(1, window=win, out_shape=(N, N),
                              resampling=Resampling.nearest, boundless=True, fill_value=0)
        except Exception:
            continue
        # merge: take classified pixels where we don't have one yet
        take = (cls == 0) & (arr != 0)
        cls[take] = arr[take]
        got = True
    if not got:
        raise FetchError("worldcover: no covering tile could be read")
    tree = (cls == 10).astype(np.uint8)
    water = (cls == 80).astype(np.uint8)
    return tree, water


# ---------------------------------------------------------------------------
# assemble a full course record
# ---------------------------------------------------------------------------

def bbox_from_holes(holes, pad_frac=0.10, min_span_m=900.0):
    lats = [p[0] for h in holes for p in h["pts"]]
    lons = [p[1] for h in holes for p in h["pts"]]
    la0, la1 = min(lats), max(lats)
    lo0, lo1 = min(lons), max(lons)
    latc = 0.5 * (la0 + la1)
    # pad
    dla = (la1 - la0) * pad_frac
    dlo = (lo1 - lo0) * pad_frac
    la0, la1, lo0, lo1 = la0 - dla, la1 + dla, lo0 - dlo, lo1 + dlo
    # enforce a minimum span so tiny/partial routings still yield a usable survey
    from common import DEG_M
    if (la1 - la0) * DEG_M < min_span_m:
        c = latc
        h = min_span_m / DEG_M / 2
        la0, la1 = c - h, c + h
    k = math.cos(math.radians(latc))
    if (lo1 - lo0) * DEG_M * k < min_span_m:
        c = 0.5 * (lo0 + lo1)
        h = min_span_m / (DEG_M * k) / 2
        lo0, lo1 = c - h, c + h
    # cap the aspect ratio (expand the short side) so the 256^2 grid stays
    # roughly isotropic and the viewer window isn't absurdly elongated.
    max_aspect = 2.2
    wm = (lo1 - lo0) * DEG_M * k
    hm = (la1 - la0) * DEG_M
    if hm > max_aspect * wm:
        need = hm / max_aspect
        c = 0.5 * (lo0 + lo1)
        half = need / (DEG_M * k) / 2
        lo0, lo1 = c - half, c + half
    elif wm > max_aspect * hm:
        need = wm / max_aspect
        c = 0.5 * (la0 + la1)
        half = need / DEG_M / 2
        la0, la1 = c - half, c + half
    return [la0, lo0, la1, lo1]


def build_course(key, label, arch, bbox, holes):
    """Fetch every layer for one course and return the schema record."""
    grid = fetch_dem(bbox)
    emin = float(grid.min())
    emax = float(grid.max())
    tree, wc_water = fetch_worldcover(bbox)
    osm_water = fetch_osm_water_mask(bbox)
    water = ((wc_water != 0) | (osm_water != 0)).astype(np.uint8)

    # min / max points (gy row 0 = north)
    imin = int(grid.argmin())
    imax = int(grid.argmax())
    mn = {"v": round(float(grid.flat[imin]), 1), "gx": imin % N, "gy": imin // N}
    mx = {"v": round(float(grid.flat[imax]), 1), "gx": imax % N, "gy": imax // N}

    # elevation profiles along each hole. Match the reference schema exactly:
    # `e` is DECIMETRES ABOVE emin (the viewer reads `emin + e/10`).
    profiles = []
    for h in holes:
        L = hole_length_m(h["pts"])
        m = max(2, int(L // 8) + 1)  # ~8 m spacing
        es = []
        for i in range(m):
            lat, lon = point_along(h["pts"], L * i / (m - 1))
            z = _sample_grid(grid, bbox, lat, lon)
            es.append(int(round((z - emin) * 10.0)))
        profiles.append({"ref": h["ref"], "par": h["par"],
                         "name": h["name"], "len": round(L, 1), "e": es})

    wm, hm = lat_lon_metres(bbox)
    return {
        "key": key,
        "label": label,
        "arch": arch or "",
        "bbox": [round(v, 8) for v in bbox],
        "b64": encode_elev(grid, emin),
        "tb64": encode_mask(tree),
        "wb64": encode_mask(water),
        "treepct": round(100.0 * float(tree.mean()), 1),
        "waterpct": round(100.0 * float(water.mean()), 1),
        "wm": round(wm, 1),
        "hm": round(hm, 1),
        "emin": round(emin, 2),
        "emax": round(emax, 2),
        "min": mn,
        "max": mx,
        "holes": holes,
        "profiles": profiles,
        "group": relief_group(emin, emax),
        "source": "collected",
    }


def _sample_grid(grid, bbox, lat, lon):
    la0, lo0, la1, lo1 = bbox
    fx = (lon - lo0) / ((lo1 - lo0) or 1e-9) * N
    fy = (la1 - lat) / ((la1 - la0) or 1e-9) * N
    return float(_bilinear(grid, np.array([fx]), np.array([fy]))[0])
