//! Stage S3 — transforms and datum ops. An ordered list of modifiers applied
//! to the skeleton. The base six use only the water-table and floodplain
//! datums; the glacial transform and boundary-retreat seams are declared here
//! but empty until the future-biome packs land.
//!
//! Emits the water polygons and basin inventory that S5 masks and S10 dress.
//!
//! Stage doc: `docs/stages/stage-03-transforms.md`.

pub mod basins;
pub mod datum;
pub mod floodplain;
pub mod seams;
pub mod water_table;
