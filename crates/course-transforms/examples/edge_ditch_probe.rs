//! Diagnose border ditches: mean elevation vs distance-from-border for
//! each edge, at S2 (carve), S3 (amplify), S4 (final) — which stage
//! digs the dip, how deep, and how far in.
use course_contracts::biome::BiomeId;
use course_seed::RunIdentity;
use course_spec::v2::{SiteSpec, SpecOverridesV2};
use course_world::grid::Grid;

fn profile(g: &Grid<f64>, edge: &str) -> Vec<f64> {
    // mean height at each distance row 0..200 cells (400 m at 2 m)
    let n = g.spec.nx as usize;
    let rows = 200.min(n / 2);
    let mut out = vec![0.0f64; rows];
    for d in 0..rows {
        let mut acc = 0.0;
        // sample the middle 60% of the border to avoid corner effects
        let lo = (n as f64 * 0.2) as usize;
        let hi = (n as f64 * 0.8) as usize;
        for t in lo..hi {
            let (x, y) = match edge {
                "W" => (d, t),
                "E" => (n - 1 - d, t),
                "S" => (t, d),
                _ => (t, n - 1 - d),
            };
            acc += g.data[y * n + x];
        }
        out[d] = acc / (hi - lo) as f64;
    }
    out
}

fn dip(p: &[f64]) -> (f64, f64) {
    // reference = mean of the 300-400 m band; dip = min in 0..300 m
    let cell = 2.0;
    let refl = p[150..].iter().sum::<f64>() / (p.len() - 150) as f64;
    let (mut dmin, mut at) = (f64::MAX, 0.0);
    for (i, &v) in p.iter().enumerate().take(150) {
        if v - refl < dmin {
            dmin = v - refl;
            at = i as f64 * cell;
        }
    }
    (dmin, at)
}

fn main() {
    let dict = ["assets/dictionary_v2.bin", "../../assets/dictionary_v2.bin"]
        .iter()
        .find_map(|p| course_amplify::dictionary::Dictionary::load(std::path::Path::new(p)).ok())
        .expect("dictionary");
    for (biome, seed) in [
        (BiomeId::RiverValley, 1u64),
        (BiomeId::RiverValley, 7),
        (BiomeId::Piedmont, 3),
        (BiomeId::HillCountry, 5),
    ] {
        let id = RunIdentity::from_seed(seed);
        let spec =
            SiteSpec::generate_builtin(id, &SpecOverridesV2 { forced_biome: Some(biome) });
        let c1 = course_primitives::generate(&spec, &id);
        let sk = course_skeleton::generate(&spec, &c1, &id);
        if std::env::var("S2ONLY").is_ok() {
            println!("{biome:?} seed {seed} (base {:?}):", sk.meta.base_level.edge);
            for edge in ["W", "E", "S", "N"] {
                let pr = profile(&sk.height, edge);
                let pts: Vec<String> =
                    (0..12).map(|i| format!("{:.1}", pr[i * 5] - pr[199])).collect();
                println!("  {edge}: {}", pts.join(" "));
            }
            continue;
        }
        let amp = course_amplify::generate(&spec, &sk, &dict, &id);
        let h = course_transforms::hydrology::generate(
            &spec,
            &sk,
            &amp,
            &id,
            &course_transforms::hydrology::DEFAULT_TRANSFORMS,
        );
        println!("{biome:?} seed {seed} (base edge {:?}):", sk.meta.base_level.edge);
        if std::env::var("CURVES").is_ok() {
            for edge in ["W", "E", "S", "N"] {
                let pr = profile(&sk.height, edge);
                let pts: Vec<String> = (0..30)
                    .map(|i| format!("{:.1}", pr[i * 5] - pr[199]))
                    .collect();
                println!("  {edge} S2 curve (0..300m rel 400m): {}", pts.join(" "));
            }
        }
        for edge in ["W", "E", "S", "N"] {
            let (d2, a2) = dip(&profile(&sk.height, edge));
            let (d3, a3) = dip(&profile(&amp.height, edge));
            let (d4, a4) = dip(&profile(&h.height, edge));
            println!(
                "  {edge}: dip S2 {d2:6.2} m @{a2:4.0}  S3 {d3:6.2} m @{a3:4.0}  S4 {d4:6.2} m @{a4:4.0}"
            );
        }
    }
}
