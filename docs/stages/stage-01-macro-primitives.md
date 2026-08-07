# Stage S1 — Macro Structure

**Status:** unclaimed
**Crate:** [`crates/course-primitives/`](../../crates/course-primitives/)

## Purpose

Establish what the site is *predisposed* to be, and hand it to the kernel as
contract C1. S1 decides that this window sits on an escarpment face, that the
material grain runs NNE, that water leaves through the south edge, that a scarp
crosses the north-east corner — and stops there.

The discipline that makes S1 work is knowing where to stop. **S1 does not place
landforms.** It places the conditions under which a kernel will place landforms.
A different kernel given the same C1 must be able to build something
structurally unrelated and have it still make sense.

S1 carries two things that early v2 dropped and that are restored here: the
**categorical window class**, and **structural discontinuities**. Both exist
because smooth continuous fields produce terrain that varies by degree but
never by kind — see [Structural variety](#structural-variety) below.

## Position

- **Upstream:** [S0](stage-00-archetype-draw.md) — `CourseSpec`.
- **Downstream:** [S2](stage-02-skeleton-kernel.md), via contract C1. Nothing
  else reads S1 directly.
- **Contracts:** emits **[C1](../contracts/C1-primitives-to-kernel.md)**.
- **Streams:** `primitives/v1`.

## Inputs / Outputs

**In:** `CourseSpec` — `structure_class` (window class + province config),
descriptors `relief_budget_m`, `wind_azimuth_rad`, `strata`; dials
`primitives.*`.

**Out:** `PrimitiveField` (`course_contracts::contracts::primitive_field`),
written as `primitives.json` + four CGRID1 fields. Full field list and
invariants in [C1](../contracts/C1-primitives-to-kernel.md).

Four 8 m fields — `tilt`, `relief`, `hardness`, `accommodation` — plus
`StructureMeta` carrying `grain_axis_rad`, `grain_strength`, `base_level`,
`wind_azimuth_rad`, the strata stack, and the **discontinuity list**.

## Responsibilities

1. **Realize the window class.** `structure_class.window` says where in an
   implied larger landscape this 3 km window sits — a valley floor, an
   interfluve, an escarpment face, a basin margin, a piedmont slope, a terrace
   flight. It sets the gross shape of `tilt`, `relief`, and `accommodation`
   together, as a coherent set rather than three independent draws.
2. **Base level and tilt.** Choose the edge or corner where water leaves and
   the regional plane, consistent with the window class. The tilt must be
   monotone toward that edge — this single decision fixes drainage direction
   for the entire course.
3. **Relief predisposition.** A band-limited, mean-zero field at wavelengths
   ≥ 400 m saying where the site is high and low. Its amplitude comes from
   `relief_budget_m`, but S1 spends only part of that budget; the kernel and
   amplification spend the rest. Overspending here produces terrain that reads
   as a smooth blob with detail painted on.
4. **Structural discontinuities.** Where `structure_class.provinces == 2`,
   place the boundary curve and its kind — scarp, valley wall, or material
   contact. A discontinuity is a **line across which terrain changes
   character**: `hardness` steps, `accommodation` steps, and S2's network is
   deflected or truncated by it.
5. **Material fabric.** Draw `grain_axis_rad` (an **axis** in `[0, π)`) and
   `grain_strength`. This axis is the origin of every oriented feature in the
   pipeline — tributary elongation, aeolian bands, terracettes, vegetation
   striping, and the `aspect_rel_grain` conditioning that
   [S3](stage-03-amplification.md) reconstructs against. It passes through C1
   and C2 unchanged all the way to S10. That shared origin is what makes a site
   read as one place.
6. **Hardness and accommodation.** Differential erosion resistance, and room
   for deposition. Where strata are present, `hardness` steps with the stack;
   where absent, it is smooth or uniform.
7. **Emit C1** with all ranges clamped and all invariants enforced at
   construction.

## Structural variety

The reason 1 and 4 above exist.

Continuous fields alone produce terrain that varies **by degree**: every course
in a biome is the same kind of place with different numbers. Categorical
structure and discontinuities produce variation **in kind** — an escarpment-face
site and a valley-floor site are not two samples of one distribution, and a
scarp crossing the site is not a large value of any smooth field.

Discontinuities also carry disproportionate golf value. A line where the ground
changes character is where dramatic holes come from: a green benched against a
scarp, a corridor running along a material contact. A pipeline of purely smooth
fields cannot produce one.

This is a restoration. v1's framing stage had both; early v2 replaced them with
smooth fields and never argued the case. The counter-argument — that
discontinuities are hard to certify — is answered by treating each window class
as its own conditioned region of the envelope rather than widening one blob.

## Biome expression

Biomes differ here mostly in amplitude and in **which window classes they draw
often** — an escarpment-face heathland is rare, an escarpment-face hill country
is typical. The two that genuinely move S1's continuous dials:

| Biome | What S1 does differently |
|---|---|
| **Hill Country** | Thick strata stack ⇒ strongly stepped `hardness`; high relief amplitude; `accommodation` low outside valley floors. **Most discontinuity-heavy biome** — scarps are its identity. |
| **River Valley** | `accommodation` is the dominant field — a broad high-accommodation floor with low-accommodation valley sides. Tilt is gentle and strongly monotone. |
| Great Plains | Present strata, low relief amplitude, high `accommodation` almost everywhere. |
| Piedmont | Thin strata, mid amplitude. The reference case. |
| Sandhills | Empty strata, uniform `hardness`; `grain_strength` high and aligned to `wind_azimuth_rad` rather than to a geological fabric. **The one biome where grain is aeolian, not material** — same field, different physical origin, no branch. |
| Heathland | Empty strata, uniform `hardness`, low `grain_strength`, rarely two provinces. Nearly featureless C1 by design; heathland's character is derangement (an S2 dial) plus hummocky texture (an S3 dictionary). |

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
   and S3 is unfixed. Proposal: S1 ≤ 40%, and S11 measures the total. Needs a
   number before implementation.
2. **Is `grain_strength` one scalar or a field?** One scalar is simpler and
   matches the "single shared axis" principle; a field would let fabric fade
   across a material contact — which now matters more, since discontinuities
   are back. Leaning scalar until a biome needs otherwise.
3. **Does `hardness` need a length scale?** A [0,1] field cannot express "hard
   in thin bands" versus "hard in broad zones". The strata stack may cover it;
   confirm against the stratigraphy module in S2. **Hill Country is the biome
   that will expose the gap.**
4. **How does a discontinuity interact with S2's network?** Deflect, truncate,
   or nucleate channels along it? Real scarps do all three depending on kind.
   Needs specifying per `boundary_kind` before S2 is implemented.
5. **Do window classes need per-class envelopes?** Mirrors
   [S0's open question 2](stage-00-archetype-draw.md). Whatever is decided
   there applies here.

## Acceptance criteria

- [ ] Same seed ⇒ byte-identical C1 artifact, cross-platform.
- [ ] All C1 invariants enforced *at construction*, with tests: no NaN, ranges
      clamped, `relief` mean-zero over the core to within 1 cm, `grain_axis_rad`
      in `[0, π)`, `wind_azimuth_rad` in `[0, 2π)`.
- [ ] `relief` is band-limited: radial PSD shows no significant energy below
      400 m wavelength (test on a spectral assertion, not by eye).
- [ ] `tilt` is monotone toward `base_level.edge`.
- [ ] **All six window classes produce a valid, structurally distinct C1** —
      distinct meaning measurably different `tilt`/`relief`/`accommodation`
      configurations, not just different noise.
- [ ] **Discontinuities are real**: where `provinces == 2`, `hardness` and
      `accommodation` show a step across the boundary curve, and the curve
      enters and exits the box rather than terminating inside it.
- [ ] `provinces == 1` produces a valid C1 with an empty discontinuity list.
- [ ] Resolution consistency: C1 computed at 8 m and at 2 m produces the same
      structure to within interpolation error.
- [ ] Empty `strata` produces a valid, uniform-`hardness` C1 (Sandhills,
      Heathland).
- [ ] Contains no field or local named `ridge`/`valley`/`channel`/`bench`.
- [ ] Golden C1 for seed 1, both golden biomes.
- [ ] Budget: < 120 ms at 8 m.
