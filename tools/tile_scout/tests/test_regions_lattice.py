"""Region disc cover + candidate lattice: determinism and geometry."""

import os
import sys

import numpy as np

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))
from tile_scout import scoring  # noqa: E402
from tile_scout.regions import Region, build, haversine_km  # noqa: E402
from tile_scout import lattice  # noqa: E402

CFG = {
    "tile": {"side_m": 3000.0},
    "regions": {"group_radius_km": 100.0, "search_radius_km": 30.0,
                "tiles_per_course": 1.5, "min_quota": 2},
    "lattice": {"spacing_m": 1500.0, "dedup_min_km": 1.4},
}


def test_disc_cover_two_clusters():
    centers = {
        "a1": (35.0, -79.0), "a2": (35.2, -79.2), "a3": (35.1, -78.9),
        "b1": (40.0, -105.0), "b2": (40.3, -105.2),
    }
    resp = np.ones((5, 2)) * 0.5
    keys = sorted(centers)
    regions = build(centers, resp, keys, CFG)
    assert len(regions) == 2
    sizes = sorted(len(r.members) for r in regions)
    assert sizes == [2, 3]
    # determinism: same input -> same seeds
    again = build(centers, resp, keys, CFG)
    assert [r.name for r in again] == [r.name for r in regions]
    q = {r.name: r.quota for r in regions}
    assert all(v >= 2 for v in q.values())


def test_haversine_sane():
    assert abs(haversine_km(35.0, -79.0, 36.0, -79.0) - 111.2) < 1.0


def test_lattice_geometry_and_keys():
    r = Region(name="r_x", seed="x", members=["x"], center_ll=(35.19, -79.47),
               epsg=26917, quota=2)
    df = lattice.candidates(r, CFG)
    assert len(df) > 900          # pi * 20^2 lattice points at 1.5 km in 30 km
    assert df["key"].is_unique
    assert (df["x0"] % 2 == 0).all() and (df["y0"] % 2 == 0).all()
    # centers within the search radius
    d = [haversine_km(row.lat, row.lon, 35.19, -79.47)
         for row in df.sample(50, random_state=0).itertuples()]
    assert max(d) <= 30.5
    # determinism
    df2 = lattice.candidates(r, CFG)
    assert df.equals(df2)


def test_dedup_prefers_nearer_region():
    r1 = Region(name="r_a", seed="a", members=["a"], center_ll=(35.0, -79.0),
                epsg=26917, quota=2)
    r2 = Region(name="r_b", seed="b", members=["b"], center_ll=(35.05, -79.05),
                epsg=26917, quota=2)
    c1 = lattice.candidates(r1, CFG)
    c2 = lattice.candidates(r2, CFG)
    out = lattice.dedup([c1, c2], [r1, r2], CFG)
    assert len(out) < len(c1) + len(c2)      # overlap removed
    assert out["key"].is_unique or True      # keys can repeat across epsg? no:
    # same epsg + close origins -> same key; dedup must leave unique keys
    assert out["key"].is_unique


def test_scoring_roundtrip():
    import pandas as pd
    df = pd.DataFrame({"m1": [1.0, 2.0, 3.0, 10.0], "m2": [0.1, 0.2, 0.3, 0.4]})
    t = scoring.transform(df, ["m1", "m2"], ["m1"])
    assert np.allclose(t["m1"], np.log1p(df["m1"]))
    med, sd = scoring.robust_scales(t)
    z = scoring.zscore(t, med, sd)
    assert abs(float(z["m2"].median())) < 1e-12
