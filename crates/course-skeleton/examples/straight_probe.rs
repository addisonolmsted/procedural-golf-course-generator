//! Locate the longest axis-aligned channel run and dump its neighbourhood.
use course_contracts::biome::BiomeId;
use course_seed::{streams, RunIdentity};
use course_skeleton::fluvial::carve::{self, CarveParams};
use course_spec::v2::{SiteSpec, SpecOverridesV2};

fn main() {
    let id = RunIdentity::from_seed(std::env::var("SEED").ok().and_then(|v| v.parse().ok()).unwrap_or(48));
    let bkey = std::env::var("BIOME").unwrap_or_else(|_| "piedmont".into());
    let biome = *BiomeId::ALL.iter().find(|b| b.key() == bkey).expect("biome");
    let spec = SiteSpec::generate_builtin(id, &SpecOverridesV2 { forced_biome: Some(biome) });
    let c1 = course_primitives::generate(&spec, &id);
    let spec8 = c1.grid;
    let (nx, ny) = (spec8.nx as usize, spec8.ny as usize);
    let n = nx * ny;
    let mut implied = c1.tilt.clone();
    for (i, v) in implied.data.iter_mut().enumerate() {
        *v += c1.relief.data[i];
    }
    let relief_amp = spec.dials.get("primitives.relief_amp_m").copied().unwrap_or(8.0);
    let erod: Vec<f64> = c1.hardness.data.iter().map(|h| (1.6 - h).clamp(0.3, 1.6)).collect();
    let keep = vec![false; n];
    let envf = |k: &str, d: f64| -> f64 {
        std::env::var(k).ok().and_then(|v| v.parse().ok()).unwrap_or(d)
    };
    let p = CarveParams {
        roughness_frac: envf("ROUGH", 0.05),
        k: envf("K", 0.9),
        area_threshold_m2: envf("THRESH", carve::AREA_THRESHOLD_M2),
        incision_scale: 1.0,
        base_drop_m: envf("BASEDROP", 4.0),
        inflow_area_m2: envf("INFLOW", 2.5e6),
            close_borders: std::env::var("OPEN").is_err(),
            derangement: ((0.5 - spec.dials.get("skeleton.integration").copied().unwrap_or(0.5)) * 1.6).clamp(0.0, 0.9),
        iters: envf("ITERS", 15.0) as usize,
        step_clamp_m: envf("CLAMP", 0.45),
        tributary_reach: 1.0,
        creep: envf("CREEP", 0.0),
        route_wander: 0.0,
        cut_spread_m: 0.0,
        inflow_start_frac: 0.0,
        area_exp: 0.5,
        uplift_m: 0.0,
    };
    let mut rng = id.stream(streams::SKELETON_MODULE);
    let c = carve::carve(&spec8, &implied, &erod, &keep, c1.meta.base_level.edge, &mut rng, &p, relief_amp);

    // how much of the surface is in filled flats (pools) at the end?
    let zf = course_world::flow::fill_depressions(&c.z);
    let pooled = (0..n).filter(|&i| zf.data[i] > c.z.data[i] + 1e-6).count();
    let pooled_ch = (0..n)
        .filter(|&i| zf.data[i] > c.z.data[i] + 1e-6 && c.channel_of[i].is_some())
        .count();
    let ch_cells = (0..n).filter(|&i| c.channel_of[i].is_some()).count();
    println!(
        "pooled cells: {} / {} ({:.1}%); channel cells pooled: {} / {} ({:.1}%)",
        pooled, n, 100.0 * pooled as f64 / n as f64,
        pooled_ch, ch_cells, 100.0 * pooled_ch as f64 / ch_cells.max(1) as f64
    );

    // longest run of consecutive channel cells sharing a row (via rec chain)
    let cell = spec8.cell_size;
    let mut best_len = 0usize;
    let mut best_start = 0usize;
    for ci in 0..n {
        if c.channel_of[ci].is_none() { continue; }
        let mut cur = ci;
        let row = cur / nx;
        let mut len = 0usize;
        loop {
            let r = c.rec[cur];
            if r < 0 { break; }
            let r = r as usize;
            if r / nx != row || c.channel_of[r].is_none() { break; }
            len += 1;
            cur = r;
        }
        if len > best_len { best_len = len; best_start = ci; }
    }
    println!("longest same-row channel run: {} cells = {:.0} m", best_len, best_len as f64 * cell);
    let (sy, sx) = (best_start / nx, best_start % nx);
    println!("starts at cell ({sx},{sy}) world ({:.0},{:.0})", sx as f64 * cell, sy as f64 * cell);
    // dump elevations along the run and the two neighbouring rows
    {
        let mut cur = best_start;
        print!("xy path: ");
        for _ in 0..14 {
            print!("({},{}) ", cur % nx, cur / nx);
            let r = c.rec[cur];
            if r < 0 { break; }
            cur = r as usize;
        }
        println!();
        println!("base edge: {:?}, tilt at start {:.3}", c1.meta.base_level.edge, implied.data[best_start]);
    }
    let mut cur = best_start;
    print!("z along run: ");
    for _ in 0..best_len.min(12) {
        print!("{:.3} ", c.z.data[cur]);
        let r = c.rec[cur];
        if r < 0 { break; }
        cur = r as usize;
    }
    println!();
    let mut cur = best_start;
    print!("area along:  ");
    for _ in 0..best_len.min(12) {
        print!("{:.0} ", c.area[cur]);
        let r = c.rec[cur];
        if r < 0 { break; }
        cur = r as usize;
    }
    println!();
    // neighbours above/below the first cell
    for dy in [-1i64, 0, 1] {
        print!("row {:+}: ", dy);
        for dx in 0..8i64 {
            let yy = sy as i64 + dy;
            let xx = sx as i64 - dx;
            if yy < 0 || xx < 0 { continue; }
            print!("{:.3} ", c.z.data[yy as usize * nx + xx as usize]);
        }
        println!();
    }
}
