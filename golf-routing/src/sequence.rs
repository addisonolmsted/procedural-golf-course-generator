//! Par sequence and per-hole length targets.
//!
//! Par 36 over 9 holes forces n₃ = n₅ (compositions are (k, 9−2k, k)), so the
//! mix is drawn from a small weighted table, then ordered by shuffle-with-
//! rejection so rare patterns (par-3 openers, back-to-back 3s/5s) stay rare.
//! Length targets use jittered Latin-hypercube quantiles through piecewise-
//! linear atlas quantile curves, so every course mixes short/mid/long holes.

use golf_core::det::DetRng;

/// Composition weights: (n₃ = n₅, weight). The 3-3-3 mix is deliberately
/// boosted above its real-world rate (~0%) for game variety (user: 10–15%).
const COMP_WEIGHTS: [(u8, f64); 5] = [
    (2, 0.62), // 2-5-2: the dominant real nine
    (1, 0.20), // 1-7-1
    (3, 0.13), // 3-3-3: deliberately above atlas
    (0, 0.03), // 0-9-0
    (4, 0.02), // 4-1-4
];

/// Ordering rejection factors (multiplied into the acceptance weight).
const OPENER_PAR3: f64 = 0.15;
const CLOSER_PAR3: f64 = 0.45;
const B2B_PAR3: f64 = 0.10;
const B2B_PAR5: f64 = 0.35;
const ORDER_TRIES: usize = 64;

/// Atlas length quantile curves per par, meters: (q, length) knots.
/// p10/p50/p90 match the measured atlas values; ends clamp the tails.
const Q3: [(f64, f64); 5] = [(0.0, 120.0), (0.10, 143.0), (0.50, 176.0), (0.90, 216.0), (1.0, 260.0)];
const Q4: [(f64, f64); 5] = [(0.0, 265.0), (0.10, 321.0), (0.50, 395.0), (0.90, 447.0), (1.0, 490.0)];
const Q5: [(f64, f64); 5] = [(0.0, 425.0), (0.10, 453.0), (0.50, 507.0), (0.90, 555.0), (1.0, 600.0)];

/// Draw the ordered par sequence (sums to 36).
pub fn draw_sequence(rng: &mut DetRng) -> [u8; 9] {
    // Composition.
    let total: f64 = COMP_WEIGHTS.iter().map(|&(_, w)| w).sum();
    let mut u = rng.next_f64() * total;
    let mut n3 = COMP_WEIGHTS[0].0;
    for &(k, w) in &COMP_WEIGHTS {
        if u < w {
            n3 = k;
            break;
        }
        u -= w;
    }
    let n5 = n3;
    let n4 = 9 - 2 * n3;

    let mut base: Vec<u8> = Vec::with_capacity(9);
    base.extend(std::iter::repeat_n(3u8, n3 as usize));
    base.extend(std::iter::repeat_n(4u8, n4 as usize));
    base.extend(std::iter::repeat_n(5u8, n5 as usize));

    // Ordering: shuffle proposals accepted by their rarity weight; if none
    // accepts, take the best-weight proposal (first among ties). Fixed
    // proposal count keeps the draw deterministic.
    let mut best: [u8; 9] = [4; 9];
    let mut best_w = -1.0f64;
    for _ in 0..ORDER_TRIES {
        let mut s = base.clone();
        // Fisher-Yates.
        for i in (1..9).rev() {
            let j = rng.below(i + 1);
            s.swap(i, j);
        }
        let mut w = 1.0;
        if s[0] == 3 {
            w *= OPENER_PAR3;
        }
        if s[8] == 3 {
            w *= CLOSER_PAR3;
        }
        for k in 1..9 {
            if s[k] == 3 && s[k - 1] == 3 {
                w *= B2B_PAR3;
            }
            if s[k] == 5 && s[k - 1] == 5 {
                w *= B2B_PAR5;
            }
        }
        if w > best_w {
            best_w = w;
            best.copy_from_slice(&s);
        }
        if rng.next_f64() < w {
            let mut out = [0u8; 9];
            out.copy_from_slice(&s);
            return out;
        }
    }
    best
}

/// Piecewise-linear quantile → length.
fn q_to_len(knots: &[(f64, f64)], q: f64) -> f64 {
    let q = q.clamp(0.0, 1.0);
    for w in knots.windows(2) {
        let (q0, l0) = w[0];
        let (q1, l1) = w[1];
        if q <= q1 {
            let t = if q1 == q0 { 0.0 } else { (q - q0) / (q1 - q0) };
            return l0 + (l1 - l0) * t;
        }
    }
    knots[knots.len() - 1].1
}

/// Length targets per hole: jittered Latin-hypercube quantiles (one per
/// ninth-ile, shuffled over holes) mapped through the per-par atlas curves.
/// The stratification is the balance mechanism — a course cannot draw all its
/// holes from the long (or short) end.
pub fn draw_lengths(rng: &mut DetRng, pars: &[u8; 9]) -> [f64; 9] {
    let mut qs = [0.0f64; 9];
    for (i, q) in qs.iter_mut().enumerate() {
        // Stratified over [0.06, 0.94]: stratum i, uniform within.
        *q = 0.06 + 0.88 * ((i as f64 + rng.next_f64()) / 9.0);
    }
    // Shuffle strata across holes.
    for i in (1..9).rev() {
        let j = rng.below(i + 1);
        qs.swap(i, j);
    }
    let mut out = [0.0f64; 9];
    for i in 0..9 {
        let knots: &[(f64, f64)] = match pars[i] {
            3 => &Q3,
            4 => &Q4,
            _ => &Q5,
        };
        out[i] = q_to_len(knots, qs[i]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sequences_sum_to_36_and_are_deterministic() {
        for seed in 0..200u64 {
            let mut a = DetRng::new(seed, b"seq-test");
            let mut b = DetRng::new(seed, b"seq-test");
            let sa = draw_sequence(&mut a);
            let sb = draw_sequence(&mut b);
            assert_eq!(sa, sb);
            assert_eq!(sa.iter().map(|&p| p as u32).sum::<u32>(), 36);
        }
    }

    #[test]
    fn rare_patterns_are_rare() {
        let n = 4000;
        let (mut opener3, mut b2b3, mut b2b5, mut mix333) = (0, 0, 0, 0);
        for seed in 0..n as u64 {
            let mut rng = DetRng::new(seed, b"seq-freq");
            let s = draw_sequence(&mut rng);
            if s[0] == 3 {
                opener3 += 1;
            }
            if s.windows(2).any(|w| w[0] == 3 && w[1] == 3) {
                b2b3 += 1;
            }
            if s.windows(2).any(|w| w[0] == 5 && w[1] == 5) {
                b2b5 += 1;
            }
            if s.iter().filter(|&&p| p == 3).count() == 3 {
                mix333 += 1;
            }
        }
        let f = |c: usize| c as f64 / n as f64;
        assert!(f(opener3) < 0.08, "opener par-3 too common: {}", f(opener3));
        assert!(f(b2b3) < 0.06, "b2b par-3 too common: {}", f(b2b3));
        assert!(f(b2b5) < 0.10, "b2b par-5 too common: {}", f(b2b5));
        assert!(
            (0.08..=0.18).contains(&f(mix333)),
            "3-3-3 mix off target: {}",
            f(mix333)
        );
    }

    #[test]
    fn lengths_stratified_and_in_range() {
        for seed in 0..200u64 {
            let mut rng = DetRng::new(seed, b"len-test");
            let pars = draw_sequence(&mut rng);
            let lens = draw_lengths(&mut rng, &pars);
            for i in 0..9 {
                let (lo, hi) = match pars[i] {
                    3 => (120.0, 260.0),
                    4 => (265.0, 490.0),
                    _ => (425.0, 600.0),
                };
                assert!(lens[i] >= lo - 1e-9 && lens[i] <= hi + 1e-9);
            }
        }
    }
}
