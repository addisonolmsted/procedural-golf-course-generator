//! Stage S0, v2: the Heartland site draw against a certified envelope.
//!
//! Lives alongside the v1 archetype/prior machinery (which the retained
//! `course-framing` still consumes); v1 retires with it. Stage doc:
//! `docs/stages/stage-00-archetype-draw.md`.

pub mod envelope;
pub mod spec;

pub use envelope::{EnvelopeSet, ENVELOPE_VERSION};
pub use spec::{SiteDescriptors, SiteSpec, SpecOverridesV2, SPEC_V2_VERSION};
