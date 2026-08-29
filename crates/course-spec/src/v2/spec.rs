//! Stage S0 (v2): seed → `SiteSpec` — biome, structural class, jointly-drawn
//! descriptors, exemplar pool, dials, preset.
//!
//! Determinism contract (the draw transcript, in order; every count fixed —
//! **no draw count ever depends on drawn values**):
//!   `arch/select/v1`: 1 biome + 1 class + 1 provinces + 1 boundary-kind
//!   (always drawn, ignored at one province) + `exemplars_per_course` pool
//!   indices (collisions resolved deterministically, not redrawn).
//!   `arch/params/v1`: 1 component pick + `dims` standard normals (Box–Muller
//!   pairs; one pair per dim, second half discarded — fixed count) + 1 wind
//!   azimuth + 1 aesthetic-seed u64.
//!
//! Rejection sampling is forbidden (`docs/stages/stage-00-archetype-draw.md`):
//! every GMM draw lands inside the support via the inverse transform.

use std::collections::BTreeMap;

use course_contracts::biome::{BiomeId, BoundaryKind, ExemplarId, StructureClass};
use course_contracts::contracts::routing_substrate::ScorerPreset;
use course_contracts::metadata::Stratum;
use course_contracts::Plasticity;
use course_seed::{streams, RunIdentity};
use serde::{Deserialize, Serialize};

use super::envelope::{pick_weighted, BiomeEnvelope, EnvelopeSet};

pub const SPEC_V2_VERSION: u32 = 1;

/// The jointly-drawn continuous site descriptors.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SiteDescriptors {
    pub relief_budget_m: f64,
    pub density_target: f64,
    pub plasticity: Plasticity,
    /// DIRECTION `[0, 2π)` — the prevailing wind (one wind system: sculpting
    /// and the exported gameplay wind are this same vector).
    pub wind_azimuth_rad: f64,
    pub wind_speed_mps: f64,
    /// Depth below the local datum; negative = above the surface.
    pub water_table_m: f64,
    pub strata: Vec<Stratum>,
    pub aesthetic_seed: u64,
}

/// Caller overrides, recorded verbatim (never range-checked — lab inputs).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SpecOverridesV2 {
    pub forced_biome: Option<BiomeId>,
}

/// The S0 artifact (v2). `spec.json`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SiteSpec {
    pub spec_version: u32,
    pub seed: u64,
    pub pipeline_version: u32,
    /// Which certified envelope produced this — the interlock.
    pub envelope_fingerprint: String,
    pub biome: BiomeId,
    pub structure_class: StructureClass,
    pub descriptors: SiteDescriptors,
    pub exemplars: Vec<ExemplarId>,
    /// Derived per-stage dials, flattened and sorted (deterministic serde).
    pub dials: BTreeMap<String, f64>,
    pub preset: ScorerPreset,
    pub overrides: SpecOverridesV2,
}

#[derive(Debug)]
pub enum SpecV2Error {
    EnvelopeFingerprintMismatch { spec: String, loaded: String },
    UnknownBiome(String),
}

impl std::fmt::Display for SpecV2Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SpecV2Error::EnvelopeFingerprintMismatch { spec, loaded } => write!(
                f,
                "spec was generated against envelope {spec}, loaded {loaded}"
            ),
            SpecV2Error::UnknownBiome(b) => write!(f, "unknown biome {b}"),
        }
    }
}

impl std::error::Error for SpecV2Error {}

impl SiteSpec {
    /// THE stage-00 entry point, against the committed provisional envelope.
    pub fn generate_builtin(identity: RunIdentity, overrides: &SpecOverridesV2) -> SiteSpec {
        Self::generate(
            identity,
            EnvelopeSet::builtin(),
            EnvelopeSet::builtin_fingerprint(),
            overrides,
        )
    }

    /// Generate against an explicit envelope (lab/tooling).
    pub fn generate(
        identity: RunIdentity,
        envelopes: &EnvelopeSet,
        fingerprint: &str,
        overrides: &SpecOverridesV2,
    ) -> SiteSpec {
        // -------- categorical draws: arch/select/v1, fixed transcript ------
        let mut sel = identity.stream(streams::ARCH_SELECT);
        let u_biome = sel.next_f64();
        let biome = match overrides.forced_biome {
            Some(b) => b, // stream still advanced: transcript is override-independent
            None => *pick_weighted(
                envelopes.biomes.iter().map(|(b, e)| (b, e.weight)),
                u_biome,
            ),
        };
        let env = &envelopes.biomes[&biome];

        let window = *pick_weighted(
            env.class_weights.iter().map(|(w, p)| (w, *p)),
            sel.next_f64(),
        );
        let two_provinces = sel.next_f64() < env.province2_p;
        // Always drawn (fixed transcript), used only with two provinces.
        let u_kind = sel.next_f64();
        let boundary_kind: Option<BoundaryKind> = if two_provinces {
            Some(*pick_weighted(
                env.boundary_kind_weights.iter().map(|(k, w)| (k, *w)),
                u_kind,
            ))
        } else {
            None
        };
        let structure_class = StructureClass::new(
            window,
            if two_provinces { 2 } else { 1 },
            boundary_kind,
        )
        .expect("class invariant holds by construction");

        // Exemplars: fixed number of index draws; collisions resolved by
        // deterministic probing, never by redrawing.
        let pool = &env.exemplar_pool;
        let mut exemplars: Vec<ExemplarId> = Vec::new();
        for _ in 0..env.exemplars_per_course {
            let mut idx = (sel.next_f64() * pool.len() as f64) as usize % pool.len();
            while exemplars.iter().any(|e| e.0 == pool[idx]) {
                idx = (idx + 1) % pool.len();
            }
            exemplars.push(ExemplarId(pool[idx].clone()));
        }

        // -------- joint descriptor draw: arch/params/v1 --------------------
        let mut par = identity.stream(streams::ARCH_PARAMS);
        let comp = pick_weighted(
            env.components.iter().map(|c| (c, c.weight)),
            par.next_f64(),
        );
        let d = env.dims.len();
        // One Box–Muller pair per dim; the second half is discarded so the
        // transcript stays fixed-count regardless of how z is consumed.
        let mut z = Vec::with_capacity(d);
        for _ in 0..d {
            let u1 = par.next_f64().max(1e-12);
            let u2 = par.next_f64();
            z.push(libm::sqrt(-2.0 * libm::log(u1))
                * libm::cos(course_contracts::units::TAU * u2));
        }
        // x = mean + L z, then inverse-transform per dim.
        let mut values = BTreeMap::new();
        for i in 0..d {
            let mut t = comp.mean[i];
            for (j, &lij) in comp.chol[i].iter().enumerate() {
                t += lij * z[j];
            }
            values.insert(env.dims[i].clone(), env.transforms[i].inverse(t));
        }
        let wind_azimuth_rad =
            course_contracts::units::normalize_direction(par.next_f64() * course_contracts::units::TAU);
        let aesthetic_seed = par.next_u64();

        // Class-conditioned relief (E6): the window class scales the drawn
        // relief budget — a valley-floor window in a river valley is flat,
        // a terrace-flight window on the same river carries margin relief.
        let class_mult = env
            .class_relief_mult
            .get(&structure_class.window)
            .copied()
            .unwrap_or(1.0);
        let get = |k: &str| -> f64 {
            *values
                .get(k)
                .unwrap_or_else(|| panic!("envelope for {} missing dim `{k}`", biome.key()))
        };
        let descriptors = SiteDescriptors {
            relief_budget_m: get("relief_budget_m") * class_mult,
            density_target: get("density_target"),
            plasticity: Plasticity::new(get("plasticity"))
                .expect("logit transform keeps plasticity in (0,1)"),
            wind_azimuth_rad,
            wind_speed_mps: get("wind_speed_mps"),
            water_table_m: get("water_table_m"),
            strata: env
                .strata
                .iter()
                .map(|t| Stratum {
                    thickness_m: t.thickness_m,
                    hardness: t.hardness,
                    dip_rad: t.dip_rad,
                    strike_axis_rad: 0.0, // aligned to grain by S1
                })
                .collect(),
            aesthetic_seed,
        };

        let dials = derive_dials(env, &structure_class, &descriptors);
        let preset = derive_preset(biome, &descriptors);

        SiteSpec {
            spec_version: SPEC_V2_VERSION,
            seed: identity.seed,
            pipeline_version: course_seed::PIPELINE_VERSION,
            envelope_fingerprint: fingerprint.to_string(),
            biome,
            structure_class,
            descriptors,
            exemplars,
            dials,
            preset,
            overrides: overrides.clone(),
        }
    }

    /// Canonical JSON (compact, sorted maps via BTreeMap) — the artifact
    /// bytes, hashable for goldens.
    pub fn canonical_json(&self) -> String {
        serde_json::to_string(self).expect("spec serializes")
    }

    /// Loud interlock: a spec may only be interpreted against the envelope
    /// that produced it.
    pub fn check_envelope(&self, loaded_fingerprint: &str) -> Result<(), SpecV2Error> {
        if self.envelope_fingerprint != loaded_fingerprint {
            return Err(SpecV2Error::EnvelopeFingerprintMismatch {
                spec: self.envelope_fingerprint.clone(),
                loaded: loaded_fingerprint.to_string(),
            });
        }
        Ok(())
    }
}

/// Derived (not drawn) per-stage dials — fewer free parameters, easier to
/// certify (`docs/stages/stage-00-archetype-draw.md`, open question 3:
/// leaning derived, and v2 commits to it).
fn derive_dials(
    env: &BiomeEnvelope,
    class: &StructureClass,
    d: &SiteDescriptors,
) -> BTreeMap<String, f64> {
    let mut m = BTreeMap::new();
    // S1 spends at most 40% of the relief budget (stage-01 open question 1's
    // proposal, adopted provisionally; S11 measures the total).
    m.insert("primitives.relief_share".into(), 0.4);
    m.insert(
        "primitives.relief_amp_m".into(),
        0.4 * d.relief_budget_m,
    );
    // Grain strength: aeolian biomes key it to wind; the envelope's strata
    // presence stands in for material fabric strength provisionally.
    m.insert(
        "primitives.grain_strength".into(),
        if env.strata.is_empty() { 0.35 } else { 0.55 },
    );
    m.insert(
        "primitives.province_relief_m".into(),
        match class.provinces {
            2 => (0.25 * d.relief_budget_m).min(12.0),
            _ => 0.0,
        },
    );
    // S1 wave-field dials — per-biome, variety-audit fitted; missing
    // envelope fields fall back to the pre-audit global constants.
    m.insert(
        "primitives.wave_iso_frac".into(),
        env.wave_iso_frac.unwrap_or(0.75),
    );
    m.insert("primitives.wave_share".into(), env.wave_share.unwrap_or(0.18));
    m.insert("primitives.wave_beta".into(), env.wave_beta.unwrap_or(1.8));
    m.insert(
        "hydrology.channel_water".into(),
        env.channel_water.unwrap_or(0.0),
    );
    m.insert("amplify.pock_field".into(), env.pock_field.unwrap_or(0.0));
    m.insert(
        "hydrology.channel_width_scale".into(),
        env.channel_width_scale.unwrap_or(1.0),
    );
    m.insert(
        "skeleton.incision_boost".into(),
        env.incision_boost.unwrap_or(1.0),
    );
    m.insert(
        "skeleton.tributary_reach".into(),
        env.tributary_reach.unwrap_or(1.0),
    );
    m.insert(
        "skeleton.tributary_depth_m".into(),
        env.tributary_depth_m.unwrap_or(1.0),
    );
    m.insert("skeleton.creep".into(), env.creep.unwrap_or(0.0));
    m.insert(
        "skeleton.route_wander".into(),
        env.route_wander.unwrap_or(0.0),
    );
    m.insert("skeleton.cut_spread_m".into(), env.cut_spread_m.unwrap_or(0.0));
    m.insert(
        "skeleton.shadow_prune_m".into(),
        env.shadow_prune_m.unwrap_or(0.0),
    );
    // Default 260 = the historic fixed reach. The discharge-keyed range was
    // laddered (70/90/130/260) and measured: shorter reaches sharpen the
    // valley lines slightly but move the corpus-policy re-derivation the
    // WRONG way (hc seed 5: density 2.18 → 2.05 km/km², d2c 123 → 130 m),
    // because a wide flank is part of what makes a valley legible to an
    // accumulation-threshold network. The knob stays; the default does not
    // change behaviour.
    m.insert(
        "skeleton.valley_reach_m".into(),
        env.valley_reach_m.unwrap_or(260.0),
    );
    m.insert(
        "skeleton.cut_ceiling_m".into(),
        env.cut_ceiling_m.unwrap_or(0.45),
    );
    m.insert(
        "primitives.relief_amp_scale".into(),
        env.relief_amp_scale.unwrap_or(1.0),
    );
    m.insert("skeleton.uplift_m".into(), env.uplift_m.unwrap_or(0.0));
    m.insert("skeleton.area_exp".into(), env.area_exp.unwrap_or(0.5));
    m.insert("skeleton.inflow_start".into(), env.inflow_start.unwrap_or(0.0));
    m.insert(
        "skeleton.bank_angle_deg".into(),
        env.bank_angle_deg.unwrap_or(0.0),
    );
    m.insert(
        "skeleton.trunk_bank_angle_deg".into(),
        env.trunk_bank_angle_deg.unwrap_or(env.bank_angle_deg.unwrap_or(0.0)),
    );
    m.insert(
        "skeleton.floor_hw_min_m".into(),
        env.floor_hw_min_m.unwrap_or(8.0),
    );
    m.insert(
        "skeleton.groove_small_m".into(),
        env.groove_small_m.unwrap_or(40.0),
    );
    m.insert("skeleton.groove_share".into(), env.groove_share.unwrap_or(0.30));
    m.insert(
        "skeleton.roughness_frac".into(),
        env.roughness_frac.unwrap_or(0.05),
    );
    m.insert(
        "amplify.amp_trim_fine".into(),
        env.amp_trim_fine.unwrap_or(1.0),
    );
    m.insert("amplify.amp_trim_mid".into(), env.amp_trim_mid.unwrap_or(1.0));
    m.insert(
        "primitives.terrace_valley".into(),
        env.terrace_valley.unwrap_or(0.0),
    );
    m.insert("primitives.grain_lock".into(), env.grain_lock.unwrap_or(0.0));
    // S1 macro aeolian structure (dune trains / mound fields, ≥400 m) is
    // gated by the same per-biome intensity as S2's mid-band aeolian
    // module — one dial, two band-owners.
    m.insert("primitives.aeolian_macro".into(), env.modules.aeolian);
    m.insert("skeleton.density_target".into(), d.density_target);
    // The five S2 module intensities, straight from the envelope record —
    // dials, not branches (stage-02 doc). S2 reads only these keys.
    m.insert("skeleton.trunk_river".into(), env.modules.trunk_river);
    m.insert("skeleton.stratigraphy".into(), env.modules.stratigraphy);
    m.insert("skeleton.closed_basin".into(), env.modules.closed_basin);
    m.insert("skeleton.aeolian".into(), env.modules.aeolian);
    m.insert("skeleton.integration".into(), env.modules.integration);
    // The dispersion rides on DERANGEMENT, not on integration. The map
    // between them, `((0.5 - integration) * 1.6).clamp(0, 0.9)`, is a
    // clamped linear, and every archetype sits against one of its stops:
    // the four integrated ones pin at the 0 floor and the two deranged
    // ones at the 0.9 ceiling. Laddered on integration, heathland came
    // back byte-identical at every spread. S2 applies this around the
    // derived base.
    m.insert(
        "skeleton.integration_spread".into(),
        env.modules.integration_spread.unwrap_or(0.0),
    );
    m.insert("amplify.residual_share".into(), 0.35);
    m
}

fn derive_preset(biome: BiomeId, d: &SiteDescriptors) -> ScorerPreset {
    let p = d.plasticity.value();
    // Play-window RECTANGLE, per-biome data (S0 is the biome-record reader,
    // so the biome key belongs here and nowhere downstream). Area from the
    // corridor-packing measurement (2026-08-28: a 600 m square fits a 9-hole
    // route on 2/12 aeolian seeds; ~140 ha reaches 11/12); shape from the
    // real 9-hole footprints (2026-08-29: PCA boxes 1517x487, 1222x304,
    // 1357x628 -- ribbons, median aspect 1.70 across 20 courses). S5 places
    // the rectangle in either axis-aligned orientation; rotation stays
    // rejected. Bounds asserted by C2.
    let (play_long_m, play_short_m) = match biome {
        // ~140 ha, matching the packed-area measurement; long side sized to
        // the real 9-hole ribbons (1222-1517 m PCA length).
        BiomeId::Sandhills | BiomeId::GreatPlains => (1450.0, 950.0),
        // ~69 ha; Pinehurst-area nines run ~1150-1360 x 500-630.
        _ => (1150.0, 600.0),
    };
    ScorerPreset {
        w_earthwork: 1.0 - 0.5 * p,
        w_clearing: 0.5,
        w_drainage: 0.5,
        w_fit: 0.5 + 0.5 * (1.0 - p),
        water_exclude_threshold: 0.6,
        target_grade_fairway: 0.08,
        target_grade_green: 0.025,
        hole_length_m: [350.0, 360.0, 160.0, 480.0, 370.0, 170.0, 355.0, 490.0, 365.0],
        feasibility_strictness: 1.0 - p,
        play_long_m,
        play_short_m,
    }
}
