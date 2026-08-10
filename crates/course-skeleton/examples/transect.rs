use course_contracts::biome::BiomeId;
use course_seed::RunIdentity;
use course_spec::v2::{SiteSpec, SpecOverridesV2};

fn main() {
    let id = RunIdentity::from_seed(301);
    let spec = SiteSpec::generate_builtin(
        id,
        &SpecOverridesV2 { forced_biome: Some(BiomeId::Piedmont) },
    );
    let c1 = course_primitives::generate(&spec, &id);
    let sk = course_skeleton::generate(&spec, &c1, &id);
    let spec8 = sk.flow_distance.spec;
    let (nx, ny) = (spec8.nx as usize, spec8.ny as usize);
    // implied
    let mut implied = c1.tilt.clone();
    for (i, v) in implied.data.iter_mut().enumerate() {
        *v += c1.relief.data[i];
    }
    let h8 = |lin: usize| {
        let (y, x) = (lin / nx, lin % nx);
        *sk.height.get((x * 4) as u32, (y * 4) as u32)
    };
    // find a mid-tile channel cell
    let mut found = None;
    for y in (ny / 3)..(2 * ny / 3) {
        for x in (nx / 3)..(2 * nx / 3) {
            if sk.flow_distance.data[y * nx + x] == 0.0 {
                found = Some((y, x));
                break;
            }
        }
        if found.is_some() { break; }
    }
    let (cy, cx) = found.expect("channel in mid-tile");
    println!("channel cell at grid ({cx},{cy}) world ({:.0},{:.0})", cx as f64 * 8.0, cy as f64 * 8.0);
    println!("{:>5} {:>9} {:>9} {:>9} {:>7}", "dx(m)", "height", "implied", "d2c(m)", "hp");
    for dx in -12i64..=12 {
        let x = (cx as i64 + dx) as usize;
        let lin = cy * nx + x;
        println!(
            "{:>5} {:>9.2} {:>9.2} {:>9.0} {:>7.2}",
            dx * 8,
            h8(lin),
            implied.data[lin],
            sk.flow_distance.data[lin],
            sk.hillslope_position.data[lin],
        );
    }
}
