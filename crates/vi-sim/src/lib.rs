//! Zero-Day Defense & Strategy Engine — seeded Monte Carlo simulation.
//!
//! Reproducibility guarantee: identical (priors, strategy, trials, seed)
//! always produces an identical Distribution. The API stores every run's seed
//! and input hash in the Root Ledger so results can be re-derived in court.
//!
//! ⚠ The weights below are a *transparent, intentionally simple prior model*.
//! They must be calibrated per-jurisdiction from vi-correlation outputs before
//! any result is cited in a filing. The model is a null hypothesis, not truth.
#![forbid(unsafe_code)]

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use rand_distr::Normal;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CasePriors {
    pub evidence_strength: f64, // 0..1 (1 = overwhelming for prosecution)
    pub charge_severity: f64,   // 0..1
    pub prior_record: f64,      // 0..1
    pub judge_propensity: f64,  // 0..1 historical conviction lean
    pub prosecutor_aggressiveness: f64, // 0..1
    pub jury_propensity: f64,   // 0..1 from H3 census-informed pool model
    pub base_plea_months: f64,
    pub base_trial_months: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Strategy {
    pub name: String,
    pub plea_discount: f64,     // 0..1 negotiation effect on offer
    pub suppression_bonus: f64, // 0..1 increases dismissal odds
    pub acquittal_bonus: f64,   // 0..1 decreases conviction odds at trial
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Distribution {
    pub trials: u32,
    pub p_dismissal: f64,
    pub p_plea: f64,
    pub p_acquittal: f64,
    pub p_conviction: f64,
    pub mean_plea_months: f64,
    pub mean_trial_sentence_months: f64,
    pub trial_penalty_months: f64,
    pub expected_months: f64, // exposure across all outcomes
}

fn clamp01(x: f64) -> f64 {
    x.clamp(0.0, 1.0)
}

/// Observed office / case statistics used to derive [`CasePriors`].
/// Missing fields fall back to the transparent null-hypothesis defaults.
#[derive(Debug, Clone, Default)]
pub struct CalibrationInputs {
    pub evidence_strength: Option<String>,
    pub charge_category: Option<String>,
    pub office_conviction_rate: Option<f64>,
    pub office_mean_plea_months: Option<f64>,
    pub office_mean_trial_months: Option<f64>,
    pub judge_conviction_rate: Option<f64>,
    pub case_plea_offer_months: Option<f64>,
    pub case_sentence_months: Option<f64>,
}

fn map_evidence(label: &str) -> f64 {
    match label.to_ascii_lowercase().as_str() {
        "weak" => 0.30,
        "mixed" => 0.55,
        "strong" => 0.80,
        _ => 0.50,
    }
}

fn map_charge_severity(category: &str) -> f64 {
    match category.to_ascii_lowercase().as_str() {
        "drug" | "possession" => 0.40,
        "assault" | "violent" => 0.65,
        "homicide" => 0.90,
        _ => 0.50,
    }
}

/// Derive priors from stored public-record aggregates.
/// Returns `(priors, source)` where source is `"calibrated"` when office
/// conviction rate and at least one sentence mean are present, else `"fallback"`.
pub fn priors_from_stats(c: &CalibrationInputs) -> (CasePriors, &'static str) {
    let evidence = c
        .evidence_strength
        .as_deref()
        .map(map_evidence)
        .unwrap_or(0.50);
    let charge = c
        .charge_category
        .as_deref()
        .map(map_charge_severity)
        .unwrap_or(0.50);
    let prosecutor = c.office_conviction_rate.unwrap_or(0.50).clamp(0.0, 1.0);
    let judge = c.judge_conviction_rate.unwrap_or(0.50).clamp(0.0, 1.0);
    let base_plea = c
        .office_mean_plea_months
        .or(c.case_plea_offer_months)
        .unwrap_or(24.0)
        .max(0.0);
    let base_trial = c
        .office_mean_trial_months
        .or(c.case_sentence_months)
        .unwrap_or(72.0)
        .max(0.0);

    let calibrated = c.office_conviction_rate.is_some()
        && (c.office_mean_plea_months.is_some() || c.office_mean_trial_months.is_some());

    (
        CasePriors {
            evidence_strength: evidence,
            charge_severity: charge,
            prior_record: 0.20,
            judge_propensity: judge,
            prosecutor_aggressiveness: prosecutor,
            jury_propensity: 0.50,
            base_plea_months: base_plea,
            base_trial_months: base_trial,
        },
        if calibrated { "calibrated" } else { "fallback" },
    )
}

pub fn simulate(p: &CasePriors, s: &Strategy, trials: u32, seed: u64) -> Distribution {
    if trials == 0 {
        return Distribution {
            trials: 0,
            p_dismissal: 0.0,
            p_plea: 0.0,
            p_acquittal: 0.0,
            p_conviction: 0.0,
            mean_plea_months: 0.0,
            mean_trial_sentence_months: 0.0,
            trial_penalty_months: 0.0,
            expected_months: 0.0,
        };
    }
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let normal = Normal::<f64>::new(0.0, 1.0).unwrap();

    let (mut dismiss, mut plea, mut acq, mut conv) = (0u32, 0u32, 0u32, 0u32);
    let (mut plea_sum, mut trial_sum, mut total_sum) = (0.0, 0.0, 0.0);

    for _ in 0..trials {
        let e = clamp01(p.evidence_strength + rng.sample(normal) * 0.1);

        let p_dismiss = clamp01(
            0.05 + s.suppression_bonus
                - 0.08 * p.charge_severity
                - 0.05 * p.prosecutor_aggressiveness,
        );
        if rng.gen::<f64>() < p_dismiss {
            dismiss += 1;
            continue;
        }

        let plea_offer =
            (p.base_plea_months * (1.0 - s.plea_discount) * (0.7 + 0.6 * p.charge_severity))
                .max(0.0);
        let exp_trial_sent = p.base_trial_months * (0.5 + 0.9 * p.charge_severity);
        let p_conv = clamp01(
            0.62 * (0.8 + 0.4 * p.judge_propensity)
                * (0.8 + 0.4 * p.prosecutor_aggressiveness)
                * (0.9 + 0.2 * p.jury_propensity)
                * (1.35 - 0.55 * e)
                * (1.0 - s.acquittal_bonus),
        );

        // Defendant-side rational-choice-ish acceptance with pressure term.
        let p_accept = clamp01(
            0.35 + (exp_trial_sent * p_conv - plea_offer) / 48.0
                + 0.15 * p.prosecutor_aggressiveness
                - 0.15 * e,
        );
        if rng.gen::<f64>() < p_accept {
            plea += 1;
            plea_sum += plea_offer;
            total_sum += plea_offer;
            continue;
        }

        if rng.gen::<f64>() < p_conv {
            let sent = (exp_trial_sent * (1.0 + 0.15 * p.prior_record)
                + rng.sample(normal) * exp_trial_sent * 0.15)
                .max(0.0);
            conv += 1;
            trial_sum += sent;
            total_sum += sent;
        } else {
            acq += 1;
        }
    }

    let n = trials as f64;
    let mean_plea = if plea > 0 {
        plea_sum / plea as f64
    } else {
        0.0
    };
    let mean_trial = if conv > 0 {
        trial_sum / conv as f64
    } else {
        0.0
    };
    Distribution {
        trials,
        p_dismissal: dismiss as f64 / n,
        p_plea: plea as f64 / n,
        p_acquittal: acq as f64 / n,
        p_conviction: conv as f64 / n,
        mean_plea_months: mean_plea,
        mean_trial_sentence_months: mean_trial,
        trial_penalty_months: mean_trial - mean_plea,
        expected_months: total_sum / n,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn priors(evidence: f64) -> CasePriors {
        CasePriors {
            evidence_strength: evidence,
            charge_severity: 0.6,
            prior_record: 0.2,
            judge_propensity: 0.5,
            prosecutor_aggressiveness: 0.7,
            jury_propensity: 0.5,
            base_plea_months: 24.0,
            base_trial_months: 72.0,
        }
    }

    fn strat() -> Strategy {
        Strategy {
            name: "suppress-then-trial".into(),
            plea_discount: 0.15,
            suppression_bonus: 0.10,
            acquittal_bonus: 0.10,
        }
    }

    #[test]
    fn fully_deterministic_given_seed() {
        let a = simulate(&priors(0.5), &strat(), 10_000, 42);
        let b = simulate(&priors(0.5), &strat(), 10_000, 42);
        assert_eq!(a.expected_months.to_bits(), b.expected_months.to_bits());
    }

    #[test]
    fn probabilities_sum_to_one() {
        let d = simulate(&priors(0.5), &strat(), 50_000, 7);
        let sum = d.p_dismissal + d.p_plea + d.p_acquittal + d.p_conviction;
        assert!((sum - 1.0).abs() < 1e-9);
    }

    #[test]
    fn weaker_prosecution_evidence_helps_defense() {
        let strong = simulate(&priors(0.9), &strat(), 50_000, 1);
        let weak = simulate(&priors(0.2), &strat(), 50_000, 1);
        assert!(weak.p_conviction < strong.p_conviction - 0.05);
        assert!(weak.expected_months < strong.expected_months);
    }

    #[test]
    fn zero_trials_is_defined() {
        let d = simulate(&priors(0.5), &strat(), 0, 1);
        assert_eq!(d.trials, 0);
        assert_eq!(d.expected_months, 0.0);
        assert_eq!(d.p_dismissal, 0.0);
    }

    #[test]
    fn priors_from_stats_calibrated_when_office_data_present() {
        let (p, src) = priors_from_stats(&CalibrationInputs {
            evidence_strength: Some("weak".into()),
            charge_category: Some("drug".into()),
            office_conviction_rate: Some(0.8),
            office_mean_plea_months: Some(12.0),
            office_mean_trial_months: Some(36.0),
            judge_conviction_rate: Some(0.7),
            case_plea_offer_months: Some(12.0),
            case_sentence_months: Some(36.0),
        });
        assert_eq!(src, "calibrated");
        assert!((p.evidence_strength - 0.30).abs() < 1e-9);
        assert!((p.charge_severity - 0.40).abs() < 1e-9);
        assert!((p.prosecutor_aggressiveness - 0.8).abs() < 1e-9);
        assert_eq!(p.base_plea_months, 12.0);
        assert_eq!(p.base_trial_months, 36.0);
    }

    #[test]
    fn priors_from_stats_fallback_without_office_means() {
        let (_, src) = priors_from_stats(&CalibrationInputs {
            evidence_strength: Some("mixed".into()),
            ..CalibrationInputs::default()
        });
        assert_eq!(src, "fallback");
    }
}
