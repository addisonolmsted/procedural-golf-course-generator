# Stage 12 — Micro Re-pass

**Status:** unclaimed

## Purpose

The finishing coat: gentle diffusion/thermal relaxation OUTSIDE protected
zones to blend earthmoving seams into the landscape, plus micro-band noise
(calibrated against natural tiles) on non-play surfaces only, amplitude-
masked to zero on greens and fairways. After this stage the heightfield is
final.

## Position

- Upstream: Stage 11 (clean composed surface + repair layers), Stage 09/10
  (protected-zone geometry), Stage 00 (`micro.*` knobs).
- Downstream: Stage 13 places objects on the final surface; the game ships
  this heightfield.
- Streams: `micro/v1` — attempt scope (noise lattice).

## Contract

Input: composed surface (2 m + 0.5 m play patches), protected zones,
`CourseSpec`.

Output — `final_terrain/`:

```
height_final.cgrid       // 2 m, full box
play_patches/*.cgrid     // 0.5 m final surfaces where earthworks exist
micro.json {
  protect_mask ref,      // greens, fairway cores, tee pads: untouched cells
  seam_report: { max_residual_step_m, blended_cells },
}
```

Two operations, in order:
1. **Seam blending**: a few iterations of the Stage-05 diffusion/thermal
   operators (same code, tiny rates) applied only within a band around
   Stage-10/11 layer boundaries, EXCLUDING the protect mask. Kills the
   "pasted-on" read of tie-ins without touching designed surfaces.
2. **Micro-band noise**: high-frequency noise whose amplitude spectrum is
   calibrated from the natural-tile corpus (per archetype), applied with a
   spatial amplitude mask: 0 on greens/fairways/tees, ramping to full on
   rough/forest/rock. Never alters drainage (amplitude cap ≪ any Stage-11
   finding threshold — state and test the bound).

## Hard requirements

1. Protect mask cells bit-identical to input (tested).
2. Post-pass drainage still clean: re-run the Stage-11 checks once — zero
   new findings (the amplitude bound makes this provable, the test makes it
   sure).
3. Micro amplitude within the calibrated band per archetype; deterministic;
   double-run.
4. Seam report: max residual step across any layer boundary ≤ θ.

## References

- Stage 05's diffusion/thermal operators (shared library — reuse, don't
  duplicate).
- `tools/dtm_metrics` roughness/spectrum families — the natural-tile
  micro-band calibration source (the corpus already carries what "natural
  micro texture" measures as, per archetype).

## Open questions

- Micro noise on the 0.5 m play patches' SURROUNDS (collars, bunker faces):
  in or out of the protect mask? Draft: collars protected, bunker faces get
  micro (sand texture is an art-layer concern anyway).
