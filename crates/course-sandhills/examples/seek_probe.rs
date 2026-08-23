use course_sandhills::{draw, Mode};
use course_seed::RunIdentity;
fn main() {
    for s in 0..200u64 {
        let d = draw::site(&RunIdentity::from_seed(s), Some(Mode::Aeolian), None);
        if d.water_table_m < 1.5 || d.allogenic_river {
            println!("{s}: table {:.1} river {}", d.water_table_m, d.allogenic_river);
        }
    }
}
