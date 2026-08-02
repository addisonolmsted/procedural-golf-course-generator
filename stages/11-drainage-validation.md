# Stage 11 — Drainage Validation & Repair

**Status:** unclaimed

## Purpose

Not erosion — plumbing inspection. Re-accumulate flow on the engineered
surface, detect ponding and concentrated flow across play surfaces, and fix
with targeted masked grading (swales, catch basins, fall adjustments).
Iterates with Stage 10 until clean or the iteration cap reports failure.

## Position

- Upstream: Stage 10 (composed surface = base + layers), Stage 09 (play
  surfaces), Stage 06 (the legitimate water network — never "repaired").
- Downstream: Stage 12 blends the repair seams; the final bundle carries
  this stage's report.
- Streams: none — repairs are deterministic consequences of findings.

## Contract

Input: composed surface, routing plan, `hydro/`, `CourseSpec`.

Output — `drainage.json` + repair layers:

```
DrainageReport {
  pass: bool,
  iterations: u32,                      // bounded (θ cap, default 3)
  findings: [ { kind: ponding | concentrated_flow | impounded,
                at: [x,y], area_m2 | acc_m2, surface: fairway|green|tee|walk } ],
  repairs:  [ { kind: swale | catch_basin | fall_adjust,
                layer: "repair<N>.cgrid", volume_m3 } ],
  budget: { reserve_m3, spent_m3 },     // repairs draw a reserved slice of
}                                       // the Stage-02 allowance
```

Checks per iteration on the CURRENT composition (base + Stage-10 layers +
prior repairs), at 0.5 m over play surfaces and 2 m elsewhere:
1. New pits on play surfaces that are not Stage-06 retained water
   (mode-aware: kettles and water-table lakes are never defects; infiltrated
   archetypes tolerate closed sandy hollows below θ volume).
2. Concentrated flow: effective accumulation above θ crossing a green, tee,
   or fairway landing zone.
3. Impoundment: flow dammed against any Stage-10 layer (the upstream water
   network must still reach base level).

Repairs are masked grading layers obeying every Stage-10 layer rule
(feather, budget) — carve a swale to the nearest legitimate drainage line,
insert a catch basin (small graded sink + marked subsurface outlet object
for Stage 13), or adjust falls. Re-check; iterate; on cap exhaustion emit
`pass=false` with full findings (an upstream bug — never papered over).

## Hard requirements

1. Audits the true composition rule (identical to Stage 10's).
2. Repairs never modify Stage-06 water entities or the terrain outside
   their masks; reserve budget respected.
3. Bounded iterations; deterministic; `pass=false` is legal and informative.
4. Clean-run guarantee: on pass, zero findings of any kind remain.

## References

- Stage 05's flow machinery (same D8 core, reused as a library — this stage
  must NOT reimplement flow).
- `main:golf-terrain/src/erosion.rs` routing internals.

## Open questions

- Catch basins: pure model objects (drain water in sim terms) vs visible
  furniture — coordinate the representation with Stage 13.
- Iteration protocol with Stage 10: does 11 call 10's passes for bigger
  fixes (choke re-cut) or only self-repair with grading? Draft: self-repair
  only; a finding needing re-routing-scale work = fail upward.
