//! Linear, non-algorithmic statistics. Every result carries its formula,
//! sample size, and inputs summary so outputs are fully auditable in court.
#![forbid(unsafe_code)]

use serde::Serialize;

pub const Z95: f64 = 1.959_963_984_540_054;

#[derive(Debug, Clone, Serialize)]
pub struct CorrResult {
    pub n: usize,
    pub r: f64,
    pub ci95: (f64, f64),
    pub formula: &'static str,
}

/// Pearson product-moment correlation with Fisher z confidence interval.
/// Point-biserial correlations (e.g., race-as-coded vs. sentence) are Pearson.
pub fn pearson(x: &[f64], y: &[f64]) -> Option<CorrResult> {
    if x.len() != y.len() || x.len() < 4 {
        return None;
    }
    let n = x.len();
    let mx = x.iter().sum::<f64>() / n as f64;
    let my = y.iter().sum::<f64>() / n as f64;
    let (mut sxy, mut sxx, mut syy) = (0.0, 0.0, 0.0);
    for i in 0..n {
        let (dx, dy) = (x[i] - mx, y[i] - my);
        sxy += dx * dy;
        sxx += dx * dx;
        syy += dy * dy;
    }
    let denom = (sxx * syy).sqrt();
    if denom == 0.0 {
        return None;
    }
    let r = (sxy / denom).clamp(-1.0, 1.0);
    let ci95 = if r.abs() >= 1.0 {
        (r, r)
    } else {
        let z = r.atanh();
        let se = 1.0 / ((n as f64) - 3.0).sqrt();
        ((z - Z95 * se).tanh(), (z + Z95 * se).tanh())
    };
    Some(CorrResult {
        n,
        r,
        ci95,
        formula: "r = Σ(x-x̄)(y-ȳ)/√(Σ(x-x̄)²Σ(y-ȳ)²); CI via Fisher z: atanh(r) ± 1.96/√(n-3)",
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct OddsResult {
    pub table: [u32; 4], // [a, b, c, d] — original (uncorrected) counts
    pub odds_ratio: f64,
    pub ci95: (f64, f64),
    pub haldane_corrected: bool,
    pub formula: &'static str,
}

/// Odds ratio for a 2×2 table:
///        outcome+  outcome-
/// expo+     a        b
/// expo-     c        d
/// Haldane–Anscombe +0.5 correction applied when any cell is zero.
pub fn odds_ratio(a: u32, b: u32, c: u32, d: u32) -> OddsResult {
    let corrected = a == 0 || b == 0 || c == 0 || d == 0;
    let (af, bf, cf, df) = if corrected {
        (
            a as f64 + 0.5,
            b as f64 + 0.5,
            c as f64 + 0.5,
            d as f64 + 0.5,
        )
    } else {
        (a as f64, b as f64, c as f64, d as f64)
    };
    let or = (af * df) / (bf * cf);
    let ln = or.ln();
    let se = (1.0 / af + 1.0 / bf + 1.0 / cf + 1.0 / df).sqrt();
    OddsResult {
        table: [a, b, c, d],
        odds_ratio: or,
        ci95: ((ln - Z95 * se).exp(), (ln + Z95 * se).exp()),
        haldane_corrected: corrected,
        formula: "OR = ad/bc; CI on ln(OR) ± 1.96·√(1/a+1/b+1/c+1/d)",
    }
}

/// Builds a 2×2 table from parallel outcome/exposure slices.
pub fn contingency(outcome: &[bool], exposure: &[bool]) -> Option<[u32; 4]> {
    if outcome.len() != exposure.len() || outcome.is_empty() {
        return None;
    }
    let (mut a, mut b, mut c, mut d) = (0, 0, 0, 0);
    for i in 0..outcome.len() {
        match (exposure[i], outcome[i]) {
            (true, true) => a += 1,
            (true, false) => b += 1,
            (false, true) => c += 1,
            (false, false) => d += 1,
        }
    }
    Some([a, b, c, d])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn perfect_correlation() {
        let x: Vec<f64> = (0..50).map(|i| i as f64).collect();
        let r = pearson(&x, &x).unwrap();
        assert!((r.r - 1.0).abs() < 1e-12);
        assert_eq!(r.n, 50);
    }

    #[test]
    fn known_odds_ratio() {
        // a=10,b=20,c=30,d=40 → OR = 400/600 ≈ 0.667
        let r = odds_ratio(10, 20, 30, 40);
        assert!((r.odds_ratio - 0.6667).abs() < 1e-3);
        assert!(!r.haldane_corrected);
        assert!(r.ci95.0 < r.odds_ratio && r.odds_ratio < r.ci95.1);
        assert_eq!(r.table, [10, 20, 30, 40]);
    }

    #[test]
    fn zero_cell_corrected() {
        let r = odds_ratio(0, 5, 5, 10);
        assert!(r.haldane_corrected);
        assert_eq!(r.table, [0, 5, 5, 10]);
        assert!(r.odds_ratio.is_finite());
    }

    #[test]
    fn small_sample_rejected() {
        assert!(pearson(&[1.0, 2.0], &[1.0, 2.0]).is_none());
    }

    #[test]
    fn constant_series_rejected() {
        assert!(pearson(
            &[1.0; 10],
            &[2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0]
        )
        .is_none());
    }

    #[test]
    fn contingency_counts() {
        let outcome = [true, true, false, false, true];
        let exposure = [true, false, true, false, true];
        assert_eq!(contingency(&outcome, &exposure), Some([2, 1, 1, 1]));
    }
}
