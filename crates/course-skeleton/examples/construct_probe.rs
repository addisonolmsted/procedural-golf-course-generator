//! M1 of the constructive network: trunks read off the C1 macro.
//!
//! Instrument only — nothing here touches the shipped pipeline. It dumps
//! the macro surface, the shipped extraction's own trunks and the
//! constructed ones, so the two can be compared on the question that
//! matters first: does a trunk placed from the macro alone sit where the
//! macro says water goes, and does it carry the corpus's sinuosity?
//!
//!   cargo run --release -p course-skeleton --example construct_probe -- <out> <biome> <seed>
//!   -> <out>/<biome>_<seed>_macro.f32    C1 tilt+relief, 8 m
//!      <out>/<biome>_<seed>_s2.f32       the shipped 2 m surface, for context
//!      <out>/<biome>_<seed>_ctrunk.txt   constructed trunks: area_m2 x,y ...
//!      <out>/<biome>_<seed>_strunk.txt   shipped trunks (top systems), same format
//!      <out>/<biome>_<seed>_meta.txt     n8 cell8 n2 cell2 trunk_count
//!
//! Env: TRUNKS overrides the trunk count (default 2), OMEGA the meander
//! swing, LAM the meander wavelength.

use course_contracts::biome::BiomeId;
use course_seed::RunIdentity;
use course_skeleton::fluvial::construct;
use course_spec::v2::{SiteSpec, SpecOverridesV2};
use course_world::math::Vec2;

fn envf(name: &str, dflt: f64) -> f64 {
    std::env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(dflt)
}

fn write_lines(path: String, lines: &[(f64, Vec<Vec2>)]) {
    let mut s = String::new();
    for (area, pts) in lines {
        s.push_str(&format!("{area:.0}"));
        for p in pts {
            s.push_str(&format!(" {:.2},{:.2}", p.x, p.y));
        }
        s.push('\n');
    }
    std::fs::write(path, s).unwrap();
}

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
    let spec = SiteSpec::generate_builtin(id, &SpecOverridesV2 { forced_biome: Some(biome) });
    let key = spec.biome.key();
    let c1 = course_primitives::generate(&spec, &id);

    // the macro S2 starts from
    let spec8 = c1.grid;
    let mut implied = c1.tilt.clone();
    for (i, v) in implied.data.iter_mut().enumerate() {
        *v += c1.relief.data[i];
    }
    let relief_amp = *spec.dials.get("primitives.relief_amp_m").unwrap_or(&8.0);
    let rim = 40.0 + 4.0 * relief_amp;

    let count = envf("TRUNKS", 2.0).round().max(1.0) as usize;
    let ff = construct::macro_flow(&spec8, &implied, c1.meta.base_level.edge, rim);
    let raw = construct::trunks(&spec8, &ff, c1.meta.base_level.edge, count);

    // Meander each trunk. Wavelength and phase are drawn per trunk from the
    // course identity so the result is deterministic and varies by seed.
    let lam0 = envf("LAM", 0.0);
    // Per-archetype corpus sinuosity at a 600 m window; TARGET overrides.
    let target = envf(
        "TARGET",
        match key {
            "piedmont" => 1.101,
            "hill_country" => 1.062,
            "river_valley" => 1.066,
            "great_plains" => 1.078,
            _ => 1.08,
        },
    );
    let mut ct: Vec<(f64, Vec<Vec2>)> = Vec::new();
    for (k, t) in raw.iter().enumerate() {
        let u = id.course_scalar(0x51_4E_00 ^ (k as u64));
        let (lo, hi) = construct::MEANDER_LAM_M;
        let lam = if lam0 > 0.0 { lam0 } else { lo + (hi - lo) * u };
        let phase = id.course_scalar(0x51_4E_01 ^ (k as u64)) * std::f64::consts::TAU;
        ct.push((t.area_m2, construct::meander_to(&t.pts, lam, target, phase)));
    }

    // For comparison: the shipped stage's own biggest systems.
    let sk = course_skeleton::generate(&spec, &c1, &id);
    let mut root_of = vec![0u32; sk.channels.len()];
    for ci in 0..sk.channels.len() {
        let mut r = ci as u32;
        while let Some(par) = sk.channels[r as usize].parent {
            r = par;
        }
        root_of[ci] = r;
    }
    let mut roots: Vec<u32> = root_of.clone();
    roots.sort_unstable();
    roots.dedup();
    let mut ranked: Vec<(f64, u32)> = roots
        .iter()
        .map(|&r| (sk.channels[r as usize].area_m2, r))
        .collect();
    ranked.sort_by(|a, b| b.0.total_cmp(&a.0));
    let st: Vec<(f64, Vec<Vec2>)> = ranked
        .iter()
        .take(count)
        .map(|&(a, r)| (a, sk.channels[r as usize].pts.clone()))
        .collect();

    let dump = |name: &str, data: &[f64]| {
        let mut buf = Vec::with_capacity(data.len() * 4);
        for v in data {
            buf.extend_from_slice(&(*v as f32).to_le_bytes());
        }
        std::fs::write(format!("{out}/{key}_{seed}_{name}.f32"), &buf).unwrap();
    };
    dump("macro", &implied.data);
    dump("s2", &sk.height.data);
    write_lines(format!("{out}/{key}_{seed}_ctrunk.txt"), &ct);
    write_lines(format!("{out}/{key}_{seed}_strunk.txt"), &st);
    std::fs::write(
        format!("{out}/{key}_{seed}_meta.txt"),
        format!(
            "{} {} {} {} {}\n",
            spec8.nx, spec8.cell_size, sk.height.spec.nx, sk.height.spec.cell_size, ct.len()
        ),
    )
    .unwrap();

    let sin = |pts: &[Vec2]| {
        construct::window_sinuosity(pts, 600.0)
            .map(|v| format!("{v:.3}"))
            .unwrap_or_else(|| "—".into())
    };
    eprintln!(
        "{key} {seed}: trunks={} (of {count} asked)  km={:.1}  sinuosity raw={} meandered={}  \
         shipped={}",
        ct.len(),
        ct.iter()
            .map(|(_, p)| course_skeleton::fluvial::carve::arc_len(p))
            .sum::<f64>()
            / 1000.0,
        raw.first().map(|t| sin(&t.pts)).unwrap_or_else(|| "—".into()),
        ct.first().map(|(_, p)| sin(p)).unwrap_or_else(|| "—".into()),
        st.first().map(|(_, p)| sin(p)).unwrap_or_else(|| "—".into()),
    );
}
