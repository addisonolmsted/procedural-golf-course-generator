"""F0 stats on synthetic patches + F1 batch query/parse round-trip."""

import os
import sys

import numpy as np

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))
from tile_scout import f0_coarse, f1_osm  # noqa: E402


def test_f0_stats_flat_vs_hilly():
    cell = 60.0
    n = 50
    yy, xx = np.mgrid[0:n, 0:n] * cell
    flat = np.full((n, n), 100.0)
    hilly = 100.0 + 12.0 * np.sin(2 * np.pi * xx / 900.0) \
        * np.sin(2 * np.pi * yy / 900.0) + 0.01 * xx
    sf = f0_coarse.f0_stats(flat, cell, 0.002)
    sh = f0_coarse.f0_stats(hilly, cell, 0.002)
    assert sf["f0_flat_frac"] == 1.0 and sf["f0_relief"] == 0.0
    assert sh["f0_relief"] > 15.0
    assert sh["f0_slope_p90"] > sh["f0_slope_p50"] > 0.0
    assert sh["f0_bp_200_800"] > sf["f0_bp_200_800"]
    assert sh["f0_flat_frac"] < 0.1


def test_f0_stats_nodata_handling():
    z = np.full((50, 50), np.nan)
    s = f0_coarse.f0_stats(z, 60.0, 0.002)
    assert s["f0_nodata_frac"] == 1.0
    assert np.isnan(s["f0_relief"])


class _Row:
    def __init__(self, key, s, w, n, e):
        self.key, self.bb_s, self.bb_w, self.bb_n, self.bb_e = key, s, w, n, e


def test_f1_query_and_parse_roundtrip():
    rows = [_Row("t1", 35.0, -79.5, 35.03, -79.47),
            _Row("t2", 35.1, -79.6, 35.13, -79.57)]
    q = f1_osm.batch_query(rows)
    assert q.count("out count;") == len(f1_osm.GROUPS) * 2
    assert "35.0,-79.5,35.03,-79.47" in q.replace(" ", "")
    # canned response: 7 counts per tile, ordered
    counts = []
    for t in range(2):
        for g in range(len(f1_osm.GROUPS)):
            counts.append({"type": "count", "id": g,
                           "tags": {"total": str(t * 10 + g)}})
    parsed = f1_osm.parse_counts({"elements": counts}, 2)
    assert parsed[0] == {"bld": 0, "big": 1, "rail": 2, "res": 3, "loc": 4,
                         "dev": 5, "agr": 6}
    assert parsed[1]["bld"] == 10


def test_f1_gates():
    f1 = {"max_buildings": 5, "max_big_road_rail": 0, "max_residential": 0,
          "max_local_roads": 2, "max_dev_landuse": 0, "max_agriculture": 4}
    ok = {"bld": 2, "big": 0, "rail": 0, "res": 0, "loc": 1, "dev": 0, "agr": 3}
    assert f1_osm.gates(ok, f1)
    assert not f1_osm.gates({**ok, "big": 1}, f1)
    assert not f1_osm.gates({**ok, "bld": 6}, f1)
    assert not f1_osm.gates({**ok, "agr": 5}, f1)


def test_f1_parse_length_mismatch_raises():
    try:
        f1_osm.parse_counts({"elements": []}, 1)
        raised = False
    except ValueError:
        raised = True
    assert raised
