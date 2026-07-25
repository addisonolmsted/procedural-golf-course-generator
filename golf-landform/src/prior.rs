//! The calibrated landform prior — serde model of `landform_prior.json`
//! (fit by tools/landform_prior from the 205-tile extraction campaign) plus
//! the deterministic sampling primitives the seeded generator draws with:
//! 11-point quantile tables (inverse-CDF, temperature-warped), log-log
//! power-law regressions on drainage area, and a Gaussian copula over the
//! valley cross-section residual vector.
//!
//! Thin archetypes were already shrunk toward the pooled population at FIT
//! time (lambda = n/(n+25)); every table is therefore present in every block,
//! but access still falls back to pooled defensively so a hand-edited prior
//! cannot panic the sampler.

use golf_core::math::{erf, exp};
use serde::{Deserialize, Serialize};

/// A weighted 11-point quantile table (q0, q10, …, q100 of the calibration
/// population). Sampling is piecewise-linear inverse-CDF — tails clamp at
/// q0/q100 by construction.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Deciles(pub Vec<f64>);

impl Deciles {
    /// Inverse-CDF sample at uniform `u` after temperature warping
    /// `u' = 0.5 + tau (u - 0.5)` (tau <= 1 concentrates toward the median,
    /// tau > 1 spreads toward — never past — the observed extremes).
    pub fn sample(&self, u: f64, tau: f64) -> f64 {
        let q = &self.0;
        if q.is_empty() {
            return 0.0;
        }
        if q.len() == 1 {
            return q[0];
        }
        let u = (0.5 + tau * (u - 0.5)).clamp(0.0, 1.0);
        let pos = u * (q.len() - 1) as f64;
        let i = (pos.floor() as usize).min(q.len() - 2);
        let f = pos - i as f64;
        q[i] + f * (q[i + 1] - q[i])
    }
}

/// ln y = b0 + b1 ln x with residual deciles (draw a residual from the table,
/// not a parametric distribution).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PowerLaw {
    pub b0: f64,
    pub b1: f64,
    pub resid_deciles: Deciles,
}

impl PowerLaw {
    /// y = exp(b0 + b1 ln x + resid) with the residual drawn at uniform `u`.
    pub fn eval(&self, x: f64, u: f64, tau: f64) -> f64 {
        exp(self.b0 + self.b1 * golf_core::math::ln(x.max(1e-9))
            + self.resid_deciles.sample(u, tau))
    }
    /// The central curve (no residual).
    pub fn median(&self, x: f64) -> f64 {
        exp(self.b0 + self.b1 * golf_core::math::ln(x.max(1e-9))
            + self.resid_deciles.sample(0.5, 1.0))
    }
}

/// Slope–area law S = k A^-theta, k = exp(ln_k).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SlopeArea {
    pub ln_k: f64,
    pub theta: f64,
    pub resid_deciles: Deciles,
}

impl SlopeArea {
    pub fn eval(&self, a_km2: f64, u: f64, tau: f64) -> f64 {
        exp(self.ln_k - self.theta * golf_core::math::ln(a_km2.max(1e-9))
            + self.resid_deciles.sample(u, tau))
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Hydraulic {
    pub floor_halfwidth: PowerLaw,
    pub incision: PowerLaw,
    pub top_width: PowerLaw,
    pub fall_gradient: SlopeArea,
    pub wall_grad_mean_deciles: Deciles,
    pub wall_asym_logratio_deciles: Deciles,
    pub floor_round_deciles: Deciles,
}

/// Cholesky factor of the rank-correlation matrix over the reach residual
/// vector `fields` (Gaussian copula). Sampling: z = L g with g iid N(0,1),
/// then each margin maps through Phi(z) into its own quantile table.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Copula {
    pub fields: Vec<String>,
    pub chol: Vec<Vec<f64>>,
    pub source: String,
    #[serde(default)]
    pub n: Option<u64>,
}

impl Copula {
    /// Map iid standard normals through the Cholesky factor and Phi into
    /// correlated uniforms (one per copula field, in field order).
    pub fn correlate(&self, g: &[f64]) -> Vec<f64> {
        let k = self.chol.len();
        let mut u = Vec::with_capacity(k);
        for row in &self.chol {
            let z: f64 = row.iter().zip(g).map(|(c, gi)| c * gi).sum();
            u.push(phi(z));
        }
        u
    }
}

/// Standard normal CDF.
pub fn phi(z: f64) -> f64 {
    0.5 * (1.0 + erf(z * std::f64::consts::FRAC_1_SQRT_2))
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MeanderPrior {
    pub fit_frac: f64,
    pub wavelength_mult_deciles: Deciles,
    pub intensity_deciles: Deciles,
    pub sinuosity_deciles: Deciles,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NetworkPrior {
    pub drainage_density_deciles: Deciles,
    pub junction_angle_deg_deciles: Deciles,
    pub junction_area_logratio_deciles: Deciles,
    pub junction_spacing_m_deciles: Deciles,
    pub trunk_junction_spacing_m_deciles: Deciles,
    pub n_junctions_deciles: Deciles,
    #[serde(rename = "trunk_mouth_logA_deciles")]
    pub trunk_mouth_log_a_deciles: Deciles,
    pub branch_len_m_deciles: Deciles,
    pub depth2_frac: f64,
    pub side_balance_beta: [f64; 2],
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TiltPrior {
    pub grade_deciles: Deciles,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RidgesPrior {
    pub per_km2_deciles: Deciles,
    pub prominence_deciles: Deciles,
    pub crest_halfwidth_deciles: Deciles,
    pub flank_grad_deciles: Deciles,
    pub len_m_deciles: Deciles,
    pub spacing_m_deciles: Deciles,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BluffsPrior {
    pub per_km2_deciles: Deciles,
    pub height_deciles: Deciles,
    pub face_grad_deciles: Deciles,
    pub len_m_deciles: Deciles,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BowlsPrior {
    pub per_km2_deciles: Deciles,
    pub depth_deciles: Deciles,
    pub radius_deciles: Deciles,
    pub inner_grad_deciles: Deciles,
    pub wobble_deciles: Deciles,
    pub cycles_deciles: Deciles,
    pub lake_frac: f64,
}

/// One archetype's calibrated block (or the pooled population when
/// `name`/`weight` are absent).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Block {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub weight: Option<f64>,
    pub hydraulic: Hydraulic,
    pub copula: Copula,
    pub meander: MeanderPrior,
    pub network: NetworkPrior,
    pub tilt: TiltPrior,
    pub ridges: RidgesPrior,
    pub bluffs: BluffsPrior,
    pub bowls: BowlsPrior,
    pub noise_defaults: std::collections::BTreeMap<String, f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LandformPrior {
    pub version: u32,
    pub prior_version: String,
    pub archetype_names: Vec<String>,
    pub pooled: Block,
    pub archetypes: Vec<Block>,
}

impl LandformPrior {
    pub fn from_json(s: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(s)
    }

    /// The repo-committed prior baked into the binary (goldens + terrain-lab
    /// run against exactly this data; a re-fit re-blesses explicitly).
    pub fn builtin() -> Self {
        Self::from_json(include_str!("../data/landform_prior.json"))
            .expect("builtin landform_prior.json parses")
    }

    /// FNV-1a over the serialized prior — folded into golden identity so a
    /// re-fit cannot silently keep stale goldens green.
    pub fn fingerprint(&self) -> u64 {
        let s = serde_json::to_string(self).expect("prior serializes");
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for b in s.as_bytes() {
            h ^= *b as u64;
            h = h.wrapping_mul(0x0000_0100_0000_01B3);
        }
        h
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_parses_and_is_complete() {
        let p = LandformPrior::builtin();
        assert_eq!(p.archetypes.len(), p.archetype_names.len());
        assert_eq!(p.pooled.copula.chol.len(), p.pooled.copula.fields.len());
        for a in &p.archetypes {
            assert!(a.weight.unwrap_or(0.0) >= 0.0);
            assert_eq!(a.copula.chol.len(), a.copula.fields.len());
            assert_eq!(a.hydraulic.floor_halfwidth.resid_deciles.0.len(), 11);
        }
    }

    #[test]
    fn decile_sampling_is_monotone_and_clamped() {
        let d = Deciles(vec![
            0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0,
        ]);
        assert_eq!(d.sample(0.0, 1.0), 0.0);
        assert_eq!(d.sample(1.0, 1.0), 10.0);
        assert!((d.sample(0.5, 1.0) - 5.0).abs() < 1e-12);
        // temperature: tau=0 collapses to the median, tau>1 stays in range
        assert!((d.sample(0.1, 0.0) - 5.0).abs() < 1e-12);
        assert!(d.sample(1.0, 1.2) <= 10.0);
        assert!(d.sample(0.0, 1.2) >= 0.0);
        let mut prev = f64::NEG_INFINITY;
        for i in 0..=100 {
            let v = d.sample(i as f64 / 100.0, 1.0);
            assert!(v >= prev);
            prev = v;
        }
    }

    #[test]
    fn phi_is_a_cdf() {
        assert!((phi(0.0) - 0.5).abs() < 1e-12);
        assert!(phi(-8.0) < 1e-9);
        assert!(phi(8.0) > 1.0 - 1e-9);
    }

    #[test]
    fn copula_identity_maps_normals_to_uniformish() {
        let c = Copula {
            fields: vec!["a".into(), "b".into()],
            chol: vec![vec![1.0, 0.0], vec![0.0, 1.0]],
            source: "own".into(),
            n: None,
        };
        let u = c.correlate(&[0.0, 1.0]);
        assert!((u[0] - 0.5).abs() < 1e-12);
        assert!(u[1] > 0.8 && u[1] < 0.9); // Phi(1) ~ 0.841
    }
}
