//! Dump S2 channel polylines (world m + discharge) for overlay drawing.
use course_contracts::biome::BiomeId;
use course_seed::RunIdentity;
use course_spec::v2::{SiteSpec, SpecOverridesV2};

fn main() {
    let out = std::path::PathBuf::from(std::env::args().nth(1).expect("outdir"));
    for (biome, tag, seed) in
        [(BiomeId::HillCountry, "hc", 2u64), (BiomeId::Piedmont, "pied", 3)]
    {
        let id = RunIdentity::from_seed(seed);
        let spec =
            SiteSpec::generate_builtin(id, &SpecOverridesV2 { forced_biome: Some(biome) });
        let c1 = course_primitives::generate(&spec, &id);
        let sk = course_skeleton::generate(&spec, &c1, &id);
        let mut csv = String::new();
        for ch in &sk.channels {
            let pts: Vec<String> =
                ch.pts.iter().map(|p| format!("{:.1}:{:.1}", p.x, p.y)).collect();
            csv.push_str(&format!("{:.0},{}\n", ch.area_m2, pts.join(";")));
        }
        std::fs::write(out.join(format!("{tag}_channels.csv")), csv).unwrap();
        eprintln!("{tag}: {} channels", sk.channels.len());
    }
}
