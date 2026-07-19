"""Cross-language frame check (commit-1 gate): render hand-written
MacroConfigs through the new `xtask landform-grid` arm and compare against the
Python analytic forms at every cell. This nails the riskiest seam FIRST —
JSON contract, grid/HG01 orientation, and the mirrored surface math — before
any detector code exists.

Test configs use a STRAIGHT (collinear, equally-spaced) valley path: Catmull-
Rom of uniform collinear control points is exactly linear, so the Python
segment projection is exact. Expected agreement = f32 storage rounding
(~1e-5 m at z≈100); gate at 1e-4 m.
"""

from __future__ import annotations

import json
import os
import subprocess

import numpy as np

from . import REPO, bridge, profiles

SYNTH = os.path.join(bridge.OUT, "synth")

TILT = {"grade_x": 0.008, "grade_y": -0.005, "curve_m": 3.0}
VALLEY = {
    "path": {"Points": [{"x": -50.0, "y": 100.0}, {"x": 400.0, "y": 400.0},
                        {"x": 850.0, "y": 700.0}]},
    "floor_z0_m": 96.0,
    "fall_gradient": 0.004,
    "floor_halfwidth": {"knots": [[0.0, 10.0], [0.5, 18.0], [1.0, 26.0]]},
    "wall_grad_left": 0.35,
    "wall_grad_right": 0.2,
    "floor_round_m": 5.0,
    "shoulder_k_m": 4.0,
    "join_trunk": None,
}
EXTENT = 800.0
BASE = 100.0
CELL = 2.0


def _config(with_valley: bool) -> dict:
    return {"schema_version": 1, "extent_m": EXTENT, "base_elev_m": BASE,
            "tilt": TILT, "ridges": [], "bluffs": [], "bowls": [],
            "valleys": [VALLEY] if with_valley else []}


def render(items: list[dict]) -> None:
    """Run `cargo xtask landform-grid` on a manifest of {out, cell, config}."""
    manifest = os.path.join(SYNTH, "manifest.json")
    with open(manifest, "w") as f:
        json.dump(items, f, indent=1)
    subprocess.run(
        ["cargo", "run", "--release", "-p", "xtask", "--", "landform-grid",
         manifest],
        cwd=REPO, check=True, capture_output=True, text=True)


def analytic(with_valley: bool) -> np.ndarray:
    n = int(round(EXTENT / CELL))
    ix = np.arange(n)
    x, y = np.meshgrid(CELL / 2 + ix * CELL, CELL / 2 + ix * CELL)  # [y, x]
    z = profiles.tilt_eval(x, y, BASE, EXTENT, TILT["grade_x"],
                           TILT["grade_y"], TILT["curve_m"])
    if with_valley:
        pts = np.array([[p["x"], p["y"]] for p in VALLEY["path"]["Points"]])
        u, d, side, total = profiles.polyline_project(pts, x.ravel(), y.ravel())
        vs = profiles.valley_surface(
            u, d, side, total, VALLEY["floor_z0_m"], VALLEY["fall_gradient"],
            VALLEY["floor_halfwidth"]["knots"], VALLEY["wall_grad_left"],
            VALLEY["wall_grad_right"], VALLEY["floor_round_m"])
        z = profiles.smin(z.ravel(), vs, VALLEY["shoulder_k_m"]).reshape(z.shape)
    return z


def run() -> int:
    from dtm_metrics.sandbox.campaign import read_hg01
    os.makedirs(SYNTH, exist_ok=True)
    items = []
    for name, wv in (("frame_tilt", False), ("frame_valley", True)):
        cfg_path = os.path.join(SYNTH, f"{name}.json")
        with open(cfg_path, "w") as f:
            json.dump(_config(wv), f, indent=1)
        items.append({"out": os.path.join(SYNTH, f"{name}.hg01"),
                      "cell": CELL, "config": cfg_path})
    render(items)

    ok = True
    for name, wv in (("frame_tilt", False), ("frame_valley", True)):
        h, cell = read_hg01(os.path.join(SYNTH, f"{name}.hg01"))
        assert cell == CELL
        za = analytic(wv)  # [y, x] with y index = south-first == read_hg01
        dmax = float(np.abs(h - za).max())
        status = "OK" if dmax < 1e-4 else "FAIL"
        if dmax >= 1e-4:
            ok = False
        print(f"{status} {name}: max|rust − python| = {dmax:.2e} m "
              f"({h.shape[0]}x{h.shape[1]} @ {cell} m)")
    return 0 if ok else 1
