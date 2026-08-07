# Stage S6 — Routing

**Status:** unclaimed
**Crate:** [`crates/course-routing/`](../../crates/course-routing/)

## Purpose

Find the golf course. Given a substrate, place nine holes forming a returning
par-36 loop, and emit them as a construction plan.

S6 sits at the pipeline's waist. **It knows nothing about terrain generation** —
not the biome, not the kernel, not whether a lake is a kettle or an oxbow. It
sees C2 and only C2. Every number it needs in order to behave differently on
sandhills than on heathland arrives in the `ScorerPreset`.

S6 is also the end of the blocking critical path: nothing can stream until the
route exists, because until then there is no first tee.

## Position

- **Upstream:** [S5](stage-05-siting-substrate.md) via
  **[C2](../contracts/C2-routing-substrate.md)**.
- **Downstream:** [S7](stage-07-earthmoving.md), [S8](stage-08-hole-layout.md)
  via **[C3](../contracts/C3-corridor-realization.md)**.
- **Streams:** `route/v1`.

## Inputs / Outputs

**In:** `RoutingSubstrate`, and nothing else. This is a hard rule and it is
CI-checkable from the dependency graph: `course-routing` depends on
`course-contracts` and `course-world` only.

**Out:** `CorridorGraph` — see [C3](../contracts/C3-corridor-realization.md)
for the full shape. Nine holes in play order, each with a spine, a per-vertex
halfwidth, an earthwork budget, borrow sites, and a neighbour list.

## Responsibilities

1. **Greens first.** Place the nine green sites before any corridor exists.
   Greens are the most constrained element — they need a specific size, grade,
   and drainage — so siting them first turns routing from an open search into a
   connect-the-anchors problem. This ordering is the single biggest determinant
   of search quality.
2. **Beam search** over candidate hole sequences connecting the anchors, with
   fixed beam width and fixed depth.
3. **Anneal** to refine, on a fixed iteration budget and a deterministic
   schedule.
4. **Score** using the C2 preset: earthwork, clearing, drainage risk, and fit,
   minus recoverable borrow.
5. **Saturating penalties, never vetoes.** Nothing in the scorer returns
   infinity. A bad route is expensive; it is never impossible. This is what
   removes the need for retries — a search that can fail is a search that needs
   a fallback, and v2 has none.
6. **Divide the budgets.** S6 is the only stage that sees all nine holes at
   once, so it allocates each hole's earthwork budget and assigns borrow sites
   to exactly one hole. S7 streams and cannot rebalance.
7. **Compute the seam list** (`neighbors`) — which corridors abut — for S9's
   blending pass.
8. **Close the loop.** `holes[0].tee` and `holes[8].green` within the clubhouse
   radius.

## Plasticity

`plasticity` ([../00-architecture.md](../00-architecture.md)) is the dial that
changes what S6 is *doing*, and it enters in two places:

- **Feasibility strictness** (via `preset.feasibility_strictness`): at low
  plasticity a steep cell is nearly disqualifying and the router must find
  ground that already works. At high plasticity it is merely expensive.
- **Fit-versus-cost balance**: low plasticity weights `w_fit` — follow the
  land; high plasticity tolerates `w_earthwork` — build what you want and pay
  for it.

Both arrive as preset numbers. S6 does not read `plasticity` to switch
behaviour; it reads the weights that plasticity produced.

## Biome expression

**S6 has no biome knowledge at all.** Everything below is a consequence of the
preset and the substrate, not of a branch — this table exists so an
implementer knows what shapes to expect, not so they can special-case them.

| Biome | What the routing problem looks like |
|---|---|
| **Piedmont** | The reference case. Rolling ground, creek corridors to cross or use, moderate everything. |
| **Great Plains** | Costs are near zero almost everywhere, so the scorer has nothing to grip. **The under-constrained case** — and the one where the search is most likely to return an arbitrary, characterless route. Fit terms carry the whole burden. |
| **River Valley** | Sharp contrast between floodplain and valley side. Routing wants the floor; interest wants the transition. |
| **Sandhills** | `water`, `clearing`, and `borrow` all identically zero. Low plasticity — the dunes are the point, so the router must fit rather than grade. The over-constrained case. |
| **Heathland** | Patchy fractional water from kettles, high borrow, low plasticity. Many small obstacles rather than a few large ones. |
| **Hill Country** | High `earthwork` and `unbuildable` everywhere; low plasticity. Feasible ground is scarce and the search is genuinely hard. |

Great Plains and Sandhills bracket the difficulty from opposite ends. If the
router produces good courses on both, the scorer is balanced.

## Determinism & budget

**1 300 ms** on the 8 m cost grid
([../02-performance-budget.md](../02-performance-budget.md)) — the last and
largest item on the 3.9 s blocking path.

**Fixed beam width × depth. Fixed anneal iterations.** Not a quality target,
not a time budget checked at runtime, not "anneal until the score stops
improving" — a fixed count from config. A search whose length depends on its
data has a worst case nobody has measured.

The annealing schedule is deterministic and seeded from `route/v1`. Beam
tie-breaks must be total and deterministic: comparing floats for equality and
falling back to insertion order is a platform-dependent bug waiting to happen —
break ties on a stable key.

## Future-biome seams

None structural. S6 is biome-agnostic by construction. Future packs arrive as
new presets and wider certified envelopes for the scorer weights, particularly
Highlands and Fire & Stone, which will push `unbuildable` and `earthwork` into
ranges the base six never reach.

The one anticipated C3 change is **carries** — a hole with no walkable ground
between tee and green — which Lowcountry Marsh will need. The shape is decided
in advance in [C3](../contracts/C3-corridor-realization.md): a span list on
`Hole`, not a break in the spine.

## Open questions

1. **Greens-first siting method.** Candidate generation over the cost grid,
   then a dispersion-aware selection? Or a Poisson-disc seeding biased by cost?
   The second is cheaper and more even; the first is more likely to find the
   genuinely good sites. **Blocks implementation.**
2. **What is the fit term, exactly?** "Follows the land" needs a formula.
   Proposal: alignment of the spine with local contour direction, weighted by
   the grade the hole would need. Needs specifying before the scorer is built.
3. **Clubhouse radius** — a constant, a preset field, or derived from the site?
   Leaning preset field.
4. **Does the router need to place tees, or only greens and spines?** Tee
   placement is comparatively unconstrained and might belong in S8. Leaning
   S6 for the first tee and last green (the loop constraint) and S8 for the
   rest.
5. **Under-constrained sites.** On Great Plains the scorer may be nearly flat
   across the search space. Does the preset need an explicit variety or
   interest term to break the symmetry? Suspect yes; confirm empirically.

## Acceptance criteria

- [ ] Same seed ⇒ byte-identical C3, cross-platform.
- [ ] **`course-routing` imports nothing upstream of C2** — enforced in CI from
      the dependency graph, and grep-clean of `course_skeleton`,
      `course_amplify`, `course_transforms`, `course_spec`.
- [ ] **No biome branch anywhere in the crate** — grep for biome names, expect
      zero hits.
- [ ] No infinities and no vetoes in the scorer; every penalty saturates.
- [ ] Fixed beam width, depth, and anneal count, asserted. No data-dependent
      iteration.
- [ ] All six biomes produce a valid C3 over 1 000 seeds with **zero
      failures** — including Sandhills with three cost fields identically zero
      and Great Plains with near-uniform cost.
- [ ] Every C3 invariant holds: 9 holes, par 36, returning loop, spines in
      core, budgets balance, borrow covers fill, no shared borrow sites.
- [ ] Each hole is independently realizable — proven by an S7 fixture test that
      builds hole 5 without holes 1–4.
- [ ] Tie-breaking is total and stable; no float-equality comparisons.
- [ ] Golden C3 for seed 1, piedmont and heathland.
- [ ] Budget: < 1 300 ms.
