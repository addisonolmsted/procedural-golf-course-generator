//! A5/A6 stage ablation: the same seed with each stage toggled, so a diff
//! against the base shows exactly what one stage adds. Discipline rule 3.
use course_sandhills::{blowout, draw, record::FormClass, rng, surface, texture, water, wind, Mode};
use course_seed::RunIdentity;
use course_world::grid::Grid;
use course_world::gridio;

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let out = std::path::Path::new(&a[0]);
    std::fs::create_dir_all(out).unwrap();
    let pack = texture::PatchPack::load(std::path::Path::new(
        "assets/sandhills_patches_aeolian.bin")).expect("pack");
    let seed: u64 = a[1].parse().unwrap();
    let id = RunIdentity::from_seed(seed);
    let d = draw::site(&id, Some(Mode::Aeolian), Some(FormClass::Train));
    println!("seed {seed}: table {:.1} m, river {}, blowout/km2 {:.1}",
             d.water_table_m, d.allogenic_river, d.blowout_km2);

    let mut wr = rng::stream(&id, rng::WIND);
    let w = wind::build(&mut wr, d.wind_rad, d.wavelength_m, d.wind_wander_rad,
                        d.wind_wander_m, d.kappa, 0.0);
    let mut hr = rng::stream(&id, rng::HUMMOCK);
    let hw = wind::build(&mut hr, d.wind_rad, d.hummock_lambda_m, d.wind_wander_rad,
                         d.wind_wander_m * 0.45, d.hummock_kappa, d.hummock_spread);
    let mut br = rng::stream(&id, rng::PATCHY);
    let sf = surface::build(&mut br, &w, &hw, &d);
    let spec8 = sf.height.spec;
    let tnorm8 = {
        let mut sorted: Vec<f64> = sf.height.data.clone();
        sorted.sort_by(|x, y| x.partial_cmp(y).unwrap());
        let n = sorted.len();
        let mut g = Grid::filled(spec8, 0.0f64);
        for i in 0..n {
            g.data[i] = sorted.partition_point(|v| *v < sf.height.data[i]) as f64 / n as f64;
        }
        g
    };

    // BASE: texture on the un-panned macro (A5a off)
    let mut tr = rng::stream(&id, rng::TEXTURE);
    let base = texture::quilt(&mut tr, &pack, &sf.height, &d);
    gridio::write_grid_f32(&out.join("0_base.cgrid"), &base).unwrap();

    // +PANS: pans applied at 8 m, then the same texture stream
    let mut h8 = Grid { spec: sf.height.spec, data: sf.height.data.clone() };
    let mut pr = rng::stream(&id, rng::BLOWOUT);
    let _ = blowout::pans(&mut pr, &mut h8, &tnorm8.data, &d);
    let mut tr2 = rng::stream(&id, rng::TEXTURE);
    let panned = texture::quilt(&mut tr2, &pack, &h8, &d);
    gridio::write_grid_f32(&out.join("1_pans.cgrid"), &panned).unwrap();

    // +BLOWOUTS on top of that (continuing the same BLOWOUT stream)
    let mut with_b = Grid { spec: panned.spec, data: panned.data.clone() };
    let bl = blowout::carve(&mut pr, &mut with_b, &tnorm8, &d, None);
    gridio::write_grid_f32(&out.join("2_blowouts.cgrid"), &with_b).unwrap();
    let s: String = bl.iter().map(|b| format!("{:.1} {:.1} {:.1} {:.1}\n",
        b.center.x, b.center.y, b.radius_m, b.depth_m)).collect();
    std::fs::write(out.join("blowouts.txt"), s).unwrap();

    // +WATER (lakes only -- the river is removed pending redesign)
    let full = with_b;
    let wat = water::find(&full, &sf.datum, &d, None);
    gridio::write_grid_f32(&out.join("3_full.cgrid"), &full).unwrap();
    gridio::write_grid_f32(&out.join("3_water.cgrid"), &wat.surface).unwrap();
    println!("lake_frac {:.3}, {} blowouts", wat.lake_frac, bl.len());
}
