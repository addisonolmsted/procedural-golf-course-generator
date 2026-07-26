# Step 04 — Conditioned Noise

**Status:** unclaimed

## Purpose

Add the mid- and fine-scale relief texture that makes the skeleton read as
land — fBm/ridged noise whose amplitude, anisotropy, and character are
modulated by the skeleton's conditioning fields, so texture reinforces the
macro structure instead of fighting it.

## Position

- Upstream: step 03 (`MacroSkeleton`), step 02 (`params.noise`).
- Downstream: step 05 erodes the composed heightfield.

## Contract

Input: `MacroSkeleton`, `CourseSpec`, `res_m`. Randomness: `noise/field/v1`.

Output — `ComposedHeight`:

```
ComposedHeight {
  height: grid<f64>,   // skeleton base + modulated noise, same spec as input
}
```

Modulation law (the contract, not a suggestion):

```
amp(cell) = base_amp_m
          × (1 − floor_damp_param × fields.floor_damp)      // no-dam guard
          × (1 + slope_gain_param × fields.slope_gain)
noise dir = fields.grain_dir_rad, strength grain_align, stretch aniso_ratio
character = (1 − ridged_mix)·fBm + ridged_mix·ridged
```

Sandhills dune trains live here: a coherent carrier wave at
`params.landform.dune_wavelength_m` along the grain, superposed with the fBm
stack (high `aniso_ratio` + `grain_align`).

## Per-archetype behavior (via params)

- `sandhills`: long-wavelength anisotropic dune carrier + soft fBm.
- `piedmont`: mid fBm, moderate floor damping (creek floors stay clean).
- `florida_lowland`: amplitude < ~2 m — micro-relief only.
- `glacial_moraine`: hummocky billowed fBm (low ridged_mix, short wavelength).
- `mountain_bench`: high ridged_mix, strong slope_gain (rocky steeps),
  strong floor damping on bench treads and valley floors.

## Hard requirements

1. **No dams:** with `fields.floor_damp = 1` the added noise on drainage
   floors is ≤ a stated epsilon; drain-spine monotonicity from step 03 is
   preserved within a tested tolerance. (terrain-v2's evidenced lesson: this
   is the single most important property of this step.)
2. Zero mean drift: noise adds texture, not elevation bias (|mean| below
   epsilon over the box).
3. Resolution-consistent and deterministic double-run.
4. Amplitude never negative-clips into artifacts (no terracing at damped
   boundaries — modulation is smooth).

## References

- `terrain-v2:golf-landform/src/noiselab.rs` — the assessed noise parameter
  set (KEEP-7) and exactly this skeleton-modulation scheme (floor_damp /
  slope_gain / grain_align), plus the composed no-dam evidence.
- `main:golf-terrain/src/noise.rs` — hash-lattice Perlin/fBm reference.
- `archetype-pipeline:course-contracts/src/fixtures.rs::fixture_height` —
  minimal example of the modulation law.

## Open questions

- Domain warp: include from the start (richer shapes) or defer (identifiability
  — terrain-v2 found warp params hard to calibrate)?
- Is the dune carrier a distinct additive component with its own params, or
  folded into the aniso fBm as currently drafted?
