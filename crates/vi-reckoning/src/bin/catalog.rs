//! Emits the statute and immunity catalogs as JSON on stdout.
//!
//! The public Cloudflare Worker ships this file so the doctrine, statute, and
//! immunity pages stay complete even when the backend is unreachable. CI
//! re-runs this binary and diffs the result against `worker/data/catalog.json`,
//! so the site can never drift from the Rust source of truth.
#![forbid(unsafe_code)]

use serde_json::json;
use vi_reckoning::statutes::{IMMUNITY, STATUTES};

fn main() {
    let doc = json!({
        "generated_by": "cargo run -p vi-reckoning --bin catalog",
        "source": "crates/vi-reckoning/src/statutes.rs",
        "statutes": STATUTES,
        "immunity": IMMUNITY,
        "statute_note": "Research catalog. Not charging decisions. Willfulness and agreement are for counsel and a fact-finder.",
        "immunity_note": "Immunity research is not a recommendation to ignore a shield or to proceed extra-legally.",
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&doc).expect("catalog serializes")
    );
}
