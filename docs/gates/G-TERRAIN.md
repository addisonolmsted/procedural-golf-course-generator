# Gate G-TERRAIN — realism of the amplified surface

Formal criteria (workplan G5): discriminants in band, energy distance
beats the v1 baseline of 2.35, **P2 blind A/B ≤ 65%**, P3 name-the-biome
≥ 80%, DR 0.7–1.3, skeleton agreement ≥ 0.9. Not yet run formally — S4
and the battery instruments are still ahead. This file logs reviewer
sessions as they happen.

## P2 dry run 2026-08-13 — 83% (12 pairs, 2 seeds × 6 biomes)

Reviewer scored 83% against the ≤65% target. Per-biome verdicts and
tells, verbatim value:

- **sandhills, heathland: coinflip.** Two biomes already pass blind.
  "These textures turned out quite nicely."
- **piedmont, great_plains**: "weirdly straight textured cuts … near
  channels or by several parallel channels … a pretty straight ribbed
  artifact." → DIAGNOSED AND FIXED same day: the taper zone kept the RAW
  carved base, whose D8 staircase showed as channel-following ribbons;
  the channel-restore step then re-drew the same staircase as thin
  stepped lines. The smoothed (≥64 m) base is now used everywhere and
  the restore pins to it. Before/after renders in the session record.
- **hill_country**: "mounds seem lower wavelength and higher amplitude,
  parallel with each other" + one seed with "a quite dramatic slope to a
  river on the edge." → tracked: hc's identity should come from
  dissection (S2 carving) more than S1 waves; ties to the known hc
  dissection-tail and hc-amplitude items. Edge-slope seed noted.
- **river_valley**: "the real one has a slightly curving grain that
  follows a very gently curving channel — absent from the produced." →
  tracked: patch orientation. The dictionary pastes patches unrotated;
  real bottomland texture (ridge-and-swale) follows the river. Fix
  direction: orient patches to the local flow/aspect direction at paste
  time (the spike's grain levers, deferred at F2).

Overall reviewer verdict: "the seeds are very well done … the ribbed
artifacts are priority one to remove, and the rest I believe is
workable." Ribbed artifacts removed; re-test at the next session.

## Orientation-aligned pasting LANDED 2026-08-14

The tracked patch-orientation fix is in (`course-amplify`): every patch
carries a load-time grain axis (structure tensor of its own heights),
the conditioning carries a local target axis (TPI structure tensor —
lp400's was blinded by the boundary drawdown ramp, and the raw rims had
to be frame-excluded from the vote, both measured in
`examples/coh_probe.rs`), and coherent positions pick the
least-rotation candidate of K=4 then rotate it the rest of the way.
Gate is kilometre-scale axis coherence (footprint coherence is ~0.83
everywhere and cannot see grain). Both bands rotate; θ==0 positions take
the bit-exact legacy path.

Measured effects: S4 skeleton agreement 0.45 → 0.50 (piedmont seed 11;
aligned texture dams across swales less — the REMAINING gap is texture
amplitude sitting on low-order channels, an amplitude question, not an
orientation one). Perf 770 ms of 900. Note for grep-hygiene: the
"rotation was considered and rejected" line in docs/00-architecture.md
refers to rotating the WORLD heightmap; patch-frame rotation inside the
synthesizer is unrelated.

Caveat the next session should know: the current S2 heathland base
carries real parallel-ridge fabric (tracked ROUGH_FLOOR item), so
heathland legitimately rotates toward it today; the isotropy gate is
proven on controlled data in `coherence_gate_responds_to_fabric`
instead. When the ROUGH_FLOOR fix lands, heathland's rotated share
should drop on its own — check it then.
