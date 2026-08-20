//! Why do heads stop? Ablation over argument.
use course_draw::{generate, Archetype};
use course_network::{account, grow, GrowParams};
use course_seed::RunIdentity;
use course_world::math::Vec2;

fn main() {
    let id = RunIdentity::from_seed(2);
    let t = course_template::build(&id, &generate(&id, Some(Archetype::Piedmont)));
    let seeds: Vec<(Vec2, f64)> = t.trunks.iter().map(|k| (k.mouth, k.azimuth_rad)).collect();
    println!("trunks: {}", seeds.len());
    println!("{:>8} {:>7} {:>7} {:>7} {:>7} {:>7} {:>9}",
             "claim_m", "nodes", "steps", "branch", "offtile", "claimed", "density");
    for claim in [0.0, 60.0, 120.0, 210.0, 300.0, 420.0] {
        for rate in [1.0, 2.0, 4.0] {
            let mut rng = course_draw::rng::stream(&id, course_network::NETWORK_GROW);
            let p = GrowParams { claim_m: claim, branch_rate: rate, ..Default::default() };
            let (nodes, st) = grow::grow(&mut rng, &seeds, &t.fields.relief_pred, &t.fields.grain_rad, &p);
            let len: f64 = nodes.iter().filter(|n| n.parent.is_some())
                .map(|n| n.p.distance(nodes[n.parent.unwrap() as usize].p)).sum();
            println!("{claim:>8.0} {:>7} {:>7} {:>7} {:>7} {:>7} {:>9.2}   rate {rate}",
                     nodes.len(), st.steps, st.branches, st.died_offtile, st.died_claimed,
                     len / 1000.0 / 9.0);
            let _ = account::ACCOUNT_RES_M;
        }
    }
}
