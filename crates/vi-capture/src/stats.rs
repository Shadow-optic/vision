//! Concentration statistics and the degree-preserving Monte Carlo null.
//!
//! Pure functions, no database: Gini and Shannon entropy over outcome-bucket
//! counts, and a seeded permutation null that preserves every entity's
//! appearance count and the corpus-wide outcome marginals. If an entity's
//! observed concentration sits far above what shuffled copies of the same
//! data produce, that is a lead worth counsel's time — nothing more.
#![forbid(unsafe_code)]

use rand::seq::SliceRandom;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

/// Outcome buckets: relief granted / mixed / adverse.
pub const N_CATEGORIES: usize = 3;
pub const CATEGORY_RELIEF: usize = 0;
pub const CATEGORY_MIXED: usize = 1;
pub const CATEGORY_ADVERSE: usize = 2;

/// Bucket an outcome signal in [0,1] (the vi-drift lexicon scale).
pub fn outcome_category(signal: f64) -> usize {
    if signal >= 0.6 {
        CATEGORY_RELIEF
    } else if signal <= 0.4 {
        CATEGORY_ADVERSE
    } else {
        CATEGORY_MIXED
    }
}

/// Per-category counts for a set of outcome signals.
pub fn category_counts(signals: &[f64]) -> [f64; N_CATEGORIES] {
    let mut counts = [0.0; N_CATEGORIES];
    for &s in signals {
        counts[outcome_category(s)] += 1.0;
    }
    counts
}

/// Gini coefficient over a count vector: 0 when appearances spread evenly
/// across categories, approaching 1 as they concentrate in one category.
pub fn gini(counts: &[f64]) -> f64 {
    let n = counts.len();
    if n == 0 {
        return 0.0;
    }
    let mut sorted: Vec<f64> = counts.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let total: f64 = sorted.iter().sum();
    if total <= 0.0 {
        return 0.0;
    }
    let weighted: f64 = sorted
        .iter()
        .enumerate()
        .map(|(i, x)| (i as f64 + 1.0) * x)
        .sum();
    (2.0 * weighted) / (n as f64 * total) - (n as f64 + 1.0) / n as f64
}

/// Shannon entropy (nats) over a count vector: 0 when every appearance
/// lands in one category, ln(k) when uniform over k categories.
pub fn shannon_entropy(counts: &[f64]) -> f64 {
    let total: f64 = counts.iter().sum();
    if total <= 0.0 {
        return 0.0;
    }
    counts
        .iter()
        .filter(|&&c| c > 0.0)
        .map(|&c| {
            let p = c / total;
            -p * p.ln()
        })
        .sum()
}

/// One entity's observed outcome categories (one entry per appearance).
#[derive(Debug, Clone)]
pub struct EntityObservations {
    pub key: String,
    pub categories: Vec<usize>,
}

/// Concentration of one entity against its Monte Carlo null.
#[derive(Debug, Clone)]
pub struct NullOutcome {
    pub key: String,
    pub appearances: usize,
    pub observed_gini: f64,
    pub observed_entropy: f64,
    pub null_mean_gini: f64,
    /// Empirical p with the plus-one correction: (1 + #{null >= observed}) /
    /// (permutations + 1). Never exactly 0.
    pub null_p: f64,
}

fn gini_of_categories(categories: &[usize]) -> f64 {
    let mut counts = [0.0f64; N_CATEGORIES];
    for &c in categories {
        if c < N_CATEGORIES {
            counts[c] += 1.0;
        }
    }
    gini(&counts)
}

/// Degree-preserving Monte Carlo null over a whole population of entities.
///
/// Each permutation shuffles the corpus's outcome-category labels while
/// keeping every entity's appearance count fixed, so both the entity degrees
/// and the global outcome marginals are preserved; the observed Gini is then
/// compared against the null distribution of the same statistic. Deterministic
/// for a fixed `seed` (ChaCha8).
pub fn monte_carlo_null(
    entities: &[EntityObservations],
    permutations: usize,
    seed: u64,
) -> Vec<NullOutcome> {
    let sizes: Vec<usize> = entities.iter().map(|e| e.categories.len()).collect();
    let mut flat: Vec<usize> = entities
        .iter()
        .flat_map(|e| e.categories.iter().copied())
        .collect();
    let observed: Vec<f64> = entities
        .iter()
        .map(|e| gini_of_categories(&e.categories))
        .collect();

    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut exceed = vec![0usize; entities.len()];
    let mut null_sum = vec![0.0f64; entities.len()];

    for _ in 0..permutations {
        flat.shuffle(&mut rng);
        let mut offset = 0usize;
        for (j, size) in sizes.iter().enumerate() {
            let g = gini_of_categories(&flat[offset..offset + size]);
            null_sum[j] += g;
            if g >= observed[j] - 1e-12 {
                exceed[j] += 1;
            }
            offset += size;
        }
    }

    entities
        .iter()
        .enumerate()
        .map(|(j, e)| {
            let mut counts = [0.0f64; N_CATEGORIES];
            for &c in &e.categories {
                if c < N_CATEGORIES {
                    counts[c] += 1.0;
                }
            }
            NullOutcome {
                key: e.key.clone(),
                appearances: e.categories.len(),
                observed_gini: observed[j],
                observed_entropy: shannon_entropy(&counts),
                null_mean_gini: if permutations > 0 {
                    null_sum[j] / permutations as f64
                } else {
                    0.0
                },
                null_p: (1.0 + exceed[j] as f64) / (permutations as f64 + 1.0),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f64 = 1e-12;

    #[test]
    fn gini_known_vectors() {
        assert!((gini(&[0.0, 0.0, 10.0]) - 2.0 / 3.0).abs() < EPS);
        assert!((gini(&[5.0, 5.0, 5.0])).abs() < EPS);
        assert!((gini(&[1.0, 2.0, 3.0, 4.0]) - 0.25).abs() < EPS);
        assert_eq!(gini(&[0.0, 0.0, 0.0]), 0.0);
        assert_eq!(gini(&[]), 0.0);
    }

    #[test]
    fn entropy_known_vectors() {
        assert!((shannon_entropy(&[5.0, 5.0, 5.0]) - 3.0f64.ln()).abs() < EPS);
        assert_eq!(shannon_entropy(&[0.0, 0.0, 10.0]), 0.0);
        assert!((shannon_entropy(&[5.0, 5.0]) - 2.0f64.ln()).abs() < EPS);
    }

    #[test]
    fn categories_split_the_signal_scale() {
        assert_eq!(outcome_category(1.0), CATEGORY_RELIEF);
        assert_eq!(outcome_category(0.6), CATEGORY_RELIEF);
        assert_eq!(outcome_category(0.5), CATEGORY_MIXED);
        assert_eq!(outcome_category(0.4), CATEGORY_ADVERSE);
        assert_eq!(outcome_category(0.0), CATEGORY_ADVERSE);
        assert_eq!(category_counts(&[1.0, 0.9, 0.5, 0.0]), [2.0, 1.0, 1.0]);
    }

    fn entity(key: &str, categories: Vec<usize>) -> EntityObservations {
        EntityObservations {
            key: key.to_string(),
            categories,
        }
    }

    #[test]
    fn planted_clique_beats_the_null() {
        // One judge with 12 relief outcomes; ten peers evenly split.
        let mut entities = vec![entity("clique", vec![CATEGORY_RELIEF; 12])];
        for i in 0..10 {
            entities.push(entity(
                &format!("peer-{i}"),
                vec![CATEGORY_RELIEF, CATEGORY_RELIEF, CATEGORY_RELIEF,
                     CATEGORY_ADVERSE, CATEGORY_ADVERSE, CATEGORY_ADVERSE],
            ));
        }
        let out = monte_carlo_null(&entities, 1000, 42);
        assert!(out[0].null_p < 0.05, "clique p = {}", out[0].null_p);
        assert!(out[0].observed_gini > out[0].null_mean_gini);
        // Evenly split peers are exactly what the null expects.
        assert!(out[1].null_p > 0.5, "peer p = {}", out[1].null_p);
    }

    #[test]
    fn uniform_data_is_not_significant() {
        let entities: Vec<_> = (0..11)
            .map(|i| {
                entity(
                    &format!("judge-{i}"),
                    vec![CATEGORY_RELIEF, CATEGORY_RELIEF, CATEGORY_RELIEF,
                         CATEGORY_ADVERSE, CATEGORY_ADVERSE, CATEGORY_ADVERSE],
                )
            })
            .collect();
        let out = monte_carlo_null(&entities, 1000, 7);
        for o in &out {
            assert!(o.null_p > 0.9, "{} unexpectedly low p = {}", o.key, o.null_p);
        }
    }

    #[test]
    fn fixed_seed_is_deterministic() {
        let entities = vec![
            entity("a", vec![0, 0, 0, 1, 2, 2]),
            entity("b", vec![1, 1, 2, 0]),
        ];
        let first = monte_carlo_null(&entities, 1000, 123);
        let second = monte_carlo_null(&entities, 1000, 123);
        for (a, b) in first.iter().zip(second.iter()) {
            assert_eq!(a.null_p.to_bits(), b.null_p.to_bits());
            assert_eq!(a.null_mean_gini.to_bits(), b.null_mean_gini.to_bits());
        }
        // A different seed may move the estimate but stays in range.
        let third = monte_carlo_null(&entities, 1000, 124);
        for o in &third {
            assert!(o.null_p > 0.0 && o.null_p <= 1.0);
        }
    }
}
