//! Verify heathland pock fields fire: diff S3 with amplify.pock_field
//! on vs off, report stamped area and cluster bboxes.
use course_contracts::biome::BiomeId;
use course_seed::RunIdentity;
use course_spec::v2::{SiteSpec, SpecOverridesV2};

fn main() {
    let dict = ["assets/dictionary_v2.bin", "../../assets/dictionary_v2.bin"]
        .iter()
        .find_map(|p| course_amplify::dictionary::Dictionary::load(std::path::Path::new(p)).ok())
        .expect("dictionary");
    for seed in [1u64, 7, 13] {
        let id = RunIdentity::from_seed(seed);
        let spec = SiteSpec::generate_builtin(
            id,
            &SpecOverridesV2 { forced_biome: Some(BiomeId::Heathland) },
        );
        let c1 = course_primitives::generate(&spec, &id);
        let sk = course_skeleton::generate(&spec, &c1, &id);
        let on = course_amplify::generate(&spec, &sk, &dict, &id);
        let mut spec_off = spec.clone();
        spec_off.dials.insert("amplify.pock_field".into(), 0.0);
        let off = course_amplify::generate(&spec_off, &sk, &dict, &id);
        let h_on = course_transforms::hydrology::generate(
            &spec,
            &sk,
            &on,
            &id,
            &course_transforms::hydrology::DEFAULT_TRANSFORMS,
        );
        let h_off = course_transforms::hydrology::generate(
            &spec_off,
            &sk,
            &off,
            &id,
            &course_transforms::hydrology::DEFAULT_TRANSFORMS,
        );
        let n2 = on.height.spec.nx as usize;
        let cell = on.height.spec.cell_size;
        let stats = |a: &[f64], b: &[f64]| {
            let mut cells = 0usize;
            let mut deepest = 0.0f64;
            let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0usize, 0usize);
            for i in 0..n2 * n2 {
                let d = b[i] - a[i];
                if d > 0.05 {
                    cells += 1;
                    deepest = deepest.max(d);
                    let (x, y) = (i % n2, i / n2);
                    x0 = x0.min(x);
                    y0 = y0.min(y);
                    x1 = x1.max(x);
                    y1 = y1.max(y);
                }
            }
            (cells, deepest, x0, y0, x1, y1)
        };
        let (c3, d3, ax0, ay0, ax1, ay1) = stats(&on.height.data, &off.height.data);
        let (c4, d4, ..) = stats(&h_on.height.data, &h_off.height.data);
        println!(
            "seed {seed:2}: S3 pocked {c3} cells deepest {d3:.2} m bbox ({:.0},{:.0})-({:.0},{:.0}) | S4 SURVIVING {c4} cells deepest {d4:.2} m",
            ax0 as f64 * cell,
            ay0 as f64 * cell,
            ax1 as f64 * cell,
            ay1 as f64 * cell
        );
    }
}
