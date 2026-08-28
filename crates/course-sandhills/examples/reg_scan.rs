//! Print seeds whose mound draw takes the regular sub-type.
use course_sandhills::{draw, FormClass, Mode};
use course_seed::RunIdentity;
fn main() {
    let n: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(2000);
    for s in 0..n {
        let d = draw::site(&RunIdentity::from_seed(s), Some(Mode::Aeolian), Some(FormClass::Mound));
        if d.regular {
            println!("{s} river={} relief={:.1}", d.allogenic_river, d.dune_relief_m);
        }
    }
}
