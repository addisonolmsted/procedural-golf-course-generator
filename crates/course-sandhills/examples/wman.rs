//! seed \t mode \t form \t river  -- descriptors only, no build.
use course_sandhills::{draw, FormClass, Mode};
use course_seed::RunIdentity;

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let seed0: u64 = a[0].parse().unwrap();
    let n: u64 = a[1].parse().unwrap();
    for k in 0..n {
        let s = seed0 + k;
        let d = draw::site(&RunIdentity::from_seed(s), None, None);
        let mode = if d.mode == Mode::Aeolian { "aeolian" } else { "fluvial" };
        let form = match d.form { FormClass::Train => "Train", _ => "Mound" };
        println!("{s}\t{mode}\t{form}\t{}", d.allogenic_river);
    }
}
