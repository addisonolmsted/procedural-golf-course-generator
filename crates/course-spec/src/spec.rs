//! CourseSpec: the step-02 artifact — archetype + fully sampled parameter
//! map — and [`CourseSpec::generate`], the selection/sampling procedure.
//! Serialized as `spec.json` (compact JSON, declared field order, no
//! trailing newline — the canonical bytes for the byte-identity requirement).

use std::collections::BTreeMap;

use course_seed::{MAX_ATTEMPTS, PIPELINE_VERSION, RunIdentity, streams};
use serde::{Deserialize, Serialize};

use crate::archetype::{ArchetypeId, HydrologyMode};
use crate::prior::Priors;
use crate::{HOLE_COUNT, PAR_SEQUENCE};

/// The step-02 artifact. Everything downstream is a pure function of this
/// (plus its upstream artifacts). Same `RunIdentity` + same prior + same
/// overrides ⇒ byte-identical `canonical_json()`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CourseSpec {
    pub identity: RunIdentity,
    pub archetype: ArchetypeId,
    /// Always `archetype.hydrology_mode()`; stored so the artifact is
    /// self-sufficient (checked on load).
    pub hydrology_mode: HydrologyMode,
    /// = [`HOLE_COUNT`] (checked on load).
    pub hole_count: u32,
    /// = [`PAR_SEQUENCE`], sums to 36 (checked on load).
    pub par_sequence: [u32; 9],
    /// Provenance: the prior file this spec was sampled from.
    pub prior_version: String,
    /// blake3 hex of the prior file bytes — a spec sampled from a different
    /// prior is stale and refuses to load.
    pub prior_fingerprint: String,
    /// `"<section>.<knob>"` → sampled value (post-override). The knob set is
    /// whatever the archetype's prior declares — data, not schema.
    pub params: BTreeMap<String, f64>,
    /// What the caller forced, recorded verbatim (steps/02 hard req 3).
    /// Empty ⇒ a pure seed run.
    pub overrides: SpecOverrides,
}

/// Caller-forced inputs (lab/tooling use), recorded verbatim in the artifact.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpecOverrides {
    pub forced_archetype: Option<ArchetypeId>,
    pub params: BTreeMap<String, f64>,
}

#[derive(Debug)]
pub enum SpecError {
    /// The bytes are not a valid `CourseSpec` (includes unknown fields).
    Json(serde_json::Error),
    /// `identity.pipeline_version` from a different contract version.
    VersionMismatch { found: u32 },
    /// `identity.attempt` outside `0..MAX_ATTEMPTS`.
    AttemptOutOfRange { attempt: u32 },
    /// Override for a knob the selected archetype's prior does not declare.
    UnknownOverrideKey { key: String },
    /// Override value that is NaN or infinite.
    NonFiniteOverride { key: String },
    /// Stored `hydrology_mode` disagrees with the archetype's fixed mode.
    HydrologyMismatch {
        archetype: ArchetypeId,
        found: HydrologyMode,
    },
    /// Stored route data disagrees with the fixed contract constants.
    RouteMismatch,
    /// Spec was sampled from a different prior file than the committed one.
    PriorFingerprintMismatch { found: String },
    /// A sampled value escaped its quantile table's range — unreachable on a
    /// validated prior; kept as belt-and-braces for hard requirement 1.
    SampleOutOfRange { key: String, value: f64 },
    /// `from_json`: params missing a knob the archetype's prior declares.
    MissingParam { key: String },
    /// `from_json`: params contain a knob the archetype's prior does not
    /// declare.
    UnknownParam { key: String },
    /// `from_json`: a non-overridden value outside its table's range — a
    /// hand-edited params map, never a generated one.
    ParamOutOfRange { key: String, value: f64 },
}

impl std::fmt::Display for SpecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SpecError::Json(e) => write!(f, "invalid spec.json: {e}"),
            SpecError::VersionMismatch { found } => {
                write!(
                    f,
                    "spec.json pipeline_version {found} != current {PIPELINE_VERSION}"
                )
            }
            SpecError::AttemptOutOfRange { attempt } => {
                write!(f, "spec.json attempt {attempt} outside 0..{MAX_ATTEMPTS}")
            }
            SpecError::UnknownOverrideKey { key } => {
                write!(
                    f,
                    "override `{key}` is not a knob in the selected archetype's prior"
                )
            }
            SpecError::NonFiniteOverride { key } => {
                write!(f, "override `{key}` must be finite")
            }
            SpecError::HydrologyMismatch { archetype, found } => {
                write!(
                    f,
                    "spec.json hydrology_mode {found:?} != {archetype}'s fixed mode"
                )
            }
            SpecError::RouteMismatch => {
                write!(
                    f,
                    "spec.json route data != fixed contract (9 holes, par 36)"
                )
            }
            SpecError::PriorFingerprintMismatch { found } => {
                write!(
                    f,
                    "spec.json prior_fingerprint {found} != committed prior — stale spec"
                )
            }
            SpecError::SampleOutOfRange { key, value } => {
                write!(f, "sampled `{key}` = {value} escaped its quantile range")
            }
            SpecError::MissingParam { key } => {
                write!(f, "spec.json params missing knob `{key}` from the prior")
            }
            SpecError::UnknownParam { key } => {
                write!(f, "spec.json knob `{key}` is not in the archetype's prior")
            }
            SpecError::ParamOutOfRange { key, value } => {
                write!(
                    f,
                    "spec.json `{key}` = {value} outside its quantile range and not overridden"
                )
            }
        }
    }
}

impl std::error::Error for SpecError {}

/// Weighted archetype pick: walk [`ArchetypeId::ALL`] (code-declared order —
/// part of the RNG contract) accumulating weights; first archetype whose
/// cumulative weight exceeds `u * total` wins. `u < 1.0` strictly, so the
/// target is strictly below the total; a zero-weight archetype can never be
/// picked (its bucket is empty).
fn pick_weighted(priors: &Priors, u: f64) -> ArchetypeId {
    let total: f64 = ArchetypeId::ALL
        .iter()
        .map(|a| priors.entry(*a).weight)
        .sum();
    let target = u * total;
    let mut acc = 0.0;
    let mut last_positive = None;
    for a in ArchetypeId::ALL {
        let w = priors.entry(a).weight;
        acc += w;
        if w > 0.0 {
            last_positive = Some(a);
            if target < acc {
                return a;
            }
        }
    }
    // Float-edge fallback (accumulated rounding could leave acc < total).
    last_positive.expect("validated priors have positive total weight")
}

impl CourseSpec {
    /// THE step-02 entry point, against the committed prior.
    pub fn generate_builtin(
        identity: RunIdentity,
        overrides: &SpecOverrides,
    ) -> Result<CourseSpec, SpecError> {
        Self::generate(
            identity,
            Priors::builtin(),
            Priors::builtin_fingerprint(),
            overrides,
        )
    }

    /// Generate against an explicit prior (lab/tooling; `prior_fingerprint`
    /// must be the hash of the bytes `priors` was loaded from).
    ///
    /// Determinism contract (the draw transcript, in order):
    /// 1. Archetype: if forced, `arch/select/v1` is NOT opened at all (safe —
    ///    streams are domain-split, so params are unaffected; forcing the
    ///    archetype a seed would draw anyway is a byte-level no-op on
    ///    `params`). Otherwise exactly one `next_f64` from `arch/select/v1`.
    /// 2. Params: exactly one `next_f64` from `arch/params/v1` per knob, in
    ///    sorted knob-key order, override or not. Inserting a knob into the
    ///    prior therefore shifts every lexicographically-later knob's draw —
    ///    by design (independent draws from one stream); the prior
    ///    fingerprint in the goldens makes that loud.
    /// 3. Overrides are applied AFTER all sampling, so the transcript is
    ///    override-independent. Overridden values are recorded verbatim and
    ///    not range-checked (they are lab inputs, not samples).
    pub fn generate(
        identity: RunIdentity,
        priors: &Priors,
        prior_fingerprint: &str,
        overrides: &SpecOverrides,
    ) -> Result<CourseSpec, SpecError> {
        let archetype = match overrides.forced_archetype {
            Some(a) => a,
            None => {
                let u = identity.stream(streams::ARCH_SELECT).next_f64();
                pick_weighted(priors, u)
            }
        };
        let entry = priors.entry(archetype);

        let mut rng = identity.stream(streams::ARCH_PARAMS);
        let mut params = BTreeMap::new();
        for (key, table) in &entry.params {
            let v = table.sample(rng.next_f64());
            if !(table.min()..=table.max()).contains(&v) {
                return Err(SpecError::SampleOutOfRange {
                    key: key.clone(),
                    value: v,
                });
            }
            params.insert(key.clone(), v);
        }

        for (key, &v) in &overrides.params {
            if !entry.params.contains_key(key) {
                return Err(SpecError::UnknownOverrideKey { key: key.clone() });
            }
            if !v.is_finite() {
                return Err(SpecError::NonFiniteOverride { key: key.clone() });
            }
            params.insert(key.clone(), v);
        }

        Ok(CourseSpec {
            identity,
            archetype,
            hydrology_mode: archetype.hydrology_mode(),
            hole_count: HOLE_COUNT,
            par_sequence: PAR_SEQUENCE,
            prior_version: priors.prior_version.clone(),
            prior_fingerprint: prior_fingerprint.to_string(),
            params,
            overrides: overrides.clone(),
        })
    }

    /// A sampled parameter. Panics loudly on a knob the archetype's prior
    /// does not declare — a missing knob is a contract violation between a
    /// consumer step and the prior file, never a recoverable condition
    /// (same stance as `RunIdentity::stream` on unregistered names).
    pub fn param(&self, key: &str) -> f64 {
        *self
            .params
            .get(key)
            .unwrap_or_else(|| panic!("spec for `{}` has no knob `{key}`", self.archetype))
    }

    /// The canonical `spec.json` bytes: compact JSON, declared field order,
    /// no trailing newline.
    pub fn canonical_json(&self) -> String {
        serde_json::to_string(self).expect("CourseSpec is always serializable")
    }

    /// Load and validate a `spec.json`. Contract mismatches are loud: wrong
    /// pipeline version, out-of-range attempt, a hydrology mode or route
    /// block that disagrees with the fixed contract, a spec sampled from a
    /// different prior than the committed one, or a params map that
    /// disagrees with that prior (missing/extra knobs, or a non-overridden
    /// value outside its table's range) — all rejected, never silently
    /// consumed. This is hard requirement 1 at the load boundary: the only
    /// way an out-of-contract params map could enter the pipeline is a
    /// hand-edited file, and it must not load.
    pub fn from_json(s: &str) -> Result<CourseSpec, SpecError> {
        let spec: CourseSpec = serde_json::from_str(s).map_err(SpecError::Json)?;
        if spec.identity.pipeline_version != PIPELINE_VERSION {
            return Err(SpecError::VersionMismatch {
                found: spec.identity.pipeline_version,
            });
        }
        if spec.identity.attempt >= MAX_ATTEMPTS {
            return Err(SpecError::AttemptOutOfRange {
                attempt: spec.identity.attempt,
            });
        }
        if spec.hydrology_mode != spec.archetype.hydrology_mode() {
            return Err(SpecError::HydrologyMismatch {
                archetype: spec.archetype,
                found: spec.hydrology_mode,
            });
        }
        if spec.hole_count != HOLE_COUNT || spec.par_sequence != PAR_SEQUENCE {
            return Err(SpecError::RouteMismatch);
        }
        if spec.prior_fingerprint != Priors::builtin_fingerprint() {
            return Err(SpecError::PriorFingerprintMismatch {
                found: spec.prior_fingerprint,
            });
        }
        // The fingerprint matched the committed prior, so its tables are the
        // authority on this spec's knob set and ranges. Overridden knobs are
        // exempt from the range check by design — overrides are recorded
        // verbatim, not sampled — but must still be declared knobs.
        let entry = Priors::builtin().entry(spec.archetype);
        for key in spec.overrides.params.keys() {
            if !entry.params.contains_key(key) {
                return Err(SpecError::UnknownOverrideKey { key: key.clone() });
            }
        }
        for key in entry.params.keys() {
            if !spec.params.contains_key(key) {
                return Err(SpecError::MissingParam { key: key.clone() });
            }
        }
        for (key, &value) in &spec.params {
            let Some(table) = entry.params.get(key) else {
                return Err(SpecError::UnknownParam { key: key.clone() });
            };
            if !spec.overrides.params.contains_key(key)
                && !(table.min()..=table.max()).contains(&value)
            {
                return Err(SpecError::ParamOutOfRange {
                    key: key.clone(),
                    value,
                });
            }
        }
        Ok(spec)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_json() -> String {
        CourseSpec::generate_builtin(RunIdentity::from_seed(7), &SpecOverrides::default())
            .unwrap()
            .canonical_json()
    }

    #[test]
    fn round_trip_bytes_stable() {
        let json = valid_json();
        let spec = CourseSpec::from_json(&json).unwrap();
        assert_eq!(spec.canonical_json(), json);
    }

    #[test]
    fn from_json_rejects_unknown_field() {
        let json = valid_json().replacen(r#"{"identity""#, r#"{"extra":1,"identity""#, 1);
        assert!(matches!(
            CourseSpec::from_json(&json),
            Err(SpecError::Json(_))
        ));
    }

    #[test]
    fn from_json_rejects_version_mismatch() {
        let json = valid_json().replacen(r#""pipeline_version":1"#, r#""pipeline_version":2"#, 1);
        assert!(matches!(
            CourseSpec::from_json(&json),
            Err(SpecError::VersionMismatch { found: 2 })
        ));
    }

    #[test]
    fn from_json_rejects_attempt_out_of_range() {
        let json =
            valid_json().replacen(r#""attempt":0"#, &format!(r#""attempt":{MAX_ATTEMPTS}"#), 1);
        assert!(matches!(
            CourseSpec::from_json(&json),
            Err(SpecError::AttemptOutOfRange { .. })
        ));
    }

    #[test]
    fn from_json_rejects_hydrology_mismatch() {
        let spec = CourseSpec::from_json(&valid_json()).unwrap();
        let wrong = if spec.hydrology_mode == HydrologyMode::Fluvial {
            "alpine"
        } else {
            "fluvial"
        };
        let current = serde_json::to_string(&spec.hydrology_mode).unwrap();
        let json = spec.canonical_json().replacen(
            &format!(r#""hydrology_mode":{current}"#),
            &format!(r#""hydrology_mode":"{wrong}""#),
            1,
        );
        assert!(matches!(
            CourseSpec::from_json(&json),
            Err(SpecError::HydrologyMismatch { .. })
        ));
    }

    #[test]
    fn from_json_rejects_tampered_route() {
        let json = valid_json().replacen(
            r#""par_sequence":[4,4,3,5,4,3,4,5,4]"#,
            r#""par_sequence":[4,4,3,5,4,3,4,5,5]"#,
            1,
        );
        assert!(matches!(
            CourseSpec::from_json(&json),
            Err(SpecError::RouteMismatch)
        ));
    }

    #[test]
    fn from_json_rejects_wrong_prior_fingerprint() {
        let fp = Priors::builtin_fingerprint();
        let json = valid_json().replacen(fp, &"0".repeat(64), 1);
        assert!(matches!(
            CourseSpec::from_json(&json),
            Err(SpecError::PriorFingerprintMismatch { .. })
        ));
    }

    #[test]
    fn from_json_rejects_out_of_range_param() {
        let mut spec = CourseSpec::from_json(&valid_json()).unwrap();
        spec.params.insert("hydro.rainfall".to_string(), 1e9);
        assert!(matches!(
            CourseSpec::from_json(&spec.canonical_json()),
            Err(SpecError::ParamOutOfRange { .. })
        ));
    }

    #[test]
    fn from_json_rejects_added_or_removed_knob() {
        let mut spec = CourseSpec::from_json(&valid_json()).unwrap();
        spec.params.insert("rogue.knob".to_string(), 1.0);
        assert!(matches!(
            CourseSpec::from_json(&spec.canonical_json()),
            Err(SpecError::UnknownParam { .. })
        ));

        let mut spec = CourseSpec::from_json(&valid_json()).unwrap();
        spec.params.remove("hydro.rainfall").unwrap();
        assert!(matches!(
            CourseSpec::from_json(&spec.canonical_json()),
            Err(SpecError::MissingParam { .. })
        ));
    }

    #[test]
    fn from_json_rejects_rogue_override_key() {
        let mut spec = CourseSpec::from_json(&valid_json()).unwrap();
        spec.overrides.params.insert("rogue.knob".to_string(), 1.0);
        assert!(matches!(
            CourseSpec::from_json(&spec.canonical_json()),
            Err(SpecError::UnknownOverrideKey { .. })
        ));
    }

    #[test]
    fn overridden_param_exempt_from_range_check_on_load() {
        // 1e6 is outside every placeholder table; as a recorded override it
        // must still load (overrides are verbatim by design).
        let ov = SpecOverrides {
            forced_archetype: None,
            params: BTreeMap::from([("hydro.rainfall".to_string(), 1e6)]),
        };
        let json = CourseSpec::generate_builtin(RunIdentity::from_seed(7), &ov)
            .unwrap()
            .canonical_json();
        let spec = CourseSpec::from_json(&json).unwrap();
        assert_eq!(spec.param("hydro.rainfall"), 1e6);
    }

    #[test]
    #[should_panic(expected = "no knob")]
    fn param_panics_on_missing_knob() {
        CourseSpec::from_json(&valid_json())
            .unwrap()
            .param("nope.nope");
    }
}
