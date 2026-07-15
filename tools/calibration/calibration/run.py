"""Orchestrate the calibration: forward campaign -> coverage gate -> emulator +
sensitivity -> continuum p(theta) -> round-trip validation.

    python3 -m calibration.run                 # reuse out/samples/forward.parquet if present
    python3 -m calibration.run --campaign 512  # (re)run the forward campaign first
"""

from __future__ import annotations

import argparse
import os

import numpy as np
import pandas as pd
import yaml

from . import coverage, emulator, forward, inverse, sample, validation
from . import param_schema as ps

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.normpath(os.path.join(HERE, "..", "out"))
REPO = os.path.normpath(os.path.join(HERE, "..", "..", ".."))
REAL_PATH = os.path.join(REPO, "tools", "metrics", "out", "features_scalar.parquet")


def load_real():
    df = pd.read_parquet(REAL_PATH)
    return df[df.clip_variant == "course_extent"].reset_index(drop=True)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--campaign", type=int, default=0, help="run forward campaign with N theta first")
    args = ap.parse_args()
    cfg = yaml.safe_load(open(os.path.join(HERE, "config.yaml")))
    os.makedirs(os.path.join(OUT, "samples"), exist_ok=True)
    ps.dump_json(os.path.join(OUT, "param_schema.json"))

    fpath = os.path.join(OUT, "samples", "forward.parquet")
    if args.campaign or not os.path.exists(fpath):
        n = args.campaign or cfg["sampling"]["n"]
        thetas, U = sample.design(n, seed=cfg["seed"], method=cfg["sampling"]["method"])
        synth = forward.run_forward(thetas, cfg, tag="full")
        synth.to_parquet(fpath)
        np.save(os.path.join(OUT, "samples", "design_U.npy"), U)
    else:
        synth = pd.read_parquet(fpath)
        print(f"reusing {fpath} ({len(synth)} rows)")

    real = load_real()
    synth_ok = synth[synth.status == "ok"].reset_index(drop=True)

    # 1. coverage gate (generator vs ALL real) --------------------------------
    metrics, cov, reach = coverage.run(real, synth_ok, OUT)
    # match only on metrics the generator can actually reach (rest = gaps)
    reach_ok = set(reach[reach.frac_real_reachable >= cfg["inverse"]["match_reach_min"]].metric)
    match_metrics = [m for m in metrics if m in reach_ok]
    print(f"matching on {len(match_metrics)}/{len(metrics)} adequately-reachable metrics")

    # 2. emulator + Sobol sensitivity vs intent map ---------------------------
    emulator.fit(synth, cfg, OUT)

    # 3. fit p(theta) on the FIT split; hold out the rest for validation ------
    rng = np.random.default_rng(cfg["seed"])
    idx = rng.permutation(len(real))
    n_hold = int(round(cfg["validation"]["holdout_frac"] * len(real)))
    real_hold = real.iloc[idx[:n_hold]].reset_index(drop=True)
    real_fit = real.iloc[idx[n_hold:]].reset_index(drop=True)
    inverse.fit_ptheta(real_fit, synth_ok, match_metrics, cfg, out=OUT)

    # 4. round-trip validation vs held-out real (+ baseline sampler) ----------
    res = validation.roundtrip(real_hold, match_metrics, cfg, OUT)

    with open(os.path.join(OUT, "SUMMARY.txt"), "w") as f:
        f.write("CALIBRATION SUMMARY\n\n")
        f.write(f"theta dims: {len(ps.NAMES)}  ({', '.join(ps.NAMES)})\n")
        f.write(f"forward rows: {len(synth)} ({len(synth_ok)} ok)\n")
        f.write(f"metrics matched: {len(metrics)}  (degenerate dropped: {cov['n_metrics_degenerate']})\n")
        f.write(f"real inside synthetic hull (PCA): {cov['frac_real_inside_synth_hull_pca']}\n")
        f.write(f"reach gaps: {cov['metrics_with_reach_gap']}\n")
        f.write(f"energy distance to held-out real: p(theta) {res['energy_matched']:.3f}  "
                f"baseline {res['energy_baseline']:.3f}\n")
    print("\nwrote out/SUMMARY.txt")


if __name__ == "__main__":
    main()
