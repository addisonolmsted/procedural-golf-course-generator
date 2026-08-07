# Stage S11 — Validation

**Status:** unclaimed
**Crate:** [`crates/course-validate/`](../../crates/course-validate/)

## Purpose

Measure the finished course and report. Playability, sightlines, difficulty
distribution, drainage, and per-stage timings.

**S11 reports; it does not reject.** Runtime never retries
([../00-architecture.md](../00-architecture.md)), so there is no branch on
S11's output — a course that scores badly is still the course that seed
produces. That is not a weakness in the design, it is where the design puts the
feedback loop: a bad score is a **calibration bug**, fixed offline by tightening
an envelope, and S11's report is the evidence that sends it there.

S11 is therefore two things at once: a per-run diagnostic, and the measurement
apparatus that keeps the calibration loop closed.

## Position

- **Upstream:** everything. Reads the final heightfield, surface fields,
  layouts, zoning, and the per-stage timing record.
- **Downstream:** nothing in the pipeline. Its consumers are developers, CI, and
  [../calibration/](../calibration/).
- **Streams:** `validate/v1` (playability sim shot dispersion only).

## Inputs / Outputs

**In:** the complete course bundle — S9's heightfield, firmness and TWI; S8's
layouts; S10's zoning and enclosure model; S7's `unresolved` depressions; the
timing record.

**Out:**

```
ValidationReport {
  report_version: u32,
  seed: u64, biome: BiomeId,
  playability: PlayabilityReport,   // per hole + course
  sightlines:  SightlineReport,
  difficulty:  DifficultyReport,
  drainage:    DrainageReport,
  perf:        PerfReport,          // per-stage ms vs budget
  metrics:     MetricVector,        // the battery, for calibration
  verdict:     Vec<Finding>,        // severity-ranked; NOT a pass/fail
}
```

`verdict` is a ranked list of findings, never a boolean. Nothing consumes it as
a gate.

## Responsibilities

1. **Playability simulation.** Play each hole with a distribution of shots from
   a distribution of player abilities. Report reachability, expected strokes,
   how often a reasonable shot finds an unrecoverable position, and whether any
   pin is unreachable. This is the check that catches "the green is behind a
   wall" — the failure mode no geometric assertion finds.
2. **Sightlines versus the enclosure model.** Can the player see the landing
   area from the tee, and the green from the landing area? Blind shots are
   legitimate in small numbers and pathological in quantity, so this is a
   distribution check, not a per-hole assertion.
3. **Difficulty distribution.** Across the nine holes: is there variety, is the
   hardest hole in a sensible place, does the round have shape? A course of nine
   identical-difficulty holes is a failure even if every hole is individually
   good — and it is the failure S8's dispersion pass exists to prevent, so this
   is the measurement that tells S8 whether it worked.
4. **Drainage audit.** Recompute flow on the final surface. Report ponding on
   playing surfaces, plus everything S7 left in `unresolved`.
5. **Performance.** Per-stage timings against
   [../02-performance-budget.md](../02-performance-budget.md). Exceeding a
   stage budget fails CI exactly as a changed golden hash does.
6. **Emit the metric vector.** Run the same battery used on real lidar tiles,
   so generated and real terrain land in one feature space. **This is the
   measurement that closes the calibration loop** and the reason the battery
   must be a pure function of `(height, cell_size, mask)` — see
   [../calibration/metric-battery.md](../calibration/metric-battery.md).
7. **Golden-seed regression.** Defined below.

## Golden seeds

Two archetypes, chosen to bracket the base set:

| Role | Biome | Why |
|---|---|---|
| **Primary** | **Piedmont** | The reference case. Every module at moderate intensity, nothing degenerate. If piedmont breaks, something fundamental broke. |
| **Adversarial** | **Heathland** | The most-different biome in the base set: deranged drainage (negative integration, density ~0.07), closed basins that must survive S7's repair, empty strata, near-featureless C1. It exercises the paths piedmont never touches. |

Sandhills is the natural third candidate — three cost fields identically zero,
no channels at all — and is worth adding as a golden even though it is not one
of the two named archetypes.

**When Lowcountry Marsh lands (Coastal pack), it takes over the adversarial
role for contract C2** ([../contracts/C2-routing-substrate.md](../contracts/C2-routing-substrate.md)).
Heathland remains the adversary for the base six; Lowcountry Marsh becomes the
adversary for the contract itself.

Goldens are hashed with `course_world::world::fnv_f64` and cover every stage
output, not only the final heightfield — a regression in S2 that S9 papers over
must still fail.

## Biome expression

**None.** S11 measures every biome identically. Its *thresholds* are per-biome
(a sandhills course legitimately has more blind shots than a piedmont one), and
those arrive from the biome record, not from a branch.

## Determinism & budget

**400 ms** ([../02-performance-budget.md](../02-performance-budget.md)).
Dev and CI only — **runtime may skip S11 entirely**, and the 400 ms is not part
of the shipping critical path.

The playability sim's shot dispersion is seeded from `validate/v1` with a fixed
sample count. Being the last stage is not a licence to be nondeterministic: a
report that changes between runs cannot be used as a regression baseline.

## Future-biome seams

None structural. Packs add threshold records. The metric battery is shared with
calibration and grows there, not here.

## Open questions

1. **What is the player-ability distribution?** Everything in the playability
   sim depends on it. Needs a concrete model — presumably a small set of
   ability tiers with per-club dispersion ellipses. **Blocks implementation.**
2. **Difficulty metric.** Expected strokes over par is the obvious choice but
   is dominated by length. Something shape-aware is wanted. Unresolved.
3. **How many blind shots are too many?** Needs per-biome numbers, and they
   should come from measuring real courses rather than from taste.
4. **Should S11 emit a machine-readable regression artifact separate from the
   human report?** CI wants stable, diffable output; developers want prose.
   Leaning both, from one pass.
5. **Enclosure model representation** — see
   [S10's open questions](stage-10-zoning-aesthetics.md); S11's sightline check
   cannot be written until it is settled.

## Acceptance criteria

- [ ] Same seed ⇒ byte-identical report, cross-platform. (Including the
      playability sim — fixed sample count, seeded dispersion.)
- [ ] **S11 has no consumers inside the pipeline**: nothing branches on
      `verdict`, and removing S11 entirely changes no generated course. Verify
      by building without it.
- [ ] Playability sim runs all nine holes for every biome and reports
      unreachable pins where they exist — validated against a deliberately
      broken fixture course.
- [ ] Sightline check queries S10's enclosure model successfully.
- [ ] Difficulty distribution detects a low-variety course — validated against a
      no-dispersion control from [S8](stage-08-hole-layout.md).
- [ ] Drainage audit reports ponding on playing surfaces and surfaces S7's
      `unresolved` list.
- [ ] Per-stage timings recorded; exceeding any budget in
      [../02-performance-budget.md](../02-performance-budget.md) fails CI.
- [ ] The metric vector uses the **same battery code** as the lidar measurement
      path — one implementation, not two. Test on a shared analytic surface.
- [ ] Golden hashes cover **every stage output**, not only the final
      heightfield, for piedmont, heathland, and sandhills at seed 1.
- [ ] Budget: < 400 ms.
