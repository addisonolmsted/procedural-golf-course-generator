use course_sandhills::{draw, Mode};
use course_seed::RunIdentity;
fn main() {
    let mut n = 0;
    for s in 67..500u64 {
        let d = draw::site(&RunIdentity::from_seed(s), Some(Mode::Aeolian), None);
        if d.allogenic_river {
            println!("{s}");
            n += 1;
            if n == 10 { break; }
        }
    }
}
