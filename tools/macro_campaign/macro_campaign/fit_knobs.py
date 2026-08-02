"""Aggregate extracted tile measurements into 11-point quantile tables and
merge them into course-spec's archetype_priors.json (values only — schema
unchanged; any knob with < MIN_N samples keeps its provisional table).

Writes out/archetype_priors.merged.json by default; --apply installs it to
crates/course-spec/data/archetype_priors.json (then re-bless the course-spec
goldens — see README)."""

import hashlib
import json
import pathlib

import numpy as np

OUT = pathlib.Path(__file__).resolve().parent.parent / "out"
REPO = pathlib.Path(__file__).resolve().parent.parent.parent.parent
PRIOR = REPO / "crates" / "course-spec" / "data" / "archetype_priors.json"

MIN_N = 4
# Below WINSOR_N samples, the table's ends would equal the single most extreme
# tile (q10 == max). Compress to the p10-p90 sample range until the corpus is
# big enough for real tails; self-relaxes at the ~30-tile scale-up.
WINSOR_N = 10
QS = np.linspace(0.0, 100.0, 11)
QS_WINSOR = np.linspace(10.0, 90.0, 11)

# knobs the extractor measures (others — e.g. meander shape-tier before the
# dtm_primitives pass — keep provisional tables until samples exist)
KNOBS = [
    "relief_amp_m",
    "core_relief_cap_m",
    "tilt_grade",
    "dune_wavelength_m",
    "valley_count",
    "valley_fall_grad",
    "valley_halfwidth_m",
    "valley_wall_grade",
    "ridge_count",
    "ridge_len_m",
    "ridge_crest_hw_m",
    "basin_count",
    "basin_radius_m",
    "basin_depth_m",
    "basin_spacing_m",
    "blowout_eccentricity",
    "bench_count",
    "bench_scarp_h_m",
    "bench_tread_w_m",
    "meander_intensity",
    "meander_wavelength_mult",
    # --- network family (M0 measurement, added to the prior at M4) ---------
    # Inert until M5 grows the network from them; they are fitted now so the
    # corpus, the cull and the fingerprint move together once instead of
    # twice.
    "junction_angle_deg",
    "junction_spacing_m",
    "branch_len_m",
    "junction_area_logratio",
    "net_strahler_max",
    "slope_area_theta",
    "net_fall_at_a0",
    "chan_hw_at_a0_m",
    "chan_hw_area_exp",
    "meander_sinuosity",
    "drainage_density_fine",
]

# Knobs whose prior name differs from the key the extractor writes.
#
# `drainage_density_0p25x` names a threshold multiplier, which is a fact
# about how the metric was swept rather than about the landform; the prior
# should name the quantity. The FINE threshold is the one the grower needs:
# it counts low-order tributaries, and growing to the nominal threshold
# instead leaves the interfluves bare (measured: `ridge_mask_area_frac` 0.297
# growing to 2.25 km/km^2 against 0.412 growing to 4.30, real 0.408).
SOURCE_KEY = {"drainage_density_fine": "drainage_density_0p25x"}

# --- gating policy (pilot review, 2026-07-27) ---------------------------------
# Never fitted FROM TILES: design levers, not landscape properties. Real
# landscapes are not golf-routable, so a landscape corpus cannot say what a
# routable core may contain.
DESIGN_KNOBS = {"core_relief_cap_m"}

# ...but a BUILT COURSE is routable by definition, so the cap does have an
# empirical source — it is just a different corpus. These knobs are fitted
# from `out/courses/*.json` (courses.py) instead of the tile extracts.
# Measured 2026-08-02 over 12 courses per region: the authored caps were
# tight by 1.2x (piedmont) to 2.2x (sandhills), and the generated cores were
# correspondingly flatter than anything anyone has actually built.
COURSE_SOURCED = {"core_relief_cap_m"}

# Estimator not yet trustworthy — keep provisional until the estimator is
# fixed (reasons recorded here so the next pass knows what to repair).
QUARANTINED = {
    # Repaired 2026-07-28 (extract v3), no longer quarantined:
    #   valley_fall_grad — now measured on the RAW surface at D8 receivers
    #     with fill-raised cells excluded (was ~1e-4 everywhere, below the
    #     primitive's 5e-4 floor; now 0.008 florida → 0.097 mountain).
    #   ridge_count / ridge_len_m — now dtm_primitives ridgepipe traced
    #     crests gated to "major" (prominence >= max(5 m, 0.10*relief),
    #     length >= 600 m) instead of counting every geomorphon spur
    #     (~25/tile even in florida; now 0-3, florida lowest).
    "dune_wavelength_m": "NOT measurable from these tiles. v2's 384 m for "
    "every archetype had two causes: mcore.radial_psd bins linearly in "
    "WAVENUMBER, leaving only 2 bins (128/384 m) across the whole 100-600 m "
    "dune band, and red-noise power rises monotonically so the argmax is "
    "always the band edge. extract.py::_dune_probe fixes the instrumentation "
    "(log-spaced bins along the strongest direction, peak vs fitted "
    "background) and records it in extras — but a falsification test kills "
    "it as a DUNE statistic: orientation coherence over the 5 Nebraska "
    "Sandhills tiles (R=0.41) is LOWER than over the 6 Cumberland Plateau "
    "tiles (R=0.75), i.e. it ranks structural grain above a real dune field. "
    "Piedmont valley spacing scores like a dune train. The provisional table "
    "(150-400 m, from dune geomorphology) is better than this measurement; "
    "revisit with dune-only crops or a mapped-dune-inventory validation.",
    "meander_intensity": "SATURATED — cannot inform the generator. intensity "
    "is defined as A / A_max where A_max = lam^2/(4*pi^2*2.5*W) is the "
    "generator's own curvature clamp; real channels bend tighter than that, "
    "so the shape pass returns exactly 1.0 on 26 of 27 pilot tiles (the "
    "27th, 0.76). A statistic pinned at its ceiling carries no information. "
    "THE FIX IS A DESIGN CHANGE, not a better estimator: sinuosity (arc / "
    "trend length) measures cleanly and discriminates — 1.18 for confined "
    "mountain and piedmont valleys up to 1.36 for sandhills — and is "
    "recorded in extras as `meander_sinuosity`. The planner should target a "
    "measured sinuosity and solve for the MeanderSpec amplitude that "
    "achieves it, instead of exposing a clamp-relative `intensity` no real "
    "landscape can be measured against.",
    "chan_hw_area_exp": "SIGN IS WRONG. Hydraulic geometry has channel "
    "width GROWING with drainage area (w ~ A^b, b ~ 0.3-0.5); the fitted "
    "exponent is 0.27 mountain / 0.40 piedmont / 0.20 moraine but -0.34 for "
    "sandhills, with 4 of its 6 tiles negative and 1 of 5 piedmont. A "
    "negative exponent says the channel NARROWS downstream, which no real "
    "network does — it is the log-log fit collapsing on the 3-6 usable "
    "reaches a single tile offers over a narrow area range, not a landscape "
    "property. The intercept `chan_hw_at_a0_m` is pivot-centred and so "
    "survives this (that is what the pivot is for), and is fitted. To "
    "repair: pool reaches ACROSS tiles within an archetype before fitting, "
    "so the regression sees two decades of drainage area instead of one. "
    "Provisional meanwhile is from hydraulic geometry, not from this fit.",
    "meander_wavelength_mult": "lam/W inherits the noisy top-width fit: "
    "0.29-12.19 across the pilot, with W floored at 40 m on narrow channels "
    "and reaching 600 m on wide floodplains, so the ratio swings two orders "
    "of magnitude on comparable creeks. Fit an absolute wavelength (or "
    "regress it on drainage area, as terrain-v2's landform_prior does) "
    "rather than a ratio to a fragile denominator.",
}

# Archetype-identity families: a provisional ZERO median on the count knob is
# a design decision the detectors cannot see (D8 fill artifacts fake sandhills
# 'valleys'; scarp-band false positives fake piedmont 'benches'). Where the
# provisional count median is 0, keep the whole family provisional.
FAMILIES = {
    # The network knobs join this family: they are all properties OF a
    # drainage network, so an archetype whose identity is "no through
    # drainage" (a closed-basin dune field) must not have them fitted from
    # whatever the detectors found on its tiles.
    "valley_count": ["valley_fall_grad", "valley_halfwidth_m", "valley_wall_grade",
                     "meander_intensity", "meander_wavelength_mult",
                     "junction_angle_deg", "junction_spacing_m", "branch_len_m",
                     "junction_area_logratio", "net_strahler_max",
                     "slope_area_theta", "net_fall_at_a0",
                     "chan_hw_at_a0_m", "chan_hw_area_exp", "meander_sinuosity",
                     "drainage_density_fine"],
    "basin_count": ["basin_radius_m", "basin_depth_m", "basin_spacing_m",
                    "blowout_eccentricity"],
    "bench_count": ["bench_scarp_h_m", "bench_tread_w_m"],
    "ridge_count": ["ridge_len_m", "ridge_crest_hw_m"],
}


def load_excluded() -> tuple[set[tuple[str, str]], str]:
    """QA cull list written by tile-lab. Returns the (archetype, tile) set
    and a short digest, stamped into the fit report so a campaign version is
    reproducible from the corpus + this list."""
    p = OUT / "exclude.json"
    if not p.exists():
        return set(), "none"
    raw = p.read_bytes()
    doc = json.loads(raw)
    pairs = {(t["archetype"], t["tile"]) for t in doc.get("tiles", [])}
    return pairs, hashlib.blake2b(raw, digest_size=8).hexdigest()


def collect(excluded: set[tuple[str, str]] | None = None) -> dict[str, dict[str, list[float]]]:
    excluded = excluded or set()
    per = {}
    ex_dir = OUT / "extract"
    for arch_dir in sorted(ex_dir.iterdir()) if ex_dir.exists() else []:
        if not arch_dir.is_dir():
            continue
        rows = per.setdefault(arch_dir.name, {})
        for p in sorted(arch_dir.glob("*.json")):
            if p.name.endswith(".regions.json"):
                continue
            if (arch_dir.name, p.stem) in excluded:
                continue
            rec = json.loads(p.read_text())
            # `extras` too: whether a measured quantity landed in knobs or
            # extras is a shape-pass bookkeeping choice, not a statement
            # about whether it can inform the prior. Reading both means
            # promoting a quantity to a knob costs a KNOBS entry rather than
            # an hour-long re-run of the transect pass over the corpus.
            # `metrics` too, for the same reason — `structure.py` writes the
            # threshold-swept drainage densities there, and the fine one is
            # what the network grower must size itself against. Verified no
            # key collides across the three blocks; knobs win if one ever does.
            for block in ("metrics", "extras", "knobs"):
                for k, v in (rec.get(block) or {}).items():
                    if v is not None and np.isfinite(v):
                        rows.setdefault(k, []).append(float(v))
    return per


def course_samples() -> dict[str, list[float]]:
    """Core relief of REAL golf courses, per archetype region (courses.py).

    The cap is the routability contract, so it cannot be fitted from
    landscape tiles — real landscapes are not golf-routable. A built course
    is routable by definition, which makes this the one honest empirical
    source for it."""
    out: dict[str, list[float]] = {}
    cdir = OUT / "courses"
    for p in sorted(cdir.glob("*.json")) if cdir.exists() else []:
        rows = json.loads(p.read_text())
        vals = [float(r["core_relief_m"]) for r in rows
                if r.get("core_relief_m") is not None
                and np.isfinite(r["core_relief_m"])]
        if vals:
            out[p.stem] = vals
    return out


def run(apply: bool = False, version: str = "campaign-pilot-1"):
    prior = json.loads(PRIOR.read_text())
    excluded, exclude_digest = load_excluded()
    samples = collect(excluded)
    courses = course_samples()
    n_fit, n_kept = 0, 0
    report = []
    if excluded:
        report.append(f"  [cull] {len(excluded)} tile(s) excluded (digest {exclude_digest})")
    for arch, blk in prior["archetypes"].items():
        rows = samples.get(arch, {})
        # identity gate: family count knobs whose PROVISIONAL median is 0
        zero_families = set()
        for count_knob, members in FAMILIES.items():
            entry = blk["params"].get(f"landform.{count_knob}")
            if entry is not None and entry["q"][5] == 0.0:
                zero_families.add(count_knob)
                zero_families.update(members)
        for knob in KNOBS:
            key = f"landform.{knob}"
            if key not in blk["params"]:
                continue
            if knob in COURSE_SOURCED:
                vals = courses.get(arch, [])
                if len(vals) >= MIN_N:
                    qs = QS_WINSOR if len(vals) < WINSOR_N else QS
                    q = np.maximum.accumulate(np.percentile(np.array(vals), qs))
                    blk["params"][key] = {"q": [round(float(v), 6) for v in q]}
                    n_fit += 1
                    report.append(
                        f"  [courses] {arch}.{knob}: n={len(vals)} med={q[5]:.4g}"
                    )
                else:
                    n_kept += 1
                    report.append(
                        f"  [thin-courses] {arch}.{knob}: n={len(vals)} < {MIN_N}, "
                        "kept provisional"
                    )
                continue
            if knob in DESIGN_KNOBS:
                n_kept += 1
                report.append(f"  [design] {arch}.{knob}: kept provisional")
                continue
            if knob in QUARANTINED:
                n_kept += 1
                report.append(f"  [quarantine] {arch}.{knob}: kept provisional")
                continue
            if knob in zero_families:
                n_kept += 1
                report.append(f"  [identity-0] {arch}.{knob}: kept provisional")
                continue
            vals = rows.get(SOURCE_KEY.get(knob, knob), [])
            if len(vals) >= MIN_N:
                qs = QS_WINSOR if len(vals) < WINSOR_N else QS
                q = np.percentile(np.array(vals), qs)
                q = np.maximum.accumulate(q)  # monotone guard
                blk["params"][key] = {"q": [round(float(v), 6) for v in q]}
                n_fit += 1
                report.append(f"  [fit] {arch}.{knob}: n={len(vals)} med={q[5]:.4g}")
            else:
                n_kept += 1
                report.append(f"  [thin] {arch}.{knob}: n={len(vals)} < {MIN_N}, kept provisional")
    prior["prior_version"] = version
    prior["fit_provenance"] = {
        "n_excluded": len(excluded),
        "exclude_digest": exclude_digest,
    }
    out_p = OUT / "archetype_priors.merged.json"
    out_p.write_text(json.dumps(prior, indent=1) + "\n")
    print(f"fitted {n_fit} knob tables, kept {n_kept} provisional")
    print("\n".join(report))
    if apply:
        PRIOR.write_text(json.dumps(prior, indent=1) + "\n")
        print(f"\nAPPLIED to {PRIOR}")
        print("Now re-bless (see tools/macro_campaign/README.md):")
        print("  cargo test -p course-spec   # copy new fingerprint into src/prior.rs")
        print("  cargo run -p course-spec --example bless_golden > crates/course-spec/tests/golden_spec_seed_1.json")
        print("  cargo test --workspace      # re-bless course-macro goldens too")
    else:
        print(f"\nwrote {out_p} (use --apply to install)")
