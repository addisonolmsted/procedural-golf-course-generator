//! Debug view for the parallel-run defect: draw the network, highlight in
//! red every 30 m sample whose nearest other-channel distance sits in the
//! 40–200 m band as part of a >400 m run. Usage: planform_svg <seed> <biome>.
use course_contracts::biome::BiomeId;
use course_seed::RunIdentity;
use course_spec::v2::{SiteSpec, SpecOverridesV2};
use course_world::math::Vec2;

fn resample(pts: &[Vec2], step: f64) -> Vec<Vec2> {
    let mut out = Vec::new();
    if pts.len() < 2 {
        return out;
    }
    out.push(pts[0]);
    let mut need = step;
    for w in pts.windows(2) {
        let (a, b) = (w[0], w[1]);
        let seg = ((b.x - a.x).powi(2) + (b.y - a.y).powi(2)).sqrt();
        let mut t0 = 0.0;
        while need <= seg - t0 {
            let t = t0 + need;
            out.push(Vec2 { x: a.x + (b.x - a.x) * t / seg, y: a.y + (b.y - a.y) * t / seg });
            t0 = t;
            need = step;
        }
        need -= seg - t0;
    }
    out
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let seed: u64 = args.get(1).map(|s| s.parse().unwrap()).unwrap_or(41_000);
    let bkey = args.get(2).cloned().unwrap_or_else(|| "piedmont".into());
    let biome = *BiomeId::ALL.iter().find(|b| b.key() == bkey).expect("biome key");
    let id = RunIdentity::from_seed(seed);
    let spec = SiteSpec::generate_builtin(id, &SpecOverridesV2 { forced_biome: Some(biome) });
    let c1 = course_primitives::generate(&spec, &id);
    let sk = course_skeleton::generate(&spec, &c1, &id);
    let sampled: Vec<Vec<Vec2>> = sk.channels.iter().map(|c| resample(&c.pts, 30.0)).collect();
    let scale = 0.25; // 3000 m -> 750 px
    let mut svg = format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='750' height='750' viewBox='0 0 750 750'>\
         <rect width='750' height='750' fill='#f8f6f0'/>"
    );
    for (ci, ch) in sk.channels.iter().enumerate() {
        let w = match ch.order {
            1 => 0.8,
            2 => 1.6,
            _ => 2.6,
        };
        let d: Vec<String> = ch
            .pts
            .iter()
            .enumerate()
            .map(|(i, p)| {
                format!("{}{:.1},{:.1}", if i == 0 { "M" } else { "L" }, p.x * scale, 750.0 - p.y * scale)
            })
            .collect();
        svg += &format!(
            "<path d='{}' fill='none' stroke='#2b5c8a' stroke-width='{}'/>",
            d.join(" "),
            w
        );
        let _ = ci;
    }
    // overlay: parallel-run samples in red
    for (i, si) in sampled.iter().enumerate() {
        let mut flags = vec![false; si.len()];
        for (pi, p) in si.iter().enumerate() {
            let mut dmin = f64::INFINITY;
            for (j, sj) in sampled.iter().enumerate() {
                if j == i {
                    continue;
                }
                let related = sk.channels[i].parent == Some(j as u32)
                    || sk.channels[j].parent == Some(i as u32);
                for q in sj {
                    let d = ((q.x - p.x).powi(2) + (q.y - p.y).powi(2)).sqrt();
                    if d < dmin {
                        dmin = d;
                    }
                }
            }
            flags[pi] = (40.0..=200.0).contains(&dmin);
        }
        // keep only runs > 400 m (>13 consecutive samples)
        let mut k = 0;
        while k < flags.len() {
            if flags[k] {
                let s0 = k;
                while k < flags.len() && flags[k] {
                    k += 1;
                }
                if ((k - s0) as f64) * 30.0 > 400.0 {
                    let d: Vec<String> = si[s0..k]
                        .iter()
                        .enumerate()
                        .map(|(m, p)| {
                            format!(
                                "{}{:.1},{:.1}",
                                if m == 0 { "M" } else { "L" },
                                p.x * scale,
                                750.0 - p.y * scale
                            )
                        })
                        .collect();
                    svg += &format!(
                        "<path d='{}' fill='none' stroke='#cc2222' stroke-width='2.2' opacity='0.75'/>",
                        d.join(" ")
                    );
                }
            } else {
                k += 1;
            }
        }
    }
    // channel index labels at midpoints + a stderr table
    for (ci, ch) in sk.channels.iter().enumerate() {
        let mid = ch.pts[ch.pts.len() / 2];
        svg += &format!(
            "<text x='{:.0}' y='{:.0}' font-size='9' fill='#333'>{}</text>",
            mid.x * scale,
            750.0 - mid.y * scale,
            ci
        );
        let maxseg = ch
            .pts
            .windows(2)
            .map(|w| ((w[1].x - w[0].x).powi(2) + (w[1].y - w[0].y).powi(2)).sqrt())
            .fold(0.0f64, f64::max);
        eprintln!(
            "ch {:3}: order {} parent {:?} len {:.0} pts {} maxseg {:.0} start ({:.0},{:.0}) end ({:.0},{:.0})",
            ci,
            ch.order,
            ch.parent,
            course_skeleton::fluvial::trunk::arc_len(&ch.pts),
            ch.pts.len(),
            maxseg,
            ch.pts[0].x,
            ch.pts[0].y,
            ch.pts.last().unwrap().x,
            ch.pts.last().unwrap().y
        );
    }
    svg += "</svg>";
    let out = format!("/tmp/planform_{}_{}.svg", bkey, seed);
    std::fs::write(&out, svg).unwrap();
    eprintln!("wrote {}", out);
}
