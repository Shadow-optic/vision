//! Transparent outcome-signal lexicon.
//!
//! Ingested opinions rarely carry a machine-readable disposition, so the
//! drift and capture engines read an outcome proxy straight off the opinion
//! text. The lexicon below is deliberately small, published in source, and
//! weight-averaged: every signal it emits is labeled `machine_derived` and
//! stays `pending` until counsel review. A text with no lexicon hit produces
//! NO data point — absence of a match is never treated as an outcome.
//!
//! Signal scale: 1.0 = relief granted to the movant/defendant (suppression
//! granted, reversed, vacated), 0.0 = relief denied / affirmed against.
//! Mixed texts land in between via the weighted mean of matched terms.
#![forbid(unsafe_code)]

/// One lexicon entry: a lowercase phrase, its weight, and the outcome value
/// it votes for (1.0 = relief granted, 0.0 = denied).
pub struct LexiconTerm {
    pub phrase: &'static str,
    pub weight: f64,
    pub value: f64,
}

/// The published lexicon. Order matters only for [`explain`] output.
/// Phrases are matched case-insensitively as substrings; more specific
/// phrases carry more weight so "suppression granted" outweighs a bare
/// "granted" elsewhere in the snippet.
pub const LEXICON: &[LexiconTerm] = &[
    // --- relief granted (value 1.0) ---
    LexiconTerm { phrase: "suppression granted", weight: 3.0, value: 1.0 },
    LexiconTerm { phrase: "motion to suppress is granted", weight: 3.0, value: 1.0 },
    LexiconTerm { phrase: "suppressed", weight: 1.5, value: 1.0 },
    LexiconTerm { phrase: "is granted", weight: 1.5, value: 1.0 },
    LexiconTerm { phrase: "are granted", weight: 1.2, value: 1.0 },
    LexiconTerm { phrase: "motion granted", weight: 1.2, value: 1.0 },
    LexiconTerm { phrase: "granting", weight: 0.8, value: 1.0 },
    LexiconTerm { phrase: "reversed", weight: 1.2, value: 1.0 },
    LexiconTerm { phrase: "vacated", weight: 1.2, value: 1.0 },
    LexiconTerm { phrase: "remanded", weight: 0.6, value: 0.8 },
    // --- relief denied (value 0.0) ---
    LexiconTerm { phrase: "suppression denied", weight: 3.0, value: 0.0 },
    LexiconTerm { phrase: "motion to suppress is denied", weight: 3.0, value: 0.0 },
    LexiconTerm { phrase: "is denied", weight: 1.5, value: 0.0 },
    LexiconTerm { phrase: "are denied", weight: 1.2, value: 0.0 },
    LexiconTerm { phrase: "motion denied", weight: 1.2, value: 0.0 },
    LexiconTerm { phrase: "denying", weight: 0.8, value: 0.0 },
    LexiconTerm { phrase: "denial of", weight: 0.8, value: 0.0 },
    LexiconTerm { phrase: "affirmed", weight: 1.0, value: 0.0 },
];

/// Which terms matched a text, for transparency surfaces.
#[derive(Debug, Clone, PartialEq)]
pub struct LexiconHit {
    pub phrase: &'static str,
    pub weight: f64,
    pub value: f64,
}

/// All matched terms plus the resulting weighted-mean signal.
#[derive(Debug, Clone, PartialEq)]
pub struct SignalExplanation {
    pub signal: f64,
    pub hits: Vec<LexiconHit>,
}

/// Outcome proxy for `text`: the weighted mean of matched term values.
/// Returns `None` when nothing matched — no lexicon hit, no data point.
pub fn outcome_signal(text: &str) -> Option<f64> {
    explain(text).map(|e| e.signal)
}

/// Like [`outcome_signal`] but also reports exactly which phrases fired.
pub fn explain(text: &str) -> Option<SignalExplanation> {
    let lower = text.to_ascii_lowercase();
    let hits: Vec<LexiconHit> = LEXICON
        .iter()
        .filter(|t| lower.contains(t.phrase))
        .map(|t| LexiconHit {
            phrase: t.phrase,
            weight: t.weight,
            value: t.value,
        })
        .collect();
    if hits.is_empty() {
        return None;
    }
    let weight_sum: f64 = hits.iter().map(|h| h.weight).sum();
    let signal = hits.iter().map(|h| h.weight * h.value).sum::<f64>() / weight_sum;
    Some(SignalExplanation { signal, hits })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn granted_terms_score_near_one() {
        let s = outcome_signal("The motion to suppress is granted.").expect("hit");
        assert!(s > 0.95, "got {s}");
    }

    #[test]
    fn denied_terms_score_near_zero() {
        let s = outcome_signal("Defendant's motion is denied. Affirmed.").expect("hit");
        assert!(s < 0.05, "got {s}");
    }

    #[test]
    fn no_hit_means_no_signal() {
        assert_eq!(outcome_signal("The court met on Tuesday to discuss calendars."), None);
        assert_eq!(outcome_signal(""), None);
    }

    #[test]
    fn matching_is_case_insensitive() {
        assert!(outcome_signal("REVERSED AND REMANDED").is_some());
    }

    #[test]
    fn mixed_text_lands_between() {
        let s = outcome_signal("The suppression motion is denied; the conviction is vacated on other grounds.")
            .expect("hit");
        assert!(s > 0.05 && s < 0.95, "got {s}");
    }

    #[test]
    fn explanation_names_every_matched_phrase() {
        let e = explain("Reversed. The evidence should have been suppressed.").expect("hit");
        let phrases: Vec<_> = e.hits.iter().map(|h| h.phrase).collect();
        assert!(phrases.contains(&"reversed"));
        assert!(phrases.contains(&"suppressed"));
    }
}
