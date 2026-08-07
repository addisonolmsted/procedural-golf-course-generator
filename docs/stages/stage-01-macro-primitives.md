# Stage S1 — Macro Primitives

**Status:** unclaimed
**Crate:** [`crates/course-primitives/`](../../crates/course-primitives/)

## Purpose

Establish what the site is *predisposed* to be, and hand it to the kernel as
contract C1. S1 decides that this quadrant sits higher, that the material grain
runs NNE, that water leaves through the south edge — and stops there.

The discipline that makes S1 work is knowing where to stop. **S1 does not place
landforms.** It places the conditions under which a kernel will place landforms.
A different kernel given the same C1 must be able to build something
structurally unrelated and have it still make sense.

## Position

- **Upstream:** [S0](stage-00-archetype-draw.md) — `CourseSpec`.
- **Downstream:** [S2](stage-02-skeleton-kernel.md), via contract C1. Nothing
  else reads S1 directly.
- **Contracts:** emits **[C1](../contracts/C1-primitives-to-kernel.md)**.
- **Streams:** `primitives/v1`.

## Inputs / Outputs

**In:** `CourseSpec` — descriptors `relief_budget_m`, `wind_azimuth_rad`,
`strata`; dials `primitives.*`.

**Out:** `PrimitiveField` (`course_contracts::contracts::primitive_field`),
written as `primitives.json` + four CGRID1 fields. Full field list and
invariants in [C1](../contracts/C1-primitives-to-kernel.md).

Four 8 m fields — `tilt`, `relief`, `hardness`, `accommodation` — plus
`StructureMeta` carrying `grain_axis_rad`, `grain_strength`, `base_level`,
`wind_azimuth_rad`, and the strata stack.

## Responsibilities

1. **Base level and tilt.** Choose the edge or corner where water leaves and
   the regional plane. The tilt must be monotone toward that edge — this single
   decision fixes drainage direction for the entire course.
2. **Relief predisposition.** A band-limited, mean-zero field at wavelengths
   ≥ 400 m saying where the site is high and low. Its amplitude comes from
   `relief_budget_m`, but S1 spends only part of that budget; the kernel and
   the finishers spend the rest. Overspending here produces terrain that reads
   as a smooth blob with detail painted on.
3. **Material fabric.** Draw `grain_axis_rad` (an **axis** in `[0, π)`) and
   `grain_strength`. This axis is the origin of every oriented feature in the
   pipeline — tributary elongation, aeolian bands, terracettes, vegetation
   striping. It is passed through C1 and C2 unchanged, all the way to S9 and
   S10. Nine stages of shared orientation is what makes a site read as one
   place.
4. **Hardness and accommodation.** Differential erosion resistance, and room
   for deposition. Where strata are present, `hardness` steps with the stack;
   where absent, it is smooth or uniform.
5. **Emit C1** with all ranges clamped and all invariants enforced at
   construction.

## Biome expression

S1 is a **near-identity pass for four of the six biomes** — they differ only in
amplitude, and the interesting structural decisions happen in S2. The two that
genuinely move S1's dials:

| Biome | What S1 does differently |
|---|---|
| **Hill Country** | Thick strata stack ⇒ strongly stepped `hardness`; high relief amplitude; `accommodation` low outside valley floors. |
| **River Valley** | `accommodation` is the dominant field — a broad high-accommodation floor with low-accommodation valley sides. Tilt is gentle and strongly monotone. |
| Great Plains | Present strata, low relief amplitude, high `accommodation` almost everywhere. |
| Piedmont | Thin strata, mid amplitude. The reference case. |
| Sandhills | Empty strata, uniform `hardness`; `grain_strength` high and aligned to `wind_azimuth_rad` rather than to a geological fabric. **The one biome where grain is aeolian, not material** — same field, different physical origin, no branch. |
| Heathland | Empty strata, uniform `hardness`, low `grain_strength`. Nearly featureless C1 by design; heathland's character is derangement, which is an S2 dial. |

## Determinism & budget

**120 ms** ([../02-performance-budget.md](../02-performance-budget.md)) at the
8 m rung (376²). Analytic per-node evaluation, no iteration.

Fixed draw counts. All transcendentals via `course_world::math` (libm).
`grain_axis_rad` reduced into `[0, π)` at construction — never assigned from a
direction.

## Future-biome seams

C1 is designed kernel-agnostic, so S1 should survive the packs unchanged. Two
anticipated pressures, both recorded in
[C1](../contracts/C1-primitives-to-kernel.md): The North needs an ice-flow
direction (a `meta` addition), and Fire & Stone needs vent points (a genuine
addition, since C1 has no vocabulary for located features).

## Open questions

1. **How much of `relief_budget_m` does S1 spend?** The split between S1, S2,
   and S4 is unfixed. Proposal: S1 ≤ 40%, and S11 measures the total. Needs a
   number before implementation.
2. **Is `grain_strength` one scalar or a field?** One scalar is simpler and
   matches the "single shared axis" principle; a field would let fabric fade
   across a material contact. Leaning scalar until a biome needs otherwise.
3. **Does `hardness` need a length scale?** Currently just a [0,1] field, which
   cannot express "hard in thin bands" versus "hard in broad zones". The strata
   stack may cover this; confirm with the stratigraphy module in S2.

## Acceptance criteria

- [ ] Same seed ⇒ byte-identical C1 artifact, cross-platform.
- [ ] All C1 invariants enforced *at construction*, with tests: no NaN, ranges
      clamped, `relief` mean-zero over the core to within 1 cm, `grain_axis_rad`
      in `[0, π)`, `wind_azimuth_rad` in `[0, 2π)`.
- [ ] `relief` is band-limited: radial PSD shows no significant energy below
      400 m wavelength (test on a spectral assertion, not by eye).
- [ ] `tilt` is monotone toward `base_level.edge`.
- [ ] Resolution consistency: C1 computed at 8 m and at 2 m produces the same
      structure to within interpolation error.
- [ ] Empty `strata` produces a valid, uniform-`hardness` C1 (Sandhills,
      Heathland).
- [ ] Contains no field or local named `ridge`/`valley`/`channel`/`bench`.
- [ ] Golden C1 for seed 1, both golden biomes.
- [ ] Budget: < 120 ms at 8 m.
