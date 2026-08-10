use course_seed::RunIdentity;
use course_spec::v2::{SiteSpec, SpecOverridesV2};
use stage_lab::render_s2::{render_s2, S2View};
fn main() {
    for arg in std::env::args().skip(1) {
        let seed: u64 = arg.parse().unwrap();
        let id = RunIdentity::from_seed(seed);
        let spec = SiteSpec::generate_builtin(id, &SpecOverridesV2::default());
        let c1 = course_primitives::generate(&spec, &id);
        let sk = course_skeleton::generate(&spec, &c1, &id);
        let img = render_s2(&sk, S2View::Base, 900, true);
        img.save(format!("/tmp/check_{seed}.png")).unwrap();
        println!("{seed}: {}", spec.biome.key());
    }
}
