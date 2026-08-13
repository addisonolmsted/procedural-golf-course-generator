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
| ~~Dune sinuosity two-sinusoid trains~~ | S2 sandhills | superseded 2026-08-10 by the multi-wave rework (row above) |
| Bench spacing 2.2× thickness, 14–34 m, fade ≤120 m of channels | S2 hill_country/gp | density + trunk-ridging reviews |

## MANUAL provisional (audit these)

| Parameter | Where visible | Fit plan |
|---|---|---|
| Biome selection weights, class_weights, province2_p, boundary kinds | Spec tab draw table | E6 (measure which classes each biome's corpus expresses) |
| Density targets (piedmont/rv/hc/heathland/sandhills) | Spec tab `dens` | E7 (gp already corrected; same method) |
| Plasticity, wind speed/azimuth ranges, water table | Spec tab | E7 / design |
| Strata templates (thickness/hardness/dip) | S2 benches | E7 from bench measurements |
| Module intensities (trunk_river, strat, basin, aeolian, integration) | S2 per-biome character | E6/E7 |
| ~~S1 mode wavelengths, isotropy~~ | C1 relief views | RESOLVED 2026-08-10: many-wave spectral fit + per-biome dials + sandhills megaform continuum (see S1 macro variety table) |
| relief_share 0.4 / incision 0.22×budget cap 16 | S2 valley depth vs upland | E7 once transects measured |
| Catena θ 0.62 | S2 hillslope curvature | E7 slope–area / hypsometry fit |
| Kettle radius 70–200 m, depth 1.6–4.2×scale | S2 heathland circles | E7 from corpus basin stats (v1 extractor measured these) |
| ~~Aeolian wavelengths 210/340 m~~ | S2 sandhills | RESOLVED 2026-08-10: 14-wave mid-band field 120–400 m, amp FIT to real mid-band std 3.58 m, continuum-coupled |
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

## S1 macro variety (added 2026-08-10, variety-audit fit)

| Parameter | Value | Tier |
|---|---|---|
| wave_iso_frac (envelope, per biome) | rv 0.12, heathland 0.45, gp 0.55, others 0.75 | FIT (audit orientation order) |
| wave_share (envelope, per biome) | heathland 0.46, others 0.18 | FIT (audit band relief) |
| grain_lock (envelope) | rv 1.0, others 0 | FIT+REVIEW (one-axis bottomland) |
| gp/hc relief chol widening; gp/heathland/sandhills log-mean shifts | see envelope JSON | FIT (audit tails + E7 totals) |
| S1 aeolian macro: train λ 1100–1500, mound band 1050–1550, share 0.30, smoothstep continuum | generate.rs | FIT (audit A/λ/relief scatter, 44 real tiles) |
| S2 aeolian: 14 waves 120–400 m, ±28° spread, amp 0.123·budget clamp [1.0,5.6], continuum coupling 0.40+0.60·w | aeolian.rs | FIT (real mid-band std 3.58 m) |

## S2 derived network (2026-08-12, replaces the authored-growth rows)

The rows for growth steering, wander, parent repulsion, commit rules and
infill placement are RETIRED — that engine is deleted. Its replacement:

| Parameter | Value | Tier |
|---|---|---|
| Roughness band / share | 140–520 m, 0.05 × relief_amp | REVIEW: short end made every channel wiggle identically; long end matches the observed meander scaling |
| Erosion iterations / clamp | 15 / 0.45 m per step | MANUAL (fixed count is a budget requirement) |
| Stream-power k | 0.9, × (1.6 − hardness) | REVIEW (fit target: valley depth vs the corpus transects, with E7) |
| Extraction threshold | 1.2e5 m², × (1 − 0.85·derangement) | FIT to the corpus DENSITY band (2.28–2.75 km/km²); the corpus's own 6e4 cut is not transferable because our tile is rimmed |
| Derangement | ((0.5 − integration) × 1.6) clamped to 0.9 | FIT: separates connectivity 1.00 (integrated) from 0.05–0.06 (deranged), matching the design intent |
| External inflow | 20 km² per unit trunk_river dial | REVIEW: two orders above the tile's own 9 km², which is what makes a trunk a trunk |
| Base drawdown / rim | 4 m over 900 m; 3 borders rimmed | MANUAL (boundary condition; the contract says the trunk exits at base level) |
| Routing dither | 0.10 m, blurred to ~40 m, faded out above 2% slope | FIT: T-junction share and sinuosity, both against corpus-measured bands |

### Acceptance bands re-based on the corpus (not on the old engine)

| Band | Old (authored) | New (corpus-measured) | Instrument |
|---|---|---|---|
| Horton Rb / Rl | 3–5 / 1.5–3 | 1.7–4.5 / 0.5–1.9 | `horton_real.py` (real: Rb 2.14–3.08, Rl 0.92–1.16, Ω 2–3) |
| T-junction share | ≤ 8% | ≤ 25%, 48 m baseline | `junction_real.py` (real: 4–20% at 48 m, 7–25% at 16 m) |
| Sandhills channels | exactly zero | present but connectivity < 0.5 | corpus d2c 118 m, density 2.36 km/km² |

Both old bands were achievable only because the previous engine constructed
those quantities directly; a derived network produces them as outcomes, so
the bands had to come from the corpus measured the identical way.

