//! Bayesian online changepoint detection (Adams & MacKay, 2007).
//!
//! Pure Rust, no database: feed observations one at a time with
//! [`Bocpd::step`], read the run-length posterior back. The observation model
//! is a Gaussian with unknown mean and variance under a Normal-Gamma prior,
//! so the predictive for each run-length hypothesis is a Student-t.
//!
//! Detection statistic. Under a constant hazard H, P(r_t = 0 | x_1..t) = H
//! identically — it carries no information about the data. The data-dependent
//! quantity is the run-length distribution itself: when a regime breaks, the
//! posterior mass abandons the "previous MAP run grew by one" branch and
//! collapses onto short runs. [`detect_changepoints`] therefore reports
//! `1 - P(r_t = r̂_{t-1} + 1)` — the posterior probability that the previously
//! dominant run did NOT survive — and declares a changepoint when that
//! probability clears the threshold and the new MAP run is a reset. On
//! stationary series the growth branch keeps its mass and nothing fires.
#![forbid(unsafe_code)]

/// Normal-Gamma prior for the Gaussian observation model.
///
/// The default is tuned for outcome signals in [0, 1]: centered at 0.5 with
/// a broad scale, weakly informative so a handful of observations can move it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Prior {
    pub mu: f64,
    pub kappa: f64,
    pub alpha: f64,
    pub beta: f64,
}

impl Default for Prior {
    fn default() -> Self {
        Prior {
            mu: 0.5,
            kappa: 1.0,
            alpha: 1.0,
            beta: 0.25,
        }
    }
}

impl Prior {
    fn validate(&self) -> Result<(), String> {
        if !self.mu.is_finite() {
            return Err("prior mu must be finite".into());
        }
        if !(self.kappa > 0.0 && self.kappa.is_finite()) {
            return Err("prior kappa must be positive".into());
        }
        if !(self.alpha > 0.0 && self.alpha.is_finite()) {
            return Err("prior alpha must be positive".into());
        }
        if !(self.beta > 0.0 && self.beta.is_finite()) {
            return Err("prior beta must be positive".into());
        }
        Ok(())
    }
}

/// What one [`Bocpd::step`] produced.
#[derive(Debug, Clone)]
pub struct StepOutput {
    /// P(r_t = r) for r = 0..=t after observing this point.
    pub run_length_posterior: Vec<f64>,
    /// Most likely current run length.
    pub map_run_length: usize,
    /// 1 - P(the previous step's MAP run grew by one). This is the
    /// data-dependent changepoint score; see module docs.
    pub reset_posterior: f64,
}

/// Online changepoint detector over a single numeric series.
#[derive(Debug, Clone)]
pub struct Bocpd {
    /// Constant hazard: per-step prior probability of a changepoint.
    hazard: f64,
    prior: Prior,
    /// R[r] = P(current run length = r), normalized after each step.
    run_posterior: Vec<f64>,
    /// Normal-Gamma sufficient statistics per run-length hypothesis, aligned
    /// with `run_posterior`. Index r holds the posterior over the last r
    /// observations (index 0 = the prior, an empty run).
    params: Vec<Prior>,
}

impl Bocpd {
    /// `expected_run_length` is the mean segment length λ of the
    /// constant-hazard model (H = 1/λ). Must be >= 1.
    pub fn new(prior: Prior, expected_run_length: f64) -> Result<Self, String> {
        prior.validate()?;
        if !(expected_run_length >= 1.0 && expected_run_length.is_finite()) {
            return Err(format!(
                "expected_run_length must be finite and >= 1, got {expected_run_length}"
            ));
        }
        Ok(Self {
            hazard: 1.0 / expected_run_length,
            prior,
            run_posterior: vec![1.0],
            params: vec![prior],
        })
    }

    pub fn hazard(&self) -> f64 {
        self.hazard
    }

    /// Current run-length posterior (before the next observation).
    pub fn run_length_posterior(&self) -> &[f64] {
        &self.run_posterior
    }

    /// Incorporate one observation and return the updated posterior.
    pub fn step(&mut self, x: f64) -> StepOutput {
        let prev_map = argmax(&self.run_posterior);

        // Student-t predictive of x under each run-length hypothesis.
        let predictive: Vec<f64> = self
            .params
            .iter()
            .map(|p| student_t_pdf(x, 2.0 * p.alpha, p.mu, p.beta * (p.kappa + 1.0) / (p.alpha * p.kappa)))
            .collect();

        // Adams-MacKay recursion: growth vs changepoint, then normalize.
        let h = self.hazard;
        let mut next: Vec<f64> = Vec::with_capacity(self.run_posterior.len() + 1);
        let mut cp_mass = 0.0;
        for (r, p) in predictive.iter().enumerate() {
            next.push(self.run_posterior[r] * p * (1.0 - h));
            cp_mass += self.run_posterior[r] * p * h;
        }
        let mut posterior = Vec::with_capacity(next.len() + 1);
        posterior.push(cp_mass);
        posterior.append(&mut next);
        let total: f64 = posterior.iter().sum();
        if total > 0.0 {
            for v in posterior.iter_mut() {
                *v /= total;
            }
        } else {
            // All predictives underflowed (an extreme outlier): every
            // hypothesis is equally (im)plausible, so fall back to the
            // hazard-weighted prior split rather than NaN.
            let n = posterior.len() as f64;
            for (r, v) in posterior.iter_mut().enumerate() {
                *v = if r == 0 { h } else { (1.0 - h) / (n - 1.0).max(1.0) };
            }
        }

        // Sufficient statistics: the changepoint hypothesis starts empty;
        // every surviving run absorbs this observation.
        let mut params = Vec::with_capacity(self.params.len() + 1);
        params.push(self.prior);
        for p in &self.params {
            params.push(Prior {
                mu: (p.kappa * p.mu + x) / (p.kappa + 1.0),
                kappa: p.kappa + 1.0,
                alpha: p.alpha + 0.5,
                beta: p.beta + p.kappa * (x - p.mu) * (x - p.mu) / (2.0 * (p.kappa + 1.0)),
            });
        }

        let grow_idx = prev_map + 1;
        let reset_posterior = if grow_idx < posterior.len() {
            1.0 - posterior[grow_idx]
        } else {
            0.0
        };
        let map_run_length = argmax(&posterior);
        self.run_posterior = posterior;
        self.params = params;
        StepOutput {
            run_length_posterior: self.run_posterior.clone(),
            map_run_length,
            reset_posterior,
        }
    }
}

/// A detected changepoint: `index` is the 0-based position of the first
/// observation of the new regime; `posterior` is the reset probability.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DetectedChangepoint {
    pub index: usize,
    pub posterior: f64,
}

/// Run BOCPD over a whole series and extract changepoints. Consecutive
/// above-threshold steps are one event; the peak step is reported.
pub fn detect_changepoints(
    series: &[f64],
    expected_run_length: f64,
    threshold: f64,
) -> Result<Vec<DetectedChangepoint>, String> {
    if !(threshold > 0.0 && threshold <= 1.0) {
        return Err(format!("threshold must be in (0, 1], got {threshold}"));
    }
    let mut model = Bocpd::new(Prior::default(), expected_run_length)?;
    let mut events: Vec<DetectedChangepoint> = Vec::new();
    let mut prev_map = 0usize;
    for (i, &x) in series.iter().enumerate() {
        let out = model.step(x);
        // A reset: the previous MAP run failed to grow AND the posterior
        // moved to a shorter run. (The first step has no previous run.)
        if i > 0 && out.reset_posterior >= threshold && out.map_run_length < prev_map + 1 {
            match events.last_mut() {
                // Consecutive alarms are one event; keep the strongest step.
                Some(last) if last.index + 1 >= i => {
                    if out.reset_posterior > last.posterior {
                        *last = DetectedChangepoint {
                            index: i,
                            posterior: out.reset_posterior,
                        };
                    }
                }
                _ => events.push(DetectedChangepoint {
                    index: i,
                    posterior: out.reset_posterior,
                }),
            }
        }
        prev_map = out.map_run_length;
    }
    Ok(events)
}

fn argmax(xs: &[f64]) -> usize {
    let mut best = 0usize;
    for (i, v) in xs.iter().enumerate() {
        if *v > xs[best] {
            best = i;
        }
    }
    best
}

/// Student-t density: `df` degrees of freedom, location `mu`, `var` the
/// squared scale (not the variance of the distribution).
fn student_t_pdf(x: f64, df: f64, mu: f64, var: f64) -> f64 {
    let z = x - mu;
    let log_p = ln_gamma((df + 1.0) / 2.0)
        - ln_gamma(df / 2.0)
        - 0.5 * (std::f64::consts::PI * df * var).ln()
        - ((df + 1.0) / 2.0) * (1.0 + z * z / (df * var)).ln();
    log_p.exp()
}

/// Lanczos approximation of ln Γ(z), valid for all z > 0 (with reflection
/// below 0.5 for completeness).
fn ln_gamma(z: f64) -> f64 {
    // Coefficients for g = 7, n = 9.
    const P: [f64; 9] = [
        0.999_999_999_999_809_93,
        676.520_368_121_885_1,
        -1_259.139_216_722_402_8,
        771.323_428_777_653_13,
        -176.615_029_162_140_59,
        12.507_343_278_686_905,
        -0.138_571_095_265_720_12,
        9.984_369_578_019_571_6e-6,
        1.505_632_735_149_311_6e-7,
    ];
    if z < 0.5 {
        // Reflection: Γ(z)Γ(1-z) = π / sin(πz)
        return (std::f64::consts::PI / ((std::f64::consts::PI * z).sin())).ln() - ln_gamma(1.0 - z);
    }
    let z = z - 1.0;
    let mut x = P[0];
    for (i, p) in P.iter().enumerate().skip(1) {
        x += p / (z + i as f64);
    }
    let t = z + 7.5;
    0.5 * (2.0 * std::f64::consts::PI).ln() + (z + 0.5) * t.ln() - t + x.ln()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::{Rng, SeedableRng};
    use rand_chacha::ChaCha8Rng;
    use rand_distr::{Distribution, Normal};

    const EPS: f64 = 1e-9;

    /// Reference vector computed by an independent implementation of the
    /// Adams-MacKay recursion (Normal-Gamma prior mu=0, kappa=1, alpha=1,
    /// beta=1; constant hazard lambda=4; series [1.0, 1.2, 0.8]).
    #[test]
    fn recursion_matches_reference() {
        let prior = Prior {
            mu: 0.0,
            kappa: 1.0,
            alpha: 1.0,
            beta: 1.0,
        };
        let mut m = Bocpd::new(prior, 4.0).unwrap();

        let o1 = m.step(1.0);
        assert!((o1.run_length_posterior[0] - 0.25).abs() < EPS);
        assert!((o1.run_length_posterior[1] - 0.75).abs() < EPS);

        let o2 = m.step(1.2);
        let expect2 = [0.25, 0.127_242_950_854, 0.622_757_049_146];
        for (got, want) in o2.run_length_posterior.iter().zip(expect2) {
            assert!((got - want).abs() < EPS, "got {got}, want {want}");
        }

        let o3 = m.step(0.8);
        let expect3 = [0.25, 0.113_959_198_161, 0.089_591_337_253, 0.546_449_464_585];
        for (got, want) in o3.run_length_posterior.iter().zip(expect3) {
            assert!((got - want).abs() < EPS, "got {got}, want {want}");
        }
    }

    #[test]
    fn posterior_is_normalized_every_step() {
        let mut m = Bocpd::new(Prior::default(), 50.0).unwrap();
        let mut rng = ChaCha8Rng::seed_from_u64(99);
        for _ in 0..50 {
            let out = m.step(rng.gen::<f64>());
            let s: f64 = out.run_length_posterior.iter().sum();
            assert!((s - 1.0).abs() < 1e-9, "posterior sums to {s}");
        }
    }

    #[test]
    fn planted_shift_is_detected_near_the_plant() {
        let mut rng = ChaCha8Rng::seed_from_u64(7);
        let noise = Normal::new(0.0, 0.05).unwrap();
        let mut series: Vec<f64> = (0..40).map(|_| 0.2 + noise.sample(&mut rng)).collect();
        series.extend((0..40).map(|_| 0.85 + noise.sample(&mut rng)));

        let events = detect_changepoints(&series, 50.0, 0.5).unwrap();
        assert_eq!(events.len(), 1, "expected exactly one changepoint: {events:?}");
        let cp = events[0];
        assert!(
            (38..=43).contains(&cp.index),
            "changepoint at {} should be near the plant at 40",
            cp.index
        );
        // Posterior strength varies with the draw (0.7-0.98 across seeds);
        // the requirement is a confident detection, not a fixed value.
        assert!(cp.posterior >= 0.6, "posterior was {}", cp.posterior);
    }

    #[test]
    fn stationary_series_raises_no_alarm() {
        // Several seeds, one detector setting: a false alarm on any of them
        // fails the test. Seeded, so the check is reproducible.
        for seed in 100..120u64 {
            let mut rng = ChaCha8Rng::seed_from_u64(seed);
            let noise = Normal::new(0.5, 0.1).unwrap();
            let series: Vec<f64> = (0..100).map(|_| noise.sample(&mut rng)).collect();
            let events = detect_changepoints(&series, 50.0, 0.5).unwrap();
            assert!(
                events.is_empty(),
                "false alarm on seed {seed}: {events:?}"
            );
        }
    }

    #[test]
    fn invalid_parameters_are_rejected() {
        assert!(Bocpd::new(Prior::default(), 0.0).is_err());
        assert!(Bocpd::new(Prior::default(), f64::NAN).is_err());
        assert!(detect_changepoints(&[1.0], 50.0, 0.0).is_err());
        assert!(Bocpd::new(Prior { alpha: -1.0, ..Prior::default() }, 50.0).is_err());
    }

    #[test]
    fn ln_gamma_matches_known_values() {
        // Γ(1) = 1, Γ(0.5) = sqrt(pi), Γ(5) = 24
        assert!(ln_gamma(1.0).abs() < 1e-12);
        assert!((ln_gamma(0.5) - 0.5 * std::f64::consts::PI.ln()).abs() < 1e-12);
        assert!((ln_gamma(5.0) - 24.0f64.ln()).abs() < 1e-12);
    }
}
