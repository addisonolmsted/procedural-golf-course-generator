"""Progress-page invariants.

The page's whole claim is that the comparison is not rigged, and the one
place rigging could hide is which tile gets photographed. So the selection
rule is pinned here: nearest to the side's OWN median, ties to the lowest
name, no dependence on file order.
"""

import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent.parent))

from macro_campaign import report  # noqa: E402

KEY = report.PICK_BY


def _recs(pairs):
    return [(name, {KEY: v}) for name, v in pairs]


def test_picks_the_median_tile_not_the_flattering_one():
    recs = _recs([("a", 1.0), ("b", 5.0), ("c", 9.0), ("d", 40.0), ("e", 100.0)])
    assert report._representative(recs) == "c"


def test_selection_ignores_file_order():
    pairs = [("a", 1.0), ("b", 5.0), ("c", 9.0), ("d", 40.0), ("e", 100.0)]
    first = report._representative(_recs(pairs))
    assert report._representative(_recs(pairs[::-1])) == first
    assert report._representative(_recs(pairs[2:] + pairs[:2])) == first


def test_ties_break_on_lowest_name():
    """An even count puts the median between two samples; both are equally
    near it, so the winner must not depend on which one was listed first."""
    recs = _recs([("zulu", 4.0), ("alpha", 6.0)])
    assert report._representative(recs) == "alpha"
    assert report._representative(recs[::-1]) == "alpha"


def test_tiles_missing_the_pick_metric_do_not_win():
    recs = [("nometric", {}), ("has", {KEY: 3.0}), ("also", {KEY: 3.2})]
    assert report._representative(recs) in ("has", "also")


def test_log_scale_is_monotone_and_clamps_below_the_floor():
    y = [report._log(v, 1.0, 100.0, 200.0, 0.0) for v in (0.01, 1.0, 10.0, 100.0)]
    assert y[0] == y[1] == 200.0          # clamped to the axis floor
    assert y[1] > y[2] > y[3] == 0.0      # y grows downward in SVG
