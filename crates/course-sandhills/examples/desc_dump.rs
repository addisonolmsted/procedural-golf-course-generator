//! Print the drawn descriptors of seeds, one JSON-ish line each, so a
//! screen can be joined against what was drawn.
//!
//!   desc_dump <seed>...
use course_sandhills::draw;
use course_seed::RunIdentity;

fn main() {
    for s in std::env::args().skip(1) {
        let seed: u64 = s.parse().unwrap();
        let d = draw::site(&RunIdentity::from_seed(seed), None, None);
        println!("{seed} {:?}", d);
    }
}
