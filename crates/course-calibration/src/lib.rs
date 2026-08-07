//! **Offline only.** The calibration harness: lidar ingestion, the metric
//! battery, inverse fitting, and parameter-envelope certification. Nothing in
//! this crate is linked into a runtime build.
//!
//! The division of labour is the whole point of the architecture: this crate
//! does the rejecting, offline and slowly, so that runtime samples only from
//! pre-validated parameter space and never has to reject anything.
//!
//! Docs: `docs/calibration/` — `lidar-pipeline.md`, `metric-battery.md`,
//! `targets.md`, `envelope-certification.md`.

pub mod battery;
pub mod envelope;
pub mod fitting;
pub mod gates;
pub mod ingest;
