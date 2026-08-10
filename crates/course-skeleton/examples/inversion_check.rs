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
    let h8 = |lin: usize| {
        let (y, x) = (lin / nx, lin % nx);
        *sk.height.get((x * 4) as u32, (y * 4) as u32)
    };
    let (mut ch_sum, mut ch_n) = (0.0, 0);
    let (mut far_sum, mut far_n) = (0.0, 0);
    for lin in 0..nx * ny {
        let d = sk.flow_distance.data[lin];
        if d == 0.0 {
            ch_sum += h8(lin);
            ch_n += 1;
        } else if d > 150.0 {
            far_sum += h8(lin);
            far_n += 1;
        }
    }
    println!("mean height AT channels:      {:8.2} m  (n={ch_n})", ch_sum / ch_n as f64);
    println!("mean height FAR from channels {:8.2} m  (n={far_n})", far_sum / far_n as f64);
    println!("base edge: {:?} at {:.1} m", sk.meta.base_level.edge, sk.meta.base_level.elev_m);
    // quadrant means (world coords: y up, S = y small)
    let q = |x0: usize, x1: usize, y0: usize, y1: usize| {
        let mut s = 0.0;
        let mut n = 0;
        for y in y0..y1 {
            for x in x0..x1 {
                s += h8(y * nx + x);
                n += 1;
            }
        }
        s / n as f64
    };
    let h = ny / 2;
    println!("quadrant means: SW {:.1}  SE {:.1}  NW {:.1}  NE {:.1}",
        q(0, nx/2, 0, h), q(nx/2, nx, 0, h), q(0, nx/2, h, ny), q(nx/2, nx, h, ny));
    // local cross-check: for 500 channel cells, count how many are LOWER
    // than the mean of their 8 neighbours
    let mut lower = 0;
    let mut tot = 0;
    for lin in 0..nx * ny {
        if sk.flow_distance.data[lin] != 0.0 { continue; }
        let (y, x) = ((lin / nx) as i64, (lin % nx) as i64);
        if y == 0 || x == 0 || y == ny as i64 - 1 || x == nx as i64 - 1 { continue; }
        let mut nb = 0.0;
        for dy in -1i64..=1 {
            for dx in -1i64..=1 {
                if dy == 0 && dx == 0 { continue; }
                nb += h8(((y + dy) as usize) * nx + (x + dx) as usize);
            }
        }
        if h8(lin as usize) < nb / 8.0 { lower += 1; }
        tot += 1;
        if tot >= 2000 { break; }
    }
    println!("channel cells lower than neighbour mean: {lower}/{tot}");
    // hillslope anatomy: mean height ABOVE the nearest channel's base, by
    // distance band — must rise monotonically if the catena is right.
    for (lo, hi) in [(8.0, 40.0), (40.0, 100.0), (100.0, 180.0), (180.0, 280.0)] {
        let mut s = 0.0;
        let mut n = 0;
        for lin in 0..nx * ny {
            let d = sk.flow_distance.data[lin];
            if d < lo || d >= hi { continue; }
            // z_ch of nearest channel = height at that channel... approximate
            // via hillslope_position denominator not exported; use local:
            s += h8(lin);
            n += 1;
        }
        let _ = s / n.max(1) as f64;
        let mut rel = 0.0;
        let mut rn = 0;
        for lin in 0..nx * ny {
            let d = sk.flow_distance.data[lin];
            if d < lo || d >= hi { continue; }
            // reconstruct z above channel via fdn*hp fields? use hp * denom unknown;
            // instead compare against the minimum height within 300 m is costly.
            // Simple proxy: hillslope_position mean in band (0=channel,1=divide).
            rel += sk.hillslope_position.data[lin];
            rn += 1;
        }
        println!("band {lo:.0}-{hi:.0} m: n={n} mean hillslope_position {:.2}", rel / rn.max(1) as f64);
    }
}
