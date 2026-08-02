"""Generated-vs-real comparison: does the generator land in the metric
space of the landscapes its prior was fitted from?

Runs the SAME `extract.measure_tile` over generated tiles (written by
`cargo run -p course-macro --example export_tiles --release`) as over the
real exemplars.

Two kinds of quantity are compared, and they answer different questions:

- **knobs / extras** — how big the parts are. A miss here usually means the
  planner is not consuming a knob faithfully.
- **metrics** (`structure.py`) — how the surface is ORGANIZED: how far you
  ever are from a channel, whether low-order tributaries keep appearing as
  the accumulation threshold drops, how much relief exists at the scale a
  hole is played at, how much of the tile is just the regional plane. Every
  knob can be in range while these fail, which is exactly what happened.
  These carry the hard gates.

Because n is small on both sides (~6 seeds vs ~5-11 tiles), "outside the
real IQR" has a real false-positive rate. `--baseline` measures it by
splitting the REAL tiles in half and comparing them to each other: any
metric that flags real-against-real is not evidence about the generator.
"""

import json
import pathlib

import numpy as np

from .extract import _dump_json, measure_tile
from .fit_knobs import KNOBS, load_excluded

OUT = pathlib.Path(__file__).resolve().parent.parent / "out"
REPORT = OUT / "report"

# Structural metrics that carry acceptance gates, with the accepted ratio
# band (generated median / real median). Bands are deliberately loose — the
# question is "same kind of landscape?", not "same tile".
GATES = {
    "dist_to_channel_p50_m": (0.7, 1.5),
    "drainage_density_0p25x": (0.6, 1.7),
    "drainage_density_1x": (0.6, 1.7),
    "drainage_density_4x": (0.6, 1.7),
    "ridge_mask_area_frac": (0.7, 1.4),
    "local_relief_100m": (0.6, 1.6),
    "local_relief_200m": (0.6, 1.6),
    # Added 2026-08-02, and the reason matters: every OTHER gate here is
    # satisfiable by SMOOTHING. A blurred surface keeps its channels where
    # they were, so distance-to-channel, drainage density, local relief and
    # plane residual all improve as it blurs — measured on the M5 spike,
    # where widening the divide blend band drove distance-to-channel to
    # 1.01x and drainage density to 1.03x of real while the terrain visibly
    # degraded into smooth blobs with hairline slashes. Twice in one session
    # the numbers improved and the picture got worse.
    #
    # Median slope is the cheapest quantity that a blur cannot fake: it
    # falls immediately when the surface is smoothed, whatever the channels
    # do. Real-vs-real noise floor is 1.10x p50 / 1.48x max, so this band is
    # wider than the corpus's own sampling spread.
    "slope_median": (0.6, 1.6),
}
# Reported prominently but NOT gated: this is the number that shows whether
# the core-relief cap is making generated cores flatter than real playable
# ground, which is a design question rather than a generator defect.
UNGATED_HEADLINE = ("core_relief_ratio", "core_relief_m", "plane_residual_frac")


def _collect(rec: dict) -> dict[str, float]:
    """Flatten one tile record's knobs, extras and metrics into one map."""
    out = {}
    for block in ("knobs", "extras", "metrics"):
        for k, v in (rec.get(block) or {}).items():
            if v is not None and np.isfinite(v):
                out[k] = float(v)
    return out


def _real_samples() -> dict[str, dict[str, list[float]]]:
    excluded, _ = load_excluded()
    per: dict[str, dict[str, list[float]]] = {}
    ex_dir = OUT / "extract"
    for arch_dir in sorted(ex_dir.iterdir()) if ex_dir.exists() else []:
        if not arch_dir.is_dir():
            continue
        rows = per.setdefault(arch_dir.name, {})
        for p in sorted(arch_dir.glob("*.json")):
            if p.name.endswith(".regions.json") or (arch_dir.name, p.stem) in excluded:
                continue
            for k, v in _collect(json.loads(p.read_text())).items():
                rows.setdefault(k, []).append(v)
    return per


def _real_halves() -> dict[str, tuple[dict, dict]]:
    """Deterministic split of the real tiles by sorted name, for the noise
    floor. Alternating assignment keeps both halves geographically mixed."""
    excluded, _ = load_excluded()
    per: dict[str, tuple[dict, dict]] = {}
    ex_dir = OUT / "extract"
    for arch_dir in sorted(ex_dir.iterdir()) if ex_dir.exists() else []:
        if not arch_dir.is_dir():
            continue
        a: dict[str, list[float]] = {}
        b: dict[str, list[float]] = {}
        files = [
            p for p in sorted(arch_dir.glob("*.json"))
            if not p.name.endswith(".regions.json")
            and (arch_dir.name, p.stem) not in excluded
        ]
        for i, p in enumerate(files):
            side = a if i % 2 == 0 else b
            for k, v in _collect(json.loads(p.read_text())).items():
                side.setdefault(k, []).append(v)
        per[arch_dir.name] = (a, b)
    return per


def _generated_samples(force: bool) -> dict[str, dict[str, list[float]]]:
    per: dict[str, dict[str, list[float]]] = {}
    gen_dir = OUT / "generated"
    cache_dir = OUT / "generated_measured"
    for arch_dir in sorted(gen_dir.iterdir()) if gen_dir.exists() else []:
        if not arch_dir.is_dir():
            continue
        rows = per.setdefault(arch_dir.name, {})
        cache = cache_dir / arch_dir.name
        cache.mkdir(parents=True, exist_ok=True)
        for tile in sorted(arch_dir.glob("*.cgrid")):
            dest = cache / (tile.stem + ".json")
            if dest.exists() and not force:
                rec = json.loads(dest.read_text())
            else:
                result = measure_tile(tile)
                if result is None:
                    print(f"  [drop] {arch_dir.name}/{tile.stem}")
                    continue
                rec = result[0]
                _dump_json(dest, rec)
                print(f"  [ok] {arch_dir.name}/{tile.stem}")
            for k, v in _collect(rec).items():
                rows.setdefault(k, []).append(v)
    return per


def _ratio(gen_med: float, real_med: float) -> float:
    if real_med == 0:
        return float("nan") if gen_med == 0 else float("inf")
    return gen_med / real_med


def baseline(real_halves) -> dict[str, dict]:
    """Noise floor from real-vs-real: split the corpus in half and compare.

    Reports BOTH tests, because they behave very differently at this n:

    - `iqr_flag_rate` — how often half-B's median falls outside half-A's
      IQR. With 2-5 tiles a side the IQR is so narrow that this sits at
      60-100% even for perfectly comparable halves, so the FLAG column is
      decoration, not evidence.
    - `ratio_spread` — how far median(B)/median(A) strays from 1. This is
      what the gates actually test, and it is the number that says whether
      a gate band is wider than the corpus's own sampling noise. A gate is
      only meaningful if its band comfortably contains this spread.
    """
    hits: dict[str, list[int]] = {}
    ratios: dict[str, list[float]] = {}
    for _arch, (a, b) in real_halves.items():
        for key, bv in b.items():
            av = a.get(key, [])
            if len(av) < 2 or len(bv) < 2:
                continue
            q25, q75 = np.percentile(av, [25, 75])
            mb, ma = float(np.median(bv)), float(np.median(av))
            hits.setdefault(key, []).append(int(not (q25 <= mb <= q75)))
            # Fold the ratio to >=1 so spread is direction-agnostic. Skip
            # when either side is 0 (e.g. florida's ridge mask), where a
            # ratio carries no information.
            if ma > 0 and mb > 0 and np.isfinite(ma) and np.isfinite(mb):
                r = mb / ma
                ratios.setdefault(key, []).append(max(r, 1.0 / r))
    out = {}
    for k, v in hits.items():
        rs = ratios.get(k, [])
        out[k] = {
            "iqr_flag_rate": float(np.mean(v)),
            "ratio_spread_p50": float(np.median(rs)) if rs else float("nan"),
            "ratio_spread_max": float(np.max(rs)) if rs else float("nan"),
        }
    return out


def run(force: bool = False, gate: bool = False):
    real = _real_samples()
    gen = _generated_samples(force)
    if not gen:
        print("no generated tiles — run:\n  cargo run -p course-macro "
              "--example export_tiles --release")
        return 0

    noise = baseline(_real_halves())
    REPORT.mkdir(parents=True, exist_ok=True)
    lines = [
        "# Generated vs real — macro landform",
        "",
        "Same extractor (`macro_campaign.extract.measure_tile`) over real "
        "exemplar tiles and over generated `base_height` at 2 m.",
        "",
        "`ratio` = generated median / real median. `FLAG` = generated median "
        "outside the real interquartile range. `noise` = how often the same "
        "test flags when one half of the REAL corpus is compared to the "
        "other half — a row with high noise is not evidence about the "
        "generator.",
        "",
    ]

    gate_failures: list[str] = []
    n_flag = n_cmp = 0

    for arch in sorted(set(real) | set(gen)):
        lines += [f"## {arch}", ""]
        for title, keys in (
            ("Structure (gated)", list(GATES)),
            ("Structure (reported)", list(UNGATED_HEADLINE)),
            ("Knobs", [k for k in KNOBS]),
        ):
            rows = []
            for key in keys:
                rv = real.get(arch, {}).get(key, [])
                gv = gen.get(arch, {}).get(key, [])
                if len(rv) < 3 or not gv:
                    continue
                q25, med, q75 = np.percentile(rv, [25, 50, 75])
                gmed = float(np.median(gv))
                flag = not (q25 <= gmed <= q75)
                ratio = _ratio(gmed, med)
                nz = (noise.get(key) or {}).get("iqr_flag_rate")
                if title == "Knobs":
                    n_cmp += 1
                    n_flag += flag
                if key in GATES:
                    lo, hi = GATES[key]
                    if not (lo <= ratio <= hi):
                        gate_failures.append(
                            f"{arch}.{key}: ratio {ratio:.2f} outside [{lo}, {hi}]"
                        )
                rows.append(
                    f"| {key} | {med:.4g} [{q25:.4g}–{q75:.4g}] | {gmed:.4g} | "
                    f"{ratio:.2f}x | {'**FLAG**' if flag else 'ok'} | "
                    f"{'—' if nz is None else f'{nz:.0%}'} |"
                )
            if rows:
                lines += [f"### {title}", "",
                          "| quantity | real med [q25–q75] | generated med | ratio | | noise |",
                          "|---|---|---|---|---|---|", *rows, ""]

    cal = [
        "### Gate calibration (real-vs-real noise floor)", "",
        "Splitting the REAL corpus in half and comparing the halves. "
        "`ratio spread` is how far median(B)/median(A) strays from 1 on "
        "data that is genuinely the same population — a gate band must be "
        "wider than this to mean anything. (The IQR flag rate is 60-100% "
        "at this sample size, which is why the gates use ratios.)", "",
        "| metric | ratio spread p50 / max | gate band |", "|---|---|---|",
    ]
    for key, band in GATES.items():
        nz = noise.get(key)
        if not nz:
            continue
        cal.append(f"| {key} | {nz['ratio_spread_p50']:.2f}x / "
                   f"{nz['ratio_spread_max']:.2f}x | {band[0]}-{band[1]}x |")
    lines[6:6] = cal + [""]
    lines.insert(6, f"**{n_cmp - n_flag}/{n_cmp} knobs within the real IQR.**\n")
    if gate_failures:
        lines.insert(7, "**Structural gates FAILING:**\n\n"
                     + "\n".join(f"- {g}" for g in gate_failures) + "\n")
    else:
        lines.insert(7, "**All structural gates pass.**\n")

    path = REPORT / "compare.md"
    path.write_text("\n".join(lines))
    print(f"\n{n_cmp - n_flag}/{n_cmp} knobs within the real IQR")
    print(f"{len(gate_failures)} structural gate failure(s)")
    for g in gate_failures:
        print(f"   {g}")
    print(f"wrote {path}")
    return 1 if (gate and gate_failures) else 0
