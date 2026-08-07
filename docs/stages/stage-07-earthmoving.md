# Stage S7 — Earthmoving Realization

**Status:** unclaimed
**Crate:** [`crates/course-earthmoving/`](../../crates/course-earthmoving/)

## Purpose

Build the plan. S7 grades the corridor graph into the ground, proportional to
plasticity, sourcing fill from the borrow sites the router committed to so that
cut and fill balance.

S7 is the first per-hole streaming stage
([../02-performance-budget.md](../02-performance-budget.md)): hole 1 is graded
and playable while hole 6 has not been looked at.

It is also the stage where a bounded-runtime discipline is most easily lost.
Drainage repair *wants* to be a loop — fix a depression, discover you made
another, fix that. **S7 does not loop.** One template pass, one verify, one
bounded repair, then stop and report whatever remains.

## Position

- **Upstream:** [S6](stage-06-routing.md) via
  **[C3](../contracts/C3-corridor-realization.md)**;
  [S5](stage-05-substrate-assembly.md) via C2 (`height`, `drainage_risk`,
  `borrow`).
- **Downstream:** [S8](stage-08-hole-layout.md), [S9](stage-09-micro-repass.md).
- **Streams:** `earthworks/v1`.

## Inputs / Outputs

**In:** one `Hole` from C3 (spine, halfwidths, `earthwork_budget_m3`,
`borrow_sites`, `target_grade`); the C2 heightfield and `drainage_risk`;
`plasticity`.

**Out:** a graded heightfield patch at 0.5 m over the corridor envelope, plus:

```
Earthworks {
  earthworks_version: u32,
  hole: u32,
  patch: Grid<f64>,               // 0.5 m, corridor-local
  cut_m3: f64, fill_m3: f64,
  borrow_used: Vec<(BorrowSiteId, f64)>,
  unresolved: Vec<Depression>,    // what the bounded repair could not fix
}
```

`unresolved` is a first-class output, not an error channel. A bounded pass that
cannot fix everything is working correctly; S11 reports what is left.

## Responsibilities

1. **Grade the corridor**, proportional to `plasticity`, toward
   `target_grade`. At plasticity 0 this is nearly a no-op; at 1 it is a
   substantial reshaping.
2. **Apply drainage-correct templates in a single pass.** The templates are
   pre-shaped grading forms — swales, crowns, tilted planes — that drain
   correctly *by construction*. Prioritized by C2's `drainage_risk`. Doing this
   right is what keeps the repair pass bounded: most depressions never exist,
   rather than being created and then fixed.
3. **Source fill from borrow.** Draw from the hole's assigned `borrow_sites`
   only. A site marked `becomes_water` is left as a pit for S10 to dress as a
   pond; otherwise it is shaped to blend.
4. **Balance mass.** Cut ≈ fill within tolerance. This is not an aesthetic
   preference — imported or exported material is a cost the plan did not budget,
   and the borrow accounting in C3 exists precisely so the balance is solvable.
5. **Verify once** with a Priority-Flood pass to find depressions the templates
   left behind. **Depressions that S3 marked as intended basins are not
   defects** — a kettle pond is supposed to be a hole in the ground, and
   repairing it away destroys the biome. This is the load-bearing reason S3's
   basin inventory is a first-class artifact.
6. **Repair once, bounded.** One breach-repair pass with a fixed budget.
   Whatever it cannot fix goes into `unresolved` and the stage ends.
7. **Clip grading to one hole per cell.** Corridor envelopes may overlap;
   grading may not. A cell is graded by exactly one hole — the lower-indexed
   one. Without this rule, streamed hole 6 modifies ground hole 1 was already
   played on.

## The bounded-repair discipline

Stated explicitly because it is the rule most likely to be eroded during
implementation:

> **One template pass. One Priority-Flood verify. One bounded breach-repair.
> Then stop.**

No `while !depressions.is_empty()`. No "repair until clean". No second verify
to check the repair worked. If the result is unsatisfactory, the fix is a
better template or a tighter envelope — offline, in calibration — never another
iteration at runtime. A stage that can run long is a stage that can miss the
frame budget, and on a streaming stage that means a player arriving at a tee
that does not exist yet.

## Biome expression

Through `plasticity` and `target_grade` only; no biome dials of its own.

| Biome | Plasticity | What S7 actually does |
|---|---|---|
| **Piedmont** | mid | Moderate grading; some fill from creek-side borrow. The reference case. |
| **Great Plains** | mid-high | Ground is already flat, so grading is mostly *adding* interest rather than removing obstruction — the one biome where earthmoving increases relief. |
| **River Valley** | mid | Floodplain needs little; valley-side holes need much. High variance between holes. |
| **Sandhills** | low | **Near-identity.** The dunes are the golf course; grading them away is the one unforgivable failure. Cut and fill both near zero. |
| **Heathland** | low-mid | Kettles are borrow sites *and* intended depressions simultaneously — the case where confusing "defect" with "feature" is most likely and most damaging. |
| **Hill Country** | low | Scarce feasible ground, so what grading happens is concentrated and deep. Highest risk of exhausting a hole's budget. |

## Determinism & budget

**1 600 ms total across 9 holes** — about **180 ms per hole**
([../02-performance-budget.md](../02-performance-budget.md)). Corridor-local
0.5 m patches; a full-extent 0.5 m grid would exceed the entire course budget
on its own.

Fixed pass counts throughout, including the repair budget. Priority-Flood must
use a deterministic tie-break on equal elevations — the classic
platform-dependence bug in flood-fill implementations.

## Future-biome seams

None structural. Future packs arrive as plasticity and template presets.

The anticipated pressure is **carries** (Lowcountry Marsh): a hole with no
walkable ground between tee and green means a corridor envelope with a gap, and
S7 must grade the two ends without inventing a causeway between them. The C3
shape for this is already decided — a span list on `Hole`.

## Open questions

1. **The template library.** What are the templates, exactly, and are they
   parametric forms or sampled? **Blocks implementation.** Prior art:
   `golf-holes/` on the `main` branch has a build-out system worth reading
   before designing this ([MIGRATION.md](../../MIGRATION.md)).
2. **Mass-balance tolerance.** "Cut ≈ fill" needs a number. Proposal: within
   5% of the hole's budgeted volume, with the residual reported.
3. **What is the repair budget?** A cell count, a volume, or a depression
   count? Leaning volume, since it is the same currency as the earthwork
   budget.
4. **Seam handling at the 0.5 m patch boundary.** Adjacent holes' patches must
   agree where they meet. S9 blends, but S7 should probably already match
   elevations at the shared edge. Confirm the division of labour with S9.
5. **Does grading ever need to modify the flow field for S11's drainage audit?**
   Presumably S11 recomputes. Confirm rather than assume.

## Acceptance criteria

- [ ] Same seed ⇒ byte-identical patches, cross-platform. Priority-Flood
      tie-breaks are deterministic and tested on equal-elevation plateaus.
- [ ] **Exactly one template pass, one verify, one repair.** Asserted by
      instrumented call counts, not by inspection. No loop constructs over the
      repair.
- [ ] **Hole 5 builds correctly given only C2 and `holes[5]`** — no dependency
      on holes 1–4. This is the streaming guarantee.
- [ ] A cell is graded by exactly one hole; overlapping envelopes do not
      double-grade.
- [ ] Cut and fill balance within tolerance; `borrow_used` never exceeds a
      site's available volume; no hole exceeds its `earthwork_budget_m3`.
- [ ] **Intended basins from S3 survive**: a heathland kettle is still a
      depression after S7. Test this specifically — it is the failure mode that
      will look like correct behaviour.
- [ ] Sandhills is near-identity: cut and fill both under a small threshold,
      and dune crests measurably preserved.
- [ ] `unresolved` is populated rather than hidden when the repair budget is
      exhausted; forcing an exhaustion produces a report, not an error.
- [ ] Golden patches for seed 1, piedmont and heathland, hole 1 and hole 5.
- [ ] Budget: < 180 ms per hole, < 1 600 ms total.
