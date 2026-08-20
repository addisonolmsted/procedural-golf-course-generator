// Shared metric kernels, included by net_battery and dial_sweep so both
// measure the same way.
const RES: f64 = 8.0;

fn nearest_field(pts: &[Vec2]) -> Vec<f64> {
    let bs = 200.0;
    let bn = (EXTENT_M / bs).ceil() as usize + 2;
    let mut b: Vec<Vec<u32>> = vec![Vec::new(); bn * bn];
    let key = |p: Vec2| (
        ((p.x / bs).floor().max(0.0) as usize).min(bn - 1),
        ((p.y / bs).floor().max(0.0) as usize).min(bn - 1),
    );
    for (i, p) in pts.iter().enumerate() { let (x, y) = key(*p); b[y * bn + x].push(i as u32); }
    let n = (EXTENT_M / RES).round() as usize + 1;
    let mut out = Vec::with_capacity(n * n);
    for gy in 0..n {
        for gx in 0..n {
            let p = Vec2::new(gx as f64 * RES, gy as f64 * RES);
            let (kx, ky) = key(p);
            let mut best = f64::MAX;
            let mut r = 1i64;
            while r <= bn as i64 {
                for oy in -r..=r { for ox in -r..=r {
                    if r > 1 && ox.abs() != r && oy.abs() != r { continue; }
                    let (x, y) = (kx as i64 + ox, ky as i64 + oy);
                    if x < 0 || y < 0 || x >= bn as i64 || y >= bn as i64 { continue; }
                    for &id in &b[y as usize * bn + x as usize] {
                        let dd = p.distance(pts[id as usize]);
                        if dd < best { best = dd; }
                    }
                }}
                if best <= (r as f64) * bs { break; }
                r += 1;
            }
            out.push(best);
        }
    }
    out
}

pub struct Net { pub nodes: Vec<course_network::Node>, pub reaches: Vec<course_network::Reach>, pub omega: u32 }
impl Net {
    fn channel_len_m(&self) -> f64 {
        self.reaches.iter().map(|r| r.pts.windows(2).map(|w| w[0].distance(w[1])).sum::<f64>()).sum()
    }
    fn channel_nodes(&self) -> impl Iterator<Item = &course_network::Node> {
        self.nodes.iter().filter(|n| n.area_m2 >= CHANNEL_AREA_M2)
    }
}

#[allow(dead_code)]
fn d2c_p50(net: &Net) -> f64 {
    let pts: Vec<Vec2> = net.channel_nodes().map(|n| n.p).collect();
    if pts.is_empty() { return f64::NAN; }
    let mut d = nearest_field(&pts);
    d.sort_by(|a, x| a.partial_cmp(x).unwrap());
    d[d.len() / 2]
}

fn horton(net: &Net) -> (f64, f64, u32) {
    let omax = net.omega as usize;
    if omax < 2 { return (f64::NAN, f64::NAN, net.omega); }
    let mut heads = vec![0f64; omax + 1];
    let mut length = vec![0f64; omax + 1];
    for r in &net.reaches {
        let o = r.order as usize;
        if o == 0 || o > omax { continue; }
        heads[o] += 1.0;
        length[o] += r.pts.windows(2).map(|w| w[0].distance(w[1])).sum::<f64>();
    }
    let fit = |y: &[f64]| -> f64 {
        let pts: Vec<(f64, f64)> = (1..=omax).filter(|w| y[*w] > 0.0).map(|w| (w as f64, y[w].ln())).collect();
        if pts.len() < 2 { return f64::NAN; }
        let n = pts.len() as f64;
        let sx: f64 = pts.iter().map(|p| p.0).sum();
        let sy: f64 = pts.iter().map(|p| p.1).sum();
        let sxy: f64 = pts.iter().map(|p| p.0 * p.1).sum();
        let sxx: f64 = pts.iter().map(|p| p.0 * p.0).sum();
        (n * sxy - sx * sy) / (n * sxx - sx * sx)
    };
    let rb = (-fit(&heads)).exp();
    let mean_len: Vec<f64> = (0..=omax).map(|w| if heads[w] > 0.0 { length[w] / heads[w] } else { 0.0 }).collect();
    let rl = fit(&mean_len).exp();
    (rb, rl, net.omega)
}

/// Confluence angle in the CORPUS convention (`junction_real.py`): between the
/// smaller donor's INCOMING direction and the CONTINUATION of the larger one.
///
/// Measuring the angle between the two donors' upstream vectors instead gives
/// roughly 180 minus this, which is why the first run reported 132-141 deg
/// against a 37-45 deg band and flagged 91-100% of junctions as orthogonal.
fn junctions(net: &Net) -> Vec<f64> {
    let n = net.nodes.len();
    let is_ch: Vec<bool> = net.nodes.iter().map(|x| x.area_m2 >= CHANNEL_AREA_M2).collect();
    let mut kids: Vec<Vec<u32>> = vec![Vec::new(); n];
    for i in 0..n {
        if !is_ch[i] { continue; }
        if let Some(p) = net.nodes[i].parent { if is_ch[p as usize] { kids[p as usize].push(i as u32); } }
    }
    let back = |i0: usize, k: usize| -> Vec2 {
        let start = net.nodes[i0].p;
        let mut i = i0;
        for _ in 0..k {
            let Some(&b) = kids[i].iter().max_by(|a, b| {
                net.nodes[**a as usize].area_m2.partial_cmp(&net.nodes[**b as usize].area_m2).unwrap()
            }) else { break };
            i = b as usize;
        }
        Vec2::new(net.nodes[i].p.x - start.x, net.nodes[i].p.y - start.y)
    };
    // Downstream continuation: walk parents from the junction.
    let fwd = |i0: usize, k: usize| -> Vec2 {
        let start = net.nodes[i0].p;
        let mut i = i0;
        for _ in 0..k {
            let Some(par) = net.nodes[i].parent else { break };
            if !is_ch[par as usize] { break }
            i = par as usize;
        }
        Vec2::new(net.nodes[i].p.x - start.x, net.nodes[i].p.y - start.y)
    };
    let mut out = Vec::new();
    for i in 0..n {
        if kids[i].len() < 2 { continue; }
        let mut ks = kids[i].clone();
        ks.sort_by(|a, b| net.nodes[*b as usize].area_m2.partial_cmp(&net.nodes[*a as usize].area_m2).unwrap());
        let small_up = back(ks[1] as usize, 5);
        let down = fwd(i, 5);
        if small_up.length() < 1e-6 || down.length() < 1e-6 { continue; }
        // incoming direction of the small donor = toward the junction
        let inc = Vec2::new(-small_up.x, -small_up.y).normalized();
        let c = inc.dot(down.normalized()).clamp(-1.0, 1.0);
        out.push(course_world::math::acos(c).to_degrees());
    }
    out
}

