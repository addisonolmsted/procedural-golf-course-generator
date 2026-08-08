# Performance Budget

**The whole course in under 10 seconds. The first tee playable in under 4.5.**

These are hard numbers, not aspirations. Every stage doc restates its own line
from this table in its *Determinism & budget* section, and S11 measures against
it ([stages/stage-11-validation.md](stages/stage-11-validation.md)).

## The table

Single course, cold, 9 holes, release build, one modern desktop core unless
noted.

| # | Stage | Budget | Working resolution | Notes |
|---|---|---:|---|---|
| S0 | Site & biome draw | 1 ms | — | Pure sampling. If this is measurable, something is wrong. |
| S1 | Macro structure | 120 ms | 8 m (376²) | Analytic fields. |
| S2 | Drainage skeleton | 900 ms | 8 m growth → 2 m raster | The largest pre-routing cost; the flow-distance transform dominates. |
| S3 | **Amplification** | 900 ms | 2 m (1501²) | Basis reconstruction per patch, ~300 M multiply-adds. **Unmeasured** — verify early. |
| S4 | **Hydrology & transforms** | 250 ms | 2 m | Flow routing at 8 m + per-cell datum ops. |
| S5 | **Site selection & substrate** | 400 ms | 2 m + 8 m cost | Mostly cost fields; the window scan is a summed-area table and nearly free. |
| S6 | Routing | 1 300 ms | 8 m cost grid | Fixed beam width × depth, fixed anneal iterations. |
| | **— first tee playable —** | **3 871 ms** | | plus hole 1's realization slice (~220 ms) ⇒ **≈ 4.1 s** |
| S7 | Earthmoving realization | 1 600 ms | 0.5 m local patches | Per hole; streams. |
| S8 | Hole layout | 900 ms | plan space + 0.5 m | Per hole; streams. |
| S9 | Micro re-pass | 1 400 ms | 0.5 m corridor | Per hole; streams. |
| S10 | Zoning & aesthetics | 700 ms | mixed | Per hole; streams. |
| S11 | Validation | 400 ms | — | Dev and CI. Runtime may skip it entirely. |
| | **Total** | **8 871 ms** | | **1.1 s headroom against the 10 s ceiling** |

The headroom is the budget's shock absorber, not spare capacity to spend. A
stage that wants more must take it from another stage explicitly, in a doc
change, not quietly at implementation time.

## Streaming order

The course does not have to exist all at once for play to begin.

1. **S0 → S6 are whole-course and blocking.** Nothing can stream until the
   route exists, because until then there is no first tee. This is the 3.9 s
   critical path and it is why S6 gets a fixed iteration budget rather than a
   quality target.
2. **S7 → S10 are per-hole and stream.** They are structured as a per-hole
   pipeline over the corridor graph. Hole 1 realizes first and the player tees
   off while holes 2–9 finish in the background.
3. **A hole is playable when S10 has finished it**, not before — S9 sets the
   surface fields the ball physics reads and S10 sets what is fairway versus
   rough.
4. **Budgets in the table are totals across all 9 holes.** Per-hole, S7 is
   ~180 ms, S8 ~100 ms, S9 ~155 ms, S10 ~78 ms — about 510 ms per hole, so the
   remaining eight complete in ~4.1 s of background work, comfortably inside
   the time it takes to play hole 1.

Streaming has one hard constraint: **a streamed hole may not modify anything a
previously-streamed hole depends on.** S7's grading is corridor-local for this
reason. Where corridors abut, the seam is resolved by S9's blending pass using
only data both holes already had — never by revisiting hole 1 after hole 2
lands.

## Resolution ladder and cost

Cost scales with node count, so the rung choice is the biggest performance lever
in the pipeline:

| Rung | Full-extent nodes | Relative |
|---|---:|---:|
| 8 m | 141 376 | 1× |
| 2 m | 2 253 001 | 16× |
| 0.5 m | 36 012 001 | 255× |

Hence the three rules in [01-conventions.md](01-conventions.md): search coarse,
deliver at 2 m, and touch 0.5 m only in corridor-local patches. A full-extent
0.5 m grid alone would exceed the entire course budget.

## The dictionary asset

[S3](stages/stage-03-amplification.md) is the first stage with a shipped data
dependency. Budget it separately from the per-course time:

- The asset is a **real-patch library** (the spike falsified basis synthesis),
  i16-quantized + zstd. The original < 15 MB target assumed a PCA basis —
  **re-estimate in Phase F**; per-bucket caps and dedup are the levers.
- **Loaded once, resident, shared across courses.** Decompression is startup
  cost, not per-course cost, and the 900 ms above assumes the library is
  already in memory.
- If the asset must shrink, the lever is patch count per bucket, and it trades
  **directly against variety and realism**. Take budget from elsewhere before
  taking it from here.
- The per-course cost now includes one Poisson solve per band (FFT-based, on
  the 2 m and 8 m grids) — cheap, but it is the reason the 900 ms figure still
  needs measuring at G1 start.

## Rules that keep the budget honest

1. **No unbounded loops anywhere in the runtime pipeline.** Every iterative
   stage takes a fixed iteration count from its config. S7's drainage repair is
   the canonical example: one template pass, one Priority-Flood verify, one
   bounded breach-repair, then stop and report whatever remains
   ([stages/stage-07-earthmoving.md](stages/stage-07-earthmoving.md)).
2. **No convergence criteria.** "Iterate until the residual drops below ε" is a
   stage whose runtime depends on its data, which means a seed that takes 40
   seconds exists somewhere. Fixed counts only.
3. **No runtime rejection.** Retrying multiplies the budget by an unbounded
   factor. Certified envelopes exist so this never happens — see
   [calibration/envelope-certification.md](calibration/envelope-certification.md).
4. **Measure at 2 m.** Budgets assume the canonical rung. A stage that only
   meets budget at preview resolution has not met budget.
5. **Regressions are failures.** S11 records per-stage timings against this
   table for the golden seeds; exceeding a stage budget fails CI the same way a
   changed golden hash does.
