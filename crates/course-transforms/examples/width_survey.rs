//! Cross-biome water width survey, S2-only (fast): replicates
//! channel_water_v2's system selection (coin, ranking, confluence) and
//! width law, over 100 seeds per biome. Widths are tile MEANS; the
//! along-run breathing is a fixed ±28% around them.
use course_contracts::biome::BiomeId;
use course_seed::RunIdentity;
use course_spec::v2::{SiteSpec, SpecOverridesV2};
use course_transforms::hydrology::{
    CHANNEL_WATER_SALT, CONFLUENCE_FRAC, CREEK_MIN_AREA_M2, RIVER_AREA_M2, RIVER_WIDTH_SALT,
};

const WIDE_M: f64 = 54.9; // 60 yards

fn main() {
    let n_seeds: u64 = std::env::var("N").ok().and_then(|v| v.parse().ok()).unwrap_or(100);
    for biome in [
        BiomeId::RiverValley,
        BiomeId::HillCountry,
        BiomeId::Piedmont,
        BiomeId::GreatPlains,
        BiomeId::Heathland,
        BiomeId::Sandhills,
    ] {
        let mut wet = 0usize;
        let mut dry_coin = 0usize;
        let mut dry_area = 0usize;
        let mut rivers = 0usize;
        let mut creeks = 0usize;
        let mut tile_widths: Vec<f64> = Vec::new(); // widest watered system per tile
        let mut wide_tiles = 0usize;
        for seed in 1..=n_seeds {
            let id = RunIdentity::from_seed(seed);
            let spec =
                SiteSpec::generate_builtin(id, &SpecOverridesV2 { forced_biome: Some(biome) });
            let c1 = course_primitives::generate(&spec, &id);
            let sk = course_skeleton::generate(&spec, &c1, &id);
            let dial = spec.dials.get("hydrology.channel_water").copied().unwrap_or(0.0);
            if dial <= 0.0 || sk.channels.is_empty() {
                dry_coin += 1;
                continue;
            }
            let always = dial >= 1.0;
            let coin = id.course_scalar(CHANNEL_WATER_SALT) < dial;
            if !always && !coin {
                dry_coin += 1;
                continue;
            }
            // rank root systems by area (mirror of channel_water_v2)
            let chans = &sk.channels;
            let mut roots: Vec<u32> = (0..chans.len() as u32)
                .map(|ci| {
                    let mut r = ci;
                    while let Some(p) = chans[r as usize].parent {
                        r = p;
                    }
                    r
                })
                .collect();
            roots.sort_unstable();
            roots.dedup();
            let mut ranked: Vec<f64> = roots.iter().map(|&r| chans[r as usize].area_m2).collect();
            ranked.sort_by(|a, b| b.total_cmp(a));
            let trunk = ranked[0];
            let min_area = if always { RIVER_AREA_M2.min(trunk) } else { CREEK_MIN_AREA_M2 };
            let mut widths: Vec<(f64, bool)> = Vec::new();
            let pers = 0.75 + id.course_scalar(RIVER_WIDTH_SALT);
            let wscale = spec.dials.get("hydrology.channel_width_scale").copied().unwrap_or(1.0);
            for (k, &a) in ranked.iter().enumerate() {
                let ok = match k {
                    0 => a >= min_area,
                    1 => a >= min_area && (always && a >= CONFLUENCE_FRAC * trunk || !always),
                    _ => false,
                };
                if ok {
                    let w = (8.5 * (a / 1.0e6).sqrt() * pers * wscale).clamp(6.0, 80.0);
                    widths.push((w, a >= RIVER_AREA_M2));
                }
            }
            if widths.is_empty() {
                dry_area += 1;
                continue;
            }
            wet += 1;
            for &(_, is_river) in &widths {
                if is_river {
                    rivers += 1;
                } else {
                    creeks += 1;
                }
            }
            let wmax = widths.iter().map(|w| w.0).fold(0.0f64, f64::max);
            tile_widths.push(wmax);
            if wmax >= WIDE_M {
                wide_tiles += 1;
            }
        }
        tile_widths.sort_by(|a, b| a.total_cmp(b));
        let q = |p: f64| -> f64 {
            if tile_widths.is_empty() {
                return f64::NAN;
            }
            tile_widths[((tile_widths.len() - 1) as f64 * p) as usize]
        };
        let mean = tile_widths.iter().sum::<f64>() / tile_widths.len().max(1) as f64;
        println!(
            "{biome:?}: wet {wet:3}/{n_seeds} (dry: coin {dry_coin}, no-area {dry_area}) | rivers {rivers:3} creeks {creeks:3} | tile widths m: mean {mean:5.1} p10 {:5.1} p50 {:5.1} p90 {:5.1} max {:5.1} | >=60yd tiles {wide_tiles}",
            q(0.1),
            q(0.5),
            q(0.9),
            q(1.0),
        );
    }
}
