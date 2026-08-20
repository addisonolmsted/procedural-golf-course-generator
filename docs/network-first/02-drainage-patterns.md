# Drainage patterns, and how they make the landforms

**The question:** which drainage pattern does each archetype get, and how does
choosing it first produce that archetype's characteristic macro terrain?

**The short answer, and it is not the expected one: all six Heartland biomes
have the same pattern.** They are all dendritic. The corpus was measured for
this document (§2) and there is no trellis, no parallel, no radial network in
any of them. Archetype character comes from five things layered *on* a
dendritic network, not from the choice of pattern (§4).

That is a narrowing, not a defeat. It means step 1 has less work to do than
planned and steps 5–6 have more, and it says so *before* the network generator
is written rather than after a gate fails.

---

## 1. Why the network can carry the landform at all

In a surface-first pipeline the network is lines cut into ground that already
exists, so it can only decorate the landform. Network-first inverts three
things:

**The interfluves are the landform.** Ground between channels is whatever the
hillslope synthesis hangs off the network. A ridge is not authored — it is the
locus of points equidistant from two channels. So channel spacing sets
interfluve width and channel parallelism sets ridge continuity.

**Structural control is one field with two consequences.** The resistance bands
from step 3 both deflect growing channels *and* place the risers in the
hillslope profile. A bench and the bend that runs along it come from the same
band, so they cannot disagree. In `heartland` these were separate authors — the
stratigraphy module quantised a surface the carve had already cut.

**Terraces are the network at earlier base levels.** A terrace *is* an abandoned
floodplain: the valley floor the river had when its datum sat higher. So a
terrace flight is step 5 run several times at successive datums, with the
valley-side surface blended toward each — which is what a terrace flight
physically is, rather than a stamp applied to finished ground.

---

## 2. What the corpus actually says

Measured for this document over **all 203 clean tiles**, at the planform
family's declared policy (8 m grid, 6 e4 m² threshold — see
`01-measurement-policy.md`). Instruments: `tools/macro_campaign/pattern_survey.py`,
raw output in `pattern_full.txt`.

| biome | n | near-par % | d2c m | density | junction p50 | >80° | aniso | main_share | n_sys | Ω |
|---|---|---|---|---|---|---|---|---|---|---|
| piedmont | 27 | 1.29 | 104.3 | 2.36 | 40.6° | 10.7% | 0.080 | 59.4% | 3 | 3 |
| great_plains | 36 | 3.06 | 115.9 | 2.21 | 39.8° | 8.5% | 0.117 | 56.7% | 4 | 3 |
| river_valley | 32 | 3.04 | 96.2 | 2.60 | 36.9° | 13.0% | 0.146 | **37.5%** | 4 | 3 |
| hill_country | 27 | 2.94 | 106.7 | 2.30 | 45.0° | 11.8% | 0.095 | 56.0% | 3 | 3 |
| heathland | 37 | 1.38 | 97.3 | 2.58 | 39.8° | 13.4% | 0.120 | 48.0% | 4 | 3 |
| sandhills | 44 | 1.95 | 104.3 | 2.39 | 41.6° | 12.4% | 0.145 | 44.7% | 4 | 3 |

`aniso` = length-weighted axial resultant of reach tangents; 0 isotropic, 1 a
single dominant axis. `main_share` = share of channel length in the largest
system. `n_sys` = systems carrying ≥5 % of channel length.

**Everything is dendritic.** Junction medians sit at 37–45°, the acute
confluence angle of an unstructured network; orthogonal junctions are 8.5–13.4 %
everywhere, with no biome showing a 90° mode. Orientation is near-isotropic
throughout. Spacing and density are the known shared invariants. Ω is 3 in every
biome.

Two caveats on these instruments, both of which narrow what the table may be
used to claim:

1. **`aniso` cannot see trellis.** Axial doubling maps a family at 0° and a
   family at 90° onto 0° and 180°, which cancel. It rules out *parallel*
   networks, not trellis ones. **Junction angles are what rule out trellis**,
   and they do so on their own.
2. **`fill_depressions` erases derangement.** Heathland and sandhills are filled
   and then routed, so their d2c / density / pattern numbers describe a
   *phantom* network that the real ground does not carry. These rows are the
   right shared-invariant targets — the corpus measures every biome this way —
   but they are **not** evidence that heathland or sandhills is dendritic.
   Derangement has to be measured with pits kept, as `network_connectivity`
   does (0.04–0.06 for these two against 1.00 integrated).

---

## 3. One growth algorithm, and where the Heartland six sit in it

Six pattern generators would violate the rule that biomes are data and stages
never branch. The patterns are regions of one dial space; the classical named
patterns are its corners.

| Dial | 0 | 1 | Axis |
|---|---|---|---|
| `anisotropy` | growth unbiased | growth locked to the grain axis | dendritic ↔ parallel |
| `junction_target` | acute (~40°) | orthogonal (~90°) | dendritic ↔ trellis |
| `resistance_response` | cut through hard bands | deflect and run along them | unstructured ↔ trellis |
| `trunk_count` | — | 1–3 mouths on the base edge | — |
| `external_inflow` | trunk drains only the tile | trunk carries a large outside catchment | piedmont ↔ river valley |
| `integration` | every branch ends in a pit | every branch reaches base level | deranged ↔ integrated |
| `density` | — | spacing, via the area threshold | sets `d2c` |

**All six Heartland biomes sit in the same corner of the first three dials**:
`anisotropy` ≈ 0.1, `junction_target` ≈ 40°, `resistance_response` low. The
dials are still built — they cost nothing at their defaults and the future packs
need them (Highlands' incised structural valleys, Desert's parallel fans) — but
**they are not what separates the base six** and no effort should go into tuning
them there.

The three that *do* carry base-six variation are `external_inflow`,
`integration` and `density`.

**`integration` is the reorder's clearest win.** A constructive network simply
does not connect a branch; it terminates in a pit. In `heartland` derangement
had to be won *against* the flow router, and the S2 bench recorded the cost —
heathland and sandhills *"draw a network their ground does not carry"*, 296–467
extracted reaches against 76–103 elsewhere, carried at 0.28–0.49×. Constructed,
carried-vs-claimed is 1 by definition.

---

## 4. Trunk dominance is discharge, not catchment — measured

The intuition that a river valley should be dominated by a single trunk is
right about the landscape and wrong about the window, and the difference decides
how step 2 is built.

`main_share` for river_valley is **37.5 %, the lowest of the six** — its
drainage is the *most* evenly divided, not the least. A probe
(`tools/macro_campaign/trunk_probe.py`, output `trunk_probe.txt`) explains it:

| biome | max_acc / tile_area | flat_floor % | relief p50 m |
|---|---|---|---|
| piedmont | 0.550 | 0.7 | 40.0 |
| great_plains | 0.478 | 2.8 | 40.5 |
| **river_valley** | **0.283** | 2.3 | **2.8** |
| hill_country | 0.627 | 0.1 | 116.3 |
| heathland | 0.448 | **12.7** | 17.4 |
| sandhills | 0.361 | 0.1 | 39.1 |

No system in a river_valley tile gathers even 30 % of the tile's own area. **A
tile-local flow router cannot see the trunk's catchment, because the catchment
is upstream of the window.** The trunk dominates by *discharge*, sourced
outside — which is why `heartland` injected an external area at the far boundary
and why `width_survey` measured rv rivers at 53 m mean against piedmont's 29 m.

> **Design consequence for step 2:** trunk dominance is **declared as external
> inflow at the mouth**, never grown from tile-internal area. A network grown
> only from the tile's own 9 km² produces three or four comparable systems in
> every archetype, including river valley.

### Trunk spacing is deliberately wide — SETTLED 2026-08-20

Step 2 places **main trunks only**, at a 750 m floor plus a divide test, which
gives median spacing near 1300 m against 510–784 m for all real outlets at the
6e4 m² channel threshold. **That gap is the design, not a residual**, and it is
recorded here so a later round does not "fix" it:

- the medium and small systems that fill the space between trunks are **step 4's
  tributaries**, and they do not exist yet;
- the comparison was never like-for-like. A real 3 km border carries 2.4–3.9
  outlets at the channel threshold but only **0.3–0.55 at trunk scale**
  (1e6–2e6 m²), and most borders carry none — `outlet_sweep.txt`.

**Judge trunk spacing again once tributaries exist, not before.**

---

## 5. A corpus problem this survey exposed — **resolved by 03, kept for the record**

The relief column above is not a rounding artifact. Across all 32 clean
river_valley tiles:

- **26 of 32 have under 10 m of relief; 18 have under 5 m.** Median 4.6 m.
- Only **4** reach 17 m. The maximum is 99.7 m.

`docs/biomes/river-valley.md` defines the biome as *"genuinely two-part and that
is the whole character — a **flat floor** and a **stepped rise**, meeting at a
distinct break."* **The corpus contains the floor and not the rise.**

The cause is visible in `macro_campaign/regions_v2.py`: the river_valley top-up
leaned on bottomland-hardwood refuges — Tensas, Upper Ouachita, Felsenthal,
White River — because *"the first pass survived worst here — 6/20 clean, the
rest farmed or graded (valley bottoms are exactly where people build)."*
Protected bottomland is, by definition, the wet floor. **The develop screen
selected against the terraces, because terraces are where the farms are.**

Two consequences, both sharp:

1. **`hypsometric_bimodality` — river valley's designated identity discriminant
   (`docs/calibration/targets.md`) — is passing for the wrong reason.** Measured
   over the rv corpus its median is 2.65 against piedmont's 2.38: no separation.
   And it scores **15.67 on a tile with 2.6 m of total relief** — it is finding
   two modes in micro-relief, not a floodplain against a valley side. This is
   the "8/8 metrics in band, visually fake" failure mode with a name attached.
2. **Calibrating river valley to this corpus produces flat bottomland**, not the
   terraced valley the biome doc describes. `heartland` built 55–75 m of terraced
   lift against a corpus median of 4.6 m, and rv carried the worst per-biome
   energy distance in the base six (9.66, later 6.58).

### The resolution uses architecture that already exists

`WindowClass` in `course-contracts` already contains **`ValleyFloor`** and
**`TerraceFlight`** as distinct categorical classes, and
`docs/00-architecture.md` already argues that *"categorical draws give courses
that differ in kind"*. The corpus simply was never split along that axis.

So: **label the river_valley corpus by window class and fit per class, rather
than pooling.** The 26 low-relief tiles are `ValleyFloor` and are a perfectly
good target for bottomland courses. The terraced class needs either the 4
valley-spanning tiles accepted as a thin corpus, or a re-sample aimed
deliberately at valley-spanning windows.

**RESOLVED 2026-08-20 — see `03-macro-is-designed.md` §5.** The macro landform
is a design target, not a realism target, so the terrace flight is authored to a
golf spec and no terraced corpus is required. The 26 low-relief tiles remain an
excellent `ValleyFloor` **texture** corpus; valley-side texture comes from hill
country's exemplar pool via the dictionary's position conditioning. **No
re-sampling campaign is needed and river valley is not blocked.**

The diagnosis above stands and is worth keeping: it is why `hypsometric_bimodality`
must not be trusted as river valley's identity discriminant, and it is a worked
example of a metric passing for the wrong reason.

---

## 6. So what actually differentiates the archetypes

Since the pattern does not: the network fixes the ridge-and-valley skeleton at
~100 m spacing in *every* archetype, and the archetype is then made by what is
hung off it.

| | carries the character | which step |
|---|---|---|
| **Piedmont** | mid relief (p50 48 m); 3 comparable systems; smooth **convex** interfluves | 6 |
| **Hill Country** | **high relief (p50 74 m)**; benched cross-section — every slope a staircase, from the resistance bands | 3 → 6 |
| **Great Plains** | low relief held flat by a caprock; **sparsest** network (d2c 116, density 2.21); breaks into small scarps | 3 → 6 |
| **River Valley** | **one trunk with large external inflow**; flat floor + **authored side-asymmetric terrace flight** (step 5 at successive datums) | 2 → 5 |
| **Heathland** | `integration` ≈ 0 — many small systems each ending in a pit; basin-and-hummock; flat_floor 12.7 %, the highest | 4 |
| **Sandhills** | **no fluvial structure at all** — aeolian dune trains, 100–400 m, oriented by wind | 1 |

Sandhills is the honest limit of a network-first design: it has no network
(`docs/biomes/sandhills.md`: *"no surface water whatsoever"*). Under this
pipeline that is not a degenerate drainage pattern but a **different structure
generator selected at step 1**, emitting ridge-and-hollow lines instead of a
drainage tree. Step 6 hangs hillslope form off *those* lines with the same
machinery — the synthesiser does not care whether the lines it interpolates
between are channels or dune crests. That is the cleanest statement of what step
1 is for: it chooses **which kind of structure** the tile has, and drainage is
one option among them.

---

## 7. What this predicts, and how it gets falsified

Every row of §6 is measurable at **M2, before any surface exists**: the dial
setting must reproduce that biome's `d2c`, density, junction distribution,
`main_share`, `n_sys` and `near_par_frac` from §2, and the **dispersion** across
seeds must sit inside the corpus per-tile spread (DR 0.7–1.3).

And one prediction is already falsifiable now: if the network generator is built
with per-archetype *pattern* dials, §2 says they will all be tuned to the same
values. That work should not be done.

If a setting hits the scalars but the render reads wrong, discipline rule 5
applies — draw the overlay before touching a dial. §5 is what that rule looks
like when a metric passes for the wrong reason.
