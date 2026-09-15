# Routing round 1 — calibration ladders and acceptance

Plan: `~/.claude/plans/lets-start-addressing-some-eager-teacup.md`
(2026-09-14). Every new weight gets a ×0.5 / ×1 / ×2 ladder on the 250
frozen seeds (`out/final250_v2`, `route_batch` → `route_audit.py`), chosen
by the target metric under the guards: 250/250 routed, play crossings ≤
baseline + 2, worst clearance p90 ≤ baseline, seconds p50 ≤ 0.5, p90 ≤ 1.0.
Baseline `out/route_rs/rs_v1.jsonl`; each item's shipped batch
`out/route_rs/rs_s<N>.jsonl` with its audit table `audit_s<N>.txt`.

## Acceptance (250 seeds)

| metric | baseline | target | s1 | s2 | s3 | s4 | s5 | s6 | s7 | s8 | s9 | s10 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| greens within 60 m of the window edge | 57 % | < 25 % | 57 % | 57 % | **23 %** | 23 % | 25 % | 25 % | 24 % | 26 % | 25 % | 25 % |
| window within 100 m of play, p50 | 48 % | > 60 % | 49 % | 49 % | **62 %** | **66 %** | 60 % | 61 % | 61 % | 63 % | 62 % | 64 % |
| LZ dead flat (< 2 %) / upland (relief_pos > 0.7) | 48 / 12 % | < 30 / > 20 % | 47 / 12 % | 47 / 12 % | 46 / 9 % | 46 / 9 % | 45 / 10 % | **26** / 11 % | 28 / 9 % | 27 / 9 % | 27 / 9 % | 27 / 8 % |
| within-course LZ setting std | 0.18 | > 0.25 | 0.18 | 0.18 | 0.15 | 0.15 | 0.15 | 0.16 | 0.16 | 0.16 | 0.16 | 0.15 |
| green surround relief p50 (real 6.1–6.8 m) | 4.6 m | > 5.5 m | 4.6 m | 4.6 m | 4.6 m | 4.6 m | 4.9 m (ae 5.8, fl 4.5) | 4.9 m | 4.8 m | 4.8 m | 4.8 m | 4.9 m |
| above-chord p90 / p99 (real 1.7 / 5.6) | 4.4 / 14.1 | < 2.5 / < 8 | 4.4 / 15.0 | 4.4 / 15.0 | 4.4 / 14.4 | 4.5 / 14.4 | 4.7 / 14 | 4.9 / 15 | **2.4 / 7.5** | 2.4 / 7.5 | 2.6 / 8 | 2.7 / 7.6 |
| water courses with a hole within 40 m of water (of 71) | 27 | > 50 | 33 | 33 | 30 | 31 | 29 | 26 | 24 | **48** (ceiling ~56) | 48 | 47 |
| par 3 median / pair within 20 m (real 163) | 180 / 37 % | 160–170 / < 15 % | 180 / 39 % | 180 / 39 % | 182 / 39 % | 181 / 39 % | 181 / 35 % | 181 / 40 % | 180 / 40 % | 180 / 40 % | **165 / 11 %** | 167 / 10 % |
| par 5 median (real 474) | 528 | < 500 | 526 | 525 | 518 | 520 | 525 | 525 | 520 | 520 | **487** | 492 |
| hole 2 par 3 / hole 9 par 5 | 40 / 39 % | ≤ 20 / ≤ 25 % | 33 / 41 % | 33 / 41 % | 42 / 58 % | 40 / 43 % | 40 / 47 % | 38 / 47 % | 38 / 53 % | 38 / 51 % | 36 / 28 % | **14 / 22 %** |
| green within 35 m of another hole's spine, cases | 48 | ≤ 3 | **1** | 1 | **0** | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| tee boxes on > 30 % ground | 39 | 0 | 52 | **0** | 4 | 5 (1 hole) | **0** | 0 | 0 | 0 | 0 | 0 |
| routed / play crossings / (2,5,2) | 250 / 0 / 92 % | 250 / 0 / 85–95 % | 250 / 0 / 91 % | 250 / 0 / 91 % | 250 / 0 / 85 % | 250 / 0 / 87 % | 250 / 0 / 89 % | 250 / 0 / 88 % | 250 / 0 / 89 % | 250 / 0 / 87 % | 250 / 0 / 94 % | 250 / 0 / 93 % |
| seconds per seed, max | 0.45 | < 1.3 | 0.38 | 0.42 | 0.48 | 0.47 | 0.48 | 0.51 | 0.52 | 0.62 | 0.56 | 0.51 |

## Item 1 — green in play (2026-09-15)

`GIP_W` on `v = clip((50 − d)/50, 0, 1)`, route and beam; veto 12 / 8
under 30 m. Cases = greens within 35 / 50 m of another hole's spine,
successor's first 60 m exempt (audit definition at ladder time; baseline
78 / 122):

| rung | GIP_W | 35 m | 50 m | guards |
|---|---|---|---|---|
| ×1 | 4 | 13 | 64 | held |
| ×2 | 8 | 7 | 57 | held — shipped |
| ×4 (diagnostic) | 16 | 4 | 42 | held |

Residual at ×2: five cases were hole 9's green beside hole 1's tee (both
pinned to the clubhouse disc, 30–34 m); the loop junction now carries the
same 60 m exemption as the consecutive junction (term, beam, audit). With
it: baseline 48 / 74 → shipped 1 / 23.

## Item 2 — tee-box slope cap (2026-09-15)

No weight: `TEE_BOX_SLOPE_MAX = 0.15` on the 2 × 2-node max under the pad,
retries 1.0 / 0.75 / 0.5 × stagger on the mask tier then a graded tier, a
graded scan 3 m ahead of the previous box, then a coincident pad.

| | s1 | s2 |
|---|---|---|
| boxes on > 30 % | 52 | 0 |
| boxes on > 15 % | 345 | 1 |
| graded boxes | 3,297 | 2,987 |
| coincident pads | 0 | 135 |

## Item 3 — soft window (2026-09-15)

`EDGE_W = 0.6` on `clip((60 − d)/120, 0, 1)` (no ladder: the target was
met at ×1 and the term is bounded); `CH_EDGE_W` 0.3, `CH_CENTRAL_BOX`
(0.42, 0.58), `CH_HALO_M` 40; pool `d_boundary_norm` pair zeroed.
Item 1 re-baselined under the compaction (35 m / 50 m cases):

| rung | 35 m | 50 m | edge < 60 m | cov100 |
|---|---|---|---|---|
| s2 (before) | 1 | 22 | 57 % | 49 % |
| edge term, GIP_W 8 | 18 | 99 | 24 % | 61 % |
| + abeam-of-tee exemption | 13 | 90 | 24 % | 62 % |
| GIP_W 16 | 8 | 64 | 24 % | 62 % |
| + veto tier 35 m — shipped | 0 | 60 | 23 % | 62 % |

## Item 4 — coverage (2026-09-15)

`COV_W_BEAM` on `new / n_cells` (40 m lattice, 120 m reach); route
`COV_W_ROUTE = 2.0 · coverage`.

| rung | cov100 p50 | cov120 p50 | hole 9 par 5 | mix | gip 50 m | walk p50 |
|---|---|---|---|---|---|---|
| s3 (before) | 62 % | 69 % | 58 % | 85 % | 60 | 808 |
| 2.0 | 65 % | 72 % | 52 % | 86 % | 52 | 839 |
| 3.0 — shipped | 66 % | 72 % | 43 % | 87 % | 57 | 861 |
| 4.0 | 66 % | 73 % | 44 % | 86 % | 44 | 862 |

## Item 5 — green setting (2026-09-15)

`SETT_W` on the pool-ranked setting.

| rung | surround p50 all / ae / fl | upland | cov100 | edge < 60 m | mix |
|---|---|---|---|---|---|
| s4 (before) | 4.7 / 5.2 / 4.4 | 12 % | 66 % | 23 % | 87 % |
| 0.3 | 4.9 / 5.5 / 4.6 | 13 % | 64 % | 24 % | 87 % |
| 0.6 — shipped | 4.9 / 5.8 / 4.5 | 15 % | 60 % | 25 % | 89 % |
| 1.2 | 5.0 / 6.1 / 4.6 | 18 % | 57 % | 28 % | 88 % |

Fluvial is pool-limited (window surround p75 6.8 m; pool candidates
centred at 4.5): the lever is `greens::generate`, parked.

## Item 6 — landing-zone setting (2026-09-15)

`LZ_INTEREST_W` on `interest40`; flatness free to 3 %; water 0.3 over
15–60 m.

| rung | LZ dead flat | LZ upland | setting std | walk p50 | mix |
|---|---|---|---|---|---|
| s5 (before) | 46 % | 12 % | 0.16 | 828 | 89 % |
| 0.2 | 27 % | 12 % | 0.16 | 836 | 90 % |
| 0.35 — shipped | 26 % | 11 % | 0.16 | 827 | 88 % |
| 0.5 | 26 % | 12 % | 0.16 | 828 | 89 % |

Setting std and upland are hole-level (beam) properties; the feature
metric saturates (100 %).

## Item 7 — line of play (2026-09-15)

`LINE_FLOW_W` 0.4 (beam and detail), `LINE_CHORD_W_BEAM`, and the new
`LINE_CHORD_W_LZ` on place_lz's legs.

| rung | above p50 | p90 | p99 | walk p50 | mix |
|---|---|---|---|---|---|
| s6 (before) | 0.81 | 4.9 | 16.8 | 827 | 88 % |
| beam 0.8 | 0.66 | 3.4 | 9.1 | 832 | 88 % |
| beam 1.2 | 0.67 | 3.0 | 7.6 | 806 | 86 % |
| beam 1.6 | 0.65 | 3.0 | 7.3 | 787 | 86 % |
| beam 1.2 + LZ 0.8 — shipped | 0.63 | 2.4 | 7.5 | 794 | 89 % |

## Item 8 — water (2026-09-15)

`WATER_SCALE` over hazard 0.5 / 0.3 (detail / beam), carry 0.4 / 0.3,
and the long-span charge.

| rung | near water (of 71) | with a carry | spans > 70 m | mix | walk p50 |
|---|---|---|---|---|---|
| s7 (before) | 24 | 10 % | 0 | 89 % | 794 |
| ×0.5 | 32 | 15 % | 1 | 88 % | 808 |
| ×1 | 35 | 21 % | 1 | 87 % | 807 |
| ×2 | 44 | 31 % | 3 | 87 % | 813 |
| ×3 | 48 | 42 % | 7 | 88 % | 808 |
| ×3, no credit on long spans | 48 | 42 % | 6 | 87 % | 811 |
| ×3, + long charge scaled — shipped | 48 | 41 % | 4 | 87 % | 811 |
| ×4 | 52 | 46 % | 13 | 84 % | 787 |

14 of the 70 water courses have no water in the window: ceiling ~56.

## Item 9 — lengths (2026-09-15)

| pass | par 3 med | pairs ≤ 20 m | par 4 med | par 5 med | total p50 | mix |
|---|---|---|---|---|---|---|
| s8 (before) | 185 | 50 % | 346 | 518 | 3,147 | 87 % |
| (a) flat IQR, other-half override, twin 0.4, flat total | 165 | 34 % | 374 | 490 | 3,177 | 91 % |
| (b) + tent total, outer-quarter override, twin 0.8 | 165 | 14 % | 371 | 489 | 3,154 | 91 % |
| (c) tents for every par | 163 | 52 % | 359 | 488 | 3,139 | 82 % |
| (d) tent for par 4 only — shipped | 165 | 11 % | 358 | 487 | 3,123 | 94 % |

## Item 10 — sequence (2026-09-15)

`SEQ_W_ROUTE` 0.4 / `SEQ_W_BEAM` 0.3, no ladder (×1 met both targets):
hole-2 par 3 36 → 14 %, hole-9 par 5 28 → 22 %, mix 94 → 93 %.

The baseline → final table for the whole round is in
`creek-planform.md` (2026-09-15, item 10) and `out/route_rs/audit_round1.txt`.

# Round 2 (2026-09-15) — bend, carry, the real par sequence

Baseline s10. Real references: `tools/golf/corpus/out/shot_profiles.json`
(3,662 par 4/5 drives: dip ≥ 1.5 m 46.7 %, p50 of those 2.95 m; dogleg
p50 16.5° / p90 43.2°, 53.8 % > 15°; S-shapes 6.8 %) and
`out/par_positions.json` (412 par-36 nines).

## Acceptance

| metric | s10 | target | r2-s3 |
|---|---|---|---|
| par-by-hole table, mean abs diff over 27 slots | 0.060 | ≤ 0.06 | **0.044** |
| hole 1 par 3 (real 3 %) | 22 % | ≤ 5 % | 2 % |
| hole 9 par 5 (real 27 %) | 22 % | 20–33 % | 33 % |
| hole 9 par 3 (real 8 %) | 19 % | ≤ 12 % | 10 % |
| back-to-back 3s (real 0 %) | 3 % | 0 | 0 |
| mix (2,5,2) | 93 % | 90–95 % | 95 % |
| dogleg p50 / p90 (real 16.5 / 43) | 28 / 53 | 17–22 / ≤ 46 | 29 / 53 |
| holes bending > 15° (real 54 %) | 77 % | 55–65 % | 77 % |
| straight-line rise resolved by the spine (aeolian) | 28 % | ≥ 50 % | 28 % |
| par-5 S-shapes (real 7 %) | 50 % | ≤ 20 % | 53 % |
| above-chord p90, aeolian | 3.0 m | ≤ 2.2 | 3.0 |
| drives carrying a ≥ 1.5 m dip (real 47 %) | 34 % | 42–52 % | 36 % |
| depth of those dips p50 (real 2.95) | 2.6 m | 2.7–3.2 | 2.6 |
| routed / crossings / gip ≤ 35 m / tee > 30 % | 250 / 0 / 0 / 0 | guards | 250 / 0 / 1 / 0 |
| seconds max | 0.51 | < 1.3 | 0.55 |

## Item 3 — par sequence

| rung (route / beam) | table dist | h1 p3 | h2 p3 | h9 p3 | h9 p5 | mix |
|---|---|---|---|---|---|---|
| s10 | 0.060 | 22 | 14 | 19 | 22 | 93 |
| 0.2 / 0.15 | 0.049 | 6 | 37 | 13 | 30 | 94 |
| 0.4 / 0.3 — shipped | 0.044 | 2 | 37 | 10 | 33 | 95 |
| 0.8 / 0.6 | 0.069 | 0 | 32 | 4 | 36 | 94 |
