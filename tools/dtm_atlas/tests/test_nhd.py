"""NHD truth channel: FCode policy, pagination merge, envelope math —
fixtures only, no network."""

import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))
from dtm_atlas import nhd  # noqa: E402


def test_flowline_policy():
    assert nhd.flowline_policy({"FCode": 46006}) == {
        "keep": True, "ftype": 460, "intermittent": False,
        "artificial": False}
    assert nhd.flowline_policy({"FCode": 46003})["intermittent"]
    art = nhd.flowline_policy({"FCode": 55800})
    assert art["keep"] and art["artificial"]
    assert not nhd.flowline_policy({"FCode": 33600})["keep"]   # canal
    assert not nhd.flowline_policy({"FCode": 42801})["keep"]   # pipeline
    assert not nhd.flowline_policy({"fcode": 33400})["keep"]   # connector


def test_query_layer_pagination(monkeypatch):
    pages = [
        {"features": [{"id": 1}, {"id": 2}], "exceededTransferLimit": True},
        {"features": [{"id": 3}], },
    ]
    calls = []

    def fake_page(sess, layer_id, env, offset):
        calls.append(offset)
        return pages[len(calls) - 1]

    monkeypatch.setattr(nhd, "_query_page", fake_page)
    out = nhd.query_layer(None, 3, {})
    assert [f["id"] for f in out["features"]] == [1, 2, 3]
    assert calls == [0, 2]


def test_query_layer_empty(monkeypatch):
    monkeypatch.setattr(nhd, "_query_page",
                        lambda s, l, e, o: {"features": []})
    out = nhd.query_layer(None, 3, {})
    assert out["features"] == []


def test_tile_envelope_from_manifest(monkeypatch, tmp_path):
    # no store meta -> falls back to the committed manifest record
    from dtm_atlas import config, tiles
    monkeypatch.setattr(config, "STORE", str(tmp_path))
    monkeypatch.setattr(tiles, "record",
                        lambda k: {"center_ll": [35.19, -79.47],
                                   "side_m": 3000.0})
    env = nhd._tile_envelope_ll("t_test")
    assert env["xmin"] < -79.47 < env["xmax"]
    assert env["ymin"] < 35.19 < env["ymax"]
    # buffered half-width ~ (1500+500)/111320 deg ≈ 0.018 in lat
    assert 0.030 < (env["ymax"] - env["ymin"]) < 0.045
