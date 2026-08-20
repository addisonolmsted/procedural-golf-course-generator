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

### Routability is a readout, not a gate

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

So `tools/golf_proxy/` is a **readout on every surface**. If it goes red the
constraint has become binding and we will know. Optimising against it would make
duller terrain than reality.

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
| **M0** | branch, allowlist, scaffold, **ruler** | corpus bands re-measured under one declared extraction policy and written down once — see `01-measurement-policy.md` |
| **M1** | steps 0–3 | blind legibility on the fields alone: a reviewer names the drainage pattern from a field render (≥80 %, the analogue of the existing C1 class-legibility test). *Expected failure: there is no terrain to judge.* |
| **M2** | step 4, network growth | **the main practical payoff of the reorder** — Horton `rb`/`rl`, `near_par_frac`, drainage density, `d2c`, junction angles and zero-crossings are all checkable **before any surface exists**. Perf measured here, not at the end. |
| **M3** | steps 5–6 | loop-free / crossing-free / monotone-descent asserted; flow routed over the synthesised surface reproduces the constructed network — the agreement metric that was a *ceiling* in `heartland` becomes a *tautology check*, and any disagreement is a synthesis bug. Golf proxy readout. P1 overlay review. *Expected failure: all texture and roughness metrics.* |
| **M4** | steps 7–8 | pooled energy distance vs the 2.35 baseline (`heartland`'s best 2.30; 1.91 reached once before the dendrite revert); valley connectivity and `moran_60` vs real 0.744 / 0.408; PSD seam at the dictionary patch pitch |
| **M5** | step 9, hydrology | water in low places, P1 |
| **Gate** | | blind A/B ≤65 %, name-the-archetype ≥80 % (protocol in `docs/03-success-indicators.md`), dispersion ratio 0.7–1.3 |
