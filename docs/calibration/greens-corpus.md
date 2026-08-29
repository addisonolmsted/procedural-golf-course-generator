# The green corpus and the fitted site scorer

*2026-08-29 (all measurements this date). Code: `tools/golf/corpus/`.*

## Why

The 22-tile real-green study exposed three failures in green siting: proposals
barely co-located with real greens, they clustered in patches, and the fitted
window covered only 56% of Carolina greens against a 70–90% target. The scorer
was hand-tuned against 519 greens in two regions.

## The corpus

- **Discovery**: 105/135 curated US courses (parkland_atlas geocodes;
  30 misses are genuine OSM gaps — Aronimink has no `leisure=golf_course`
  polygon) + 548 polygons from 10 disjoint regional Overpass boxes.
- **653 course records**; greens assigned to the **smallest containing course
  polygon** (splits resort properties — the confound behind the NC-56%
  number). `>26` greens without sub-polygons ⇒ `multi_course_unsplit`,
  excluded from window fitting, greens kept for the scorer (26, not 22:
  Riviera holds 24 OSM greens = 18 holes + 6 practice).
- **356 keepers** (52 nines, 304 eighteens), **6,506 assigned greens**;
  +73 unsplit properties ⇒ **9,119 greens** in the point corpus.
- **651 3DEP tiles** at 2 m (variable side 3.0–4.5 km), **647 water masks**
  (OSM ∪ flat-plane; 13 flat-plane-only where mirrors 500'd).
- **Fame tiers** (`corpus/fame_tiers.json`): 21 iconic (tier 3), 54
  architect-match tier-2 suggestions in `fame_review.txt` for human review.
- Georeferencing verified before use: greens 1.31× flatter / 1.56× smoother
  than same-polygon controls; AUC 0.593 on flatness alone (0.500 = broken).

## The fit

45,595 rows (9,119 greens vs 36,476 controls, K=4 per green, uniform in the
same polygon, **matched on normalized boundary distance** so coefficients
measure terrain character, not interiority). By-course 80/20 split frozen in
`corpus/holdout.json`. Fame-weighted L2 logistic via IRLS.

| model | held-out AUC |
|---|---|
| previous hand score | **0.4716** — worse than random |
| full 31 features | 0.8504 |
| construction-signature 6 only | 0.7719 |
| `subgrid_rough` alone | 0.6997 |
| **shipped: 25 setting-only** | **0.7928** (train 0.8008) |

**Construction leakage** is why the full model doesn't ship: the DEM is
post-construction, so smoothness features detect "already bulldozed". Under
Carolina pine canopy (noisy returns) the signature vanishes and the full
model scores **0.464** there — worse than chance. `fit.py BUILD_FEATURES`
carries the record.

The hand score failed because `room`, `vis_best/mean`, `recept_best`,
`aroom_best` all fit **negative** — real greens are more enclosed and benched
than the "wide open and visible" pads we rewarded — and `room` is degenerate
(median 0 for greens *and* controls). Strongest new signal: **`tpi60`** —
real greens sit on a +0.2 m local rise at 60 m scale; controls sit at 0.

**Archetype test** (pre-registered: interaction gain ≥ 0.01 AND coefficient
outside pooled CI): only `sandhills_nc` fired (+0.042), but on **3 held-out
courses** — recorded provisional, not shipped. Fame weighting is currently
inert (unweighted fit also 0.7928); kept, reported honestly.

## Recall (the gate that matters)

Full generate→confirm→thin A/B on 72 held-out keeper courses — same window,
same seeding/thinning, only the score differs:

| | within 60 m | within 100 m | median dist |
|---|---|---|---|
| old hand score | 36% | 78% | 70 m |
| **fitted** | **50%** | **84%** | **60 m** |

Per-course within-100: new wins 24, ties 32, old wins 16. Gate (+5) met.

## Windows (the 70–90% coverage target)

With greens assigned per-course, the **shipped dims already exceed the
target**: median coverage 100%, 97% of 356 courses ≥ 70%, every region ≥ 85%.
The old 56% was the multi-course confound, not window failure. Real 9-hole
greens-hulls run 772×390 m / 32 ha (median) — windows are generous — but the
aspect sweep showed thinner windows lose recall, and corridor packing floors
the area from below, so dims stand: 1450×950 sandhills/great plains,
1000×700 the rest.

## Dispersion (the clustering fix that wasn't needed)

Real targets (356 courses): NN-p10 66 m, 7/8 sectors, hull 0.39 of window
(p25 0.30). The new-scorer pool measured on 14 held-out courses: **hull 0.86,
sectors 8/8, NN-p10 57–64 m** — the pre-confirm 40 m stamp-thin plus the
fitted scorer already disperses the pool. The planned sector-quota thinning
was **not built**; `eval_recall` carries the numbers as a standing regression
gate (`dispersion_gate`).

## Files

- `tools/golf/corpus/` — the pipeline; `out/registry/` committed,
  tiles/caches gitignored; re-fetch is resumable and response-cached.
- `tools/golf/greens.py` — `FIT_COEF`/`FIT_B0` transcription; `W_GRADED`
  earthwork cost is a design choice, not fitted.
- `corpus/out/fitted/coeffs.json` — coefficients + CIs + AUC table + leakage
  record. `corpus/holdout.json` — frozen split; every future eval uses it.

## Net etiquette lessons (paid for in wall-clock)

Overpass 200s with zero elements under load — never cache empties. 504s need
per-query tolerance, not per-run. `golf=green` over a 2.6° box needs
quadrant-splitting. Water queries over lake-dense Michigan need fail-fast
(60 s, 2 tries) with flat-plane fallback — OSM water is an enhancement, not
a prerequisite.
