# The Carolina Sandhills corpus

**Phase 1 deliverable.** 30 clean tiles, measured 2026-08-22. Corpus key
`sandhills_nc`; Nebraska keeps `sandhills` unrenamed, because renaming it would
move every published 203-tile band and every `proxy_thresholds.json` row.

---

## 1. The campaign

There was no natural Carolina Sandhills terrain in the corpus. All 108 existing
`sandhills` tiles are Nebraska (41.6–42.5 N); the Carolina-longitude tiles
already on disk are Uwharrie **piedmont**, and `courses_v2.py:37` annotates that
box *"W of the NC sandhills"* — deliberately excluding this landform.

Course grids needed no work: `courses.py`'s `sandhills` box is `(35.2, −79.45,
0.5)`, the NC golf province, so 12 of the 64 real course grids behind the golf
proxy's floors were already Pinehurst / Southern Pines / Whispering Pines.

| stage | tiles |
|---|---:|
| centres defined across four protected blocks | 76 |
| fetched (11 lost to USGS 502/504/400s) | 65 |
| survived the OSM screen at `MAX_DEVELOPED_FRAC` 2 % | 34 |
| **kept by human review** | **30** |

Attrition **61 %**, against the recorded v1 ~40 %. Carolina longleaf ground
carries firebreaks, forest roads and old fields that Nebraska refuge ground does
not. The top-up round targeted the two blocks with the best pass rate (Fort
Bragg 70 %, Carolina Sandhills NWR 54 %) rather than the two worst.

| block | kept |
|---|---:|
| Fort Bragg NW / northern training areas, NC | 12 |
| Carolina Sandhills NWR, SC | 9 |
| Sandhills Game Land, NC | 5 |
| Sand Hills State Forest, SC | 4 |

The state-forest block is thin and is the candidate for a later top-up if it
proves to carry distinct character.

**Human review** was done on the review sheet (`tools/sandhills_review.py` +
`tools/build_sand_review.py`), which renders every survivor as a tinted
hillshade with its metrics and advisory flags. Four tiles were culled. The
measured correlates: `t03469_08001` and `t03506_07953` are the flattest and
calmest tiles in the set (relief p0 and p2, slope p5 and p8) with the highest
levelled fractions — 9.3 % and 10.3 % against a 3.9 % kept median, the
graded-ground signature; `t03453_08017` has the least contiguous calm and the
strongest grain; `t03523_07935` is unremarkable in every scalar and was caught
by eye alone. That last one is the argument for the human gate existing.

**Cull reasons** (reviewer's own categories): `t03453_08017` and `t03523_07935`
— *road lines*; `t03506_07953` and `t03469_08001` — *flat patches*.

**And the road detector does not work.** The `leveled` flag agreed with the
reviewer — both flat-patch culls sit at the 86th percentile. But `road_score`
put the two road-line culls at the 76th and **20th** percentiles, the latter
*below* the kept median of 0.261, while the lowest-scoring tile in the whole set
(0.196) was culled for something else entirely. The statistic is a p95 of
gradient-orientation concentration in 256 m windows, so a thin sharp firebreak
is swamped by the valley slopes around it: it detects broad linear LANDFORMS
(embankments, causeways) and not thin CUTS. Auto-culling on it would have kept
both road tiles. **It stays advisory and must not be promoted to a gate** — the
human eye is the instrument here, which is the whole reason this step exists.

---

## 2. The headline: Carolina is routable found-ground and Nebraska is not

Golf proxy over all 233 clean tiles (`tools/golf_proxy/proxy_corpus.py`), floors
from the 64 real course grids — `cap ≥ 0.409`, `steep ≥ 0.713`,
`contig ≥ 124 ha`, relief 7.0–81.9 m:

| biome | n | cap p50 | steep p50 | contig ha | relief m | **passes all** |
|---|---:|---:|---:|---:|---:|---:|
| **sandhills_nc** | 30 | **0.744** | **0.966** | **598.9** | 40.8 | **97 %** |
| heathland | 37 | 0.815 | 0.936 | 595.3 | 18.3 | 92 % |
| great_plains | 36 | 0.707 | 0.916 | 462.6 | 44.5 | 83 % |
| river_valley | 32 | 0.957 | 0.984 | 667.5 | 4.6 | 31 % |
| piedmont | 27 | 0.327 | 0.627 | 81.5 | 47.7 | 30 % |
| **sandhills (NE)** | 44 | 0.386 | 0.672 | 144.4 | 37.7 | **23 %** |
| hill_country | 27 | 0.169 | 0.368 | 29.7 | 74.4 | 0 % |

**Carolina is the most routable landform in the corpus** — 97 % against
Nebraska's 23 %, at nearly the same relief (40.8 m against 37.7 m). It is not
flatter; it is *smoother*. Median slope 0.052 against Nebraska's 0.107, and
contiguous calm 599 ha against 144 ha.

**Design consequence, and it is the opposite of the aeolian mode's.** The two
modes of one archetype need opposite interventions. Nebraska needs the
`largest_contig_ha` help that `03-macro-is-designed.md` licenses; **Carolina
needs none** — its found ground already clears every floor, and the risk there
is the other one the corpus warns about: *"calm is a FLOOR, not a maximand …
otherwise generated courses will systematically sit on duller ground than real
ones."* At 0.744 calm the Carolina mode's job is to stay interesting, not to get
calmer.

---

## 3. Shared invariants — Carolina is dendritic like everything else

`tools/macro_campaign/pattern_survey.py`, 8 m grid, 6e4 m² threshold:

| biome | n | near-par % | d2c m | density | junction p50 | >80° | aniso | main_share | Ω |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| **sandhills_nc** | 30 | 1.52 | 107.3 | 2.33 | 40.7° | 13.2 % | 0.108 | 57.1 % | 3 |
| piedmont | 27 | 1.29 | 104.3 | 2.36 | 40.6° | 10.7 % | 0.080 | 59.4 % | 3 |
| sandhills (NE) | 44 | 1.95 | 104.3 | 2.39 | 41.6° | 12.4 % | 0.145 | 44.7 % | 3 |
| *band, 233 tiles* | | 1.3–3.1 | 96–116 | 2.21–2.60 | 37–45° | 8.5–13.4 % | | | 3 |

Every number lands inside the existing band. Horton (Policy B) reads Rb 2.27 /
Rl 0.94 / Ω 3, also in band. **Carolina is an ordinary dendritic network**,
which extends `02-drainage-patterns.md`'s finding — *"all six Heartland biomes
have the same pattern"* — to a seventh. As there, the archetype's character is
not in the pattern; it is in what hangs off it.

Its nearest neighbour on these numbers is **piedmont**, which is the expected
result and the one to watch: Carolina Sandhills is a fluvially dissected upland,
and what separates it from piedmont is not planform but **cross-section and
texture**.

---

## 4. What actually separates the three

`extract_v2` scalars, medians:

| | relief m | fine_std m | mid_std m | slope med | d2c m | density | lake frac |
|---|---:|---:|---:|---:|---:|---:|---:|
| **sandhills_nc** | 46.9 | **0.196** | **1.43** | **0.052** | 109.1 | 2.43 | 0.047 |
| sandhills (NE) | 46.4 | 0.605 | 2.30 | 0.107 | 118.4 | 2.36 | **0.258** |
| piedmont | 57.1 | 0.469 | 3.41 | 0.119 | 107.6 | 2.43 | 0.028 |

Carolina is **the smoothest ground in the corpus at every band** — fine-band
texture 0.196 m against piedmont's 0.469 and Nebraska's 0.605, mid-band 1.43
against 3.41 and 2.30. Deep permeable sand does not hold fine relief: rain
infiltrates instead of running off, so there is little to carve and little to
gully.

That is the discriminant, and it is a **texture** one — consistent with
`docs/biomes/README.md`'s claim that identity is texture rather than structure,
and it means the Carolina mode will live or die at Phase 5 rather than Phase 4.

Nebraska's `lake_frac` of 0.258 against Carolina's 0.047 is the other clean
separator, and it is real: the Ogallala intersects the surface across the
Nebraska dune field, giving thousands of interdune lakes. Carolina's water is
creek bottoms and bays.

---

## 5. A caution for P3

`tools/aeolian/dune_stats.py` over the 30 Carolina tiles:

| | spectral A | λ_dom m | band relief m | flank asym | crest len p50 | defect/km² |
|---|---:|---:|---:|---:|---:|---:|
| sandhills_nc | **0.355** | 1178 | 15.7 | 0.425 | 1022 | 0.444 |
| sandhills : **mound** | **0.356** | 1184 | 19.1 | 0.667 | 1023 | 0.444 |
| sandhills : **train** | 0.767 | 1301 | 29.2 | 0.355 | 796 | 0.667 |

**Carolina and the Nebraska mound class are indistinguishable on orientation
order** — 0.355 against 0.356, at almost the same wavelength. That is a
coincidence of two different processes landing on the same statistic, and it is
exactly the "metric passing for the wrong reason" failure `02-drainage-patterns.md`
§5 documents for `hypsometric_bimodality`.

**Do not use spectral A to tell the two modes apart.** It is a within-aeolian
discriminant (train vs mound) and nothing more. The mode discriminants are the
texture bands of §4 and `lake_frac`. This matters for P3: a reviewer asked to
name the mode has to be able to do it from fabric, so if P3 confuses Carolina
with Nebraska mounds, the fix is texture, not macro.

---

## 6. Ruler re-baseline

Adding a corpus key edits six sweep drivers' biome lists plus `exclude.json` and
`review_v2.json`, all digest-locked. Re-baselined deliberately with
`tools/ruler_lock.sh write` — **the sweep tier only; all 17 measurement kernels
verified unchanged**, which is what the two-tier split exists to prove.

Published bands that now describe **233** tiles rather than 203:
`01-measurement-policy.md` §4 (Horton) and §5 (planform). The per-biome rows for
the original six are unchanged — the new key adds a row, it does not move one.

`proxy_thresholds.json` was **not** re-derived. The floors stay pinned to the
same 64 course grids, or every published golf band shifts underneath us.
