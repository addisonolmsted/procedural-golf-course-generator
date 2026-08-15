//! The certified-envelope schema and its rejection-free sampler (stage S0).
//!
//! An envelope is a **Gaussian mixture in transformed space** per biome:
//! multi-modality (variety) and rejection-free sampling (determinism) in one
//! representation. Covariance is stored as a lower-triangular Cholesky factor,
//! so sampling is `mean + L·z` — PSD by construction, no decomposition at
//! runtime, and every draw lands inside the support by the inverse transform.
//!
//! The committed `data/envelope_provisional.json` is **hand-authored data in
//! the final schema** — Phase H's certified fit swaps the data in with no
//! code change (`docs/calibration/envelope-certification.md`). The
//! fingerprint interlock is the same pattern as the v1 prior's
//! `FINGERPRINT_GOLDEN`: a spec generated against one envelope loudly refuses
//! to be interpreted against another.

use std::collections::BTreeMap;

use course_contracts::biome::{BiomeId, BoundaryKind, WindowClass};
use serde::{Deserialize, Serialize};

pub const ENVELOPE_VERSION: u32 = 1;

/// blake3 of the committed provisional envelope bytes. Any data edit must
/// re-bless this (the `bless_golden` example prints the new value).
pub const ENVELOPE_FINGERPRINT_GOLDEN: &str =
    include_str!("../../data/envelope_provisional.fingerprint");

/// Per-dimension transform: the GMM lives in transformed space; samples map
/// back through the inverse, clamped to the declared support — which is what
/// makes sampling rejection-free.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Transform {
    /// x = value (clamped to [lo, hi]).
    Linear { lo: f64, hi: f64 },
    /// GMM over ln(value); support (0, ∞) with clamp [lo, hi].
    Log { lo: f64, hi: f64 },
    /// GMM over logit((value - lo)/(hi - lo)); support (lo, hi) open.
    Logit { lo: f64, hi: f64 },
}

impl Transform {
    /// Transformed → value space, always inside the support.
    pub fn inverse(&self, t: f64) -> f64 {
        match *self {
            Transform::Linear { lo, hi } => t.clamp(lo, hi),
            Transform::Log { lo, hi } => libm::exp(t).clamp(lo, hi),
            Transform::Logit { lo, hi } => {
                let s = 1.0 / (1.0 + libm::exp(-t));
                lo + s * (hi - lo)
            }
        }
    }
}

/// One mixture component: mean + lower-triangular Cholesky factor over the
/// biome's descriptor dimensions (in `dims` order).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Component {
    pub weight: f64,
    pub mean: Vec<f64>,
    /// Row-major lower triangle, row i has i+1 entries.
    pub chol: Vec<Vec<f64>>,
}

/// A biome's envelope: categorical weights + the descriptor GMM + the
/// exemplar pool.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BiomeEnvelope {
    /// Relative selection weight of the biome itself.
    pub weight: f64,
    pub class_weights: BTreeMap<WindowClass, f64>,
    /// P(two provinces).
    pub province2_p: f64,
    pub boundary_kind_weights: BTreeMap<BoundaryKind, f64>,
    /// Descriptor dimension names, fixing the order of `mean`/`chol`.
    pub dims: Vec<String>,
    pub transforms: Vec<Transform>,
    pub components: Vec<Component>,
    /// The real tiles whose residuals may texture this biome's courses.
    pub exemplar_pool: Vec<String>,
    /// How many exemplars a course draws.
    pub exemplars_per_course: u32,
    /// Per-biome strata template (top-down); v2 keeps this derived-not-drawn.
    pub strata: Vec<StratumTemplate>,
    /// S2 module intensities — dials, not branches. A biome that does not
    /// use a module runs it at zero through the identical code path
    /// (stage-02 doc, "The five modules").
    pub modules: ModuleIntensities,
    /// Class-conditioned relief multipliers (E6): the descriptor draw was
    /// blind to the class draw, so a river-valley terrace_flight window
    /// drew floodplain-flat relief. Missing class = 1.0.
    #[serde(default)]
    pub class_relief_mult: BTreeMap<WindowClass, f64>,
    /// S1 wave-field orientation mixture: fraction of background waves
    /// drawn isotropic (the rest concentrate on the grain axis). Missing =
    /// the pre-variety-audit global 0.75. Fitted per biome against the
    /// corpus macro orientation order (docs/calibration/variety-audit.md).
    #[serde(default)]
    pub wave_iso_frac: Option<f64>,
    /// S1 background-wave share of the relief amplitude. Missing = the
    /// global 0.18. Heathland carries its relief in the 400–1600 m band
    /// (kettle-and-ridge country), which the audit found ×2 low.
    #[serde(default)]
    pub wave_share: Option<f64>,
    /// How strongly the grain axis locks to the base-edge (class) axis,
    /// [0,1]. Real river-valley bottomland has ONE axis — bluffs, meander
    /// belts and terraces all share the valley direction — so its drawn
    /// grain must not be independent of the class orientation. 0 (default)
    /// keeps the independent draw.
    #[serde(default)]
    pub grain_lock: Option<f64>,
    /// S1 wave spectral tilt (amplitude exponent over the 400-1600 m
    /// band). Missing = the global 1.8. Large values concentrate power
    /// at the LONG end: hill country's signature is parallel ridge
    /// trains spanning 1/3-1/2 of the tile.
    #[serde(default)]
    pub wave_beta: Option<f64>,
    /// Probability/extent of visible water IN channels (S4): 1.0 = a
    /// river valley always carries its river; fractional values are a
    /// per-course coin for creeks. Missing = 0 (dry channels).
    #[serde(default)]
    pub channel_water: Option<f64>,
    /// Clustered small sinkhole fields stamped at 2 m in S3 (reviewer:
    /// heathland pock marks, 280-740 m² each, concentrated regions).
    #[serde(default)]
    pub pock_field: Option<f64>,
    /// Probability that a terrace_flight draw realizes as the CENTERED
    /// ASYMMETRIC TERRACED VALLEY (river down the middle, treads
    /// climbing one side, smooth shoulder the other — reviewer's
    /// Banff/Jasper configuration). Remainder keeps the classic
    /// edge-stepping flight.
    #[serde(default)]
    pub terrace_valley: Option<f64>,
}

/// The five S2 structural modules, as data. All in `[0,1]` except
/// `integration`, which is signed: negative deranges the network
/// (heathland's kettle country does not drain to base level).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ModuleIntensities {
    pub trunk_river: f64,
    pub stratigraphy: f64,
    pub closed_basin: f64,
    pub aeolian: f64,
    pub integration: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct StratumTemplate {
    pub thickness_m: f64,
    pub hardness: f64,
    pub dip_rad: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EnvelopeSet {
    pub envelope_version: u32,
    /// Provenance label, e.g. "provisional-c1" or a campaign id.
    pub provenance: String,
    pub biomes: BTreeMap<BiomeId, BiomeEnvelope>,
}

#[derive(Debug)]
pub enum EnvelopeError {
    Parse(String),
    Invalid(String),
    FingerprintMismatch { found: String, expected: String },
}

impl std::fmt::Display for EnvelopeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EnvelopeError::Parse(e) => write!(f, "envelope parse: {e}"),
            EnvelopeError::Invalid(e) => write!(f, "envelope invalid: {e}"),
            EnvelopeError::FingerprintMismatch { found, expected } => write!(
                f,
                "envelope fingerprint mismatch: found {found}, expected {expected}"
            ),
        }
    }
}

impl std::error::Error for EnvelopeError {}

impl EnvelopeSet {
    /// The committed provisional envelope, fingerprint-checked at load.
    pub fn builtin() -> &'static EnvelopeSet {
        use std::sync::OnceLock;
        static SET: OnceLock<EnvelopeSet> = OnceLock::new();
        SET.get_or_init(|| {
            let bytes = include_bytes!("../../data/envelope_provisional.json");
            let fp = blake3::hash(bytes).to_hex().to_string();
            let expected = ENVELOPE_FINGERPRINT_GOLDEN.trim();
            if fp != expected {
                panic!("envelope fingerprint mismatch: found {fp}, expected {expected} — re-bless via the bless_golden example");
            }
            let set: EnvelopeSet = serde_json::from_slice(bytes)
                .unwrap_or_else(|e| panic!("envelope parse: {e}"));
            set.validate()
                .unwrap_or_else(|e| panic!("envelope invalid: {e}"));
            set
        })
    }

    pub fn builtin_fingerprint() -> &'static str {
        ENVELOPE_FINGERPRINT_GOLDEN.trim()
    }

    /// Loud structural validation — every sampler assumption is checked here,
    /// once, so sampling itself never re-checks.
    pub fn validate(&self) -> Result<(), EnvelopeError> {
        if self.envelope_version != ENVELOPE_VERSION {
            return Err(EnvelopeError::Invalid(format!(
                "envelope_version {} != {ENVELOPE_VERSION}",
                self.envelope_version
            )));
        }
        for (biome, env) in &self.biomes {
            let b = biome.key();
            let d = env.dims.len();
            if d == 0 || env.transforms.len() != d {
                return Err(EnvelopeError::Invalid(format!(
                    "{b}: dims/transforms length mismatch"
                )));
            }
            if env.components.is_empty() {
                return Err(EnvelopeError::Invalid(format!("{b}: no components")));
            }
            for (ci, c) in env.components.iter().enumerate() {
                if !(c.weight.is_finite() && c.weight > 0.0) {
                    return Err(EnvelopeError::Invalid(format!("{b}: comp {ci} weight")));
                }
                if c.mean.len() != d {
                    return Err(EnvelopeError::Invalid(format!("{b}: comp {ci} mean len")));
                }
                if c.chol.len() != d
                    || c.chol.iter().enumerate().any(|(i, row)| row.len() != i + 1)
                {
                    return Err(EnvelopeError::Invalid(format!(
                        "{b}: comp {ci} chol must be a lower triangle of {d} rows"
                    )));
                }
                for (i, row) in c.chol.iter().enumerate() {
                    if row[i] <= 0.0 || row.iter().any(|v| !v.is_finite()) {
                        return Err(EnvelopeError::Invalid(format!(
                            "{b}: comp {ci} chol row {i}: diagonal must be > 0, all finite"
                        )));
                    }
                }
            }
            let cw: f64 = env.class_weights.values().sum();
            if env.class_weights.is_empty() || cw <= 0.0 {
                return Err(EnvelopeError::Invalid(format!("{b}: class weights")));
            }
            if !(0.0..=1.0).contains(&env.province2_p) {
                return Err(EnvelopeError::Invalid(format!("{b}: province2_p")));
            }
            if env.province2_p > 0.0 && env.boundary_kind_weights.values().sum::<f64>() <= 0.0 {
                return Err(EnvelopeError::Invalid(format!(
                    "{b}: province2_p > 0 but no boundary kind weights"
                )));
            }
            if env.exemplar_pool.is_empty() || env.exemplars_per_course == 0 {
                return Err(EnvelopeError::Invalid(format!("{b}: exemplar pool")));
            }
            for t in &env.transforms {
                let ok = match *t {
                    Transform::Linear { lo, hi }
                    | Transform::Log { lo, hi }
                    | Transform::Logit { lo, hi } => lo.is_finite() && hi.is_finite() && lo < hi,
                };
                if !ok {
                    return Err(EnvelopeError::Invalid(format!("{b}: bad transform support")));
                }
            }
            let m = &env.modules;
            let unit = [
                ("trunk_river", m.trunk_river),
                ("stratigraphy", m.stratigraphy),
                ("closed_basin", m.closed_basin),
                ("aeolian", m.aeolian),
            ];
            for (name, v) in unit {
                if !(0.0..=1.0).contains(&v) {
                    return Err(EnvelopeError::Invalid(format!(
                        "{b}: module {name} must be in [0,1], got {v}"
                    )));
                }
            }
            if !(-1.0..=1.0).contains(&m.integration) {
                return Err(EnvelopeError::Invalid(format!(
                    "{b}: module integration must be in [-1,1], got {}",
                    m.integration
                )));
            }
            if let Some(f) = env.wave_iso_frac {
                if !(0.0..=1.0).contains(&f) {
                    return Err(EnvelopeError::Invalid(format!(
                        "{b}: wave_iso_frac = {f} out of [0,1]"
                    )));
                }
            }
            if let Some(f) = env.grain_lock {
                if !(0.0..=1.0).contains(&f) {
                    return Err(EnvelopeError::Invalid(format!(
                        "{b}: grain_lock = {f} out of [0,1]"
                    )));
                }
            }
            if let Some(f) = env.channel_water {
                if !(0.0..=1.0).contains(&f) {
                    return Err(EnvelopeError::Invalid(format!(
                        "{b}: channel_water = {f} out of [0,1]"
                    )));
                }
            }
            if let Some(f) = env.wave_beta {
                if !(0.5..=5.0).contains(&f) {
                    return Err(EnvelopeError::Invalid(format!(
                        "{b}: wave_beta = {f} out of [0.5,5]"
                    )));
                }
            }
            if let Some(f) = env.wave_share {
                if !(0.02..=0.8).contains(&f) {
                    return Err(EnvelopeError::Invalid(format!(
                        "{b}: wave_share = {f} out of [0.02,0.8]"
                    )));
                }
            }
            for (w, v) in &env.class_relief_mult {
                if !(0.05..=10.0).contains(v) {
                    return Err(EnvelopeError::Invalid(format!(
                        "{b}: class_relief_mult[{w:?}] = {v} out of (0.05,10)"
                    )));
                }
            }
        }
        Ok(())
    }
}

/// Weighted pick with a single uniform: deterministic, rejection-free.
pub fn pick_weighted<'a, T>(items: impl Iterator<Item = (&'a T, f64)>, u: f64) -> &'a T {
    let items: Vec<(&T, f64)> = items.collect();
    let total: f64 = items.iter().map(|(_, w)| w).sum();
    let mut acc = 0.0;
    let target = u * total;
    let mut last = items[0].0;
    for (item, w) in &items {
        acc += w;
        last = item;
        if target < acc {
            return item;
        }
    }
    last
}
