"""E5 — skeleton + residual extraction for the Heartland v2 corpus.

Runs over the KEPT tiles (``review_v2.json`` minus ``exclude.json``) and
stages, per tile, exactly what the two downstream consumers need:

- **F2 (dictionary builder)**: the band-separated residuals and the
  conditioning fields, in the spike's proven decomposition — fine < 64 m at
  2 m, mid 64–400 m at 8 m (band-limited, so the 4× decimation is safe),
  everything below 400 m being S3's property and everything above it S1/S2's.
- **Phase D (S2 fit) / E6-E7 (targets)**: the drainage skeleton — channel
  mask from Barnes-flat-resolved D8 (``flow.py``, NOT ``mcore.d8_accumulation``
  — its tie-breaking drains every flat along a dead-straight axis line), the
  lake-pool and road-ditch acceptance policy copied from the v1 extractor,
  plus the ``structure_metrics`` family (``dist_to_channel_p50_m`` above all).

Outputs per tile:

    out/extract_v2/<biome>/<id>.npz    mid8 (375², f16), fine (1500², f16),
                                       cond8 (375²×5, f16: lp_slope, tpi,
                                       relief_pos, lp_aspect, dist_channel_m)
    out/extract_v2/<biome>/<id>.json   scalars: band stds, structure metrics,
                                       channel stats, valid_frac
    out/extract/<biome>/<id>.classes.cgrid   u8 overlay raster for tile-lab
    out/extract/<biome>/<id>.regions.json    channel bboxes + tilt for tile-lab
    out/extract/<biome>/<id>.json            KnobRecord for tile-lab's panel

The tile-lab artifacts intentionally use the v1 exchange format (the viewer
needs no changes) and OVERWRITE any stale v1 extract output for the shared
piedmont/sandhills directories — v1 is superseded on this branch.

Determinism: pure function of the tile bytes + the OSM develop mask; no
draws, no iteration-order dependence. ``EXTRACT_V2_VERSION`` gates re-runs.

KNOWN CAVEATS (2026-08 review):
- The "perfect rectangles" first reported over every tile were CHANNEL
  COMPONENT BOUNDING BOXES drawn by tile-lab's v1 vector layer, not data;
  the viewer no longer draws them (regions.json still records components
  for the count). Diagnosis history kept because it changed twice.
- The fill-flat mask genuinely sprawls over leveled/ditched agricultural
  ground (river_valley bottomland above all) and drained peat (heathland).
  Ponding areas are excluded from the clean mask; graded-but-drained
  fields can slip through. Before F2 harvests patches: (a) add OSM
  landuse=farmland/orchard to the develop screen mask, (b) consider a
  flatness screen on the fine band. river_valley's low fine_std (0.19 m)
  is partly this contamination and must not be treated as a texture
  target until re-masked.
"""

import json
import pathlib
import sys

import numpy as np
from scipy import ndimage

_ROOT = pathlib.Path(__file__).resolve().parent.parent.parent
sys.path.insert(0, str(_ROOT / "metrics"))
sys.path.insert(0, str(_ROOT / "dtm_primitives"))

from metrics import core as mcore  # noqa: E402
from dtm_primitives import geomorphons  # noqa: E402

from . import cgrid, develop, flow, structure  # noqa: E402

OUT = pathlib.Path(__file__).resolve().parent.parent / "out"

EXTRACT_V2_VERSION = 2

V2_BIOMES = (
    "piedmont", "sandhills", "great_plains",
    "river_valley", "hill_country", "heathland",
    # Attempt 5: the CAROLINA Sandhills mode. Same game archetype as
    # "sandhills" (Nebraska), different structural mode -- fluvially dissected
    # sand cap vs constructional dune field. Kept as a separate corpus key so
    # the two are measured and gated separately; see docs/sandhills/README.md.
    "sandhills_nc",
)

# The spike's band split (tools/spike/spike.py): half-amplitude Gaussian
# cuts at 400 m (S1/S2 | S3 boundary) and 64 m (mid | fine boundary).
L_MACRO = 400.0
L_MID = 64.0
SIGMA_PER_L = float(np.sqrt(np.log(2.0) / (2.0 * np.pi**2)))  # ~0.18739

# Channel acceptance, copied from the v1 extractor (extract.py): thalwegs
# keep their identity across filled lakes only at 4x the area threshold,
# and OSM-developed cells never count (road ditches drain like channels).
CHANNEL_AREA_M2 = 6.0e4

# classes.cgrid bits, matching extract.py / tile-lab's LAYERS table.
CLASS_NODATA = 1
CLASS_CHANNEL = 2
CLASS_RIDGE = 4
CLASS_FILL_FLAT = 32
CLASS_AGRI = 64
CLASS_DEVELOPED = 128

# tile-lab draws a bbox per channel component; sub-800 m fragments are
# noise at overlay scale (same floor as the v1 extractor).
CHANNEL_COMPONENT_MIN_DIAG_M = 800.0


def kept_tiles() -> list[tuple[str, str]]:
    """The E5 work list: reviewed-and-kept, minus anything excluded since."""
    rv = json.loads((OUT / "review_v2.json").read_text())
    ex = json.loads((OUT / "exclude.json").read_text())
    exset = {(t["archetype"], t["tile"]) for t in ex["tiles"]}
    return [
        (t["archetype"], t["tile"])
        for t in rv["kept"]
        if t["archetype"] in V2_BIOMES and (t["archetype"], t["tile"]) not in exset
    ]


def decompose(z: np.ndarray, cell: float) -> dict:
    """Band split + conditioning fields; the spike's decomposition verbatim,
    plus lp_aspect (grain conditioning needs a direction, not just a rate)."""
    valid = np.isfinite(z)
    filled = mcore._fill_nearest(z, valid)
    lp400 = ndimage.gaussian_filter(filled, SIGMA_PER_L * L_MACRO / cell)
    lp64 = ndimage.gaussian_filter(filled, SIGMA_PER_L * L_MID / cell)
    fine = filled - lp64               # < 64 m band, 2 m grid
    mid = lp64 - lp400                 # 64–400 m band, band-limited
    mid8 = mid[::4, ::4].copy()

    gy, gx = np.gradient(lp400, cell)
    lp_slope = np.hypot(gx, gy)
    lp_aspect = np.arctan2(gy, gx)
    w = max(3, int(round(L_MACRO / cell)) | 1)
    tpi = lp400 - ndimage.uniform_filter(lp400, w)
    relief_pos = np.zeros_like(lp400)
    lv = lp400[valid]
    if lv.size:
        order = np.argsort(lv, kind="stable")
        ranks = np.empty_like(order, dtype=np.float64)
        ranks[order] = np.linspace(0.0, 1.0, lv.size)
        relief_pos[valid] = ranks

    return dict(valid=valid, filled=filled, lp400=lp400, fine=fine, mid8=mid8,
                lp_slope=lp_slope, lp_aspect=lp_aspect, tpi=tpi,
                relief_pos=relief_pos)


def skeleton(z: np.ndarray, cell: float, developed: np.ndarray | None) -> dict:
    """Channel mask + drainage area via the flat-resolved router, with the
    v1 acceptance policy; distance field from the ACCEPTED mask."""
    zfill = mcore.fill_depressions(z, cell)
    rec, _slope = flow.receivers(zfill, cell)
    acc = flow.accumulate(rec).astype(float) * cell * cell
    lake = zfill > z + 0.01
    channels = acc >= CHANNEL_AREA_M2
    channels &= ~lake | (acc >= 4.0 * CHANNEL_AREA_M2)
    if developed is not None:
        channels &= ~developed
    dist = (
        ndimage.distance_transform_edt(~channels, sampling=cell)
        if channels.any()
        else np.full(z.shape, np.hypot(*z.shape) * cell)
    )
    return dict(acc=acc, channels=channels, lake=lake, dist=dist)


def _channel_components(channels: np.ndarray, cell: float) -> list[dict]:
    lab, n = ndimage.label(channels, structure=np.ones((3, 3)))
    out = []
    H = channels.shape[0]
    for sl in ndimage.find_objects(lab):
        if sl is None:
            continue
        dy = (sl[0].stop - sl[0].start) * cell
        dx = (sl[1].stop - sl[1].start) * cell
        diag = float(np.hypot(dx, dy))
        if diag < CHANNEL_COMPONENT_MIN_DIAG_M:
            continue
        # world coords: x = col*cell, y flips row (row 0 = north)
        x0, x1 = sl[1].start * cell, sl[1].stop * cell
        y0, y1 = (H - sl[0].stop) * cell, (H - sl[0].start) * cell
        out.append({"diag_m": diag, "bbox_m": [x0, y0, x1, y1], "fall_grad": None})
    out.sort(key=lambda c: -c["diag_m"])
    return out


def _plane_tilt(z: np.ndarray, cell: float) -> dict:
    valid = np.isfinite(z)
    ys, xs = np.nonzero(valid)
    if ys.size < 3:
        return {"grade": 0.0, "downhill_xy": None}
    A = np.stack([xs * cell, ys * cell, np.ones(ys.size)], axis=1)
    coef, *_ = np.linalg.lstsq(A, z[valid], rcond=None)
    gx, gy_row = float(coef[0]), float(coef[1])
    gy = -gy_row  # row index increases southward
    grade = float(np.hypot(gx, gy))
    if grade <= 1e-12:
        return {"grade": grade, "downhill_xy": None}
    return {"grade": grade, "downhill_xy": [-gx / grade, -gy / grade]}


def extract_tile(biome: str, tid: str) -> dict:
    src = OUT / "tiles" / biome / f"{tid}.cgrid"
    z, (ox, oy, cell) = cgrid.read_f32(src)
    z = z.astype(np.float64)
    developed = develop.load_mask(biome, tid)
    if developed is not None and developed.shape != z.shape:
        developed = None
    agri = develop.load_agri_mask(biome, tid)
    if agri is not None and agri.shape != z.shape:
        agri = None

    d = decompose(z, cell)
    sk = skeleton(z, cell, developed)

    # ---- staged arrays (f16: residuals are metre-scale; 0.5 mm error) ----
    cond8 = np.stack(
        [
            d["lp_slope"][::4, ::4],
            d["tpi"][::4, ::4],
            d["relief_pos"][::4, ::4],
            d["lp_aspect"][::4, ::4],
            sk["dist"][::4, ::4],
        ],
        axis=-1,
    )
    # F2's mask-aware extraction hook: a patch is harvestable only where the
    # ground is real AND natural — not nodata, not a flattened water surface
    # (near-zero residual would dilute bucket amplitude stats), not developed.
    clean = d["valid"] & ~sk["lake"]
    if developed is not None:
        clean &= ~developed
    if agri is not None:
        clean &= ~agri
    # Physical leveled-ground screen (the OSM agri mask is empty across
    # most of the rural corpus — measured zero landuse tags of ANY kind on
    # the Louisiana bottomland). Laser-leveled fields carry ~3–9 cm of
    # local fine-band texture vs ≥ 15 cm p25 on natural ground; the 4 cm
    # threshold keeps ≥ 95% of natural cells (natural p5 ≈ 5.2 cm).
    fm = ndimage.uniform_filter(d["fine"], 15)
    fm2 = ndimage.uniform_filter(d["fine"] ** 2, 15)
    leveled = np.sqrt(np.maximum(fm2 - fm * fm, 0.0)) < 0.04
    leveled = ndimage.binary_dilation(leveled, iterations=2)
    clean &= ~leveled

    vdir = OUT / "extract_v2" / biome
    vdir.mkdir(parents=True, exist_ok=True)
    np.savez_compressed(
        vdir / f"{tid}.npz",
        mid8=d["mid8"].astype(np.float16),
        fine=d["fine"].astype(np.float16),
        cond8=cond8.astype(np.float16),
        valid=d["valid"],
        clean=clean,
    )

    # ---- scalars ---------------------------------------------------------
    sm = structure.structure_metrics(z, cell, sk["acc"], sk["channels"], CHANNEL_AREA_M2)
    fine_v = d["fine"][d["valid"]]
    # mid8 is decimated; its valid mask decimates with it
    mid_v = d["mid8"][d["valid"][::4, ::4]]
    scalars = {
        "extract_v2_version": EXTRACT_V2_VERSION,
        "cell_m": cell,
        "valid_frac": float(d["valid"].mean()),
        "fine_std_m": float(fine_v.std()),
        "mid_std_m": float(mid_v.std()),
        "relief_p99_p1_m": float(
            np.percentile(z[d["valid"]], 99) - np.percentile(z[d["valid"]], 1)
        ),
        "channel_frac": float(sk["channels"].mean()),
        "lake_frac": float(sk["lake"].mean()),
        **{k: (None if isinstance(v, float) and not np.isfinite(v) else v)
           for k, v in sm.items()},
    }
    (vdir / f"{tid}.json").write_text(json.dumps(scalars, indent=1, sort_keys=True) + "\n")

    # ---- tile-lab overlays (v1 exchange format) --------------------------
    cls10 = geomorphons.classify(
        _block_mean(d["filled"], 5), cell * 5.0, lookup_m=500.0
    )
    ridge10 = geomorphons.ridge_mask(cls10)
    ridge = np.kron(ridge10, np.ones((5, 5), dtype=bool))[: z.shape[0], : z.shape[1]]

    classes = np.zeros(z.shape, dtype=np.uint8)
    classes[~d["valid"]] |= CLASS_NODATA
    classes[sk["channels"]] |= CLASS_CHANNEL
    classes[ridge] |= CLASS_RIDGE
    classes[sk["lake"]] |= CLASS_FILL_FLAT
    if agri is not None:
        classes[agri] |= CLASS_AGRI
    classes[leveled] |= CLASS_AGRI
    if developed is not None:
        classes[developed] |= CLASS_DEVELOPED

    edir = OUT / "extract" / biome
    edir.mkdir(parents=True, exist_ok=True)
    cgrid.write_u8(edir / f"{tid}.classes.cgrid", classes, ox, oy, cell)
    regions = {
        "tilt": _plane_tilt(z, cell),
        "channels": _channel_components(sk["channels"], cell),
    }
    (edir / f"{tid}.regions.json").write_text(
        json.dumps(regions, indent=1, sort_keys=True) + "\n"
    )
    knobs = {
        "extract_version": 100 + EXTRACT_V2_VERSION,  # >99 = v2 lineage
        "shape_version": None,
        "knobs": {
            "dist_to_channel_p50_m": scalars["dist_to_channel_p50_m"],
            "drainage_density_1x": scalars["drainage_density_1x"],
            "fine_std_m": scalars["fine_std_m"],
            "mid_std_m": scalars["mid_std_m"],
            "relief_p99_p1_m": scalars["relief_p99_p1_m"],
            "slope_median": scalars["slope_median"],
        },
        "extras": {
            "clean_frac": float(clean.mean()),
        "channel_frac": scalars["channel_frac"],
            "lake_frac": scalars["lake_frac"],
            "local_relief_200m": scalars["local_relief_200m"],
        },
        "valid_frac": scalars["valid_frac"],
    }
    (edir / f"{tid}.json").write_text(json.dumps(knobs, indent=1, sort_keys=True) + "\n")
    return scalars


def _block_mean(a: np.ndarray, k: int) -> np.ndarray:
    H, W = a.shape
    Hk, Wk = H // k, W // k
    return a[: Hk * k, : Wk * k].reshape(Hk, k, Wk, k).mean(axis=(1, 3))


def run(archetype: str | None = None, force: bool = False):
    tiles = kept_tiles()
    if archetype:
        tiles = [(b, t) for b, t in tiles if b == archetype]
    print(f"extract-v2: {len(tiles)} kept tiles")
    done = skipped = failed = 0
    for biome, tid in tiles:
        marker = OUT / "extract_v2" / biome / f"{tid}.json"
        if not force and marker.exists():
            try:
                if json.loads(marker.read_text()).get("extract_v2_version") == EXTRACT_V2_VERSION:
                    skipped += 1
                    continue
            except (json.JSONDecodeError, OSError):
                pass
        try:
            s = extract_tile(biome, tid)
        except Exception as exc:  # noqa: BLE001 — record and continue
            print(f"  [FAIL] {biome}/{tid}: {exc}")
            failed += 1
            continue
        p50 = s["dist_to_channel_p50_m"]
        p50s = "None" if p50 is None else f"{p50:.0f}"
        print(
            f"  [ok] {biome}/{tid}  fine {s['fine_std_m']:.2f} m  "
            f"mid {s['mid_std_m']:.2f} m  d2c_p50 {p50s} m"
        )
        done += 1
    print(f"extract-v2: {done} done, {skipped} current, {failed} failed")
