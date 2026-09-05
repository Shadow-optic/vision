/**
 * Typed view of `catalog.json`, which is generated from
 * `crates/vi-reckoning/src/statutes.rs` by
 * `cargo run -p vi-reckoning --bin catalog`. CI fails if the two diverge.
 */
import catalog from "./catalog.json";

export interface Statute {
	citation: string;
	title: string;
	kind: string;
	elements: string[];
	statutory_maximum: string;
	finding_types: string[];
	research_note: string;
}

export interface ImmunityNote {
	doctrine: string;
	leading_case: string;
	scope: string;
	recognized_limits: string;
}

export const STATUTES = catalog.statutes as Statute[];
export const IMMUNITY = catalog.immunity as ImmunityNote[];
export const STATUTE_NOTE: string = catalog.statute_note;
export const IMMUNITY_NOTE: string = catalog.immunity_note;
export const CATALOG_SOURCE: string = catalog.source;

/** True when the cited title authorizes any term of years or life. */
export function authorizesLife(statute: Statute): boolean {
	return /\blife\b/i.test(statute.statutory_maximum);
}
