# Attempt 4 — network-first terrain

**Branch:** `network-first` (off `heartland`, 2026-08-19)
**Scope:** replaces today's S0–S4. S5–S11 stay doc-only stubs; C0/C2/C3 stay as
the seam terrain owes downstream.

---

## 1. The carry-over allowlist

> **The rule.** Nothing from a previous attempt enters this branch unless it is
> on this list. Adding to the list is an explicit, written decision made
> *before* the code is written. Anything admitted is **copied**, never depended
> on, and carries a one-line provenance comment naming its source commit.
>
> This rule exists because the previous restart reused machinery silently and
> walked straight back into the failures it was meant to escape.

### Admitted (user decision, 2026-08-19)

| Item | Why it is not approach-specific |
|---|---|
| `tools/metrics/` — `core.py`, `features.py`, `METRICS.md`, `config.yaml` | Pure `f(height, cell_size, mask)` kernels, tested against analytic surfaces (plane, tilt, cone, sinusoid, fBm). The *same code* measures a 3DEP tile and generator output, which is the only reason real and synthetic land in one feature space. Its own doc: *"Reuse; do not rewrite."* |
| The corpus — `tools/macro_campaign/out/` (~8.5 GB) plus the corpus-side measurement scripts `structure.py`, `netstats.py`, `flow.py`, `horton_real.py`, `junction_real.py`, `real_planform.py`, `cgrid.py` | 4.1 GB of real 2 m DTM tiles, 64 real course grids, the `exclude.json` / `review_v2.json` cull lists. Irreplaceable measured data; 203 clean tiles. |
| `assets/dictionary_v2.bin` (44 MB) + `.fingerprint`, and the `CDIC` reader `crates/course-amplify/src/dictionary.rs` | Real lidar residual patches per conditioning bucket. Measured data plus its format reader — not algorithm. |

### Not admitted

Everything in `crates/course-skeleton/src/fluvial/` (`carve.rs`, `catena.rs`,
`construct.rs`) and `crates/course-skeleton/src/modules/`; `course-primitives`'
macro-as-heightfield; `course-amplify`'s `synth`/`conditioning`/`blend`;
`course-transforms`' hydrology. All of these encode the subtractive
surface-first assumption this attempt replaces.

### Admitted — substrate (user decision, 2026-08-19)

| Item | Why it is not approach-specific |
|---|---|
| `crates/course-world` | Grid/GridSpec, the **CGRID1 codec** (the interchange format the frozen Python ruler reads), `flow::route` (priority-flood + D8, **pinned by parity fixtures to `macro_campaign/flow.py`**), Vec2/libm bit-stable math, splines, noise, ease. ~1 950 LOC, deps `serde` + `libm` only, zero pipeline types. |
| `crates/course-seed` | `DetRng` (ChaCha8 over blake3, explicit 53-bit mantissa mapping), `RunIdentity`, the stream registry. ~820 LOC, no pipeline types. |

**Used in place, not copied** — the same exception as the ruler, for the same
reason. Forking `Grid`/CGRID1 would create two interchange formats, and forking
`flow::route` would create a second D8 that can disagree with the ruler's D8 in
ways indistinguishable from a generator defect. The parity fixtures are the
whole point: they are what makes M3's *"does the synthesised surface route to
the constructed network"* check mean something.

*Trim on arrival:* `course-seed/src/streams.rs` stream names are v2-stage-shaped
and the `Scope`/reroll machinery is already documented as vestigial.

### Flagged as likely re-requests — **do not take without asking**

`course-viz` + `tile-lab` are a self-contained heightfield-plus-network viewer.
Rebuilding them buys nothing — but reuse stays opt-in, per the rule.

---

## 2. Why this attempt exists

Four generations, each retired on measurement:

| Branch | Approach | Why it died |
|---|---|---|
| `main` (1 commit, frozen) | noise + hydrology parameters, 88-knob fitted prior | **22 of 71 knobs inside the real IQR** (`tools/macro_campaign/out/report/compare.md`). Unroutable and unnatural. Noise has no structure. |
| `pipeline` | 14-stage, a landscape-evolution model as co-author | a LEM has structure *and* statistics but cannot be bounded or certified inside a sub-10 s no-retry budget |
| `archetype-pipeline` | 10-stage typed contracts, reroll on gate fail | superseded |
| `heartland` | 12-stage: macro heightfield → flow routed over it → network *read off* the result → texture quilted from real lidar patches | got genuinely close — pooled ED 2.30, D5 passing, `d2c` in the corpus band — then hit a ceiling on network *shape* that no dial reaches |

Two artifacts state the ceiling from opposite sides:

- `crates/course-skeleton/src/fluvial/divides.rs:1` — *"an authored divide that
  disagrees with the flow field is the exact failure mode of the retired
  generator."*
- The trunk-meander revert in `construct.rs` — *"displacing the trunk off the
  flow path carves a corridor across ground that does not drain there, the
  erosion declines to adopt it, and what is left is a straight artificial cut."*
  Measured: constructed-vs-carried disagreement doubled, 26 → 56 m on piedmont.

Same fact both times. **When a network and a surface are generated
independently, one has to be argued into agreeing with the other, and the
argument is unwinnable.** `heartland` made the surface authoritative, which
bought correctness for free — a D8 receiver graph is a forest, so loops and
crossings are structurally unreachable — and cost expressiveness, because the
network can only ever be as interesting as flow over a band-limited macro
allows. Every residual defect is a shape defect.

**This attempt inverts the authority: the network is the primary object and the
surface is a consequence of it.** That removes the disagreement instead of
refereeing it.

---

## 3. The pipeline

| # | Stage | Owns | Emits |
|---|---|---|---|
| 0 | Archetype draw | biome, structural class, site descriptors | spec |
| 1 | **Structural template** | drainage pattern *and the macro constraints it implies* | template |
| 2 | Trunk placement | 1–3 mouths on the base edge, trunk axes, external inflow | trunk polylines |
| 3 | **Macro fields** | grain, resistance bands, escarpment traces, relief predisposition — **fields, not terrain** | steering fields |
| 4 | Network growth | fractal headward branching, steered by (3), down to ~2 m | 2-D network graph |
| 5 | **Long profiles** | node elevations; base level, concavity, relief budget | 3-D network graph |
| 6 | **Surface synthesis** | hillslope form off the network; divides emerge | base heightfield |
| 7 | Texturing | fabric + amplitude, from the dictionary, conditioned on (4)–(6) | textured heightfield |
| 8 | Re-conditioning | drainage consistency, divide rounding | final heightfield |
| 9 | Hydrology | water surfaces, ponds, floodplain | water + final |

Three properties of this ordering are load-bearing.

**No heightfield exists until step 6.** Step 3 emits *fields* — grain, hardness,
scarp traces, relief predisposition. They steer network growth; they never
compete with it.

**Step 6 is synthesis, not carving.** Divides emerge where interpolation from
two channels meets. They are never authored, so the `divides.rs` rule survives
intact.

**Steps 1–3 keep the pattern coherent.** A trellis network is a consequence of
folded alternating-resistance beds, radial of a dome, parallel of a uniform
steep slope. Drawing a pattern and generating macro independently can
contradict itself, and contradictions read fake even when every scalar is in
band.

### The candle-wax objection, and why it does not apply

`docs/stages/stage-02-skeleton-kernel.md` warns that elevation as
`f(flow_distance, hillslope_position)` gives *"smooth, tubular interfluves — the
characteristic candle-wax signature of hydrology-first synthesis"*, and
`heartland` accepted it, delegating all spur-and-hollow structure to texture.

Candle wax is a symptom of a **sparse** network plus smooth interpolation. A
network grown to 2 m has no smooth interfluve left — every hollow is a channel
head. This is also what `heartland`'s round-7 dendritic-gully work was trying to
bolt on afterwards; it was reverted with the verdict that the dendrites *"read
as randomly placed… hash seeding and synthetic growth had no relation to
drainage structure."* Growing them as part of the network is the version of that
idea that can work, because placement comes from the drainage tree by
construction.

### The band split

`heartland` divided ownership by wavelength (S1 ≥400 m / S2 64–400 m / S3
<64 m). A 2 m network breaks that. Replace the wavelength cut with an ownership
cut:

> **The network owns geometry and placement at every scale.
> The dictionary owns fabric and amplitude.**

The dictionary's conditioning axes — slope, TPI, hillslope position,
distance-to-channel — are all still produced here, so `dictionary_v2.bin`
survives as an asset and only the conditioning is re-derived. Its role narrows
from *"supplies the spur-and-hollow structure no distance transform can"* to
*"supplies the surface fabric"*, which is what patch quilting is actually good
at.

### What the two texturing steps mean

Step 7 adds fabric. **Step 8 is drainage-consistent re-conditioning** — short,
monotone-preserving, plus divide rounding. Measured justification: after
`heartland`'s texture pass, valley-line connectivity fell to 0.184 against a
real 0.744 and `moran_60` to 0.272 against 0.408. Texture in the corridor
fragments valley lines. Step 8 is a finisher, **never the author** — "erosion"
has already failed here once as a general-purpose realism source.

### Routability — SUPERSEDED 2026-08-20, see `03-macro-is-designed.md`

Attempt 1 died on unroutable terrain, so a blocking gate is the obvious
reflex. The corpus says don't:

- `docs/calibration/d6w-siting-baseline.md` — *"**Real courses do not require
  calm ground.** Sand Hills GC plays at 0.45 calm on 19 m of window relief…
  **no v2 biome produces unroutable seeds**."* And: *"**Architects don't even
  take the calmest window**… the scorer should treat calm as a FLOOR, not a
  maximand — otherwise generated courses will systematically sit on duller
  ground than real ones."*
- `docs/gates/G-SKELETON.md` — generated cores pass the proxy at
  100/80/40/40/20/20 % against real cores at 53/51/0/7/9/5 %. Generated land is
  *more* sitable than real land in every biome.

The finding that survives is **calm is a floor, not a maximand** — optimising
`frac_under_cap` makes duller terrain than reality.

What changed: measuring the proxy over the corpus (`03-macro-is-designed.md` §1)
showed that **corpus-matched macro terrain is mostly not routable** — 0 % of real
hill-country tiles pass, piedmont 30 %, and river valley fails for being too
*flat*. So the proxy is **the macro gate**, not a readout; the macro landform is
designed for golf while texture stays fitted to the corpus. The two statements
are compatible: meet the floors, then maximise interest — never maximise calm.

---

## 4. Discipline

1. **New crates, new names.** Nothing depends on `course-skeleton`,
   `course-primitives`, `course-amplify`, or `course-transforms`.
2. **The allowlist above is the only door.**
3. **The ruler is frozen.** Carried measurement code does not change during the
   build. Changing generator and metric together makes every result
   uninterpretable.
4. **Ablation over argument.** `heartland`'s most expensive lesson and its
   cheapest win: four wrong diagnoses of "the carves look artificial" were each
   killed by an ablation plus a stage dump, never by reasoning — and the real
   cause turned out to be that `box_blur_sep` has *square* support, so smearing
   a cut that lives on a D8 chain paints rectangles. Every claim about a visual
   defect gets a toggle and a render before a fix is written.
5. **P1 overlay review before any stage's metrics are trusted.** Two independent
   results demand it. (a) Drawn-coefficient PCA synthesis passed 8/8 metrics and
   was visually fake. (b) `docs/03-success-indicators.md` — *"every scalar was in
   range while three detectors were geometrically wrong; only drawing the
   overlays on the hillshade exposed it. Metrics cannot tell you a thing is in
   the wrong place."*
6. **Each stage carries an expected-failure list** alongside its targets. A
   step-6 surface is *supposed* to fail texture metrics.

---

## 5. Milestones

| M | Content | Verification |
|---|---|---|
| **M0** | branch, allowlist, scaffold, **ruler** | corpus bands re-measured under one declared extraction policy and written down once — `01-measurement-policy.md`; drainage patterns measured — `02-drainage-patterns.md`; macro/texture authority split — `03-macro-is-designed.md` |
| **M1** | steps 0–3 | blind legibility on the fields alone: a reviewer names the drainage pattern from a field render (≥80 %, the analogue of the existing C1 class-legibility test). *Expected failure: there is no terrain to judge.* |
| **M2** | step 4, network growth | **the main practical payoff of the reorder** — Horton `rb`/`rl`, `near_par_frac`, drainage density, `d2c`, junction angles and zero-crossings are all checkable **before any surface exists**. Perf measured here, not at the end. |
| **M3** | steps 5–6 | loop-free / crossing-free / monotone-descent asserted; flow routed over the synthesised surface reproduces the constructed network — the agreement metric that was a *ceiling* in `heartland` becomes a *tautology check*, and any disagreement is a synthesis bug. Golf proxy readout. P1 overlay review. *Expected failure: all texture and roughness metrics.* |
| **M4** | steps 7–8 | **ED_texture** vs the real split-half floor (ED_macro reported, not gated — `03-macro-is-designed.md` §4) (`heartland`'s best 2.30; 1.91 reached once before the dendrite revert); valley connectivity and `moran_60` vs real 0.744 / 0.408; PSD seam at the dictionary patch pitch |
| **M5** | step 9, hydrology | water in low places, P1 |
| **Gate** | | blind A/B ≤65 %, name-the-archetype ≥80 % (protocol in `docs/03-success-indicators.md`), dispersion ratio 0.7–1.3 |

---

## 6. M1 — steps 0-3, built 2026-08-20

Crates: `course-draw` (step 0), `course-template` (steps 1-3), `course-lab`
(the visualiser). 16 tests. Step 1-3 runs in **3-4 ms/tile**.

### Stream registry (attempt 4)

`course-seed`'s registry is mirrored against `ARCHITECTURE.md` by a test and
enumerates the RETIRED pipeline's stages, so `RunIdentity::stream` panics on any
name this branch invents. Attempt 4 keeps its own list in
`course_draw::rng::REGISTRY` and builds the `DetRng` identically
(`DetRng::new(stream_seed, name)`). Reusing one stream for two purposes stays
forbidden.

| stream | owner |
|---|---|
| `n4/draw/select/v1` | step 0 — which archetype |
| `n4/draw/params/v1` | step 0 — continuous descriptors |
| `n4/template/trunk/v1` | step 2 — mouths, azimuths, inflow |
| `n4/template/fields/v1` | step 3 — grain, strata, relief predisposition |
| `n4/template/scarp/v1` | step 3 — escarpment traces |

### The visualiser

`course-lab` is written fresh rather than reusing `course-viz`, for a technical
reason: `course-viz` renders **heightfields**, and steps 1-6 have none. What
needs looking at is vector geometry, direction fields and scalar fields over an
empty tile.

```
cargo run --release -p course-template --example template_render -- out/m1_template 1 2 3
cargo run --release -p course-template --example template_sheet  -- out/m1_template/SHEET.png 1 2 3 4
```

The sheet is six archetype rows (in `Archetype::ALL` order, colour-tabbed) by
seed columns. **The relief field is identical down a column** — all six
archetypes share a seed's site, which isolates the archetype's contribution. In
production a seed draws exactly one archetype, so this is a diagnostic
property, not a leak.

### Three defects the visualiser caught that the tests did not

Recorded because each is an argument for rendering early, and because two of
them passed every assertion that existed at the time.

1. **Resistance was a barcode.** Generating resistance as an (x, y) field
   produces uniform stripes across the map. Real beds outcrop where the land
   surface crosses them, so a contact's map trace follows a **contour** — which
   is precisely why a dissected plateau reads as a staircase. Fixed by making
   resistance a property of the rock **column** (`fields::Strata`: layers, dip
   azimuth, dip grade, datum) evaluated at an elevation. The consequence for a
   network-first pipeline is structural: **the true map pattern cannot exist
   until step 6**, so step 3 emits the column plus a `resistance_hint`
   evaluated on a proxy elevation, which is all step 4 needs to steer.
2. **`relief_pred` used a fifth of its declared range.** Documented `[-1, 1]`,
   measured rms 0.21 and range −0.52..0.62 — a summed-octave field never
   reaches its bounds. Every consumer would have been silently mis-scaled.
   Now normalised symmetrically about zero.
3. **Scarp chaining was O(n²), and 190× too slow on the archetype with the
   LEAST structure.** Piedmont measured 555 ms/tile against hill country's
   14 ms, because piedmont's thin beds make the most contacts. Bucketed by
   endpoint cell: 2.9 ms. A stage that gets slower the less structure it has is
   a bug, not a budget problem.

Also tightened: `MOUTH_CORNER_CLEARANCE_M` = 500. Before it existed, 23.6 % of
trunk mouths sat within 450 m of a corner (closest 260 m) — a mouth there has
almost no catchment behind it and its trunk runs along the adjacent edge, which
is the edge-hugging trunk `heartland` spent a round diagnosing. Where the drawn
count no longer fits at `MOUTH_SEP_M`, placement **drops** a trunk rather than
crowding two rivers together (measured: 3 shortfalls in 3549).

### Expected failures at M1

Every terrain metric. There is no surface. The archetype signatures that ARE
checkable — and are, in tests and on the sheet — are: piedmont draws no scarps
(thin veneer), great plains and hill country do, river valley gets exactly one
trunk carrying 40-160 km² of external inflow, heathland and sandhills get no
scarps, and sandhills is the one archetype with real grain anisotropy
(0.55-0.85 against 0.05-0.20).


---

## 7. M2 — the headward maze: RETIRED (2026-08-20)

The first step-4 formulation (headward growth with a spacing claim) produced a
**space-filling maze, not a drainage network** — the renders showed channels
snaking, doubling back, and converging on nothing, because growth had no
elevation constraint: it organised by where there was *room*, not where water
can *go*. Four measured fixes (branch grace, ancestry exemption, claim
recalibration, tree-distance spacing ramp) and two dial sweeps could not save
it; the spacing rule and repulsion were doing three jobs at once and every fix
to one moved the others. Two findings survive it:

- **A strictly binary tree has Rb = 2 by construction** (corpus 3.92–4.74); no
  dial reaches the band. Branching must be asymmetric — a stem with side
  tributaries.
- The battery-on-the-graph works: d2c, density, Horton, junctions are all
  measurable before any surface exists.

`grow.rs` is deleted (stream `n4/network/grow/v1` retired, never to be
reused). The replacement is the N-phase plan below.

## 8. The N-phase network build (user direction, 2026-08-20)

**Trunks move end-to-end** (high-ground start acceptable): very high-wavelength
meander, smooth curves. **Tributaries begin on high ground and converge onto
trunks**: lower-wavelength meander, still smooth. Built in phases with a review
gate between each. Elevations exist DURING construction — step 5 folds into
step 4. Full plan in the session plan file; summary:

| phase | content | gate |
|---|---|---|
| **N1** | trunk paths: authored spline through the lows, λ 900–1800 m, long profile at construction | ✅ built — sinuosity 600/1500 m = 1.03–1.04 / 1.07–1.14, min radius 150 m enforced + asserted, monotone z, 0 crossings (disjoint far-end sectors + rebuild ladder). **User review before N2.** |
| **N2** | major tributaries: descend a proto-elevation field from territory peaks, join at the trunk's downstream tangent | junctions 37–45°, >80° 8.5–13.4%, near-par band |
| **N3** | minor tiers to density | the full M0 battery |
| **N4** | archetype expression (rv single trunk ✅ already, heathland pit-termination, resistance deflection) | battery per archetype + viewer |

Why authored trunks are safe here when they failed in `heartland`: there the
authored line disagreed with an independently generated surface ("the erosion
declines to adopt it"); here the surface is built FROM the network, so there is
no second authority. Divides are still never authored.

New machinery: `TrunkPath { pts, z, external_km2, through }`;
`Trunk.far`/`through` + edge-Voronoi far-end sectors in `course-template`;
through-rivers (≥ 8 km² external) enter at a far edge and drop gently
(4–12 % of relief budget over the tile), headwater trunks climb to 45–65 %.
Streams: `n4/network/trunkpath/v1`, `n4/network/tribs/v1` (reserved).
