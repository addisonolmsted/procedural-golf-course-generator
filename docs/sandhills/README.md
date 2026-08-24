# Attempt 5 — the Sandhills archetype, built alone

**Branch:** `sandhills` (off `network-first`, 2026-08-22)
**Scope:** terrain generation for ONE archetype, through texture. Routing and
green-siting are a separate decision taken after this lands.

---

## 1. Why this attempt exists

Four generations have been retired and a fifth is mid-flight:

| Branch | Approach | Why it died |
|---|---|---|
| `main` | noise + an 88-knob fitted prior | 22 of 71 knobs inside the real IQR |
| `pipeline` | a landscape-evolution model as co-author | a LEM cannot be bounded in a sub-10 s no-retry budget |
| `archetype-pipeline` | typed contracts, reroll on gate fail | superseded |
| `heartland` | macro heightfield → flow → network read off it | network-shape ceiling; the surface was the authority |
| `network-first` | network primary, surface derived from it | reached T2; one engine still serving six landforms |

**Not one of them died from being too archetype-specific.** Three died chasing a
whole-tile aggregate realism statistic, and in four attempts no generated terrain
has ever been handed to a router. A realism target has no ceiling — you can
always be more real. That is the restart engine, and architecture alone does not
switch it off.

So this attempt changes two things at once:

1. **Fork the structure → macro-surface step per archetype.** Share everything
   downstream.
2. **Build one archetype all the way** before starting a second.

### The reframe is the project's own conclusion

`docs/network-first/02-drainage-patterns.md` §6 already says sandhills needs
*"a different structure generator selected at step 1"*, and
`docs/network-first/03-macro-is-designed.md` §8 already lists six archetypes with
six different mechanisms. The "stages never branch per biome" rule
(`docs/biomes/README.md`) earned its keep while the process was shared; it stopped
being true once the set contained an aeolian dune field and a deranged outwash
plain.

The tax is measured, not argued. The S2 diagnostic bench found heathland and
sandhills *"draw a network their ground does not carry"* — 296–467 extracted
reaches against 76–103 elsewhere, carried at 0.28–0.49×. And through every
`heartland` calibration round the energy-distance gap concentrated on hill country
and sandhills (×1.9–2.4) while four biomes passed. The engine was fitting four
archetypes and lying about two.

### Why sandhills first

It is the archetype where terrain quality and golf quality are the **same
question**: lowest plasticity in the base six, earthmoving near-identity,
*"the land already is a golf course; the job is to find it."* If the founding
thesis — realistic terrain in great-golf country yields golfability — is going to
fail, it fails here most cheaply and most visibly.

---

## 2. One archetype, two structural modes

Both landscapes are **deep permeable sand carrying an aeolian surface mantle**.
What differs is where the relief came from.

| | Nebraska (aeolian) | Carolina (fluvial) |
|---|---|---|
| sand pile | young dunes over the Ogallala | old sand cap (Pinehurst / Middendorf Fm) on the Fall Line |
| relief is | **constructional** — built by wind | **erosional** — cut by blackwater creeks |
| structure object | crest line (positive relief) | channel (negative relief) |
| drainage | none internal; rivers are allogenic, spring-fed | integrated, very low density |
| relict aeolian | *is* the landform | sand ridges and Carolina bays *on top of* the fluvial ridges |
| golf | Sand Hills CC, Dismal River, Ballyneal | Pinehurst, Tobacco Road, Dormie Club |

**This is a mode, not a dial.** At one end the structural object is a crest —
positive, transverse to wind, terminating in defects. At the other it is a channel
— negative, a converging tree under monotone descent. A 0.5 blend is ground that
neither wind nor water made, which is the "8/8 metrics in band, visually fake"
zone. Variety comes from a continuum *inside* each mode, not between them.

Cohesion is physical as well as artistic: the mantle stage lays low-amplitude dune
form onto Carolina interfluves, so the two modes share a surface treatment and not
only a palette.

**Identity is texture.** `dist_to_channel_p50` measures 104–120 m in *every* real
archetype regardless of relief. Structure makes a tile correct; texture makes it
recognisable.

---

## 3. The carry-over allowlist

> **The rule.** Terrain-generation code on this branch is written fresh. Nothing
> from a previous attempt enters unless it is on this list. Adding to the list is
> an explicit, written decision made *before* the code is written. Anything
> admitted is **copied**, never depended on, and carries a one-line provenance
> comment naming its source commit.
>
> The one exception is the measurement stack, which is **used in place and
> digest-locked** — see Tier A.
>
> This rule exists because a previous restart reused machinery silently and walked
> straight back into the failures it was meant to escape.

### Tier A — used in place, never copied (measurement)

Forking any of these breaks the metric-purity rule in
`docs/calibration/metric-battery.md`: *"the same function must measure a 3DEP tile
and generator output; two implementations would drift, and the drift would be
indistinguishable from a generator improvement."*

| Item | Why it is not approach-specific |
|---|---|
| `tools/metrics/` — `core.py`, `features.py`, `io.py`, `config.yaml`, `METRICS.md` | The 55-scalar ruler. Pure `f(height, cell_size, mask)` kernels tested against analytic surfaces. Its own doc: *"Reuse; do not rewrite."* |
| `tools/macro_campaign/` corpus scripts + the `out/` corpus | 4.1 GB of real 2 m DTM, 203 clean tiles, 64 real course grids, the `exclude.json` / `review_v2.json` cull lists. Irreplaceable measured data. |
| `tools/golf_proxy/` + `proxy_thresholds.json` | Floors derived from 64 real course grids — **12 of which are already Pinehurst-area**. |
| **`tools/dtm_primitives/`** — `geomorphons.py`, `ridgepipe.py`, `blufffit.py`, `bowlfit.py`, `transects.py`, `profiles.py` | **New admission, 2026-08-22.** The aeolian measurement stack: `ridgepipe` traces crest lines by running the valley pipeline on the inverted surface, `blufffit` fits slip faces, `bowlfit` fits blowouts. Analytically tested. The *locked* `structure.py` already imports it, so leaving it unadmitted was a loose thread either way. **Added to the ruler lock by this branch.** |

### Tier B — used in place (interchange + determinism)

Forking `Grid`/CGRID1 would create two interchange formats; forking `flow` would
create a second D8 that can disagree with the ruler's in ways indistinguishable
from a generator defect. The `flow_parity` fixtures are the whole point.

| Item | What is used |
|---|---|
| `crates/course-world` | `Grid`/`GridSpec`/`bilinear`, `gridio` (the CGRID1 codec), `flow::route` and `flow::fill_depressions_masked`, `math::Vec2`, `spline::{Spine, SegIndex}`, `noise::perlin2`, `ease::{smin, smax, smoothstep}`, `world::EXTENT_M` |
| `crates/course-seed` | `DetRng`, `RunIdentity` |

### Tier C — copied with provenance (~80 lines total)

| Item | Source | Why copy rather than rewrite |
|---|---|---|
| `band_noise` — the octave sum | `course-template/src/fields.rs:19` @ `f87ba41` | 7 lines; `course-world` has no fBm helper |
| `Canvas` + `render_terrain` — the tinted hillshade | `course-lab/src/lib.rs:246` @ `d8e625d` | Its constants were *earned*: green→tan ramp, `lambert^1.15`, `Z_EXAG 2.4` on the shading normals only. The note records that a transect proved dune trains were in the surface while an honest-gain render showed blobs. |
| The C1 cut/deposit shoulder idiom | `course-relief/src/trib_cut.rs:177` @ `c8df207` | 3 lines; the envelope boundary that never prints a crease |

### Tier D — reference only, NOT taken

Read for the lesson; write fresh.

- `course-relief/src/macro_surface.rs::build_aeolian` — the two-form sinusoid macro.
  The lesson worth keeping: the cross-wind envelope **chopped the trains into
  segments** at macro scale and had to be removed. The limitation worth escaping:
  a pure sum of sinusoids has no slip-face asymmetry, no interdune flats, no
  blowouts, and no crest-distance field.
- `course-primitives/src/generate.rs`, `course-skeleton/src/modules/aeolian.rs`,
  `course-micro/src/aeolian.rs`
- All of `course-network` and `course-relief` — the fluvial machinery
- All of `course-amplify` — the texture machinery

**Enforcement is mechanical.** `tools/no_old_deps.sh` gives `course-sandhills` a
stricter forbidden list than the attempt-4 crates: it may import **only**
`course-world` and `course-seed`.

---

## 4. Discipline

1. **The allowlist above is the only door**, and `tools/no_old_deps.sh` is the lock.
2. **The ruler is frozen.** `tools/ruler_lock.sh verify`. Changing generator and
   metric together makes every result uninterpretable.
3. **Ablation over argument.** Every claim about a visual defect gets a toggle and
   a render before a fix is written. `heartland`'s four wrong diagnoses of "the
   carves look artificial" were each killed by an ablation plus a stage dump, never
   by reasoning — and the real cause was that `box_blur_sep` has *square* support,
   so smearing a cut that lives on a D8 chain paints rectangles.
4. **Render before trusting metrics.** Drawn-coefficient PCA synthesis passed 8/8
   metrics and was visually fake. "Resistance was a barcode" passed every assertion
   that existed at the time.
5. **An instrument that has never been seen to fail is not evidence.** Both
   `ruler_lock.sh` and `no_old_deps.sh` were negative-tested on a planted violation
   before being trusted; anything new here is too.
6. **Each stage carries an expected-failure list** alongside its targets. A
   pre-texture surface is *supposed* to fail texture metrics.
7. **Macro is designed for golf; texture is fitted to the corpus**
   (`03-macro-is-designed.md`). `ED_texture` is gated; `ED_macro` is reported.
   Calm is a **floor, never a maximand** — architects do not take the calmest
   window.
8. **Gate the two modes separately.** Pooling would hide exactly the per-mode
   failure that pooled ED hid for two rounds in `heartland`.

---

## 5. The architectural spine

> **A structure generator emits *lines*. One surface synthesiser hangs ground off a
> line set, given a section program and a composition sign.**

Dunes are positive (max-composition); valleys are negative (min-composition). The
synthesiser is shared; the structure generators are not. This is what makes the
*next* archetype cheap, and it is the version of
`02-drainage-patterns.md` §6's claim — *"the synthesiser does not care whether the
lines it interpolates between are channels or dune crests"* — that can actually be
built.

---

## 6. Stages

### Aeolian mode

| # | Stage | Owns |
|---|---|---|
| A0 | Draw | mode, wind azimuth, form class, λ and H drawn as a *pair* on the measured H(λ) relation, stabilisation, water table, blowout density, allogenic-river coin |
| A1 | Wind field | slowly-varying axial direction field, ±15–25° about paleowind |
| A2 | **Crest network** | crests transverse to local wind at spacing λ that **merge, offset and terminate** |
| A3 | Interdune datum | the flat floor the dunes sit on; the routable ground |
| A4 | Dune bodies | asymmetric stoss/lee section; max-composition ⇒ coalescing crests merge into **saddles** |
| A5 | Blowouts + deflation | steep-walled bowls with a mass-conserved downwind apron; the bunkers |
| A6 | Allogenic water | on the coin, one spring-fed river incised across the tile; otherwise interdune lakes and wet meadows. **No drainage network is ever grown.** |

### Fluvial mode

| # | Stage | Owns |
|---|---|---|
| C1 | Sand-ridge datum | broad flat-topped interfluves |
| C2 | Channel network | low-density dendritic blackwater creeks |
| C3 | Valley surface | **one-zone section**, shallow concave cut, gentle walls |
| C4 | Wet bottoms | water table near surface; branch bottoms and pocosin margins |
| C5 | Relict aeolian overlay | low-amplitude dune form on the interfluves |
| C6 | Carolina bays | gated, low count |

Explicitly not built for either mode: strata/hardness column, scarps, benches,
terraces, bluffs, caprock, floor-widening.

### Shared

| # | Stage | Owns |
|---|---|---|
| T | Texture | fabric and amplitude, conditioned on slope, aspect-relative-to-wind, position, distance-to-structure |

---

## 7. Stream registry (attempt 5)

`course-seed`'s registry enumerates the retired pipeline's stages and
`course-draw`'s enumerates attempt 4's, so both panic on any name this branch
invents. Attempt 5 keeps its own list in `course_sandhills::rng::REGISTRY` and
builds the `DetRng` identically — `DetRng::new(stream_seed, name)`.

**Reusing one stream for two purposes stays forbidden**: that is what keeps steps
independently deterministic. Adding a name is a one-line change there plus a row
here.

| stream | owner |
|---|---|
| `n5/sand/draw/v1` | A0 — mode and descriptors |
| `n5/sand/wind/v1` | A1 — wind direction field |
| `n5/sand/crest/v1` | A2 — crest network |
| `n5/sand/blowout/v1` | A5 — blowouts and deflation hollows |
| `n5/sand/datum/v1` | C1 — sand-cap datum |
| `n5/sand/channel/v1` | C2 — channel network |
| `n5/sand/water/v1` | A6 / C4 — allogenic river, lakes, wet bottoms |
| `n5/sand/texture/v1` | T — fabric |

---

## 8. Gates

| gate | bar |
|---|---|
| **P3** name-the-archetype | ≥ 80 % |
| **P2** blind A/B | ≤ 65 %, at course scale **and** close range; worst reviewer counts |
| **ED_texture** | ≤ the per-mode split-half floor |
| **ED_macro** | reported, never gated |
| Dune discriminants | spacing, defect density, orientation order, stoss/lee asymmetry, `variogram_sill`, `local_relief_p50` in the Phase-2 bands |
| `drainage_density` | ≈ 0 for the aeolian mode — **measured, not assumed** |
| Golf proxy | floors met (`cap ≥ 0.409`, `steep ≥ 0.713`, `contig ≥ 124 ha`, relief 7.0–81.9 m) |
| Dispersion ratio | 0.7–1.3 across seeds |

Encouraging prior: the P2 dry run recorded *"sandhills, heathland: coinflip. Two
biomes already pass blind."* Sandhills texture is already the strongest biome
perceptually — the gap has always been macro scale and megaform.
