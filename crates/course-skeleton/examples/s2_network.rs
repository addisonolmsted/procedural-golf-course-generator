//! S2 IN ISOLATION: the skeleton surface with no S3 texture at all, plus
//! the channel network that carved it (tier-1 polylines + the tier-2
//! dendritic side-valley mask). This is the review instrument for "what
//! does S2 actually carve" — S3/S4 are never run.
//!
//!   cargo run --release -p course-skeleton --example s2_network -- <out> <biome> <seed>
//!   -> <out>/<biome>_<seed>_s2.f32     2 m S2 surface (what S3 receives)
//!      <out>/<biome>_<seed>_chan8.u8   tier-1 channel cells, 8 m
//!      <out>/<biome>_<seed>_tier2.u8   tier-2 side-valley cells, 8 m
//!      <out>/<biome>_<seed>_net.txt    tier-1 polylines: order area_m2 x,y ...
//!      <out>/<biome>_<seed>_meta.txt   n2 cell2 n8 cell8
//!
//! Env: TRIB_REACH / TRIB_DEPTH override the tier-2 dials (ladder runs).

use course_contracts::biome::BiomeId;
use course_seed::RunIdentity;
use course_spec::v2::{SiteSpec, SpecOverridesV2};

fn main() {
    let out = std::env::args().nth(1).expect("out dir");
    std::fs::create_dir_all(&out).unwrap();
    let biome_arg = std::env::args().nth(2).expect("biome");
    let seed: u64 = std::env::args().nth(3).expect("seed").parse().unwrap();
    let biome = BiomeId::ALL
        .iter()
        .copied()
        .find(|b| {
            format!("{b:?}").to_lowercase().replace(' ', "_") == biome_arg.to_lowercase()
                || SiteSpec::generate_builtin(
                    RunIdentity::from_seed(1),
                    &SpecOverridesV2 { forced_biome: Some(*b) },
                )
                .biome
                .key()
                    == biome_arg
        })
        .expect("biome key");
    let id = RunIdentity::from_seed(seed);
    let mut spec = SiteSpec::generate_builtin(id, &SpecOverridesV2 { forced_biome: Some(biome) });
    for (env, dial) in [
        ("TRIB_REACH", "skeleton.tributary_reach"),
        ("TRIB_DEPTH", "skeleton.tributary_depth_m"),
        ("CREEP", "skeleton.creep"),
        ("WANDER", "skeleton.route_wander"),
        ("SPREAD", "skeleton.cut_spread_m"),
        ("VREACH", "skeleton.valley_reach_m"),
        ("CEIL", "skeleton.cut_ceiling_m"),
        ("RELIEF", "primitives.relief_amp_scale"),
        ("BOOST", "skeleton.incision_boost"),
        ("UPLIFT", "skeleton.uplift_m"),
        ("MEXP", "skeleton.area_exp"),
        ("INFLOW_START", "skeleton.inflow_start"),
        ("ROUGH", "skeleton.roughness_frac"),
        ("BANK_ANGLE", "skeleton.bank_angle_deg"),
        ("CH_SPREAD", "skeleton.channel_spread"),
        ("TRUNK_ANGLE", "skeleton.trunk_bank_angle_deg"),
        ("TRUNK_RIVER", "skeleton.trunk_river"),
        ("STRAT", "skeleton.stratigraphy"),
        ("GROOVE", "skeleton.groove_scale"),
        ("FLOOR_MIN", "skeleton.floor_hw_min_m"),
        ("GROOVE_SMALL", "skeleton.groove_small_m"),
        ("GROOVE_SHARE", "skeleton.groove_share"),
        ("WAVE_ISO", "primitives.wave_iso_frac"),
        ("WAVE_BETA", "primitives.wave_beta"),
    ] {
        if let Ok(v) = std::env::var(env) {
            if let Ok(x) = v.parse::<f64>() {
                spec.dials.insert(dial.into(), x);
            }
        }
    }
    let c1 = course_primitives::generate(&spec, &id);
    let sk = course_skeleton::generate(&spec, &c1, &id);

    let key = spec.biome.key();
    let mut buf = Vec::with_capacity(sk.height.data.len() * 4);
    for v in &sk.height.data {
        buf.extend_from_slice(&(*v as f32).to_le_bytes());
    }
    std::fs::write(format!("{out}/{key}_{seed}_s2.f32"), &buf).unwrap();

    let chan: Vec<u8> = sk
        .flow_distance
        .data
        .iter()
        .map(|&d| u8::from(d <= 0.0))
        .collect();
    std::fs::write(format!("{out}/{key}_{seed}_chan8.u8"), &chan).unwrap();
    let tier2 = sk
        .tier2
        .as_ref()
        .map(|g| g.data.clone())
        .unwrap_or_else(|| vec![0u8; chan.len()]);
    std::fs::write(format!("{out}/{key}_{seed}_tier2.u8"), &tier2).unwrap();
    // the kernel's own accumulation on the built surface (m^2, 8 m) — the
    // honest discharge for "how big is the channel this bank belongs to"
    let mut ab = Vec::with_capacity(sk.flow_accum.data.len() * 4);
    for v in &sk.flow_accum.data {
        ab.extend_from_slice(&(*v as f32).to_le_bytes());
    }
    std::fs::write(format!("{out}/{key}_{seed}_acc8.f32"), &ab).unwrap();

    let mut net = String::new();
    for ch in &sk.channels {
        net.push_str(&format!("{} {:.0}", ch.order, ch.area_m2));
        for p in &ch.pts {
            net.push_str(&format!(" {:.2},{:.2}", p.x, p.y));
        }
        net.push('\n');
    }
    std::fs::write(format!("{out}/{key}_{seed}_net.txt"), net).unwrap();
    std::fs::write(
        format!("{out}/{key}_{seed}_meta.txt"),
        format!(
            "{} {} {} {}\n",
            sk.height.spec.nx,
            sk.height.spec.cell_size,
            sk.flow_distance.spec.nx,
            sk.flow_distance.spec.cell_size
        ),
    )
    .unwrap();
    // Corpus-policy re-derivation (identical to examples/measure_battery.rs
    // and structure.py): channels by 6e4 m² accumulation ON THE BUILT
    // SURFACE. This is the number that says whether the TERRAIN carries the
    // network, as opposed to the extraction claiming it does.
    let spec8 = sk.flow_distance.spec;
    let (nx8, ny8) = (spec8.nx as usize, spec8.ny as usize);
    let mut h8 = course_world::grid::Grid::filled(spec8, 0.0f64);
    for y in 0..ny8 {
        for x in 0..nx8 {
            h8.data[y * nx8 + x] = *sk.height.get((x * 4) as u32, (y * 4) as u32);
        }
    }
    let ff = course_world::flow::route(&h8);
    let a_thresh = 6.0e4 / (spec8.cell_size * spec8.cell_size);
    let ch: Vec<bool> = (0..nx8 * ny8)
        .map(|i| {
            let lake = ff.filled.data[i] > h8.data[i] + 0.01;
            let a = ff.acc[i] as f64;
            a >= a_thresh && (!lake || a >= 4.0 * a_thresh)
        })
        .collect();
    let n_ch = ch.iter().filter(|&&c| c).count();
    let rederived_km = n_ch as f64 * spec8.cell_size / 1000.0 / 9.0;
    let seeds: Vec<(usize, course_skeleton::fluvial::flow_distance::Nearest)> = ch
        .iter()
        .enumerate()
        .filter(|&(_, &c)| c)
        .map(|(lin, _)| {
            (
                lin,
                course_skeleton::fluvial::flow_distance::Nearest {
                    dist_m: 0.0,
                    z_channel: 0.0,
                    implied_channel: 0.0,
                    order: 0,
                    area_m2: 0.0,
                },
            )
        })
        .collect();
    let d2c = if seeds.is_empty() {
        f64::NAN
    } else {
        let near = course_skeleton::fluvial::flow_distance::dijkstra(&spec8, &seeds);
        let mut d: Vec<f64> = near.iter().map(|n| n.dist_m).collect();
        d.sort_by(|a, b| a.total_cmp(b));
        d[d.len() / 2]
    };
    let mut zs = sk.height.data.clone();
    zs.sort_by(|a, b| a.total_cmp(b));
    let relief = zs[zs.len() * 99 / 100] - zs[zs.len() / 100];
    eprintln!(
        "{key} {seed}: channels={} tier2_cells={} density={:.2} \
         rederived={:.2} km/km2 d2c_p50={:.0} m relief={:.1} m",
        sk.channels.len(),
        tier2.iter().filter(|&&t| t == 1).count(),
        sk.diagnostics.achieved_density_km_km2,
        rederived_km,
        d2c,
        relief
    );
}
