//! Pure statistics for resonance scoring. Every function documents its
//! formula; no I/O, no DB — hand-verifiable against published references.
#![forbid(unsafe_code)]

/// Minimum corpus size for an empirical-CDF p-value. Below this the
/// distribution is too thin to say anything honest; the signal is dropped.
pub const MIN_CORPUS: usize = 8;

/// Chi-square survival function for even degrees of freedom (df = 2k):
///   P(X ≥ x) = e^{-x/2} · Σ_{j=0}^{k-1} (x/2)^j / j!
/// Exact for even df — this is all Fisher's method needs.
pub fn chi2_sf_even_df(df: usize, x: f64) -> f64 {
    debug_assert!(df > 0 && df % 2 == 0);
    if x <= 0.0 {
        return 1.0;
    }
    let k = df / 2;
    let h = x / 2.0;
    let mut term = 1.0;
    let mut sum = 1.0;
    for j in 1..k {
        term *= h / j as f64;
        sum += term;
    }
    (-h).exp() * sum
}

/// Fisher's method: χ² = −2·Σ ln pᵢ, df = 2k → (χ², combined p).
/// p-values are clamped above zero so ln is always finite.
pub fn fisher_combine(ps: &[f64]) -> Option<(f64, f64)> {
    if ps.is_empty() {
        return None;
    }
    let chi2 = -2.0
        * ps.iter()
            .map(|p| p.clamp(1e-300, 1.0).ln())
            .sum::<f64>();
    Some((chi2, chi2_sf_even_df(2 * ps.len(), chi2)))
}

/// Weighted Stouffer's method: Z = Σ wᵢ·Φ⁻¹(1−pᵢ) / √(Σ wᵢ²).
/// Larger Z = stronger joint signal.
pub fn stouffer_combine(ps: &[f64], weights: &[f64]) -> Option<f64> {
    if ps.is_empty() || ps.len() != weights.len() {
        return None;
    }
    let denom = weights.iter().map(|w| w * w).sum::<f64>().sqrt();
    if denom == 0.0 {
        return None;
    }
    let z = ps
        .iter()
        .zip(weights)
        .map(|(p, w)| w * normal_ppf(1.0 - p.clamp(1e-16, 1.0 - 1e-16)))
        .sum::<f64>()
        / denom;
    Some(z)
}

/// One-sided (upper tail) empirical-CDF p-value of `value` against `corpus`:
///   p = (1 + #{x ∈ corpus : x ≥ value}) / (n + 1)
/// The +1 smoothing keeps p strictly positive (Fisher needs ln p finite).
/// `None` when the corpus is below [`MIN_CORPUS`] — the min-count guard.
pub fn empirical_p_upper(value: f64, corpus: &[f64]) -> Option<f64> {
    if corpus.len() < MIN_CORPUS {
        return None;
    }
    let ge = corpus.iter().filter(|&&c| c >= value).count();
    Some((1.0 + ge as f64) / (corpus.len() as f64 + 1.0))
}

/// Benjamini–Hochberg q-values, aligned with the input order:
///   q(i) = min over j≥rank(i) of p(j)·m/j, clamped to [0,1].
pub fn benjamini_hochberg(ps: &[f64]) -> Vec<f64> {
    let m = ps.len();
    if m == 0 {
        return Vec::new();
    }
    let mut order: Vec<usize> = (0..m).collect();
    order.sort_by(|&a, &b| ps[a].partial_cmp(&ps[b]).unwrap_or(std::cmp::Ordering::Equal));
    let mut q = vec![1.0; m];
    let mut running = 1.0f64;
    for (rev, &idx) in order.iter().rev().enumerate() {
        let rank = (m - rev) as f64; // 1-based rank of this p in ascending order
        running = running.min(ps[idx] * m as f64 / rank);
        q[idx] = running.clamp(0.0, 1.0);
    }
    q
}

/// Standard normal CDF via Abramowitz–Stegun 7.1.26 (|ε| ≤ 1.5e-7).
pub fn normal_cdf(x: f64) -> f64 {
    0.5 * (1.0 + erf_as(x / std::f64::consts::SQRT_2))
}

fn erf_as(x: f64) -> f64 {
    let sign = if x < 0.0 { -1.0 } else { 1.0 };
    let x = x.abs();
    let t = 1.0 / (1.0 + 0.3275911 * x);
    let y = 1.0
        - (((((1.061405429 * t - 1.453152027) * t) + 1.421413741) * t - 0.284496736) * t
            + 0.254829592)
            * t
            * (-x * x).exp();
    sign * y
}

/// Standard normal inverse CDF (Acklam's rational approximation,
/// max |ε| ≈ 1.15e-9 on the central region).
pub fn normal_ppf(p: f64) -> f64 {
    const A: [f64; 6] = [
        -3.969_683_028_665_376e1,
        2.209_460_984_245_205e2,
        -2.759_285_104_469_687e2,
        1.383_577_518_672_690e2,
        -3.066_479_806_614_716e1,
        2.506_628_277_459_239e0,
    ];
    const B: [f64; 5] = [
        -5.447_609_879_822_406e1,
        1.615_858_368_580_409e2,
        -1.556_989_798_598_866e2,
        6.680_131_188_771_972e1,
        -1.328_068_155_288_572e1,
    ];
    const C: [f64; 6] = [
        -7.784_894_002_430_293e-3,
        -3.223_964_580_411_365e-1,
        -2.400_758_277_161_838e0,
        -2.549_732_539_343_734e0,
        4.374_664_141_464_968e0,
        2.938_163_982_698_783e0,
    ];
    const D: [f64; 4] = [
        7.784_695_709_041_462e-3,
        3.224_671_290_700_398e-1,
        2.445_134_137_142_996e0,
        3.754_408_661_907_416e0,
    ];
    const P_LOW: f64 = 0.02425;
    const P_HIGH: f64 = 1.0 - P_LOW;

    if p <= 0.0 {
        return f64::NEG_INFINITY;
    }
    if p >= 1.0 {
        return f64::INFINITY;
    }
    if p < P_LOW {
        let q = (-2.0 * p.ln()).sqrt();
        (((((C[0] * q + C[1]) * q + C[2]) * q + C[3]) * q + C[4]) * q + C[5])
            / ((((D[0] * q + D[1]) * q + D[2]) * q + D[3]) * q + 1.0)
    } else if p <= P_HIGH {
        let q = p - 0.5;
        let r = q * q;
        (((((A[0] * r + A[1]) * r + A[2]) * r + A[3]) * r + A[4]) * r + A[5]) * q
            / (((((B[0] * r + B[1]) * r + B[2]) * r + B[3]) * r + B[4]) * r + 1.0)
    } else {
        let q = (-2.0 * (1.0 - p).ln()).sqrt();
        -(((((C[0] * q + C[1]) * q + C[2]) * q + C[3]) * q + C[4]) * q + C[5])
            / ((((D[0] * q + D[1]) * q + D[2]) * q + D[3]) * q + 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chi2_sf_df2_is_plain_exponential() {
        // df=2: P(X ≥ x) = e^{-x/2}. At x=2 → e^{-1}.
        assert!((chi2_sf_even_df(2, 2.0) - (-1f64).exp()).abs() < 1e-12);
        assert!((chi2_sf_even_df(2, 0.0) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn chi2_sf_df4_known_value() {
        // df=4: P(X ≥ x) = e^{-x/2}(1 + x/2). At x=4 → e^{-2}·3.
        let expected = (-2f64).exp() * 3.0;
        assert!((chi2_sf_even_df(4, 4.0) - expected).abs() < 1e-12);
    }

    #[test]
    fn fisher_two_equal_pvalues_hand_computed() {
        // ps = [0.05, 0.05]: χ² = −4·ln 0.05 = 11.9829290942 (df 4).
        // p = e^{-χ²/2}(1 + χ²/2); χ²/2 = ln 400 → e^{-χ²/2} = 1/400.
        // p = 6.9914645471 / 400 = 0.0174786613…
        let (chi2, p) = fisher_combine(&[0.05, 0.05]).unwrap();
        assert!((chi2 - 11.982929094215963).abs() < 1e-9);
        assert!((p - 0.0174786613).abs() < 1e-9);
    }

    #[test]
    fn fisher_single_signal_returns_that_pvalue() {
        // Independence edge case: one p in, same p out
        // (df=2 → sf = e^{-χ²/2} = e^{ln p} = p).
        let (_, p) = fisher_combine(&[0.3]).unwrap();
        assert!((p - 0.3).abs() < 1e-12);
    }

    #[test]
    fn fisher_empty_is_none() {
        assert!(fisher_combine(&[]).is_none());
    }

    #[test]
    fn stouffer_two_equal_pvalues_hand_computed() {
        // ps=[0.05,0.05], w=[1,1]: Z = 2·Φ⁻¹(0.95)/√2 = √2·1.6448536…
        let z = stouffer_combine(&[0.05, 0.05], &[1.0, 1.0]).unwrap();
        assert!((z - 2.3261743).abs() < 1e-6);
    }

    #[test]
    fn stouffer_weights_scale_influence() {
        // Down-weighting a weak p leaves Z dominated by the strong one.
        let z_heavy = stouffer_combine(&[0.01, 0.9], &[1.0, 1.0]).unwrap();
        let z_light = stouffer_combine(&[0.01, 0.9], &[1.0, 0.1]).unwrap();
        assert!(z_light > z_heavy);
    }

    #[test]
    fn stouffer_rejects_mismatched_inputs() {
        assert!(stouffer_combine(&[0.5], &[1.0, 1.0]).is_none());
        assert!(stouffer_combine(&[], &[]).is_none());
        assert!(stouffer_combine(&[0.5], &[0.0]).is_none());
    }

    #[test]
    fn normal_cdf_ppf_round_trip_known_values() {
        assert!((normal_cdf(0.0) - 0.5).abs() < 1e-7);
        assert!((normal_cdf(1.9599640) - 0.975).abs() < 1e-6);
        assert!((normal_ppf(0.975) - 1.9599640).abs() < 1e-6);
        assert!((normal_ppf(0.5)).abs() < 1e-9);
        assert!((normal_cdf(normal_ppf(0.01)) - 0.01).abs() < 1e-6);
    }

    #[test]
    fn empirical_p_upper_hand_computed() {
        let corpus: Vec<f64> = (1..=8).map(|i| i as f64).collect();
        // value 8 (the max): (1 + 1)/(8+1) = 2/9
        assert!((empirical_p_upper(8.0, &corpus).unwrap() - 2.0 / 9.0).abs() < 1e-12);
        // value 0.5 (below all): (1 + 8)/9 = 1.0
        assert!((empirical_p_upper(0.5, &corpus).unwrap() - 1.0).abs() < 1e-12);
        // value 5: #{x ≥ 5} = 4 → 5/9
        assert!((empirical_p_upper(5.0, &corpus).unwrap() - 5.0 / 9.0).abs() < 1e-12);
    }

    #[test]
    fn empirical_p_min_count_guard() {
        let corpus: Vec<f64> = (1..7).map(|i| i as f64).collect(); // 7 < 8
        assert!(empirical_p_upper(6.0, &corpus).is_none());
        assert!(empirical_p_upper(6.0, &[]).is_none());
    }

    #[test]
    fn bh_hand_computed_ordering() {
        // ps = [0.01, 0.04, 0.03, 0.0025, 0.05], m = 5.
        // sorted: 0.0025(r1), 0.01(r2), 0.03(r3), 0.04(r4), 0.05(r5)
        // raw:    .0125,     .025,     .05,      .05,      .05
        // monotone from the top: [.0125, .025, .05, .05, .05]
        let q = benjamini_hochberg(&[0.01, 0.04, 0.03, 0.0025, 0.05]);
        assert!((q[3] - 0.0125).abs() < 1e-12);
        assert!((q[0] - 0.025).abs() < 1e-12);
        assert!((q[2] - 0.05).abs() < 1e-12);
        assert!((q[1] - 0.05).abs() < 1e-12);
        assert!((q[4] - 0.05).abs() < 1e-12);
    }

    #[test]
    fn bh_single_and_empty() {
        assert_eq!(benjamini_hochberg(&[]), Vec::<f64>::new());
        let q = benjamini_hochberg(&[0.03]);
        assert!((q[0] - 0.03).abs() < 1e-12);
    }

    #[test]
    fn bh_q_never_below_p_rank_monotone() {
        let ps = [0.5, 0.5, 0.5];
        let q = benjamini_hochberg(&ps);
        assert!(q.iter().all(|&v| (v - 0.5).abs() < 1e-12));
    }
}
