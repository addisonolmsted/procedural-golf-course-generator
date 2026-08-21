//! Export one real trunk + the relief field for the interactive explorer.
use course_seed::RunIdentity;
fn main() {
    let id = RunIdentity::from_seed(2);
    let draw = course_draw::generate(&id, Some(course_draw::Archetype::Piedmont));
    let t = course_template::build(&id, &draw);
    let net = course_network::build(&id, &t);
    let tk = &net.trunks[0];
    let pts: Vec<String> = tk.pts.iter().map(|p| format!("[{:.1},{:.1}]", p.x, p.y)).collect();
    let zs: Vec<String> = tk.z.iter().map(|z| format!("{z:.3}")).collect();
    // relief field at 32 m (94x94)
    let n = 94usize;
    let mut rel = Vec::with_capacity(n * n);
    for gy in 0..n {
        for gx in 0..n {
            let p = course_world::math::Vec2::new(gx as f64 * 32.0, gy as f64 * 32.0);
            rel.push(format!("{:.3}", t.fields.relief_pred.bilinear(p)));
        }
    }
    println!(
        r#"{{"pts":[{}],"z":[{}],"rel_n":{n},"rel":[{}]}}"#,
        pts.join(","), zs.join(","), rel.join(",")
    );
}
