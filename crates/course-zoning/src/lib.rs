//! Stage S10 — zoning and aesthetics. The vegetation and enclosure mosaic,
//! substrate-reveal materials, the palette chord and its per-biome fairway
//! tint, the light rig, the signature exaggeration each biome is allowed, and
//! the placement manifests the renderer consumes.
//!
//! This is where a biome stops being a set of dials and starts looking like
//! itself. It is also the last stage that may touch appearance — S11 only
//! measures.
//!
//! Stage doc: `docs/stages/stage-10-zoning-aesthetics.md`.

pub mod enclosure;
pub mod exaggeration;
pub mod light;
pub mod manifest;
pub mod materials;
pub mod palette;
pub mod vegetation;
