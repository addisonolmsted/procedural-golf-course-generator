//! Archetype priors: per-archetype selection weight + one 11-point quantile
//! table per knob, loaded from the committed `data/archetype_priors.json`.
//!
//! The prior file IS the knob registry — adding/removing a knob or re-fitting
//! values is a data edit, fingerprinted so goldens force an explicit
//! re-bless. Archetypes MAY declare different knob sets (an archetype can
//! carry knobs no other archetype has); consumers fail loudly on a knob
//! their archetype's prior does not declare.
//!
//! The committed file is a hand-authored placeholder until the archetype
//! data campaign (PLAN.md) delivers a fitted file with the same schema.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::archetype::ArchetypeId;

/// The committed prior bytes (also what [`Priors::builtin_fingerprint`]
/// hashes — raw bytes, so even a formatting edit is a visible re-bless).
const BUILTIN_JSON: &str = include_str!("../data/archetype_priors.json");

/// 11-point quantile table (q0, q10, …, q100) for one knob. A degenerate
/// table (all 11 values equal) encodes a fixed, non-sampled value — same
/// schema, same one-draw cost, so promoting a constant to a distribution
/// later never shifts any other knob's draw.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Quantiles {
    pub q: [f64; 11],
}

impl Quantiles {
    pub fn median(&self) -> f64 {
        self.q[5]
    }

    pub fn min(&self) -> f64 {
        self.q[0]
    }

    pub fn max(&self) -> f64 {
        self.q[10]
    }

    /// Piecewise-linear inverse CDF at `u ∈ [0, 1)`. On a monotone table the
    /// result is always within `[q0, q100]`.
    pub fn sample(&self, u: f64) -> f64 {
        let u = u.clamp(0.0, 1.0) * 10.0;
        let i = (u.floor() as usize).min(9);
        let t = u - i as f64;
        self.q[i] + (self.q[i + 1] - self.q[i]) * t
    }
}

/// One archetype's prior: its relative selection likelihood and its knob
/// tables, keyed `"<section>.<knob>"` (BTreeMap: sorted keys = the sampling
/// draw order, so iteration is deterministic by construction).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PriorEntry {
    /// Relative selection likelihood, normalized over all archetypes at draw
    /// time — a rare biome just gets a small weight. All equal (1.0) in the
    /// placeholder. Weight 0 = never drawn (still forceable).
    pub weight: f64,
    pub params: BTreeMap<String, Quantiles>,
}

/// The full prior file. Load via [`Priors::from_json`] (validating) or use
/// the committed [`Priors::builtin`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Priors {
    pub prior_version: String,
    /// How the values were produced (campaign corpus provenance). Advisory
    /// metadata — it rides in the fingerprinted bytes so a prior version is
    /// traceable to the tile set it was fitted from, but nothing reads it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fit_provenance: Option<FitProvenance>,
    pub archetypes: BTreeMap<String, PriorEntry>,
}

/// Corpus provenance for a fitted prior (see `tools/macro_campaign`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FitProvenance {
    /// Tiles the QA cull removed before fitting.
    pub n_excluded: u32,
    /// Digest of `out/exclude.json` — pins which cull produced these values.
    pub exclude_digest: String,
}

#[derive(Debug)]
pub enum PriorError {
    /// The bytes are not the prior schema (includes unknown fields).
    Json(serde_json::Error),
    /// A registered archetype has no prior entry.
    MissingArchetype { key: &'static str },
    /// An entry keyed by something that is not a registered archetype.
    UnknownArchetype { key: String },
    /// A weight that is NaN, infinite, or negative.
    BadWeight { archetype: String },
    /// All weights are zero — nothing is drawable.
    ZeroTotalWeight,
    /// A quantile value that is NaN or infinite.
    NonFinite { archetype: String, knob: String },
    /// `q[index] > q[index + 1]` — the table is not a CDF.
    NonMonotone {
        archetype: String,
        knob: String,
        index: usize,
    },
    /// Knob key not of the form `<section>.<knob>` in `[a-z0-9_]`.
    BadKnobKey { archetype: String, key: String },
}

impl std::fmt::Display for PriorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PriorError::Json(e) => write!(f, "invalid prior file: {e}"),
            PriorError::MissingArchetype { key } => {
                write!(f, "prior missing archetype `{key}`")
            }
            PriorError::UnknownArchetype { key } => {
                write!(f, "prior entry `{key}` is not a registered archetype")
            }
            PriorError::BadWeight { archetype } => {
                write!(f, "archetype `{archetype}` weight must be finite and >= 0")
            }
            PriorError::ZeroTotalWeight => {
                write!(f, "all archetype weights are zero; nothing is drawable")
            }
            PriorError::NonFinite { archetype, knob } => {
                write!(f, "`{archetype}` knob `{knob}` has a non-finite quantile")
            }
            PriorError::NonMonotone {
                archetype,
                knob,
                index,
            } => {
                write!(
                    f,
                    "`{archetype}` knob `{knob}` quantiles not monotone at q[{index}]"
                )
            }
            PriorError::BadKnobKey { archetype, key } => {
                write!(
                    f,
                    "`{archetype}` knob key `{key}` is not `<section>.<knob>` in [a-z0-9_]"
                )
            }
        }
    }
}

impl std::error::Error for PriorError {}

fn knob_key_ok(key: &str) -> bool {
    let Some((section, knob)) = key.split_once('.') else {
        return false;
    };
    let part_ok = |s: &str| {
        !s.is_empty()
            && s.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    };
    part_ok(section) && part_ok(knob)
}

impl Priors {
    /// Parse and validate a prior file. Every structural guarantee the
    /// sampler relies on is checked HERE, loudly, once (steps/02 hard
    /// requirement 1: monotone tables validated on load).
    pub fn from_json(s: &str) -> Result<Priors, PriorError> {
        let p: Priors = serde_json::from_str(s).map_err(PriorError::Json)?;
        for a in ArchetypeId::ALL {
            if !p.archetypes.contains_key(a.key()) {
                return Err(PriorError::MissingArchetype { key: a.key() });
            }
        }
        let mut total_weight = 0.0f64;
        for (key, entry) in &p.archetypes {
            if ArchetypeId::from_key(key).is_none() {
                return Err(PriorError::UnknownArchetype { key: key.clone() });
            }
            if !entry.weight.is_finite() || entry.weight < 0.0 {
                return Err(PriorError::BadWeight {
                    archetype: key.clone(),
                });
            }
            total_weight += entry.weight;
            for (knob, table) in &entry.params {
                if !knob_key_ok(knob) {
                    return Err(PriorError::BadKnobKey {
                        archetype: key.clone(),
                        key: knob.clone(),
                    });
                }
                if table.q.iter().any(|v| !v.is_finite()) {
                    return Err(PriorError::NonFinite {
                        archetype: key.clone(),
                        knob: knob.clone(),
                    });
                }
                if let Some(i) = (0..10).find(|&i| table.q[i] > table.q[i + 1]) {
                    return Err(PriorError::NonMonotone {
                        archetype: key.clone(),
                        knob: knob.clone(),
                        index: i,
                    });
                }
            }
        }
        if total_weight <= 0.0 {
            return Err(PriorError::ZeroTotalWeight);
        }
        Ok(p)
    }

    /// The committed prior, validated once. A panic here means the committed
    /// `data/archetype_priors.json` is broken — tests catch that before any
    /// consumer can.
    pub fn builtin() -> &'static Priors {
        static BUILTIN: OnceLock<Priors> = OnceLock::new();
        BUILTIN.get_or_init(|| {
            Priors::from_json(BUILTIN_JSON).expect("committed archetype_priors.json is valid")
        })
    }

    /// blake3 hex of the committed prior file's RAW bytes. Folded into
    /// `spec.json` and the goldens, so ANY edit to the prior (even
    /// formatting) is an explicit, visible re-bless event.
    pub fn builtin_fingerprint() -> &'static str {
        static FP: OnceLock<String> = OnceLock::new();
        FP.get_or_init(|| blake3::hash(BUILTIN_JSON.as_bytes()).to_hex().to_string())
    }

    /// An archetype's entry. Infallible after validation (`from_json`
    /// guarantees every registered archetype is present).
    pub fn entry(&self, a: ArchetypeId) -> &PriorEntry {
        self.archetypes
            .get(a.key())
            .unwrap_or_else(|| panic!("validated prior missing archetype {}", a.key()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal valid prior for rejection tests: mutate one aspect per test.
    fn tiny(weight_sandhills: f64) -> String {
        let entries: Vec<String> = ArchetypeId::ALL
            .iter()
            .map(|a| {
                let w = if *a == ArchetypeId::Sandhills {
                    weight_sandhills
                } else {
                    1.0
                };
                // {w:?} so 1.0 renders as `1.0` (Display would drop the .0
                // and break the replace-needles below).
                format!(
                    r#""{}": {{"weight": {w:?}, "params": {{"hydro.rainfall": {{"q": [0,1,2,3,4,5,6,7,8,9,10]}}}}}}"#,
                    a.key()
                )
            })
            .collect();
        format!(
            r#"{{"prior_version": "test-1", "archetypes": {{{}}}}}"#,
            entries.join(",")
        )
    }

    #[test]
    fn builtin_loads_and_validates() {
        let p = Priors::builtin();
        // Placeholder ships equal weights (steps/02 open question, resolved).
        for a in ArchetypeId::ALL {
            assert_eq!(p.entry(a).weight, 1.0, "{a}");
            assert!(!p.entry(a).params.is_empty(), "{a}");
        }
        assert_eq!(Priors::builtin_fingerprint().len(), 64);
    }

    #[test]
    fn sample_endpoints_and_median() {
        let t = Quantiles {
            q: [0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0],
        };
        assert_eq!(t.sample(0.0), 0.0);
        assert_eq!(t.sample(0.5), t.median());
        assert_eq!(t.sample(1.0), 10.0);
        assert_eq!(t.sample(0.95), 9.5);
        // In-range for arbitrary u on a monotone table.
        for i in 0..=1000 {
            let v = t.sample(i as f64 / 1000.0);
            assert!((t.min()..=t.max()).contains(&v));
        }
    }

    #[test]
    fn degenerate_table_is_constant() {
        let t = Quantiles { q: [7.5; 11] };
        for u in [0.0, 0.3, 0.999] {
            assert_eq!(t.sample(u), 7.5);
        }
    }

    #[test]
    fn tiny_prior_is_valid() {
        Priors::from_json(&tiny(1.0)).unwrap();
    }

    #[test]
    fn rejects_zero_total_weight() {
        let s = tiny(0.0).replace(r#""weight": 1.0"#, r#""weight": 0.0"#);
        assert!(matches!(
            Priors::from_json(&s),
            Err(PriorError::ZeroTotalWeight)
        ));
    }

    #[test]
    fn zero_weight_for_one_archetype_is_legal() {
        Priors::from_json(&tiny(0.0)).unwrap();
    }

    #[test]
    fn rejects_bad_weight() {
        for bad in ["-1.0", "null"] {
            let s = tiny(1.0).replace(
                r#""sandhills": {"weight": 1.0"#,
                &format!(r#""sandhills": {{"weight": {bad}"#),
            );
            let err = Priors::from_json(&s);
            assert!(
                matches!(
                    err,
                    Err(PriorError::BadWeight { .. }) | Err(PriorError::Json(_))
                ),
                "{bad}: {err:?}"
            );
        }
    }

    #[test]
    fn rejects_missing_archetype() {
        let s = tiny(1.0).replace(r#""piedmont""#, r#""piedmont_gone""#);
        // Renamed key trips either check depending on scan order; both loud.
        assert!(matches!(
            Priors::from_json(&s),
            Err(PriorError::MissingArchetype { key: "piedmont" })
                | Err(PriorError::UnknownArchetype { .. })
        ));
    }

    #[test]
    fn rejects_non_monotone() {
        let s = tiny(1.0).replace("[0,1,2,3,4,5,6,7,8,9,10]", "[0,1,2,3,4,5,4,7,8,9,10]");
        assert!(matches!(
            Priors::from_json(&s),
            Err(PriorError::NonMonotone { index: 5, .. })
        ));
    }

    #[test]
    fn rejects_non_finite() {
        let s = tiny(1.0).replace("[0,1,2,3,4,5,6,7,8,9,10]", "[0,1,2,3,4,5,6,7,8,9,1e999]");
        // serde_json refuses infinite literals; either failure is loud.
        assert!(matches!(
            Priors::from_json(&s),
            Err(PriorError::NonFinite { .. }) | Err(PriorError::Json(_))
        ));
    }

    #[test]
    fn rejects_bad_knob_key() {
        for bad in ["rainfall", "hydro.rain.fall", "Hydro.rainfall", "hydro."] {
            let s = tiny(1.0).replace("hydro.rainfall", bad);
            assert!(
                matches!(Priors::from_json(&s), Err(PriorError::BadKnobKey { .. })),
                "{bad}"
            );
        }
    }

    #[test]
    fn rejects_unknown_field() {
        let s = tiny(1.0).replace(
            r#""prior_version": "test-1","#,
            r#""prior_version": "test-1", "extra": 1,"#,
        );
        assert!(matches!(Priors::from_json(&s), Err(PriorError::Json(_))));
    }

    #[test]
    fn archetype_specific_knobs_are_legal() {
        // One archetype may carry a knob the others lack.
        let s = tiny(1.0).replace(
            r#"{"q": [0,1,2,3,4,5,6,7,8,9,10]}}},"piedmont""#,
            r#"{"q": [0,1,2,3,4,5,6,7,8,9,10]}, "landform.dune_wavelength_m": {"q": [150,175,200,225,250,275,300,325,350,375,400]}}},"piedmont""#,
        );
        assert!(s.contains("dune_wavelength_m"), "test needle went stale");
        let p = Priors::from_json(&s).unwrap();
        assert_eq!(p.entry(ArchetypeId::Sandhills).params.len(), 2);
        assert_eq!(p.entry(ArchetypeId::Piedmont).params.len(), 1);
    }

    /// Placeholder directional anchors (steps/02 “Per-archetype behavior”),
    /// read from medians. A prior re-fit that breaks the physics of an
    /// archetype fails here, not three steps downstream.
    #[test]
    fn placeholder_anchors() {
        let p = Priors::builtin();
        let med = |a: ArchetypeId, k: &str| p.entry(a).params[k].median();
        let rng = |a: ArchetypeId, k: &str| {
            let t = &p.entry(a).params[k];
            (t.min(), t.max())
        };

        // Sandhills: infiltration ≈ 1, dunes 150–400 m.
        assert!(med(ArchetypeId::Sandhills, "hydro.infiltration") >= 0.9);
        let (lo, hi) = rng(ArchetypeId::Sandhills, "landform.dune_wavelength_m");
        assert!(lo >= 150.0 && hi <= 400.0);

        // Florida: flattest archetype (campaign: FL lake-district tiles carry
        // more relief than the flatwoods stereotype), water table ≤ 1.5 m.
        assert!(med(ArchetypeId::FloridaLowland, "landform.relief_amp_m") < 20.0);
        assert!(med(ArchetypeId::FloridaLowland, "hydro.water_table_depth_m") <= 1.5);

        // Mountain: dominant relief, 2–5 benches at the median.
        let m_relief = med(ArchetypeId::MountainBench, "landform.relief_amp_m");
        assert!((90.0..=160.0).contains(&m_relief));
        let m_bench = med(ArchetypeId::MountainBench, "landform.bench_count");
        assert!((2.0..=5.0).contains(&m_bench));

        // Moraine: kettled. The band widened at campaign-pilot-3: the pilot
        // corpus sat in farmed Kettle Moraine country and measured ~12
        // kettles/tile, while genuinely pitted protected outwash (Hiawatha
        // NF) measures ~37. The anchor's job is "moraine is kettled", not a
        // ground truth on density.
        let g_basins = med(ArchetypeId::GlacialMoraine, "landform.basin_count");
        assert!((10.0..=60.0).contains(&g_basins));
        assert!(med(ArchetypeId::GlacialMoraine, "hydro.depression_keep") >= 0.9);

        // Sandhills: blowout field (campaign: ~47 basins/tile median — the
        // densest basin archetype).
        let s_basins = med(ArchetypeId::Sandhills, "landform.basin_count");
        assert!(s_basins > g_basins);

        // Piedmont: the fluvial midpoint — between florida and mountain on
        // relief, no dunes, no forced depressions.
        let pied_relief = med(ArchetypeId::Piedmont, "landform.relief_amp_m");
        assert!(med(ArchetypeId::FloridaLowland, "landform.relief_amp_m") < pied_relief);
        assert!(pied_relief < med(ArchetypeId::MountainBench, "landform.relief_amp_m"));
        assert_eq!(
            med(ArchetypeId::Piedmont, "landform.dune_wavelength_m"),
            0.0
        );
        assert!(med(ArchetypeId::Piedmont, "hydro.depression_keep") <= 0.2);
    }

    /// Any edit to the committed prior file fails HERE first. Re-bless
    /// deliberately: a prior edit changes every downstream sample and must
    /// re-bless the spec goldens with it (see tests/generate.rs).
    #[test]
    fn builtin_fingerprint_golden() {
        assert_eq!(Priors::builtin_fingerprint(), FINGERPRINT_GOLDEN);
    }

    // Re-blessed 2026-07-26: step-03 landform knob expansion (placeholder-2,
    // +12 landform.* knobs with provisional quantiles for the macro planner).
    // Re-blessed 2026-07-27: campaign-pilot-1 — 37 landform tables fitted from
    // the 27-tile exemplar pilot (tools/macro_campaign, winsorized p10–p90);
    // design/quarantined/identity-zero knobs keep provisional tables.
    // Re-blessed 2026-07-28: campaign-pilot-2 — same corpus, extract v3
    // estimators. 50 tables fitted: valley_fall_grad (now measured on the raw
    // surface, not the depression-filled one) and ridge_count/ridge_len_m
    // (traced crests gated to major ridges, not geomorphon spurs) left
    // quarantine; dune_wavelength_m stays provisional (falsified — see
    // fit_knobs.QUARANTINED). Adds `fit_provenance` (corpus cull digest) and
    // the `landform.ridge_crest_hw_m` knob the de-mesa shaping reads.
    // Re-blessed 2026-07-30: campaign-pilot-3 — the corpus itself was
    // rebuilt. tile-lab QA found the exemplar tiles were 5-64% developed and
    // three detectors were measuring in the wrong place (see steps/03); the
    // centers moved onto protected land, an OSM screen auto-culls built-up
    // tiles, and the geometry/routing/scarp fixes landed. 51 tables fitted
    // from the usable tiles, after both an OSM development cull and an
    // archetype-MEMBERSHIP cull (macro_campaign/character.py).
    // Re-blessed 2026-07-31: campaign-pilot-4 — detector sensitivity. Ridge
    // transects were 300 m (a valley setting) on interfluves 200-600 m
    // apart, so ridge branches arrived with no usable cross-sections;
    // at 120 m ridge_count rose ~3x with placement quality UP (91.5%
    // on-mask vs 42.3% chance). Scarps: the 300 m length gate was rejecting
    // 98 of 138 candidates on length alone and none on height. bench_count
    // now counts contour-parallel cascade LEVELS, not scarp segments.
    // Re-blessed 2026-08-02: campaign-m4 — the prior gains the drainage
    // NETWORK family (junction angle/spacing, branch length, junction area
    // ratio, Strahler order, slope-area theta and its pivot intercept,
    // channel half-width at the pivot, sinuosity) that M0 measured and M5
    // will grow the network from, and `core_relief_cap_m` is now sourced
    // from REAL GOLF COURSES rather than kept provisional. That last one is
    // the change with teeth: a built course is routable by definition, and
    // 12 courses per region say the authored caps were tight by 1.2x
    // (piedmont) to 2.2x (sandhills), so generated cores were flatter than
    // anything anyone has built. Also fixes a corpus defect — piedmont's
    // course box overlapped the sandhills box, so all six of its "piedmont"
    // courses were Pinehurst; with a disjoint box it is 12 real piedmont
    // courses and the median moves 30.5 -> 35.1 m.
    // Re-blessed 2026-08-02: campaign-m5 — adds `landform.drainage_density_fine`,
    // the target the M5 network grower sizes itself against. Fitted from the
    // METRICS block (`drainage_density_0p25x`, the FINE accumulation
    // threshold) rather than the nominal one, because the fine threshold is
    // what counts low-order tributaries: growing to the nominal 2.25 km/km^2
    // leaves the interfluves bare at `ridge_mask_area_frac` 0.297, and growing
    // to the fine 4.30 lands it at 0.412 against a real 0.408. Florida and
    // sandhills keep pooled provisionals via the `valley_count` identity-zero
    // family — a closed-basin archetype has no through drainage to size.
    const FINGERPRINT_GOLDEN: &str =
        "aed23ac1530b2466fc31451fdc7cf36532016747086302e96468b0582390a26e";
}
