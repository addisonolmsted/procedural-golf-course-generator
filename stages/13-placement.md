# Stage 13 — Water, Vegetation, Paths, Zoning

**Status:** unclaimed

## Purpose

The object placement manifest: everything the game instantiates that is not
the heightfield. Hazard finalization, tree instancing against the cover map,
cart paths, tee/green furniture, scatter layers — all with seed-deterministic
IDs so a course is reproducible object-for-object.

## Position

- Upstream: Stage 12 (final terrain), Stages 06/07/09/10/11 (water, cover,
  routing, features, catch basins), Stage 00 (`placement.*` knobs +
  archetype = art kit key).
- Downstream: the game/renderer. This is the last stage.
- Streams: `placement/v1` — attempt scope.

## Contract

Input: the full artifact set.

Output — `manifest.json` (+ optional per-layer binary point sets):

```
PlacementManifest {
  hazards:  [ { id, kind: water | penalty | bunker, outline, hole refs } ],
  trees:    [ { id, species_slot, pos, scale, rot } ],   // instanced against
                                                         // canopy/class maps
  paths:    { cart: [ { id, spine, width_m } ],          // clubhouse-connected
              walk: [ … ] },
  furniture:[ { id, kind: tee_marker | pin | bench | …, hole, pos, rot } ],
  scatter:  [ { layer: rocks | deadfall | grass_tufts | …,
                points_ref: "<layer>.bin", count } ],
  zones:    [ { id, kind: fairway | green | tee | rough | waste | ob,
                outline, hole } ],                        // gameplay zoning
}
```

Rules:
- **IDs are seed-deterministic**: stable hash of (stream draw index +
  layer), so the same seed always yields the same object identities —
  saves/replays and art baking can reference them.
- Hazards: Stage 06 lakes/creeks that intersect play corridors are
  finalized as water hazards with legal drop geometry; Stage 10 bunkers
  become bunker hazards.
- Trees: Poisson-disc-style instancing (deterministic, stream-driven)
  against `canopy.cgrid`, species slots from the archetype art kit,
  cleared inside corridors/zones per θ.
- Cart paths: routed clubhouse → every tee/green, following Stage 09 walk
  paths where possible, grade-limited on the final terrain.
- Zones: authoritative gameplay surfaces rasterizable from outlines.

## Hard requirements

1. Determinism incl. IDs: byte-identical manifest on double-run; object
   count stable across platforms.
2. No object inside a wrong zone (trees on fairways, furniture in water) —
   validated against the zone set.
3. Cart path grades ≤ θ; network connected (clubhouse reaches every hole).
4. Every Stage-11 catch basin appears (as drain furniture or explicit
   invisible object per the resolved open question there).
5. The manifest references only artifacts in the final bundle (no dangling
   layer refs).

## References

- Archetype art-kit mapping: the archetype id has been carried end-to-end
  for exactly this stage.
- `main:golf-holes/` furniture conventions (tee markers, pins) as prior art.

## Open questions

- Art kit schema (species slots, material sets per archetype): owned here or
  in a separate art-kit doc the game team maintains? Draft: separate doc;
  this stage consumes slot names only.
- Scatter density budgets per layer — perf-driven; decide with the renderer.
