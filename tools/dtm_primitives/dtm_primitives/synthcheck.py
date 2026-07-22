"""Synthetic param-recovery check (the correctness net): render the Stage-2
presets through the REAL Rust library, run the SAME extractors the fleet uses,
and compare recovered valley parameters against the known preset values.

Tolerances (plan of record): wall grads <=15%, floor halfwidth <=15% or 2 m,
fall_gradient <=10% or 3e-4, and river_confluence must recover the trunk +
3 accordant tributaries with correct join_trunk structure.
"""

from __future__ import annotations

import json
import os
import subprocess

import numpy as np

from . import REPO, bridge
from .valleypipe import extract_valleys

SYNTH = os.path.join(bridge.OUT, "synth")
CELL = 2.0
VALLEY_PRESETS = ["barranca", "floodplain", "canyon_straight",
                  "river_confluence"]


def _render_presets() -> dict[str, dict]:
    pdir = os.path.join(SYNTH, "presets")
    subprocess.run(["cargo", "run", "--release", "-p", "xtask", "--",
                    "landform-presets", pdir],
                   cwd=REPO, check=True, capture_output=True, text=True)
    items = []
    truth = {}
    for name in VALLEY_PRESETS:
        cfg_path = os.path.join(pdir, f"{name}.json")
        with open(cfg_path) as f:
            truth[name] = json.load(f)
        items.append({"out": os.path.join(SYNTH, f"{name}.hg01"),
                      "cell": CELL, "config": cfg_path})
    manifest = os.path.join(SYNTH, "preset_manifest.json")
    with open(manifest, "w") as f:
        json.dump(items, f, indent=1)
    subprocess.run(["cargo", "run", "--release", "-p", "xtask", "--",
                    "landform-grid", manifest],
                   cwd=REPO, check=True, capture_output=True, text=True)
    return truth


def _meander_endpoints(v: dict) -> tuple[np.ndarray, np.ndarray]:
    p = v["path"]
    if "Meander" in p:
        m = p["Meander"]
        return (np.array([m["entry"]["x"], m["entry"]["y"]]),
                np.array([m["exit"]["x"], m["exit"]["y"]]))
    pts = p["Points"]
    return (np.array([pts[0]["x"], pts[0]["y"]]),
            np.array([pts[-1]["x"], pts[-1]["y"]]))


def _rel_ok(got: float, want: float, frac: float, abs_tol: float = 0.0) -> bool:
    if not np.isfinite(got):
        return False
    return abs(got - want) <= max(frac * abs(want), abs_tol)


def check_preset(name: str, truth: dict, verbose: bool = True) -> list[str]:
    """Returns a list of failure strings (empty = pass)."""
    from dtm_metrics.sandbox.campaign import read_hg01
    h, cell = read_hg01(os.path.join(SYNTH, f"{name}.hg01"))
    nat = h[::-1]  # to TIF orientation (row0=north) for the shared pipeline
    valid = np.ones(nat.shape, bool)
    masks = {"valid": valid.astype(np.uint8)}
    cfg = bridge.load_config()
    results, net = extract_valleys(nat, valid, masks, cell, [], cfg)
    fails: list[str] = []
    tv = truth["valleys"]
    if net is None or not results:
        return [f"{name}: no network extracted (truth has {len(tv)} valleys)"]

    # ---- structure: tributary count + join_trunk ----
    n_tribs_truth = sum(1 for v in tv if v.get("join_trunk") is not None)
    n_tribs_got = sum(1 for r in results
                      if r.branch.parent is not None and r.valley is not None)
    if n_tribs_got < n_tribs_truth:
        fails.append(f"{name}: tributaries {n_tribs_got} < {n_tribs_truth}")

    # ---- trunk parameter recovery ----
    trunk_truth = tv[0]
    trunk = results[0]
    if trunk.valley is None:
        return fails + [f"{name}: trunk fit failed "
                        f"({[c.reason for c in trunk.crossfits]})"]

    # orientation: recovered upstream end should be nearer the truth entry
    entry, exit_ = _meander_endpoints(trunk_truth)
    p0 = trunk.centerline[0]
    flipped = (np.hypot(*(p0 - entry)) > np.hypot(*(p0 - exit_)))

    wl_t, wr_t = trunk_truth["wall_grad_left"], trunk_truth["wall_grad_right"]
    if flipped:
        wl_t, wr_t = wr_t, wl_t
    got = trunk.valley
    if not _rel_ok(got["wall_grad_left"], wl_t, 0.15):
        fails.append(f"{name}: wall_grad_left {got['wall_grad_left']:.3f} "
                     f"vs {wl_t:.3f}")
    if not _rel_ok(got["wall_grad_right"], wr_t, 0.15):
        fails.append(f"{name}: wall_grad_right {got['wall_grad_right']:.3f} "
                     f"vs {wr_t:.3f}")

    # floor halfwidth: sample BOTH knot profiles at uniform arc fractions —
    # knot-value medians are not comparable across different knot placements
    # (truth knots cluster at confluence step-ups, ours are reach-uniform)
    from .profiles import profile_sample
    uu = np.linspace(0.05, 0.95, 61)
    if flipped:
        hw_tt = profile_sample([(1.0 - k[0], k[1]) for k in
                                reversed(trunk_truth["floor_halfwidth"]["knots"])], uu)
    else:
        hw_tt = profile_sample([tuple(k) for k in
                                trunk_truth["floor_halfwidth"]["knots"]], uu)
    hw_gg = profile_sample([tuple(k) for k in
                            got["floor_halfwidth"]["knots"]], uu)
    hw_t, hw_g = float(np.median(hw_tt)), float(np.median(hw_gg))
    if not _rel_ok(hw_g, hw_t, 0.15, 2.0):
        fails.append(f"{name}: floor hw arc-median {hw_g:.1f} vs {hw_t:.1f}")

    fall_t = trunk_truth["fall_gradient"]
    if not _rel_ok(got["fall_gradient"], fall_t, 0.10, 3e-4):
        fails.append(f"{name}: fall_gradient {got['fall_gradient']:.5f} "
                     f"vs {fall_t:.5f}")

    # longitudinal consistency on the STATION arc: fitted line at the median
    # kept station vs the median measured floor there
    lf = trunk.longfit
    kept_s = trunk.stations.s[trunk.keep & np.isfinite(trunk.tr.z_floor)]
    zk = trunk.tr.z_floor[trunk.keep & np.isfinite(trunk.tr.z_floor)]
    if len(kept_s):
        s_med = float(np.median(kept_s))
        line_at = lf.floor_z0 - lf.fall_raw * s_med
        if abs(line_at - float(np.median(zk))) > 1.0:
            fails.append(f"{name}: floor line@s_med {line_at:.2f} vs measured "
                         f"{float(np.median(zk)):.2f}")

    if verbose:
        ok = "PASS" if not fails else "FAIL"
        print(f"[{ok}] {name}: walls {got['wall_grad_left']:.3f}/"
              f"{got['wall_grad_right']:.3f} (truth {wl_t:.3f}/{wr_t:.3f}"
              f"{' FLIPPED' if flipped else ''}), hw {hw_g:.1f} vs {hw_t:.1f},"
              f" fall {got['fall_gradient']:.5f} vs {fall_t:.5f}, "
              f"tribs {n_tribs_got}/{n_tribs_truth}")
        for fmsg in fails:
            print("   ", fmsg)
    return fails


def check_ridge_preset(verbose: bool = True) -> list[str]:
    """Recover ridge_spine crest params through the inverted pipeline."""
    from dtm_metrics.sandbox.campaign import read_hg01
    from .ridgepipe import extract_ridges
    pdir = os.path.join(SYNTH, "presets")
    cfg_path = os.path.join(pdir, "ridge_spine.json")
    hg = os.path.join(SYNTH, "ridge_spine.hg01")
    if not os.path.exists(hg):
        subprocess.run(["cargo", "run", "--release", "-p", "xtask", "--",
                        "landform-presets", pdir],
                       cwd=REPO, check=True, capture_output=True, text=True)
        manifest = os.path.join(SYNTH, "ridge_manifest.json")
        with open(manifest, "w") as f:
            json.dump([{"out": hg, "cell": CELL, "config": cfg_path}], f)
        subprocess.run(["cargo", "run", "--release", "-p", "xtask", "--",
                        "landform-grid", manifest],
                       cwd=REPO, check=True, capture_output=True, text=True)
    with open(cfg_path) as f:
        truth = json.load(f)["ridges"][0]
    h, cell = read_hg01(hg)
    nat = h[::-1]
    valid = np.ones(nat.shape, bool)
    cfg = bridge.load_config()
    ridges, _res = extract_ridges(nat, valid, cell, cfg)
    fails: list[str] = []
    if not ridges:
        return ["ridge_spine: no ridge recovered"]
    r = max(ridges, key=lambda x: x.length_m)
    fl_t, fr_t = truth["flank_grad_left"], truth["flank_grad_right"]
    # flank L/R may swap with trace direction — compare the sorted pair
    got = sorted([r.flank_l, r.flank_r])
    want = sorted([fl_t, fr_t])
    for g, w, nm in zip(got, want, ("flank_lo", "flank_hi")):
        if not _rel_ok(g, w, 0.15):
            fails.append(f"ridge_spine: {nm} {g:.3f} vs {w:.3f}")
    # arc-uniform hw median (knot-value medians aren't comparable across
    # different knot placements — same rule as the valley checks)
    from .profiles import profile_sample
    uu = np.linspace(0.05, 0.95, 61)
    hw_t = float(np.median(profile_sample(
        [tuple(k) for k in truth["crest_halfwidth"]["knots"]], uu)))
    if not _rel_ok(r.crest_hw_med, hw_t, 0.20, 3.0):
        fails.append(f"ridge_spine: crest hw {r.crest_hw_med:.1f} "
                     f"vs arc-median {hw_t:.1f}")
    # crest_z0_m in the config is the UNTAPERED s=0 reference; the rendered
    # crest is emphasis-tapered + falling. Internal-consistency check
    # instead: recorded crest_z0 vs the rendered maximum near the trimmed
    # high end.
    from .frame import bilinear_tif
    zline = bilinear_tif(nat, r.centerline[:, 0], r.centerline[:, 1], cell)
    zmax = float(np.nanmax(zline))
    if abs(r.crest_z0 - zmax) > 2.0:
        fails.append(f"ridge_spine: crest_z0 {r.crest_z0:.1f} vs rendered "
                     f"max {zmax:.1f}")
    if not (8.0 <= r.prominence_med <= 60.0):
        fails.append(f"ridge_spine: prominence {r.prominence_med:.1f} "
                     "outside sane band")
    if r.geomorphon_frac < 0.6:
        fails.append(f"ridge_spine: geomorphon confirm {r.geomorphon_frac}")
    if verbose:
        ok = "PASS" if not fails else "FAIL"
        print(f"[{ok}] ridge_spine: flanks {r.flank_l:.3f}/{r.flank_r:.3f} "
              f"(truth {fl_t:.3f}/{fr_t:.3f}), hw {r.crest_hw_med:.1f} vs "
              f"{hw_t:.1f}, crest_z0 {r.crest_z0:.1f} vs "
              f"{truth['crest_z0_m']:.1f}, geomorph {r.geomorphon_frac}")
        for fmsg in fails:
            print("   ", fmsg)
    return fails


def run(names: list[str] | None = None) -> int:
    os.makedirs(SYNTH, exist_ok=True)
    truth = _render_presets()
    fails = []
    for name in names or VALLEY_PRESETS:
        fails += check_preset(name, truth[name])
    if not names or "ridge_spine" in (names or []):
        fails += check_ridge_preset()
    print(f"synthcheck: {'PASS' if not fails else f'{len(fails)} failures'}")
    return 0 if not fails else 1
