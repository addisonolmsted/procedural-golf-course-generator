use course_sandhills::{draw, rng, water, Mode};
use course_seed::RunIdentity;
fn main() {
    for s in 31..400u64 {
        let id = RunIdentity::from_seed(s);
        let d = draw::site(&id, Some(Mode::Aeolian), None);
        if !d.allogenic_river { continue; }
        // replicate: pick + plan draws up to the incised coin is heavy;
        // just build the plan for real and read it
        let mut r = rng::stream(&id, rng::WIND);
        let w = course_sandhills::wind::build(&mut r, d.wind_rad, d.wavelength_m,
            d.wind_wander_rad, d.wind_wander_m, d.kappa, 0.0);
        let _ = &w;
        let mut rr = rng::stream(&id, rng::WATER);
        let pick = water::RiverStyle::PASSED[rr.below(water::RiverStyle::PASSED.len())];
        // a coarse grid is enough for the plan's bilinear reads
        let g = course_world::grid::Grid::filled(
            course_world::grid::GridSpec::new(course_world::math::Vec2::ZERO, 8.0, 376, 376), 10.0);
        if let Some(pl) = water::plan_river(&mut rr, &g, &d, pick) {
            if pl.incised { println!("{s}"); }
        }
    }
}
