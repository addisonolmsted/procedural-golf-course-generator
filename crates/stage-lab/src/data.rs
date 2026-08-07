//! In-process regeneration for the lab: identity + overrides → spec + framing.
//! Shared by the app and the headless snapshots example.

use std::collections::BTreeMap;

use course_framing::{generate, Framing};
use course_seed::RunIdentity;
use course_spec::{ArchetypeId, CourseSpec, SpecOverrides};

pub struct Case {
    pub spec: CourseSpec,
    pub framing: Framing,
}

/// Build one case. `identity` (not just a seed) so the lab's attempt
/// scrubber can demonstrate reroll stability. Override errors (unknown knob,
/// out-of-range value) surface as strings for the UI, never panic.
pub fn build_case(
    identity: RunIdentity,
    forced: Option<ArchetypeId>,
    overrides: &BTreeMap<String, f64>,
) -> Result<Case, String> {
    let ov = SpecOverrides {
        forced_archetype: forced,
        params: overrides.clone(),
    };
    let spec = CourseSpec::generate_builtin(identity, &ov).map_err(|e| e.to_string())?;
    let framing = generate(&spec);
    Ok(Case { spec, framing })
}
