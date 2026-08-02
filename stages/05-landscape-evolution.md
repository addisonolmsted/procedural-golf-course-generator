# Stage 05 — Landscape Evolution (the co-author)

**Status:** unclaimed — **critical path: build first**

## Purpose

Run the landscape evolution model that turns forcing fields into plausible
terrain. Stream power organizes the drainage; diffusion and thermal
relaxation give hillslopes and channel flanks their form; the mask region
comes out smooth and gentle BECAUSE its forcing said so. This stage is why
the redesign exists: the retired authored generator measured 22/71 knobs
inside the real-tile IQR — the sim earns those statistics instead of
imitating them.

## Position

- Upstream: Stage 04 (`forcing/` — the ONLY terrain input), Stage 00
  (`lem.*` knobs).
- Downstream: Stage 06 extracts hydrography from the final surface;
  Stage 12 reuses the diffusion/thermal operators for seam blending.
- Streams: none preferred (the legacy erosion core is RNG-free). If any:
  `lem/v1`, attempt scope.

## Contract

Input: `forcing/` artifact, `CourseSpec`.

Output — `terrain/` artifact:

```
height.cgrid          // the macro+meso heightfield at sim res
height_2m.cgrid       // bilinear-refined to RES_FULL_M (2 m) — micro detail
                      // is Stage 12's job, NOT hallucinated here
lem_meta.json {
  iterations_run, dt, mass_ledger: { eroded, deposited, exported },
  datum_report: [ per-datum achieved elev/tolerance ],
}
```

Processes, v1 scope (each with θ on/off + rate):
1. **Implicit stream-power incision** (Fastscape / Braun–Willett O(n)
   ordering, m≈0.5, n=1 closed form) with spatially variable K from
   `erodibility.cgrid`; effective discharge scaled by runoff (rainfall ×
   (1−infiltration)) so `infiltrated` archetypes barely incise.
2. **Uplift** applied per iteration from `uplift.cgrid`.
3. **Linear hillslope diffusion** with spatially variable D (implicit or
   stability-capped explicit).
4. **Thermal/talus relaxation** (slope-limited material shed) — gives scarp
   faces and channel walls their angle-of-repose reading; this plus
   diffusion is the v1 stand-in for lateral erosion (channel width comes
   from Stage 06 hydraulic geometry, NOT simulated).
5. **Depression policy** per `hydrology_mode`: fluvial/alpine =
   priority-flood ε-breach; kettle/water_table = depressions PRESERVED below
   the θ retention threshold (they become Stage-06 ponds/lakes); datum
   outlines are hard floors (never incised below datum elev).
6. Fixed iteration count from θ (determinism — no convergence tests in the
   loop).

Guidance-vs-sim balance is emergent from θ: piedmont sim-heavy (many
iterations, real uplift), florida/sandhills nearly off (few iterations, high
D — constructional initial surface survives), moraine mostly built +
kettle-carved with minimal fluvial work.

## Hard requirements

1. **Mask survival**: on the output surface, mask cells satisfy the Stage-02
   invariant (slope ≤ s_play_max; no channel > a_chan_max off-crossing).
   Tested per archetype at sim res — this stage's forcing discipline, not
   the gate, is the mechanism.
2. Mass ledger balances to tolerance; no NaN/Inf; datum floors respected.
3. Bit-deterministic: fixed D8 scan order, documented tie-breaks, ordered
   solvers, no parallel nondeterminism. Double-run test.
4. Monotone drainage: following receivers from any cell reaches an open
   boundary or a retained depression.
5. Performance: full sim-res run ≤ ~5 s release-mode on a dev laptop
   (budget; measure and record).
6. Golden field hash per archetype (seed-pinned) via the workspace fnv
   convention.

## References

- `main:golf-terrain/src/erosion.rs` — **port seed**: priority-flood
  (Barnes ε), Braun–Willett ordering, implicit m=½ n=1 stream power, mass
  ledger, `fkey` heap determinism trick. 782 lines, proven bit-stable.
- Braun & Willett 2013 (Fastscape); Barnes 2014 (priority-flood).
- `tools/macro_campaign/macro_campaign/structure.py` — the metrics the
  output is calibrated against (drainage density, local relief,
  dist-to-channel); `netstats.py` families (slope-area θ, hw scaling) are
  VALIDATION targets now, not generator inputs.

## Open questions

- Sim res 4 m vs 6 m: measure incision-pattern sensitivity; pick the
  coarsest that passes calibration.
- Implicit vs explicit diffusion (implicit ADI is order-dependent — needs a
  fixed sweep order to stay deterministic). Prototype both, keep simpler.
- Do datum floors need a soft shoulder (blend band) to avoid visible rims at
  outline edges, or does diffusion handle it?
