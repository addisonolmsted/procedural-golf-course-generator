//! Stage S4 — hydrology and transforms. Reads the finished surface and decides
//! where the water is.
//!
//! S4 runs **after** amplification, deliberately. Deriving flow from the
//! surface that actually exists — rather than asserting that an authored flow
//! field still describes it — is what removes the self-consistency gap between
//! the skeleton and the ground, and it is why S3 needs no "may not
//! restructure" rule.
//!
//! Order: re-derive flow ([`flow`]) → apply the ordered datum modifiers
//! ([`datum`], [`water_table`], [`floodplain`]) → inventory basins
//! ([`basins`]). The glacial and boundary-retreat seams are declared and empty
//! until the future-biome packs land.
//!
//! Stage doc: `docs/stages/stage-04-hydrology.md`.

pub mod basins;
pub mod hydrology;
pub mod datum;
pub mod floodplain;
pub mod flow;
pub mod seams;
pub mod water_table;
