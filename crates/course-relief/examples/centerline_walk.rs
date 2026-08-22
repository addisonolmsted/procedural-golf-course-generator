//! Walk each trunk centerline and report every surface-z jump — the user's
//! diagnostic prescription for the convergent-trunk discontinuity.
use course_draw::Archetype;
use course_seed::RunIdentity;

fn main() {
    for (a, seed) in [
        (Archetype::GreatPlains, 37u64),
        (Archetype::HillCountry, 58),
        (Archetype::Piedmont, 2),
        (Archetype::HillCountry, 2),
    ] {
        let id = RunIdentity::from_seed(seed);
        let (_, trunks, _, ms, _) = course_relief::build_macro(&id, Some(a));
        println!("== {a} seed {seed}: {} trunks", trunks.len());
        for (ti, tk) in trunks.iter().enumerate() {
            let mut worst = (0.0f64, 0usize);
            let mut prev = ms.height.bilinear(tk.pts[0]);
            for (i, p) in tk.pts.iter().enumerate().skip(1) {
                let z = ms.height.bilinear(*p);
                let dz = (z - prev).abs();
                if dz > worst.0 {
                    worst = (dz, i);
                }
                prev = z;
            }
            let i = worst.1;
            println!(
                "  trunk {ti} (joins {:?}): worst step {:.2} m per 20 m at pt {i}/{} ({:.0},{:.0})",
                tk.joins, worst.0, tk.pts.len(), tk.pts[i].x, tk.pts[i].y
            );
            // print the local profile around the worst step
            let lo = i.saturating_sub(4);
            let hi = (i + 4).min(tk.pts.len() - 1);
            let prof: Vec<String> = (lo..=hi)
                .map(|j| format!("{:.2}", ms.height.bilinear(tk.pts[j])))
                .collect();
            println!("    profile: {}", prof.join(" "));
        }
    }
}
