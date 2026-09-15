//! JSON out: one record per tile in the shape `tools/golf/batch_route.py`
//! writes, so the Rust batch and the Python reference compare directly.
//!
//! Key ORDER differs from the Python (`serde_json::Map` sorts keys; the
//! workspace does not enable `preserve_order`); the key SET, nesting and
//! rounding of the v1 keys match `batch_route.route_one`.
//!
//! RECORD V2 (`rec_v: 2`, 2026-09-14) is a superset of v1 for the audit
//! (`tools/golf/route_audit.py`): per hole `index`, `kind`, `green_idx`,
//! `approach_bin`, `terms`, `tee_graded[5]`, `tee_slope[5]`, `bridges` as
//! `[{a, b, span_m, kind}]` with the v1 count kept as `n_bridges`, and
//! `walk: {path, grade_mean, bridges}`; per route `crossings: [[i, j, y,
//! x]]` beside `n_crossings`, `green_in_play: [[green_hole, spine_hole,
//! d_m]]` and `coverage` (null until plan item 4 fills it). The one v1 key
//! whose TYPE changed is the hole's `bridges` (count -> list); no reader in
//! `tools/golf` consumed it.

use serde_json::{json, Value};

use crate::{Bridge, BridgeKind, Route, Siting, Walk};

pub const REC_V: u32 = 2;

fn kind_str(k: BridgeKind) -> &'static str {
    match k {
        BridgeKind::Walk => "walk",
        BridgeKind::Spine => "spine",
    }
}

fn bridge_json(b: &Bridge) -> Value {
    json!({
        "a": [b.a_yx.0, b.a_yx.1],
        "b": [b.b_yx.0, b.b_yx.1],
        "span_m": rnd(b.span_m, 1),
        "kind": kind_str(b.kind),
    })
}

fn walk_json(w: &Walk) -> Value {
    let bridges: Vec<Value> = w.bridges.iter().map(bridge_json).collect();
    json!({
        "path": [[w.path[0].0, w.path[0].1], [w.path[1].0, w.path[1].1]],
        "grade_mean": rnd(w.grade_mean, 4),
        "bridges": bridges,
    })
}

/// Python `round(x, d)` (half-to-even on the decimal only matters on exact
/// ties, which the rounded quantities here never hit in practice).
fn rnd(x: f64, d: i32) -> f64 {
    let m = 10f64.powi(d);
    (x * m).round() / m
}

/// The batch record: `rec_v, seed, mode, seconds, routed, window_m, pool,
/// pars, total_length_m, total_walk_m, n_crossings, crossings, clubhouse,
/// score, terms, green_in_play, coverage, holes[{index, par, kind,
/// green_idx, green, approach_bin, length_m, tees, tee_graded, tee_slope,
/// lzs, spine, walk_m, walk, n_bridges, bridges, terms}]`. With `route ==
/// None` only the first seven keys are present (the Python's six plus
/// `rec_v`).
pub fn record(seed: u64, mode: &str, seconds: f64, pool: usize, sit: &Siting,
              route: Option<&Route>) -> Value {
    let (wy, wx, wh, ww) = sit.window_m;
    let mut rec = json!({
        "rec_v": REC_V,
        "seed": seed,
        "mode": mode,
        "seconds": rnd(seconds, 2),
        "routed": route.is_some(),
        "window_m": [wy, wx, wh, ww],
        "pool": pool,
    });
    let r = match route {
        Some(r) => r,
        None => return rec,
    };
    let obj = rec.as_object_mut().expect("json object");
    obj.insert("pars".into(), json!(r.par_sequence));
    obj.insert("total_length_m".into(), json!(rnd(r.total_length_m, 1)));
    obj.insert("total_walk_m".into(), json!(rnd(r.total_walk_m, 1)));
    obj.insert("n_crossings".into(), json!(r.crossings.len()));
    let crossings: Vec<Value> = r.crossings.iter()
        .map(|&(i, j, (y, x))| json!([i, j, y, x])).collect();
    obj.insert("crossings".into(), Value::Array(crossings));
    let gip: Vec<Value> = r.green_in_play.iter()
        .map(|&(g, h, d)| json!([g, h, rnd(d, 1)])).collect();
    obj.insert("green_in_play".into(), Value::Array(gip));
    obj.insert("coverage".into(), Value::Null);
    obj.insert("clubhouse".into(), json!([r.clubhouse_yx.0, r.clubhouse_yx.1]));
    obj.insert("score".into(), json!(rnd(r.score, 3)));
    let mut terms = serde_json::Map::new();
    for (k, v) in &r.terms {
        terms.insert(k.clone(), json!(rnd(*v, 4)));
    }
    obj.insert("terms".into(), Value::Object(terms));
    let mut holes = Vec::with_capacity(r.holes.len());
    for h in &r.holes {
        let tees: Vec<[f64; 2]> = h.tee_boxes.iter().map(|b| [b.yx.0, b.yx.1]).collect();
        let lzs: Vec<[f64; 3]> = h.lzs.iter().map(|l| [l.0, l.1, l.2]).collect();
        let spine: Vec<[f64; 2]> = h.spine.iter().map(|p| [p.0, p.1]).collect();
        let tee_graded: Vec<bool> = h.tee_boxes.iter().map(|b| b.graded).collect();
        let tee_slope: Vec<f64> = h.tee_boxes.iter().map(|b| rnd(b.slope, 4)).collect();
        let bridges: Vec<Value> = h.bridges.iter().map(bridge_json).collect();
        let mut hterms = serde_json::Map::new();
        for (k, v) in &h.terms {
            hterms.insert(k.clone(), json!(rnd(*v, 4)));
        }
        holes.push(json!({
            "index": h.index,
            "par": h.par,
            "kind": h.kind,
            "green_idx": h.green_idx,
            "green": [h.green_yx.0, h.green_yx.1],
            "approach_bin": h.approach_bin,
            "length_m": rnd(h.length_m, 1),
            "tees": tees,
            "tee_graded": tee_graded,
            "tee_slope": tee_slope,
            "lzs": lzs,
            "spine": spine,
            "walk_m": rnd(h.walk_from_prev.length_m, 1),
            "walk": walk_json(&h.walk_from_prev),
            "n_bridges": h.bridges.len(),
            "bridges": bridges,
            "terms": Value::Object(hterms),
        }));
    }
    obj.insert("holes".into(), Value::Array(holes));
    rec
}

/// The error record the Python batch writes when a seed raises:
/// `{seed, mode, routed: false, error}`.
pub fn error_record(seed: u64, mode: &str, error: &str) -> Value {
    json!({ "seed": seed, "mode": mode, "routed": false, "error": error })
}

/// One line for logs: pars, total length, walk, crossings, score.
pub fn route_summary(route: &Route) -> String {
    let pars: Vec<String> = route.par_sequence.iter().map(|p| p.to_string()).collect();
    format!("pars [{}] total {:.0} m walk {:.0} m crossings {} score {:.2}",
            pars.join(","), route.total_length_m, route.total_walk_m,
            route.crossings.len(), route.score)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Clubhouse, Hole, TeeBox};
    use std::collections::BTreeMap;

    fn sit() -> Siting {
        let ch = Clubhouse {
            yx: (100.0, 200.0),
            score: 1.0,
            reserved_green: (0.0, 0.0),
            reserved_tee: (0.0, 0.0),
            alternates: Vec::new(),
        };
        Siting {
            window_ij: (1, 2),
            window_m: (8.0, 16.0, 700.0, 1000.0),
            clubhouse: ch,
            pair_score: 0.5,
            shortlist: Vec::new(),
        }
    }

    fn route() -> Route {
        let tb = TeeBox { yx: (1.5, 2.5), size_m: 7.0, axis: 0.0, length_m: 300.0, graded: false,
                          slope: 0.0123456 };
        let wb = Bridge { a_yx: (0.5, 0.5), b_yx: (1.0, 1.5), span_m: 4.04, kind: BridgeKind::Walk,
                          hole: 0 };
        let walk = Walk { path: [(0.0, 0.0), (1.5, 2.5)], length_m: 2.9155, grade_mean: 0.05,
                          bridges: vec![wb] };
        let mut terms = BTreeMap::new();
        terms.insert("green".to_string(), 0.123456);
        let sb = Bridge { a_yx: (100.0, 100.0), b_yx: (110.0, 100.0), span_m: 10.0,
                          kind: BridgeKind::Spine, hole: 0 };
        let hole = Hole {
            index: 0, par: 4, green_idx: 3, kind: "hot".into(), green_yx: (300.0, 400.0),
            tee_boxes: vec![tb; 5], lzs: vec![(200.0, 250.0, 22.0)],
            spine: vec![(1.5, 2.5), (200.0, 250.0), (300.0, 400.0)],
            length_m: 400.04, approach_bin: 2, bridges: vec![sb],
            walk_from_prev: walk, terms,
        };
        let mut rterms = BTreeMap::new();
        rterms.insert("entropy".to_string(), 0.33333333);
        rterms.insert("worst_clear".to_string(), 0.12);
        Route {
            holes: vec![hole], par_sequence: vec![4], total_length_m: 400.04,
            total_walk_m: 2.9155, clubhouse_yx: (100.0, 200.0), score: 12.34567,
            terms: rterms, crossings: vec![(0, 1, (5.0, 6.0))],
            green_in_play: vec![(2, 4, 31.26)],
        }
    }

    #[test]
    fn unrouted_shape() {
        let v = record(7, "aeolian", 1.234567, 88, &sit(), None);
        let o = v.as_object().unwrap();
        let mut keys: Vec<&String> = o.keys().collect();
        keys.sort();
        assert_eq!(keys, vec!["mode", "pool", "rec_v", "routed", "seconds", "seed", "window_m"]);
        assert_eq!(v["rec_v"], 2);
        assert_eq!(v["routed"], false);
        assert_eq!(v["seconds"], 1.23);
        assert_eq!(v["window_m"], json!([8.0, 16.0, 700.0, 1000.0]));
        assert_eq!(v["pool"], 88);
    }

    #[test]
    fn routed_shape() {
        let r = route();
        let v = record(600009, "fluvial", 3.0, 100, &sit(), Some(&r));
        let o = v.as_object().unwrap();
        let mut keys: Vec<&String> = o.keys().collect();
        keys.sort();
        assert_eq!(keys, vec!["clubhouse", "coverage", "crossings", "green_in_play", "holes",
                              "mode", "n_crossings", "pars", "pool", "rec_v", "routed", "score",
                              "seconds", "seed", "terms", "total_length_m", "total_walk_m",
                              "window_m"]);
        assert_eq!(v["rec_v"], 2);
        assert_eq!(v["pars"], json!([4]));
        assert_eq!(v["crossings"], json!([[0, 1, 5.0, 6.0]]));
        assert_eq!(v["green_in_play"], json!([[2, 4, 31.3]]));
        assert!(v["coverage"].is_null());
        assert_eq!(v["total_length_m"], 400.0);
        assert_eq!(v["total_walk_m"], 2.9);
        assert_eq!(v["n_crossings"], 1);
        assert_eq!(v["clubhouse"], json!([100.0, 200.0]));
        assert_eq!(v["score"], 12.346);
        assert_eq!(v["terms"]["entropy"], 0.3333);
        let h = &v["holes"][0];
        let mut hk: Vec<&String> = h.as_object().unwrap().keys().collect();
        hk.sort();
        assert_eq!(hk, vec!["approach_bin", "bridges", "green", "green_idx", "index", "kind",
                            "length_m", "lzs", "n_bridges", "par", "spine", "tee_graded",
                            "tee_slope", "tees", "terms", "walk", "walk_m"]);
        assert_eq!(h["tees"].as_array().unwrap().len(), 5);
        assert_eq!(h["lzs"], json!([[200.0, 250.0, 22.0]]));
        assert_eq!(h["spine"].as_array().unwrap().len(), 3);
        assert_eq!(h["index"], 0);
        assert_eq!(h["kind"], "hot");
        assert_eq!(h["green_idx"], 3);
        assert_eq!(h["approach_bin"], 2);
        assert_eq!(h["tee_graded"], json!([false, false, false, false, false]));
        assert_eq!(h["tee_slope"], json!([0.0123, 0.0123, 0.0123, 0.0123, 0.0123]));
        assert_eq!(h["terms"]["green"], 0.1235);
        assert_eq!(h["n_bridges"], 1);
        assert_eq!(h["bridges"], json!([{"a": [100.0, 100.0], "b": [110.0, 100.0],
                                         "span_m": 10.0, "kind": "spine"}]));
        assert_eq!(h["walk"]["path"], json!([[0.0, 0.0], [1.5, 2.5]]));
        assert_eq!(h["walk"]["grade_mean"], 0.05);
        assert_eq!(h["walk"]["bridges"], json!([{"a": [0.5, 0.5], "b": [1.0, 1.5],
                                                 "span_m": 4.0, "kind": "walk"}]));
        assert_eq!(h["walk_m"], 2.9);
        assert_eq!(route_summary(&r),
                   "pars [4] total 400 m walk 3 m crossings 1 score 12.35");
        let e = error_record(1, "aeolian", "boom");
        assert_eq!(e["routed"], false);
        assert_eq!(e["error"], "boom");
    }
}
