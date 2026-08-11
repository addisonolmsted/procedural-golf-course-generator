//! River-valley terrace expression check: per-seed window class, core
//! relief, and a tread/riser profile (mean elevation per 100 m along-axis
//! bin; a terrace reads as plateaus separated by >=1.5 m risers). Dumps
//! the S2 surface (375x375 f32 @ 8 m) for the flagged terrace seeds.
use course_contracts::biome::BiomeId;
use course_seed::RunIdentity;
use course_spec::v2::{SiteSpec, SpecOverridesV2};

fn main() {
    let out_dir = std::env::args().nth(1).unwrap_or_else(|| "/tmp/rv_check".into());
    std::fs::create_dir_all(&out_dir).unwrap();
    for k in 0..40u64 {
        let seed = 41_000 + k;
        let id = RunIdentity::from_seed(seed);
        let spec = SiteSpec::generate_builtin(
            id,
            &SpecOverridesV2 { forced_biome: Some(BiomeId::RiverValley) },
        );
        let c1 = course_primitives::generate(&spec, &id);
        let sk = course_skeleton::generate(&spec, &c1, &id);
        let g = &sk.height; // 2 m
        // core relief p95-p5
        let (c0, c1i) = (375u32, 1125u32);
        let mut v = Vec::new();
        for y in (c0..c1i).step_by(4) {
            for x in (c0..c1i).step_by(4) {
                v.push(*g.get(x, y));
            }
        }
        v.sort_by(|a, b| a.total_cmp(b));
        let q = |p: f64| v[((v.len() - 1) as f64 * p) as usize];
        let relief = q(0.95) - q(0.05);
        // along-axis profile over the full tile: mean z per 100 m bin along
        // the base-edge (tilt) axis; risers = bin-to-bin steps >= 1.5 m
        let edge = spec_edge_axis(&spec);
        let mut bins = vec![(0.0f64, 0usize); 30];
        for y in (0..1500u32).step_by(4) {
            for x in (0..1500u32).step_by(4) {
                let (px, py) = (x as f64 * 2.0, y as f64 * 2.0);
                let along = if edge_is_ns(edge) { py } else { px };
                let b = ((along / 100.0) as usize).min(29);
                bins[b].0 += *g.get(x, y);
                bins[b].1 += 1;
            }
        }
        let prof: Vec<f64> = bins.iter().map(|(s, n)| s / (*n).max(1) as f64).collect();
        let mut risers = 0;
        let mut max_step = 0.0f64;
        for w in prof.windows(2) {
            let d = (w[1] - w[0]).abs();
            max_step = max_step.max(d);
            if d >= 1.5 {
                risers += 1;
            }
        }
        println!(
            "seed {} class {:?} prov {} | core relief {:5.1} m | risers>=1.5m {} | max 100m step {:4.1} m",
            seed, spec.structure_class.window, spec.structure_class.provinces, relief, risers, max_step
        );
        if format!("{:?}", spec.structure_class.window).contains("Terrace") && k < 20 {
            let mut buf = Vec::with_capacity(375 * 375 * 4);
            for y in 0..375u32 {
                for x in 0..375u32 {
                    buf.extend_from_slice(&(*g.get(x * 4, y * 4) as f32).to_le_bytes());
                }
            }
            std::fs::write(format!("{}/rv_{}.f32", out_dir, seed), &buf).unwrap();
        }
    }
}

fn spec_edge_axis(spec: &SiteSpec) -> u8 {
    // recompute the drawn base edge the same way S1 does: peek via C1 meta
    let id = RunIdentity::from_seed(spec.seed);
    let c1 = course_primitives::generate(spec, &id);
    match c1.meta.base_level.edge {
        course_contracts::metadata::Edge::N | course_contracts::metadata::Edge::S => 0,
        _ => 1,
    }
}

fn edge_is_ns(e: u8) -> bool {
    e == 0
}
