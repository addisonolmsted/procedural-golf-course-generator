# Stage 02 — Routability Mask

**Status:** unclaimed

## Purpose

Author the routability GUARANTEE before any terrain exists: a connected
blob-with-corridors inside the core where golf will fit, sized to the
archetype's playable-fraction target minus an earthmoving allowance. Nothing
downstream is allowed to break it — the mask propagates into the physics
(Stage 04 suppresses uplift and raises diffusivity inside it), so reserved
ground comes out of the sim smooth and natural-reading rather than stamped.

## Position

- Upstream: Stage 00 (`mask.*` knobs), Stage 01 (`Framing` — the mask is
  placed respecting base-level direction so it does not straddle the trunk
  drainage unnecessarily).
- Downstream: Stage 03 (strokes route relative to it), Stage 04 (forcing
  fields read it), Stage 08 (gate verifies it survived), Stage 09 (the
  earthmoving allowance enters routing as a soft cost).
- Streams: `mask/v1` — **attempt scope** (a gate fail resamples this).

## Contract

Input: `CourseSpec`, `Framing`, `res_m`.

Output — `mask/` artifact:

```
mask.cgrid        // u8 grid: 0 outside, 1 mask, 2 designated crossing corridor
mask.json {
  playable_frac_target: f64,     // θ: fraction of core area
  earthmoving_allowance_m3: f64, // θ: rides to Stage 09 as budget/soft cost
  w_min_m: f64,                  // guaranteed min corridor width, everywhere
  crossings: [ { center: [x,y], dir_rad: f64, width_m: f64 } ],
  area_m2: f64,                  // achieved (reported)
}
```

Construction: threshold a low-frequency noise field (θ: wavelength,
threshold level) → keep the largest connected component intersecting the
core → morphological dilation until the distance transform certifies
`w_min_m` everywhere (no pinch below it) → clip/extend to hit the area
target within tolerance. Crossing corridors are authored slots where the
Stage-03 trunk may pass through the mask; elsewhere the trunk must go
around.

**The invariant downstream stages must honor** (verified at Stage 08):
inside the mask after Stage 05, slope ≤ `mask.s_play_max`; no channel with
effective accumulation > `mask.a_chan_max` crosses except within a
designated crossing; 4-connectivity of the mask preserved; corridor width
`w_min_m` never pinched by water/steep cells.

## Per-archetype behavior (via knobs)

playable_frac highest for florida/sandhills, lowest for mountain_bench;
mountain's mask is elongated along the bench/terrace grain (mask noise
anisotropy follows `Framing.grain`); moraine's mask threads between the
kettle point-field (Stage 03 places kettles OUTSIDE the mask). Allowance
largest for mountain (it buys corridor cuts), smallest for florida.

## Hard requirements

1. Mask is one 4-connected component; distance transform ≥ `w_min_m / 2`
   everywhere inside (no pinch); achieved area within ±10% of target.
2. Mask ⊆ the 3 km box with ≥ the stated fraction inside the core window.
3. Deterministic double-run; resamples differ across attempts (attempt
   scope test).
4. Resolution-consistent: the 8 m preview mask is the 2 m mask downsampled
   (same component, same crossings), not a different draw.

## References

- `crates/course-world/src/noise.rs` (`perlin2`) for the low-freq field;
  `grid.rs` for the u8 grid; a distance transform is new shared code (put it
  in `course-world`, it is stage-agnostic).
- `core_relief_cap_m` course-corpus measurement
  (`tools/macro_campaign/macro_campaign/courses.py`, 2026-08-02) — the
  method for fitting playable-fraction targets from real courses.

## Open questions

- Exact playable-fraction targets per archetype: refit from the course
  corpus (structure exists) — provisional numbers until then.
- Should the clubhouse anchor site be chosen here (mask must contain a flat
  anchor disc) or in Stage 09? Draft: here, as a marked cell region — routing
  needs a fixed anchor and the mask can guarantee it cheaply.
