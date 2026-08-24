//! Dump C1+C2 fluvial skeletons for the network viewer round: the datum
//! cgrid plus the channel polylines and battery stats as JSON.
use course_sandhills::{build_fluvial_skeleton, channel};
use course_world::gridio;

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let out = std::path::Path::new(&a[0]);
    std::fs::create_dir_all(out).unwrap();
    for s in &a[1..] {
        let seed: u64 = s.parse().unwrap();
        let id = course_seed::RunIdentity::from_seed(seed);
        let (d, datum, net) = build_fluvial_skeleton(&id);
        gridio::write_grid_f32(&out.join(format!("net_{seed}.cgrid")), &datum).unwrap();
        let st = channel::stats(&net);
        let mut j = String::new();
        j.push_str(&format!(
            "{{\"seed\":{seed},\"stats\":{{\"density\":{:.3},\"d2c\":{:.1},\"junc_p50\":{:.1},\
             \"junc_gt80\":{:.3},\"main_share\":{:.3},\"n_sys\":{},\"n_chans\":{},\
             \"crossings\":{},\"cap_relief\":{:.1},\"cap_flat\":{:.2},\"n_sys_drawn\":{}}},\"chans\":[",
            st.density_km_km2, st.d2c_p50_m, st.junc_p50_deg, st.junc_gt80_frac,
            st.main_share, st.n_sys, st.n_chans, st.crossings,
            d.cap_relief_m, d.cap_flat, d.n_sys));
        for (i, c) in net.chans.iter().enumerate() {
            if i > 0 { j.push(','); }
            j.push_str(&format!("{{\"tier\":{},\"sys\":{},\"pts\":[", c.tier, c.sys));
            for (k, p) in c.pts.iter().enumerate() {
                if k > 0 { j.push(','); }
                j.push_str(&format!("[{:.0},{:.0}]", p.x, p.y));
            }
            j.push_str("]}");
        }
        j.push_str("]}");
        std::fs::write(out.join(format!("net_{seed}.json")), j).unwrap();
        println!("seed {seed}: {} chans, dens {:.2}, d2c {:.0}, X {}",
                 st.n_chans, st.density_km_km2, st.d2c_p50_m, st.crossings);
    }
}
