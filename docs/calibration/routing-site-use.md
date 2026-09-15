# Routing round 1 — calibration ladders and acceptance

Plan: `~/.claude/plans/lets-start-addressing-some-eager-teacup.md`
(2026-09-14). Every new weight gets a ×0.5 / ×1 / ×2 ladder on the 250
frozen seeds (`out/final250_v2`, `route_batch` → `route_audit.py`), chosen
by the target metric under the guards: 250/250 routed, play crossings ≤
baseline + 2, worst clearance p90 ≤ baseline, seconds p50 ≤ 0.5, p90 ≤ 1.0.
Baseline `out/route_rs/rs_v1.jsonl`; each item's shipped batch
`out/route_rs/rs_s<N>.jsonl` with its audit table `audit_s<N>.txt`.

## Acceptance (250 seeds)

| metric | baseline | target | s1 | s2 | s3 | s4 |
|---|---|---|---|---|---|---|
| greens within 60 m of the window edge | 57 % | < 25 % | 57 % | 57 % | **23 %** | 23 % |
| window within 100 m of play, p50 | 48 % | > 60 % | 49 % | 49 % | **62 %** | **66 %** |
| LZ dead flat (< 2 %) / upland (relief_pos > 0.7) | 48 / 12 % | < 30 / > 20 % | 47 / 12 % | 47 / 12 % | 46 / 9 % | 46 / 9 % |
| within-course LZ setting std | 0.18 | > 0.25 | 0.18 | 0.18 | 0.15 | 0.15 |
| green surround relief p50 (real 6.1–6.8 m) | 4.6 m | > 5.5 m | 4.6 m | 4.6 m | 4.6 m | 4.6 m |
| above-chord p90 / p99 (real 1.7 / 5.6) | 4.4 / 14.1 | < 2.5 / < 8 | 4.4 / 15.0 | 4.4 / 15.0 | 4.4 / 14.4 | 4.5 / 14.4 |
| water courses with a hole within 40 m of water (of 71) | 27 | > 50 | 33 | 33 | 30 | 31 |
| par 3 median / pair within 20 m (real 163) | 180 / 37 % | 160–170 / < 15 % | 180 / 39 % | 180 / 39 % | 182 / 39 % | 181 / 39 % |
| par 5 median (real 474) | 528 | < 500 | 526 | 525 | 518 | 520 |
| hole 2 par 3 / hole 9 par 5 | 40 / 39 % | ≤ 20 / ≤ 25 % | 33 / 41 % | 33 / 41 % | 42 / 58 % | 40 / 43 % |
| green within 35 m of another hole's spine, cases | 48 | ≤ 3 | **1** | 1 | **0** | 0 |
| tee boxes on > 30 % ground | 39 | 0 | 52 | **0** | 4 | 5 (1 hole) |
| routed / play crossings / (2,5,2) | 250 / 0 / 92 % | 250 / 0 / 85–95 % | 250 / 0 / 91 % | 250 / 0 / 91 % | 250 / 0 / 85 % | 250 / 0 / 87 % |
| seconds per seed, max | 0.45 | < 1.3 | 0.38 | 0.42 | 0.48 | 0.47 |

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
