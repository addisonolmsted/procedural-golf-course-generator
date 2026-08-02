# Stage 04 — Forcing Field Synthesis

**Status:** unclaimed

## Purpose

Compile stages 1–3 into the fields the sim consumes. This is the compiler
between authored intent and physics: after this stage nothing downstream
knows about windows, strokes, or provinces — only fields.

## Position

- Upstream: Stage 00 (`forcing.*` knobs), Stage 01 (`Framing`), Stage 02
  (mask), Stage 03 (`Strokes`).
- Downstream: Stage 05 (the LEM consumes exactly these four fields +
  boundary conditions).
- Streams: `forcing/v1` — **attempt scope** (noise lattices).

## Contract

Input: all upstream artifacts, `res_m` (sim resolution — see Stage 05).

Output — `forcing/` artifact (all CGRID1 f32 at sim res + `forcing.json`
with the scalars):

```
initial_height.cgrid   // regional trend (tilt + window form) + shallow trunk
                       // carve + constructional noise for depositional
                       // archetypes (dune trains, moraine hummock, kettles)
uplift.cgrid           // m/iter: noise conditioned on grain (anisotropic,
                       // province-aware), SUPPRESSED inside the mask,
                       // shaped by the boundary landform
erodibility.cgrid      // K field: strata bands for structural archetypes
                       // (bench/scarp = alternating hard/soft layers vs
                       // elevation), patchy noise elsewhere
diffusivity.cgrid      // D field: ELEVATED inside the mask (reserved ground
                       // forms smooth through physics), archetype base rate
forcing.json {
  boundary: { open_edges, base_level_elev_m },   // from Framing
  datum_targets: [ … ],                          // pass-through for 05/06
  iterations: u32, dt: f64,                      // θ: sim length (fixed count)
}
```

Composition rules (the contract's semantics):
- `initial_height` = regional trend (Framing tilt + window form) + trunk
  carve (capped per Stage 03) + datum flattening (blend toward datum elevs
  inside outlines) + constructional noise where θ says the archetype BUILDS
  its relief (dunes, moraine) rather than eroding it.
- `uplift` inside mask: multiplied by `forcing.mask_uplift_damp` (≈0). Grain
  conditioning: anisotropic noise stretched along `Framing.grain`.
- `erodibility` strata: `K(z)` alternating bands (θ: layer thickness,
  contrast) for mountain_bench / escarpment windows; elsewhere patchy.
- `diffusivity` inside mask: multiplied by `forcing.mask_diff_boost` (>1).

All noise here is authored ONLY at the envelope level (wavelengths,
amplitudes, anisotropy from θ) — no hand-placed landforms beyond the
strokes.

## Per-archetype behavior (via knobs)

- piedmont: modest uplift, mid K, low D — the sim does the work.
- sandhills: constructional dune trains in `initial_height` (wavelength
  150–400 m along grain), near-zero uplift, high D, huge infiltration
  downstream ⇒ LEM nearly off.
- florida_lowland: initial ≈ datum plane + microtopo, zero uplift, high D.
- glacial_moraine: constructional hummock + kettle depressions in initial,
  low uplift, mid D; kettles preserved by Stage 05's depression policy.
- mountain_bench: strong uplift + strata K (benches emerge as hard layers),
  low D outside mask.

## Hard requirements

1. Mask semantics: uplift damp and diffusivity boost applied exactly per
   knobs; a test integrates uplift inside vs outside the mask.
2. `initial_height` honors every stroke cap (Stage 03 req 1 re-checked on
   the compiled surface) and every datum (surface within tolerance of datum
   elev inside its outline).
3. Fields are finite, K > 0, D ≥ 0 everywhere; boundary metadata consistent
   with Framing.
4. Deterministic; resolution-consistent (coarse fields = downsampled fine).

## References

- `crates/course-world/src/noise.rs` (perlin), `ease.rs`, `grid.rs`.
- Constructional forms: retired step-03 `prims/` (commit 0313432) for dune /
  hummock / kettle shape vocabulary — reused as INITIAL-SURFACE texture
  generators, not finished landforms.

## Open questions

- Sim resolution: 4 m (751²) proposed — confirm against Stage 05 perf.
- Strata K: piecewise-constant bands vs smoothed — decide on first mountain
  renders.
- Does province boundary also modulate K (material contact = K contrast)?
  Draft: yes for `material_contact`, no for pure scarp.
