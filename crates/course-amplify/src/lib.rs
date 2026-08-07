//! Stage S3 — amplification. Where the terrain stops being a construction and
//! starts being a place.
//!
//! S2 authors the structure — the drainage network, the divides, a smooth
//! catena base surface. That base is correct and characterless: a distance
//! transform gives every point equidistant from a channel the same treatment,
//! so interfluves come out smooth and tubular. S3 supplies everything the
//! base cannot, by **reconstructing detail from a dictionary of real terrain
//! patches** fitted offline to the biome's lidar corpus and conditioned on
//! position within the skeleton.
//!
//! This is where archetype identity lives. The measured evidence is that
//! drainage spacing does *not* discriminate archetypes
//! (`dist_to_channel_p50` is 104-120 m in every one), so identity cannot come
//! from the skeleton — it comes from hillslope form and texture, which is
//! exactly what the dictionary carries.
//!
//! Stage doc: `docs/stages/stage-03-amplification.md`.

pub mod blend;
pub mod conditioning;
pub mod dictionary;
pub mod polish;
pub mod synth;
