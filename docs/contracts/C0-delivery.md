# Contract C0 — Course Delivery

**Between:** the pipeline → **the frontend/game team**
**Type:** `course_contracts::contracts::delivery::CourseBundle`
**Status:** guarded — *the most externally-guarded contract in the project*,
because its consumer is another team. Several fields are marked **TBD-frontend**
and must be settled *with* that team, not for them. Changes bump `C0_VERSION`.

## Why this contract exists

Every other contract is internal; this one is the product. The pipeline's
output is consumed by a team that does not read this repo, so the delivery
format has to be specified as carefully as any internal seam — and specified
*early*, because quantization and format choices interact with how much fine
texture is worth generating at [S3](../stages/stage-03-amplification.md).

Guiding principles, all already load-bearing elsewhere:

1. **Plain axis-aligned grids, no transforms attached.** The play window is
   translation-only for exactly this reason
   ([C2](C2-routing-substrate.md)); the deliverable honours it.
2. **Self-describing.** A bundle on disk is complete: a consumer needs no other
   file and no out-of-band knowledge.
3. **Deterministic bytes.** Same seed ⇒ byte-identical bundle. The bundle is a
   golden artifact like everything else.

## Shape

A directory (or archive) per course:

```
course_<seed>/
├── manifest.json          # the index -- always read this first
├── height_full.<fmt>      # 2 m, full 3 km extent (1501^2)
├── height_play.<fmt>      # 0.5 m, play window + apron (TBD-frontend: extent)
├── water.json             # polygons + surfaces + provenance
├── cover.<fmt>            # cover-class raster (S10)
├── surface.<fmt>          # firmness + TWI channels (S9)
└── placements.json        # object manifest (S13-era; absent until then)
```

```
manifest.json {
  c0_version: u32,
  seed: u64, pipeline_version: u32,
  biome: BiomeId, structure_class: StructureClass,

  play_window: Rect,            // world metres; the 600 m square
  world: { extent_m: 3000.0, origin: "SW", x: "east", y: "north" },
  datum_note: "elevations are metres in a local datum; 0 = core reference",

  prevailing_wind: {            // ONE wind system -- see below
    azimuth_rad: f64,           // direction wind blows TOWARD, [0, 2pi)
    speed_mps: f64,             // prevailing mean
  },

  layers: [ { name, path, format, resolution_m, extent: Rect,
              encoding: { dtype, scale, offset },   // for quantized formats
              blake3: hex } ],

  provenance: { exemplars: [ExemplarId], envelope_fingerprint: hex,
                dictionary_fingerprint: hex },
}
```

## The fields that need the frontend's answer (TBD-frontend)

Do not resolve these unilaterally — they are the other team's working format.

| Question | Options | Pipeline's default if unasked |
|---|---|---|
| Heightmap format | raw f32 little-endian · 16-bit PNG (quantized) · EXR | **raw f32 + JSON sidecar** — lossless, trivial to parse, no library dependency |
| If 16-bit: quantization | fixed scale per course (`scale`/`offset` in the layer entry) | 16 bits over the course's actual range ⇒ sub-centimetre steps; documented in `encoding` |
| `height_play` extent | window only · window + fixed apron (e.g. 100 m) | window + 100 m apron |
| Beyond-window LOD | full 2 m everywhere · 2 m core + 8 m margin | **2 m core + 8 m margin** — the margin is scenery, seen only at distance; shipping it at 2 m quadruples the data the player never walks on. The nesting ladder makes the downsample exact |
| Cover raster format | u8 class ids + a palette table in the manifest | u8 + table |
| Normal maps | pipeline-baked · frontend-derived | frontend-derived (they own the renderer's tangent space) |

## Prevailing wind

**One wind system, stated as a contract term.** The `prevailing_wind` in the
manifest is byte-identical to the vector the terrain was sculpted with — the
azimuth [S2](../stages/stage-02-skeleton-kernel.md)'s dune trains run along,
[S3](../stages/stage-03-amplification.md)'s oriented texture follows, and
[S10](../stages/stage-10-zoning-aesthetics.md)'s vegetation flags under.

Runtime gameplay wind is the game's to vary — gusts, hole-by-hole shifts — but
its **mean must be this vector**. The point is coherence a player can feel:
the wind you fight on the tee points down the same axis the dunes run. If the
game invents an independent wind, the terrain silently stops making sense, and
nobody will be able to say why.

## Invariants

1. **Axis-aligned, no transforms.** Every raster layer is a plain grid; `extent`
   is an axis-aligned rect in world metres. Nothing in the bundle requires a
   rotation, projection, or CRS to interpret.
2. **Self-describing.** Every layer is listed in the manifest with its format,
   resolution, extent, and encoding. No magic constants live outside the
   bundle.
3. **Deterministic.** Same seed ⇒ byte-identical bundle, hashed per layer
   (blake3, recorded in the manifest). Quantization, if any, is deterministic.
4. **Bed, not surface.** Height under water bodies is the bed, exactly as in
   [C2](C2-routing-substrate.md); the water surface lives in `water.json`.
   Stated here too because the frontend renders both and must not guess.
5. **Resolutions come from the ladder.** Any layer's `resolution_m` is one of
   the ladder rungs (8 / 2 / 0.5), so layers nest exactly.
6. **Additive evolution.** New layers may be added without a major bump;
   removing or re-encoding an existing layer bumps `C0_VERSION`.

## Versioning

`C0_VERSION` bumps on any change a consumer could observe: format, encoding,
extent semantics, field removal. Because the consumer is external, C0 changes
additionally require **notice to the frontend team before merge** — this is the
one contract where "explicit reviewed event" includes people outside the repo.

## Open questions

1. All **TBD-frontend** rows above. **Blocks nothing yet** — but must be
   settled before [S3](../stages/stage-03-amplification.md) is implemented,
   because quantization depth bounds how much fine texture is worth
   generating.
2. **Directory or archive?** A zip/tar is atomic and hashable as one object; a
   directory streams better. Leaning directory with a manifest-level bundle
   hash.
3. **Does the frontend want the 8 m preview grid** as an explicit layer for
   loading screens / minimaps? Cheap to include.
4. **Streaming delivery** (S7–S10 emit per-hole): does the bundle support
   partial delivery, or is C0 only the complete course? Leaning complete-only
   for C0 v1, with per-hole streaming as a separate runtime channel.
