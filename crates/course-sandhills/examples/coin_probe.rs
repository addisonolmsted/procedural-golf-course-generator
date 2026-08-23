use course_sandhills::{draw, Mode};
use course_seed::RunIdentity;
fn main() {
    let n = 400;
    let hits = (0..n).filter(|s| draw::site(&RunIdentity::from_seed(*s as u64),
        Some(Mode::Aeolian), None).allogenic_river).count();
    println!("river coin: {hits}/{n} = {:.3} (record says 0.30)", hits as f64 / n as f64);
    let wt: Vec<f64> = (0..40).map(|s| draw::site(&RunIdentity::from_seed(s),
        Some(Mode::Aeolian), None).water_table_m).collect();
    let shallow = wt.iter().filter(|v| **v < 2.0).count();
    println!("water_table < 2 m (lake-forming): {shallow}/40");
}
