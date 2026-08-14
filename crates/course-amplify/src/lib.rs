//! Stage S3 — amplification. Where the terrain stops being a construction and
//! starts being a place.
//!
//! S2 authors the structure — the drainage network, the divides, a smooth
//! catena base surface. That base is correct and characterless: a distance
//! transform gives every point equidistant from a channel the same treatment,
//! so interfluves come out smooth and tubular. S3 supplies everything the
//! base cannot, by **reconstructing detail from a dictionary of real terrain
//! patches** fitted offline to the biome's lidar corpus and conditioned on
//! position within the skeleton.
//!
//! This is where archetype identity lives. The measured evidence is that
//! drainage spacing does *not* discriminate archetypes
//! (`dist_to_channel_p50` is 104-120 m in every one), so identity cannot come
//! from the skeleton — it comes from hillslope form and texture, which is
//! exactly what the dictionary carries.
//!
//! Stage doc: `docs/stages/stage-03-amplification.md`.

pub mod blend;
pub mod conditioning;
pub mod dictionary;
pub mod polish;
pub mod synth;

use course_contracts::biome::BiomeId;
use course_seed::RunIdentity;
use course_skeleton::kernel::Skeleton;
use course_spec::v2::SiteSpec;
use course_world::grid::Grid;

use conditioning::Conditioning;
use dictionary::Dictionary;
use synth::Band;

/// S3's output: the amplified 2 m surface plus what downstream verifies.
pub struct Amplified {
    pub height: Grid<f64>,
    /// Residual band std (clean measure for the discriminant gates).
    pub mid_std_m: f64,
    pub fine_std_m: f64,
}

/// THE stage-03 entry point. The dictionary is loaded once by the caller
/// (fingerprint-interlocked) and shared across seeds.
///
/// No biome branch: the biome only selects which dictionary shelf to read,
/// exactly as the envelope selects dials. `amplify/v1` draws nothing — all
/// stochastic choice is position-seeded (acceptance: permuting patch order
/// is bit-identical, tested directly).
pub fn generate(
    spec: &SiteSpec,
    sk: &Skeleton,
    dict: &Dictionary,
    identity: &RunIdentity,
) -> Amplified {
    let t_all = std::time::Instant::now();
    let trace = std::env::var("S3_TRACE").is_ok();
    macro_rules! lap {
        ($label:expr, $t:expr) => {
            if trace {
                eprintln!("  s3::{:16} {:6.0} ms", $label, $t.elapsed().as_secs_f64() * 1e3);
            }
        };
    }
    let biome_key = biome_key(spec.biome);
    // 8 m working views of the skeleton
    let n8 = sk.flow_distance.spec.nx as usize;
    let base8 = {
        // sample the 2 m height at the 8 m nodes (they align exactly)
        let mut g = Grid::filled(sk.flow_distance.spec, 0.0f64);
        for y in 0..n8 {
            for x in 0..n8 {
                let p = g.spec.world_of(x as u32, y as u32);
                g.data[y * n8 + x] = sk.height.bilinear(p);
            }
        }
        g
    };
    let t = std::time::Instant::now();
    let cond = Conditioning::compute(&base8, &sk.flow_distance.data);
    lap!("conditioning", t);

    // mid band on the 8 m grid, fine band on the 2 m grid — same
    // conditioning planes, footprint-scaled (this mirrors the builder,
    // whose fine patches averaged an 8×8 window of the 8 m cond planes).
    let t = std::time::Instant::now();
    let mid8 = synth::quilt(dict, biome_key, Band::Mid, n8, 8.0, &cond, 1, identity);
    lap!("quilt_mid", t);
    let n2 = sk.height.spec.nx as usize;
    let t = std::time::Instant::now();
    let fine2 = synth::quilt(dict, biome_key, Band::Fine, n2, 2.0, &cond, 4, identity);
    lap!("quilt_fine", t);

    let mid_std_m = std_of(&mid8);
    let fine_std_m = std_of(&fine2);

    let t = std::time::Instant::now();
    let mut height = blend::assemble(&sk.height, &mid8, &fine2, &sk.flow_distance);
    lap!("blend", t);
    let t = std::time::Instant::now();
    polish::polish(&mut height, &base8, &sk.embryos);
    lap!("polish", t);
    lap!("TOTAL", t_all);

    Amplified { height, mid_std_m, fine_std_m }
}

fn std_of(a: &[f64]) -> f64 {
    let n = a.len() as f64;
    let m = a.iter().sum::<f64>() / n;
    (a.iter().map(|v| (v - m) * (v - m)).sum::<f64>() / n).sqrt()
}

/// The dictionary's shelf names are the biome keys.
fn biome_key(b: BiomeId) -> &'static str {
    b.key()
}
