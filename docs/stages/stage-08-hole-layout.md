# Stage S8 — Hole Layout

**Status:** unclaimed
**Crate:** [`crates/course-layout/`](../../crates/course-layout/)

## Purpose

Turn a graded corridor into an actual golf hole: tee, fairway, landing areas,
approach, green complex, hazards, and the width and shape that make it play the
way it should.

S8 is organized as **ten ordered passes**, split between two spaces:

- **Plan space** — corridor coordinates `(s, t)`, where `s` is arc length along
  the spine and `t` is offset from it. Everything about *where things go* is
  decided here, because in plan space a hole is a one-dimensional problem with
  a width, and questions like "is the bunker in the landing area" are trivial.
- **Surface space** — world metres at 0.5 m. Everything about *what the ground
  does* is realized here.

The split is the stage's core idea. Deciding placement in world space means
constantly re-deriving "how far along the hole is this", and every layout system
that skips the plan-space step ends up doing it badly.

## Position

- **Upstream:** [S6](stage-06-routing.md) via C3 (spine, halfwidths, par,
  target grade); [S7](stage-07-earthmoving.md) (graded patch).
- **Downstream:** [S9](stage-09-micro-repass.md),
  [S10](stage-10-zoning-aesthetics.md), [S11](stage-11-validation.md).
- **Streams:** `layout/v1`.

## Inputs / Outputs

**In:** one `Hole` from C3; S7's graded 0.5 m patch; the biome's feature budget
and layout dials.

**Out:**

```
HoleLayout {
  layout_version: u32,
  hole: u32, par: u32,
  spine: Spine,                    // arc-length parameterized (course_world)
  width_profile: Vec<(f64, f64)>,  // (s, halfwidth_m)
  zones: Vec<Zone>,                // Tee|Landing|Approach|Green|Rough|Hazard
  green: GreenComplex,
  hazards: Vec<Hazard>,
  surface_delta: Grid<f64>,        // 0.5 m, corridor-local
}
```

## The ten passes

In order. Passes 1–6 are plan space; 7–10 are surface space.

| # | Pass | Space | What it decides |
|---|---|---|---|
| 1 | **Spine fit** | plan | Fit `course_world::Spine` to C3's polyline; establish arc length. All later `s` values mean something only after this. |
| 2 | **Width profile** | plan | Halfwidth as a function of `s` — pinches at carries, opens at landing areas. |
| 3 | **Zoning** | plan | Tee, landing area(s), approach, green, rough. Landing-area count follows par. |
| 4 | **Green siting** | plan | Exact green position and orientation within the green zone. |
| 5 | **Hazard placement** | plan | Bunkers and water against the feature budget, positioned relative to landing areas. |
| 6 | **Dispersion validation** | plan | Compare this hole's signature against the others; see below. |
| 7 | **Green complex** | surface | Build the putting surface, its contours, surrounds, and run-offs. |
| 8 | **Fairway shaping** | surface | Realize the width profile as actual ground — crowns, tilts, and the contours that feed landing areas. |
| 9 | **Hazard realization** | surface | Cut bunkers, shape their faces and drainage. |
| 10 | **Tee construction** | surface | Level tee pads at their several lengths. |

### Dispersion validation (pass 6)

The pass that stops nine holes from being the same hole nine times.

Each hole has a **signature**: length, dogleg direction and severity, green
approach angle, hazard pattern, elevation change. Pass 6 compares this hole's
signature against those already laid out and, when it is too close to an
existing one, adjusts within the corridor — moving the green within its zone,
shifting the dogleg, re-siting hazards.

The awkward part is that S8 streams, so hole 6 can see holes 1–5 but not 7–9.
Dispersion is therefore **greedy and order-dependent**, which is acceptable
(the result is deterministic and every hole differs from its predecessors) but
should be understood rather than discovered. If greedy dispersion proves too
weak, the fix is for S6 to pre-assign signature targets per hole in C3, where
all nine are visible at once — not for S8 to stop streaming.

## Biome expression

Through the feature budget, the width profile, and the green-complex style.
No biome branches.

| Biome | Layout character |
|---|---|
| **Piedmont** | Balanced parkland. Tree-lined corridors, moderate widths, mid feature budget. |
| **Great Plains** | Very wide corridors; hazards carry nearly all the strategic burden because the terrain carries none. **Highest feature budget of the six.** |
| **River Valley** | Width varies sharply between floodplain and valley-side holes. Water hazards are natural rather than placed. |
| **Sandhills** | Wide, sandy corridors; the dunes are the hazards. Low placed-feature budget — bunkers are blowouts, not constructions. Greens sit in natural hollows. |
| **Heathland** | Narrow corridors between heather rough; kettle ponds as natural hazards. Small, firm greens. |
| **Hill Country** | Corridors constrained by feasible ground; substantial elevation change per hole. Greens benched into slopes. |

## Determinism & budget

**900 ms total across 9 holes** — about **100 ms per hole**
([../02-performance-budget.md](../02-performance-budget.md)). Plan-space passes
are cheap; the surface passes at 0.5 m dominate.

All ten passes have fixed iteration counts, dispersion included: pass 6 makes
**one** adjustment attempt per signature conflict, not a search until the
signature is sufficiently distinct.

Arc-length parameterization must be deterministic — `course_world::Spine`
already handles this; do not reimplement it.

## Future-biome seams

None structural. The ten passes are biome-agnostic; packs arrive as feature
budgets and green-complex style presets.

The anticipated pressure is again **carries**: pass 2's width profile assumes a
continuous corridor. A hole with a gap needs the profile to express "no
corridor here", which is a zero halfwidth span rather than a new concept —
noting it so the profile representation admits zero.

## Open questions

1. **Where does the ten-pass design come from in detail?** The pass list is
   settled; the *contents* of passes 7–9 are not. `golf-holes/` on the `main`
   branch is prior art for green complexes and bunker shaping and should be
   read before designing them ([MIGRATION.md](../../MIGRATION.md)).
   **Blocks implementation of the surface passes**; passes 1–6 can proceed.
2. **Is greedy dispersion enough?** See above. Resolve by measuring signature
   spread across 1 000 courses before committing to the S6 pre-assignment
   alternative.
3. **How many tee pads, and does par drive it?** Presumably 4–5 lengths per
   hole. Needs a number.
4. **Does S8 own tee placement or S6?** [S6's doc](stage-06-routing.md) leans
   toward S6 owning only the first tee and last green (the loop constraint) and
   S8 owning the rest. Confirm and record in one place.
5. **Feature budget units.** Count of hazards, total hazard area, or a cost?
   Leaning area, since it scales with corridor width.

## Acceptance criteria

- [ ] Same seed ⇒ byte-identical layout, cross-platform.
- [ ] All ten passes run in order, with the plan/surface split respected — no
      surface-space pass makes a placement decision, and no plan-space pass
      reads the heightfield.
- [ ] **Hole 5 lays out correctly given only C3 and S7's hole-5 patch** (the
      streaming guarantee), with dispersion comparing against holes 0–4 only.
- [ ] Fixed iteration counts in every pass, dispersion included; asserted.
- [ ] Par drives landing-area count: par-3 holes have none, par-5 holes have
      two.
- [ ] Zones tile the corridor without gaps or overlaps.
- [ ] Hazards respect the feature budget and fall inside the corridor envelope.
- [ ] Green complexes are playable: putting surface grades within the biome's
      `target_grade_green`, with no unpinnable regions.
- [ ] **Dispersion works**: across 1 000 generated courses, signature spread
      within a course is measurably greater than under a no-dispersion control.
- [ ] Sandhills produces a low placed-feature count and sites greens in natural
      hollows rather than constructing them.
- [ ] Golden layouts for seed 1, piedmont and heathland, holes 1 and 5.
- [ ] Budget: < 100 ms per hole, < 900 ms total.
