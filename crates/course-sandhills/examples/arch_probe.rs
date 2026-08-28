//! Archetype likelihoods: what fraction of seeds draw what?
//!
//! The dials in `record.rs` are per-coin probabilities. This reports the
//! REALISED joint distribution over N seeds, because every coin comes off one
//! DRAW stream and a nominal probability is only a claim about that stream.
//!
//!     cargo run --release -p course-sandhills --example arch_probe -- 200000
use course_sandhills::{draw, FormClass, Mode};
use course_seed::RunIdentity;

fn main() {
    let n: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(200_000);

    let (mut aeo, mut train_a, mut train_f) = (0u64, 0u64, 0u64);
    let (mut river, mut wide, mut narrow) = (0u64, 0u64, 0u64);
    let (mut river_train, mut river_mound) = (0u64, 0u64);
    let mut upland = 0u64;
    let mut regular = 0u64;
    let mut wsum = 0.0f64;

    for s in 0..n {
        let d = draw::site(&RunIdentity::from_seed(s), None, None);
        let is_aeo = d.mode == Mode::Aeolian;
        let is_train = d.form == FormClass::Train;
        if is_aeo {
            aeo += 1;
            if is_train { train_a += 1; }
            if d.regular { regular += 1; }
            if d.allogenic_river {
                river += 1;
                if d.river_w_m > 7.0 { wide += 1; } else { narrow += 1; }
                wsum += d.river_w_m;
                if is_train { river_train += 1; } else { river_mound += 1; }
            }
        } else {
            if is_train { train_f += 1; }
            if d.upland { upland += 1; }
        }
    }

    let flu = n - aeo;
    let pc = |a: u64, b: u64| if b == 0 { 0.0 } else { 100.0 * a as f64 / b as f64 };
    println!("over {n} seeds\n");
    println!("MODE");
    println!("  aeolian  (Nebraska)   {:6.2}%   n={aeo}", pc(aeo, n));
    println!("  fluvial  (Carolina)   {:6.2}%   n={flu}\n", pc(flu, n));

    println!("NEBRASKA, given aeolian");
    println!("  dune form   train      {:6.2}%", pc(train_a, aeo));
    println!("              mound      {:6.2}%", pc(aeo - train_a, aeo));
    println!("  running water          {:6.2}%   (of which:)", pc(river, aeo));
    println!("      river  9-15 m      {:6.2}% of aeolian   {:6.2}% of watered",
             pc(wide, aeo), pc(wide, river));
    println!("      creek  3-5 m       {:6.2}% of aeolian   {:6.2}% of watered",
             pc(narrow, aeo), pc(narrow, river));
    println!("  no running water       {:6.2}%   (lakes only, or dry)",
             pc(aeo - river, aeo));
    println!("  regular mound sub-type {:6.2}% of mounds", pc(regular, aeo - train_a));
    println!("  mean drawn width       {:6.2} m\n", wsum / river.max(1) as f64);

    println!("NEBRASKA, the four combinations");
    for (lbl, c) in [("train + river ", river_train), ("mound + river ", river_mound),
                     ("train, no river", train_a - river_train),
                     ("mound, no river", aeo - train_a - river_mound)] {
        println!("  {lbl}        {:6.2}% of aeolian   {:6.2}% of all seeds",
                 pc(c, aeo), pc(c, n));
    }

    println!("\nCAROLINA, given fluvial");
    println!("  upland (Pinehurst-like) {:6.2}%", pc(upland, flu));
    println!("  ordinary                {:6.2}%", pc(flu - upland, flu));
    println!("  form coin is drawn but unused in this mode ({:6.2}% train)",
             pc(train_f, flu));
}
