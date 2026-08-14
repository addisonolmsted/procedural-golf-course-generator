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
/// Max height change allowed on channel-centreline cells.
pub const CHANNEL_TOL_M: f64 = 0.35;
/// Full channel-restore clamp inside this distance of the centreline…
pub const RESTORE_CORE_M: f64 = 5.0;
/// …fading to no restore at this distance (kills the blocky outline).
pub const RESTORE_FEATHER_M: f64 = 13.0;

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
    // <64 m lowpass of the base for the away-from-channel blend
    let base_lp64 = {
        let sigma = conditioning::SIGMA_PER_L * 64.0 / 2.0;
        let box_w = ((sigma * 1.153) as usize * 2 + 1).max(3);
        let mut lp = sk.height.data.clone();
        for _ in 0..3 {
            lp = synth::box_filter(&lp, n2, box_w);
        }
        lp
    };
    let mut height = blend::assemble(&sk.height, &base_lp64, &mid8, &fine2, &sk.flow_distance);
    lap!("blend", t);
    let t = std::time::Instant::now();
    polish::polish(&mut height, &base8, &sk.embryos);
    // CHANNEL RESTORE: texture can dam a shallow reach, and polish then
    // raises the bed upstream of the dam (measured worst case 1.63 m).
    // The channel profile is S2's word: centreline cells clamp back to
    // within CHANNEL_TOL_M of the carved bed. Any residual ±wobble on
    // near-flat reaches is S4's to adjudicate — it re-routes the final
    // surface anyway.
    // FEATHERED by true distance, not per-8 m block: the block version
    // drew the channel outline as a bright axis-aligned staircase
    // polyline (clamped blocks stepping against unclamped neighbors) —
    // one of the reviewer's staircase-edge tells. Full clamp holds
    // within RESTORE_CORE_M of the centreline (the taper test's
    // guarantee), fading to nothing at RESTORE_FEATHER_M.
    let n8 = sk.flow_distance.spec.nx as usize;
    let mut visited = vec![false; n2 * n2];
    for y8 in 0..n8 {
        for x8 in 0..n8 {
            if sk.flow_distance.data[y8 * n8 + x8] > RESTORE_FEATHER_M + 8.0 {
                continue;
            }
            let p8 = sk.flow_distance.spec.world_of(x8 as u32, y8 as u32);
            let cx = (p8.x / 2.0) as usize;
            let cy = (p8.y / 2.0) as usize;
            for dy in 0..4usize {
                for dx in 0..4usize {
                    let (x2, y2) = ((cx + dx).min(n2 - 1), (cy + dy).min(n2 - 1));
                    let i2 = y2 * n2 + x2;
                    if visited[i2] {
                        continue;
                    }
                    visited[i2] = true;
                    let p2 = height.spec.world_of(x2 as u32, y2 as u32);
                    let d = sk.flow_distance.bilinear(p2);
                    let t = ((RESTORE_FEATHER_M - d)
                        / (RESTORE_FEATHER_M - RESTORE_CORE_M))
                        .clamp(0.0, 1.0);
                    if t <= 0.0 {
                        continue;
                    }
                    // clamp toward the SMOOTHED base: pinning to the raw
                    // base re-drew its D8 staircase as thin stepped lines
                    // along every channel (the ribbed-cut tell, second
                    // appearance)
                    let base = base_lp64[i2];
                    let dz = height.data[i2] - base;
                    let target = base + dz.clamp(-CHANNEL_TOL_M, CHANNEL_TOL_M);
                    height.data[i2] += (target - height.data[i2]) * t;
                }
            }
        }
    }
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
