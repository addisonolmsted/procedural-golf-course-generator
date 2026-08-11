# Parameter provenance — manual vs fit (2026-08-10)

What to watch in stage-lab: FIT and REVIEW values have evidence behind
them; MANUAL values are provisional and are what your eye should audit
hardest. Viewer locations given per row.

## FIT from the corpus (evidence-backed)

| Parameter | Where visible | Source |
|---|---|---|
| Relief budgets (per biome, log-mean) | Spec tab `relief` column; S2 hillshade amplitude | E7 fit: generated core relief matched to 203-tile medians (±5%) |
| great_plains density_target | Spec tab `dens`; S2 status `dens x/y` | Corrected to corpus drainage_density (2.16 measured vs 0.85 authored) |
| Infill placement threshold (265 m) | S2: channel spacing rhythm | Calibrated until authored d2c matched corpus quantiles (p25–p90) |
| d2c/density/Horton/junction bands | D5 battery output | Measured from corpus (`extract_v2` scalars) |

## REVIEW-calibrated (your sessions)

| Parameter | Where visible | Session |
|---|---|---|
| Class/mode amplitude ratio 0.85/0.18 | C1 tab legibility | Blind legibility session (17/18) |
| Catena profile shape (finite wall slope, D_FULL 260, composite groove) | S2 hillshade valley form | Inversion/trench reviews |
| Channel floors 8/14/22/30 m by order | S2: valley floor widths | "knife cut" review (provisional widths until transect fit) |
| Junction angle band 30–62°, reflect-overshoot | S2 junctions | T-junction measurement session |
| Dune sinuosity ±55 m @ 760 m + 7° second train | S2 sandhills | corduroy review |
| Bench spacing 2.2× thickness, 14–34 m, fade ≤120 m of channels | S2 hill_country/gp | density + trunk-ridging reviews |

## MANUAL provisional (audit these)

| Parameter | Where visible | Fit plan |
|---|---|---|
| Biome selection weights, class_weights, province2_p, boundary kinds | Spec tab draw table | E6 (measure which classes each biome's corpus expresses) |
| Density targets (piedmont/rv/hc/heathland/sandhills) | Spec tab `dens` | E7 (gp already corrected; same method) |
| Plasticity, wind speed/azimuth ranges, water table | Spec tab | E7 / design |
| Strata templates (thickness/hardness/dip) | S2 benches | E7 from bench measurements |
| Module intensities (trunk_river, strat, basin, aeolian, integration) | S2 per-biome character | E6/E7 |
| S1 mode wavelengths [1500…400], isotropy | C1 relief views | E6 spectral fit — KNOWN GAP: no wind-aligned megaforms (stage-01 OQ0) |
| relief_share 0.4 / incision 0.22×budget cap 16 | S2 valley depth vs upland | E7 once transects measured |
| Catena θ 0.62 | S2 hillslope curvature | E7 slope–area / hypsometry fit |
| Kettle radius 70–200 m, depth 1.6–4.2×scale | S2 heathland circles | E7 from corpus basin stats (v1 extractor measured these) |
| Aeolian wavelengths 210/340 m | S2 sandhills | E7 directional spectra (v1 dune probe exists) |
| Floodplain widths 60+320×intensity, riser 2.2 m | S2 river_valley | E7 transect fit |

## Planform (added 2026-08-10, planform-instrument fit)

| Parameter | Value | Tier |
|---|---|---|
| wander θ range | 0.40–0.75 rad | REVIEW→FIT: tuned until 600 m-window sinuosity matched the real corpus (1.06–1.10) via examples/planform.rs vs real_planform.py |
| wander λ range | 500–1150 m | same fit loop |
| wander score weight | 30 m-equiv | MANUAL (2:1 vs turn penalty; terrain still dominates at metre-scale relief) |
| infill `to_net` fade-in | < 600 m of network | REVIEW: kills far-field compass beelines; terrain carries the far field |
| parent-corridor repulsion | 250 m / 20 m-equiv, past 150 m arc | MANUAL (defence-in-depth; measured contribution small) |
| in-band commit rule | +0.6 net-pull per step after 3 in-band steps | MANUAL (bounds approach glides) |
| INFILL_PLACE_THRESH_M | 300 (was 265) | FIT: restores d2c medians to 96–100 after wander shortened them |
