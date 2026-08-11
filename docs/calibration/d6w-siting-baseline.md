# D6-W siting baseline — what real courses accept, per v2 province

*2026-08-11. Instruments: `tools/macro_campaign/site_window_audit.py`
(best 600 m window over real kept tiles + generated S2 dumps) and
`tools/macro_campaign/courses_v2.py` (60 real courses found via OSM in
disjoint boxes around each v2 exemplar province, 3DEP tile per course,
the course's own central 600 m window measured with identical metrics).
Course tiles cache under `out/courses_v2/` (gitignored; graded ground —
measurement only, NEVER texture harvest). Calm = fraction of cells under
the 8% fairway grade. Position: 0 = valley floor, 1 = hilltop of the
local 3 km tile.*

## The three-way table (medians; [p10–p90] where it matters)

| biome | REAL LAND best window calm | REAL COURSES accept calm | course relief | course position | GEN best window calm |
|---|---|---|---|---|---|
| piedmont | 0.61 | 0.58 [.39–.74] | 18 m | 0.45 | 0.92 |
| sandhills | 0.68 | 0.46 [.32–.79] | 22 m | 0.43 | 0.81 |
| great_plains | 0.95 | 0.84 [.70–.94] | 14 m | 0.40 | 1.00 |
| river_valley | 1.00 | 0.85 [.60–.97] | 7 m | 0.46 [.39–.75] | 1.00 |
| hill_country | 0.25 | 0.66 [.38–.82] | 21 m | 0.41 | 0.84 |
| heathland | 0.99 | 0.42 [.20–.80] | 36 m | 0.45 | 1.00 |

## Findings

1. **Real courses do not require calm ground.** Accepted calm medians
   run 0.42–0.85 by province; Sand Hills GC (the flagship minimalist
   course) plays at 0.45 calm on 19 m of window relief; Dismal River at
   0.28; CapRock Ranch sits on 72 m of relief. Every generated biome's
   best-window distribution clears the corresponding accepted-calm band
   with margin — **no v2 biome produces unroutable seeds**, including
   post-widening hill_country.
2. **Architects don't even take the calmest window.** Next to almost
   every course a calmer window existed (median best-local calm
   0.89–1.00 vs the 0.42–0.85 they occupy). Siting optimizes character
   + access, not flatness. Design input for S5: the scorer should treat
   calm as a FLOOR (per-province accepted band above), not a maximand —
   otherwise generated courses will systematically sit on duller ground
   than real ones.
3. **Position: low-to-mid everywhere, except river_valley avoids the
   bottom.** Course position p50 is 0.40–0.46 in every province (valley
   floors, benches, interdune flats) — but rv courses all sit at ≥0.39
   with many at 0.5–0.8: in bottomland the constraint inverts and the
   HIGHER, flood-free ground (terraces, natural levees) is where courses
   go. S5's rv scoring must weight drainage/water exclusion above grade
   (the preset's water_exclude_threshold is the right lever; S4's water
   table informs it).
4. **Heathland caveat.** The Michigan kettle-belt courses sit on
   hummocky ice-contact moraine (course relief p50 36 m — The Monument
   60 m) — a rougher glacial facies than the pitted OUTWASH our
   heathland biome deliberately models (UK-heath analog, band relief
   ~8 m). The acceptance evidence still transfers (architects handle
   36 m happily); just don't read the 36 m as a miscalibration of our
   14 m biome.
5. **Sandhills asymmetry note** (from the land audit): real sandhills
   calm ground sits very low (position 0.18 — flat interdune floors
   under sharp crests); generated sine-symmetric dunes are equally calm
   on crests (best windows at 0.56). Real courses sit mid-low (0.43),
   so this doesn't block siting — but the crest/floor asymmetry is a
   real S3/F2 signature (conditioning includes relief position, so the
   dictionary can carry it).

## Standing use

Re-run `site_window_audit.py` after S3 lands (pre-texture candle-wax
inflates generated calm; the real numbers include full lidar texture).
The per-province accepted-calm bands above are the empirical envelope
for S5's feasibility floors.
