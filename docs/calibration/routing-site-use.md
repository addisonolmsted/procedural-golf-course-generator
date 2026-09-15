# Routing round 1 — calibration ladders and acceptance

Plan: `~/.claude/plans/lets-start-addressing-some-eager-teacup.md`
(2026-09-14). Every new weight gets a ×0.5 / ×1 / ×2 ladder on the 250
frozen seeds (`out/final250_v2`, `route_batch` → `route_audit.py`), chosen
by the target metric under the guards: 250/250 routed, play crossings ≤
baseline + 2, worst clearance p90 ≤ baseline, seconds p50 ≤ 0.5, p90 ≤ 1.0.
Baseline `out/route_rs/rs_v1.jsonl`; each item's shipped batch
`out/route_rs/rs_s<N>.jsonl` with its audit table `audit_s<N>.txt`.

## Acceptance (250 seeds)

| metric | baseline | target | s1 |
|---|---|---|---|
| greens within 60 m of the window edge | 57 % | < 25 % | 57 % |
| window within 100 m of play, p50 | 48 % | > 60 % | 49 % |
| LZ dead flat (< 2 %) / upland (relief_pos > 0.7) | 48 / 12 % | < 30 / > 20 % | 47 / 12 % |
| within-course LZ setting std | 0.18 | > 0.25 | 0.18 |
| green surround relief p50 (real 6.1–6.8 m) | 4.6 m | > 5.5 m | 4.6 m |
| above-chord p90 / p99 (real 1.7 / 5.6) | 4.4 / 14.1 | < 2.5 / < 8 | 4.4 / 15.0 |
| water courses with a hole within 40 m of water (of 71) | 27 | > 50 | 33 |
| par 3 median / pair within 20 m (real 163) | 180 / 37 % | 160–170 / < 15 % | 180 / 39 % |
| par 5 median (real 474) | 528 | < 500 | 526 |
| hole 2 par 3 / hole 9 par 5 | 40 / 39 % | ≤ 20 / ≤ 25 % | 33 / 41 % |
| green within 35 m of another hole's spine, cases | 48 | ≤ 3 | **1** |
| tee boxes on > 30 % ground | 39 | 0 | 52 |
| routed / play crossings / (2,5,2) | 250 / 0 / 92 % | 250 / 0 / 85–95 % | 250 / 0 / 91 % |
| seconds per seed, max | 0.45 | < 1.3 | 0.38 |

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
