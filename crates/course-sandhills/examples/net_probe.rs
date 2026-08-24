//! Diagnose crossing pairs in the C2 network.
use course_sandhills::{build_fluvial_skeleton, channel};
fn main() {
    for s in std::env::args().skip(1) {
        let seed: u64 = s.parse().unwrap();
        let id = course_seed::RunIdentity::from_seed(seed);
        let (_, _, net) = build_fluvial_skeleton(&id);
        let st = channel::stats(&net);
        println!("seed {seed}: chans {} dens {:.2} d2c {:.0} junc {:.1}/{:.2} share {:.2} sys {} X {}",
            st.n_chans, st.density_km_km2, st.d2c_p50_m, st.junc_p50_deg,
            st.junc_gt80_frac, st.main_share, st.n_sys, st.crossings);
        // who crosses whom
        for (i, c) in net.chans.iter().enumerate() {
            for (j, o) in net.chans.iter().enumerate() {
                if i == j { continue; }
                for p in &c.pts {
                    let mind = o.pts.iter().map(|q| q.distance(*p)).fold(f64::MAX, f64::min);
                    let mouth_d = p.distance(c.pts[0]);
                    if mind < 10.0 && mouth_d > 60.0 && o.pts.iter().all(|q| q.distance(c.pts[0]) > 1e-9 || true) {
                        let rel = if c.parent == Some(j as u32) { "PARENT" }
                            else if o.parent == Some(i as u32) { "CHILD" } else { "other" };
                        println!("  chan {i}(t{} p{:?}) within {mind:.1} of {j}(t{}) rel {rel} at ({:.0},{:.0}) mouth_d {:.0}",
                            c.tier, c.parent, o.tier, p.x, p.y, mouth_d);
                        break;
                    }
                }
            }
        }
    }
}
