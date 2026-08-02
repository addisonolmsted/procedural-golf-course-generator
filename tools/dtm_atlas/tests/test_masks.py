"""Mask machinery: metric dilation radii and relation ring assembly."""
import numpy as np

from dtm_atlas import osm
from dtm_atlas.masks import dilate_m


def test_dilation_radius_in_meters():
    m = np.zeros((100, 100), bool)
    m[50, 50] = True
    d = dilate_m(m, 10.0, 2.0)              # 10 m at 2 m/px -> 5 px radius
    assert d[50, 55] and d[50, 56] is np.False_ or not d[50, 56]
    assert d[50, 55]                        # 5 px = 10 m -> inside
    assert not d[50, 57]                    # 7 px = 14 m -> outside
    # area ~ pi r^2 in cells
    assert 70 <= d.sum() <= 90


def test_dilation_zero_radius_identity():
    m = np.zeros((10, 10), bool)
    m[3, 3] = True
    assert (dilate_m(m, 0.0, 2.0) == m).all()


def test_assemble_rings_stitches_split_ways():
    # a square split into two open ways, second reversed
    w1 = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]]
    w2 = [[0.0, 0.0], [0.0, 1.0], [1.0, 1.0]]   # needs reversal to append
    rings = osm.assemble_rings([w1, w2])
    assert len(rings) == 1
    r = rings[0]
    assert r[0] == r[-1] and len(r) == 5


def test_relation_polygon_with_hole():
    el = {
        "type": "relation", "id": 1,
        "members": [
            {"type": "way", "role": "outer", "geometry": [
                {"lon": 0.0, "lat": 0.0}, {"lon": 4.0, "lat": 0.0},
                {"lon": 4.0, "lat": 4.0}, {"lon": 0.0, "lat": 4.0},
                {"lon": 0.0, "lat": 0.0}]},
            {"type": "way", "role": "inner", "geometry": [
                {"lon": 1.0, "lat": 1.0}, {"lon": 2.0, "lat": 1.0},
                {"lon": 2.0, "lat": 2.0}, {"lon": 1.0, "lat": 2.0},
                {"lon": 1.0, "lat": 1.0}]},
        ],
    }
    polys = osm.element_polygons(el)
    assert len(polys) == 1
    assert len(polys[0]["coordinates"]) == 2      # outer + hole
