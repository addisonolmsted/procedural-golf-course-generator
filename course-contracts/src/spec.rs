//! Stage 1–2 output: `CourseSpec` — the seed, the archetype, and one sampled
//! `ParamPack` with a section per downstream stage. Every stage reads its own
//! section and nothing else; adding a knob is a contract change (bump
//! [`crate::PIPELINE_VERSION`], update ARCHITECTURE.md, re-fit or re-author the
//! prior entry).
//!
//! Parameters come from `data/archetype_priors.json` — 11-point quantile
//! tables per knob per archetype. M0 ships hand-authored placeholder values;
//! the Python campaign (`tools/archetype_atlas`) later replaces the DATA
//! without touching this schema. `course-seed` will sample the tables
//! (independent quantile draws for now; copula later); until then
//! [`ParamPack::from_prior_median`] gives the deterministic center of an
//! archetype.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::archetype::{ArchetypeId, HydrologyMode};

/// Stage 3 knobs — macro landform skeleton.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LandformParams {
    /// Target relief (m) across the full 3 km box.
    pub relief_amp_m: f64,
    /// Hard cap on relief inside the routable core (the gate-pass lever).
    pub core_relief_cap_m: f64,
    /// Regional tilt as grade (rise/run).
    pub tilt_grade: f64,
    /// Expected drainage-spine count (valleys crossing the box).
    pub valley_count: f64,
    pub ridge_count: f64,
    /// Bench/terrace systems (mountain archetype; 0 elsewhere).
    pub bench_count: f64,
    /// Closed basins / kettles (moraine, florida; 0 elsewhere).
    pub basin_count: f64,
    /// Aeolian dune-train wavelength in m (sandhills; 0 = no dunes).
    pub dune_wavelength_m: f64,
}

/// Stage 4 knobs — skeleton-conditioned noise.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NoiseParams {
    pub base_amp_m: f64,
    pub base_wavelength_m: f64,
    pub octaves: f64,
    /// 0 = billowy fBm, 1 = fully ridged.
    pub ridged_mix: f64,
    /// Anisotropy stretch ratio along the skeleton grain.
    pub aniso_ratio: f64,
    /// Noise suppression on valley floors (the no-dam guard), 0..1.
    pub floor_damp: f64,
    /// Extra amplitude on steep skeleton slopes, 0..1.
    pub slope_gain: f64,
    /// How strongly noise anisotropy follows the skeleton grain, 0..1.
    pub grain_align: f64,
}

/// Stage 5 knobs — erosion + hydrography. Which knobs matter is governed by
/// the archetype's [`HydrologyMode`]; the others are still present (uniform
/// contract) but inert.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HydroParams {
    /// Relative rainfall forcing (1 = temperate reference).
    pub rainfall: f64,
    /// Fraction of rainfall that infiltrates (sandhills → ~1: no runoff).
    pub infiltration: f64,
    /// Stream-power erodibility (alpine high-contrast, clay low).
    pub erodibility: f64,
    /// Water-table depth below median terrain (m). Meaningful for
    /// `WaterTable`; large values elsewhere.
    pub water_table_depth_m: f64,
    /// Tendency to preserve closed depressions instead of breaching, 0..1
    /// (`Kettle` ≈ 1, fluvial ≈ 0).
    pub depression_keep: f64,
    /// Effective drainage area (ha) at which a channel becomes a stream.
    pub channel_threshold_ha: f64,
}

/// Stage 6 knobs — cover assignment.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CoverParams {
    /// Target canopy fraction over the box, 0..1.
    pub canopy_frac: f64,
    /// Exposed sand/waste fraction, 0..1.
    pub sand_exposure: f64,
    /// Wetland propensity near water, 0..1.
    pub wetland_bias: f64,
    /// Clump correlation length (m) for cover mosaics.
    pub clump_scale_m: f64,
}

/// Stage 7 thresholds — the routability gate (per-archetype, fixed switches
/// rather than sampled: the gate must be stable while terrain knobs vary).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GateParams {
    /// Min fraction of core cells with slope < 8%.
    pub slope_lt8_min: f64,
    /// Min largest connected routable patch in the core (ha).
    pub routable_patch_ha_min: f64,
    /// Max fraction of the core blocked by water/wetland.
    pub water_blockage_max: f64,
    /// Max forced water crossings on a feasible 9-corridor layout.
    pub crossings_max: f64,
}

/// Stage 8 switches — routing (fixed, not prior-sampled).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RouteParams {
    pub hole_count: u32,
    /// Par per hole, in play order; must sum to 36 for 9 holes.
    pub par_sequence: Vec<u32>,
}

/// Stage 9 knobs — earthmoving budgets.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EarthworksParams {
    /// Total cut+fill budget (m³) across the course.
    pub cut_fill_budget_m3: f64,
    pub green_area_m2: f64,
    pub tee_area_m2: f64,
    /// Tie-in feather width (m) at patch edges.
    pub feather_m: f64,
}

/// One section per stage. Field names are the prior-file section keys.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ParamPack {
    pub landform: LandformParams,
    pub noise: NoiseParams,
    pub hydro: HydroParams,
    pub cover: CoverParams,
    pub gate: GateParams,
    pub route: RouteParams,
    pub earthworks: EarthworksParams,
}

/// The stage 1–2 artifact. Everything downstream is a pure function of this
/// (plus its upstream artifacts).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CourseSpec {
    pub pipeline_version: u32,
    pub seed: u64,
    pub archetype: ArchetypeId,
    pub hydrology_mode: HydrologyMode,
    pub params: ParamPack,
}

impl CourseSpec {
    /// Artifact-store directory slug: `<seed hex>-<archetype>`.
    pub fn slug(&self) -> String {
        format!("{:016x}-{}", self.seed, self.archetype.key())
    }
}

// ---------------------------------------------------------------------------
// Priors
// ---------------------------------------------------------------------------

/// 11-point quantile table (q0, q10, …, q100) for one knob.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Quantiles {
    pub q: [f64; 11],
}

impl Quantiles {
    pub fn median(&self) -> f64 {
        self.q[5]
    }

    /// Piecewise-linear inverse CDF at u ∈ [0,1].
    pub fn sample(&self, u: f64) -> f64 {
        let u = u.clamp(0.0, 1.0) * 10.0;
        let i = (u.floor() as usize).min(9);
        let t = u - i as f64;
        self.q[i] + (self.q[i + 1] - self.q[i]) * t
    }
}

/// One archetype's prior entry: quantile tables keyed `"<section>.<knob>"`
/// plus the non-sampled switches.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PriorEntry {
    pub params: BTreeMap<String, Quantiles>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ArchetypePriors {
    pub prior_version: String,
    pub archetypes: BTreeMap<String, PriorEntry>,
}

impl ArchetypePriors {
    /// The committed prior (placeholder until the Python campaign delivers a
    /// fitted file with the same schema).
    pub fn builtin() -> ArchetypePriors {
        serde_json::from_str(include_str!("../data/archetype_priors.json"))
            .expect("committed archetype_priors.json parses")
    }

    pub fn entry(&self, a: ArchetypeId) -> &PriorEntry {
        self.archetypes
            .get(a.key())
            .unwrap_or_else(|| panic!("prior missing archetype {}", a.key()))
    }

    /// Fingerprint folded into goldens so a prior re-fit cannot silently keep
    /// them green (mirrors the landform_prior convention).
    pub fn fingerprint(&self) -> u64 {
        crate::fnv_bytes(serde_json::to_string(self).expect("prior serializes").as_bytes())
    }
}

impl ParamPack {
    /// Deterministic archetype center: every knob at its prior median.
    /// (`course-seed` replaces this with real quantile sampling.)
    pub fn from_prior_median(priors: &ArchetypePriors, a: ArchetypeId) -> ParamPack {
        let e = priors.entry(a);
        let g = |key: &str| -> f64 {
            e.params
                .get(key)
                .unwrap_or_else(|| panic!("prior for {} missing knob {key}", a.key()))
                .median()
        };
        ParamPack {
            landform: LandformParams {
                relief_amp_m: g("landform.relief_amp_m"),
                core_relief_cap_m: g("landform.core_relief_cap_m"),
                tilt_grade: g("landform.tilt_grade"),
                valley_count: g("landform.valley_count"),
                ridge_count: g("landform.ridge_count"),
                bench_count: g("landform.bench_count"),
                basin_count: g("landform.basin_count"),
                dune_wavelength_m: g("landform.dune_wavelength_m"),
            },
            noise: NoiseParams {
                base_amp_m: g("noise.base_amp_m"),
                base_wavelength_m: g("noise.base_wavelength_m"),
                octaves: g("noise.octaves"),
                ridged_mix: g("noise.ridged_mix"),
                aniso_ratio: g("noise.aniso_ratio"),
                floor_damp: g("noise.floor_damp"),
                slope_gain: g("noise.slope_gain"),
                grain_align: g("noise.grain_align"),
            },
            hydro: HydroParams {
                rainfall: g("hydro.rainfall"),
                infiltration: g("hydro.infiltration"),
                erodibility: g("hydro.erodibility"),
                water_table_depth_m: g("hydro.water_table_depth_m"),
                depression_keep: g("hydro.depression_keep"),
                channel_threshold_ha: g("hydro.channel_threshold_ha"),
            },
            cover: CoverParams {
                canopy_frac: g("cover.canopy_frac"),
                sand_exposure: g("cover.sand_exposure"),
                wetland_bias: g("cover.wetland_bias"),
                clump_scale_m: g("cover.clump_scale_m"),
            },
            gate: GateParams {
                slope_lt8_min: g("gate.slope_lt8_min"),
                routable_patch_ha_min: g("gate.routable_patch_ha_min"),
                water_blockage_max: g("gate.water_blockage_max"),
                crossings_max: g("gate.crossings_max"),
            },
            route: RouteParams {
                hole_count: 9,
                par_sequence: vec![4, 4, 3, 5, 4, 3, 4, 5, 4],
            },
            earthworks: EarthworksParams {
                cut_fill_budget_m3: g("earthworks.cut_fill_budget_m3"),
                green_area_m2: g("earthworks.green_area_m2"),
                tee_area_m2: g("earthworks.tee_area_m2"),
                feather_m: g("earthworks.feather_m"),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_prior_covers_all_archetypes_and_knobs() {
        let p = ArchetypePriors::builtin();
        for a in ArchetypeId::ALL {
            // panics if any knob is missing
            let pack = ParamPack::from_prior_median(&p, a);
            assert_eq!(pack.route.par_sequence.iter().sum::<u32>(), 36);
            assert_eq!(pack.route.hole_count, 9);
        }
    }

    #[test]
    fn quantile_tables_are_monotone() {
        let p = ArchetypePriors::builtin();
        for (ak, e) in &p.archetypes {
            for (k, q) in &e.params {
                for i in 0..10 {
                    assert!(
                        q.q[i] <= q.q[i + 1],
                        "{ak}.{k} quantiles not monotone at {i}"
                    );
                }
            }
        }
    }

    #[test]
    fn archetype_physics_read_sensibly_from_the_placeholder() {
        let p = ArchetypePriors::builtin();
        let sh = ParamPack::from_prior_median(&p, ArchetypeId::Sandhills);
        let fl = ParamPack::from_prior_median(&p, ArchetypeId::FloridaLowland);
        let mt = ParamPack::from_prior_median(&p, ArchetypeId::MountainBench);
        assert!(sh.hydro.infiltration > 0.9, "sandhills infiltrate");
        assert!(fl.hydro.water_table_depth_m < 2.0, "florida water table at surface");
        assert!(fl.landform.relief_amp_m < 10.0, "florida is flat");
        assert!(mt.landform.relief_amp_m > 60.0, "mountain has relief");
    }

    #[test]
    fn quantile_sampling_interpolates() {
        let q = Quantiles { q: [0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0] };
        assert_eq!(q.sample(0.0), 0.0);
        assert_eq!(q.sample(1.0), 10.0);
        assert!((q.sample(0.55) - 5.5).abs() < 1e-12);
        assert_eq!(q.median(), 5.0);
    }
}
