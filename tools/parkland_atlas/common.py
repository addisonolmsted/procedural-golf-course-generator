"""Shared encoding / geometry helpers for the Parkland Atlas collector.

The on-disk course record and the emitted ``window.COURSES`` entry use the
EXACT schema of the reference ``parkland_atlas.html`` so the existing Rust
``golf-atlas`` parser ingests our output unchanged:

    {
      "label", "arch", "bbox":[lat0,lon0,lat1,lon1],   # lat_min,lon_min,lat_max,lon_max
      "b64",                 # 256*256 little-endian u16 decimetres above emin
      "tb64", "wb64",        # 256*256 MSB-first bit masks (tree / water)
      "treepct", "waterpct",
      "wm", "hm",            # survey extent metres (E-W, N-S)
      "emin", "emax",
      "min":{"v","gx","gy"}, "max":{"v","gx","gy"},     # gy row 0 = NORTH
      "holes":   [{"ref","par","name","pts":[[lat,lon],...]}],
      "profiles":[{"ref","par","name","len","e":[...]}],
    }

Grid convention (matches the reference JS and golf-atlas): N=256, ROW 0 = NORTH
edge (lat_max), column 0 = WEST edge (lon_min).
"""

import base64
import math

import numpy as np

N = 256
R_EARTH = 6378137.0
DEG_M = 111320.0  # metres per degree latitude (also per degree lon at the equator)


# ---------------------------------------------------------------------------
# base64 packing (byte-identical to the reference decoders)
# ---------------------------------------------------------------------------

def encode_elev(grid, emin):
    """256x256 float metres -> base64 of little-endian u16 decimetres above emin."""
    dm = np.rint((grid - emin) * 10.0)
    dm = np.clip(dm, 0, 65535).astype(np.uint16)
    # little-endian: low byte then high byte, matching `byte[2i] | byte[2i+1]<<8`
    return base64.b64encode(dm.astype("<u2").tobytes()).decode("ascii")


def decode_elev(b64, emin):
    raw = base64.b64decode(b64)
    dm = np.frombuffer(raw, dtype="<u2").astype(np.float64)
    return (emin + dm / 10.0).reshape(N, N)


def encode_mask(mask):
    """256x256 {0,1} -> base64, MSB-first bit packing (`m[i*8+k]=(b>>(7-k))&1`)."""
    bits = (np.asarray(mask).reshape(-1) != 0).astype(np.uint8)
    packed = np.packbits(bits, bitorder="big")  # MSB-first within each byte
    return base64.b64encode(packed.tobytes()).decode("ascii")


def decode_mask(b64):
    raw = np.frombuffer(base64.b64decode(b64), dtype=np.uint8)
    bits = np.unpackbits(raw, bitorder="big")[: N * N]
    return bits.reshape(N, N).astype(np.uint8)


# ---------------------------------------------------------------------------
# geometry
# ---------------------------------------------------------------------------

def lat_lon_metres(bbox):
    """(wm, hm): survey extent metres, E-W and N-S, for bbox [la0,lo0,la1,lo1]."""
    la0, lo0, la1, lo1 = bbox
    latc = 0.5 * (la0 + la1)
    hm = (la1 - la0) * DEG_M
    wm = (lo1 - lo0) * DEG_M * math.cos(math.radians(latc))
    return wm, hm


def relief_group(emin, emax):
    """Atlas StyleGroup band from the elevation range (metres)."""
    r = emax - emin
    if r < 30.0:
        return "lowland"
    if r < 80.0:
        return "rolling"
    return "mountain"


def hole_length_m(pts):
    """Cumulative planar length of a lat/lon polyline, metres."""
    if len(pts) < 2:
        return 0.0
    latc = math.radians(sum(p[0] for p in pts) / len(pts))
    k = math.cos(latc)
    total = 0.0
    for a, b in zip(pts, pts[1:]):
        dy = (b[0] - a[0]) * DEG_M
        dx = (b[1] - a[1]) * DEG_M * k
        total += math.hypot(dx, dy)
    return total


def point_along(pts, t):
    """lat/lon at planar distance t along a polyline."""
    latc = math.radians(sum(p[0] for p in pts) / len(pts))
    k = math.cos(latc)
    cum = [0.0]
    for a, b in zip(pts, pts[1:]):
        dy = (b[0] - a[0]) * DEG_M
        dx = (b[1] - a[1]) * DEG_M * k
        cum.append(cum[-1] + math.hypot(dx, dy))
    total = cum[-1]
    t = max(0.0, min(total, t))
    j = 0
    while j < len(cum) - 2 and cum[j + 1] < t:
        j += 1
    seg = max(1e-9, cum[j + 1] - cum[j])
    f = (t - cum[j]) / seg
    return [pts[j][0] + (pts[j + 1][0] - pts[j][0]) * f,
            pts[j][1] + (pts[j + 1][1] - pts[j][1]) * f]


# ---------------------------------------------------------------------------
# web-mercator tile math (terrarium DEM), inline — no mercantile dependency
# ---------------------------------------------------------------------------

def lonlat_to_tile_frac(lon, lat, z):
    """Fractional web-mercator tile coords (x,y) at zoom z."""
    n = 2.0 ** z
    x = (lon + 180.0) / 360.0 * n
    lat_r = math.radians(lat)
    y = (1.0 - math.log(math.tan(lat_r) + 1.0 / math.cos(lat_r)) / math.pi) / 2.0 * n
    return x, y


def tile_frac_to_lonlat(x, y, z):
    n = 2.0 ** z
    lon = x / n * 360.0 - 180.0
    lat = math.degrees(math.atan(math.sinh(math.pi * (1.0 - 2.0 * y / n))))
    return lon, lat
