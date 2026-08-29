#!/usr/bin/env python3
"""The sandhills exit gates, run on a dump. (docs/sandhills/README.md §8)

Everything in that table a machine can decide. P2 (blind A/B) and P3
(name-the-archetype) are human protocols and are NOT run here -- see
`blind_sheets.py`, which builds them.

    python3 tools/aeolian/gate_run.py <dump_dir>

The dump is `final_pass` output (`<seed>.cgrid` + `<seed>.water.cgrid`) plus its
TSV on stdin-adjacent path, or a `full_dump` directory of `train_*`/`mound_*`.

One caution recorded where it belongs: `02-dune-targets.md` §6 declares
`coherence`, `crest_len_p50`, `ridge_spacing_m`, `prominence_p50`, `defect_km2`,
`flank_asym` and `hole_depth` DESCRIPTIVE, not gated -- the crest tracer is
unreliable at the ~1 crest/tile this corpus returns. The README's gate table
names spacing and defect density anyway; where the two disagree this follows
§6, reports those as descriptive, and gates on the §5 generator targets
(spectral A, lambda_dom, band relief) plus the golf proxy and dispersion.
"""
from __future__ import annotations

import pathlib
import sys

import numpy as np
from scipy import ndimage

ROOT = pathlib.Path(__file__).resolve().parent.parent.parent
for p in ("macro_campaign", "metrics", "golf_proxy", "aeolian"):
    sys.path.insert(0, str(ROOT / "tools" / p))

from macro_campaign import cgrid                    # noqa: E402
import dune_stats as ds                             # noqa: E402
from proxy import quantities                        # noqa: E402

# --- targets -------------------------------------------------------------
# 02-dune-targets.md §5 (generator targets) and §8 (gate table).
TARGETS = {
    "train": {"spectral_A": 0.767, "lam_dom_m": 1301.0, "band_relief_m": 29.2},
    "mound": {"spectral_A": 0.356, "lam_dom_m": 1184.0, "band_relief_m": 19.1},
}
TOL = 0.35            # +-35% of target counts as in band for these three
PROXY_FLOORS = {"frac_under_cap": 0.409, "frac_under_steep": 0.713,
                "largest_contig_ha": 124.0}
RELIEF_BAND = (7.0, 81.9)
DISPERSION_BAND = (0.7, 1.3)


def drainage_density(z, cell):
    """Channel length per km2 at a 0.05 km2 accumulation threshold.

    The aeolian gate is 'approximately zero, MEASURED not assumed'. Sand is
    too permeable to shed surface flow, so a dune tile that is not a river
    seed should carry essentially no integrated channel network.
    """
    from macro_campaign import flow
    from metrics import core
    zf = core.fill_depressions(z, cell)
    rec, _ = flow.receivers(zf, cell)
    acc = flow.accumulate(rec) * cell * cell
    chan = acc > 0.05e6
    # skeleton length ~= cell count * cell (channels are one cell wide here)
    km2 = (z.shape[0] * cell) * (z.shape[1] * cell) / 1e6
    return float(chan.sum() * cell / 1000.0 / km2)


def dispersion_ratio(rows, keys):
    """Spread ACROSS seeds over spread WITHIN the real corpus, per metric.

    0.7-1.3 means the generator varies about as much as the real ground does:
    below, every seed is the same course; above, it is inventing variety the
    landscape does not have.
    """
    out = {}
    for k in keys:
        g = np.array([r.get(k, np.nan) for r in rows["gen"]], float)
        r = np.array([r.get(k, np.nan) for r in rows["real"]], float)
        g, r = g[np.isfinite(g)], r[np.isfinite(r)]
        if len(g) < 4 or len(r) < 4 or np.std(r) < 1e-9:
            continue
        out[k] = float(np.std(g) / np.std(r))
    return out


def main():
    d = pathlib.Path(sys.argv[1])
    tiles = sorted(p for p in d.glob("*.cgrid") if ".water." not in p.name)
    if not tiles:
        print(f"no tiles in {d}")
        return 1

    print(f"measuring {len(tiles)} tiles from {d.name} ...")
    rows = []
    for p in tiles:
        z, (_, _, c) = cgrid.read_f32(p)
        wp = p.with_suffix(".water.cgrid")
        wet = None
        if wp.exists():
            w, _ = cgrid.read_f32(wp)
            wet = ~np.isnan(w)
        m = ds.measure_tile(p, with_variogram=False)
        m["relief_m"] = float(np.percentile(z, 98) - np.percentile(z, 2))
        m.update(quantities(z, c))
        m["drainage_density"] = drainage_density(z, c)
        m["has_water"] = bool(wet is not None and wet.any())
        m["wet_frac"] = float(wet.mean()) if wet is not None else 0.0
        rows.append(m)

    def med(k, sub=None):
        v = np.array([r.get(k, np.nan) for r in (sub or rows)], float)
        v = v[np.isfinite(v)]
        return float(np.median(v)) if len(v) else float("nan")

    print("\n--- golf proxy floors (README §8) ---")
    ok_all = True
    for k, floor in PROXY_FLOORS.items():
        v = med(k)
        ok = v >= floor
        ok_all &= ok
        print(f"  {k:>20s} {v:9.3f}  floor {floor:7.3f}   {'PASS' if ok else 'FAIL'}")
    rel = med("relief_m")
    ok = RELIEF_BAND[0] <= rel <= RELIEF_BAND[1]
    ok_all &= ok
    print(f"  {'relief_m':>20s} {rel:9.2f}  band {RELIEF_BAND}   {'PASS' if ok else 'FAIL'}")

    print("\n--- drainage density, aeolian (gate: ~0, measured) ---")
    dry = [r for r in rows if not r["has_water"]]
    dd_all, dd_dry = med("drainage_density"), med("drainage_density", dry or None)
    print(f"  all tiles          {dd_all:7.3f} km/km2")
    print(f"  no-water tiles     {dd_dry:7.3f} km/km2   (n={len(dry)})")

    print("\n--- descriptive, NOT gated (02-dune-targets §6) ---")
    for k in ("coherence", "crest_len_p50", "ridge_spacing_m", "prominence_p50",
              "defect_km2", "flank_asym"):
        print(f"  {k:>18s} {med(k):9.2f}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
