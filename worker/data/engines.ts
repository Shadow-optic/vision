/** The engines, described for the public site. Row counts come from `GET /engines`. */
export interface EngineDescriptor {
	name: string;
	crate: string;
	summary: string;
	routes: string[];
}

export const ENGINES: EngineDescriptor[] = [
	{
		name: "Root Ledger",
		crate: "vi-ledger",
		summary:
			"Append-only, BLAKE3 hash-chained record of every action the platform takes. Anyone can verify the chain end to end.",
		routes: ["/ledger/verify"],
	},
	{
		name: "Case-law DB",
		crate: "vi-api",
		summary:
			"Full-text searchable corpus of opinions and dockets ingested from public court records.",
		routes: ["/cases/search", "/cases/:id"],
	},
	{
		name: "Correlation",
		crate: "vi-correlation",
		summary:
			"Pearson coefficients with Fisher confidence intervals and odds ratios. Every formula is printed with its result.",
		routes: ["/stats/pearson", "/stats/odds", "/stats/plea-sentence"],
	},
	{
		name: "Tactics DB",
		crate: "vi-tactics",
		summary:
			"Catalog of documented courtroom tactics with occurrence rates computed from public records.",
		routes: ["/tactics", "/tactics/:id/stats"],
	},
	{
		name: "Abuse detection",
		crate: "vi-trustscript",
		summary:
			"TrustScript rule language — lexer, parser, evaluator. Rules raise flags for counsel review; a flag is never a publication.",
		routes: ["/rules", "/rules/run", "/flags"],
	},
	{
		name: "Zero-day sim",
		crate: "vi-sim",
		summary:
			"Monte Carlo outcome simulation with priors calibrated from stored office statistics.",
		routes: ["/simulate", "/simulate/from-case/:case_id"],
	},
	{
		name: "H3 intelligence",
		crate: "vi-geo",
		summary:
			"Multi-resolution H3 geospatial ladder built at ingest, with k-ring disparity queries.",
		routes: ["/geo/cells/:cell", "/geo/kring/:cell"],
	},
	{
		name: "Telemetry / ingest",
		crate: "vi-ingest",
		summary:
			"Live public feeds — courts registry, search, and per-court Atom — with cursor checkpoints. Where a feed publishes only an extract of an opinion, the record says so rather than presenting it as the whole text.",
		routes: ["/ingest/run", "/ingest/status", "/ingest/sources"],
	},
	{
		name: "Post-ingest pipeline",
		crate: "vi-pipeline",
		summary:
			"Walks each ingested record through forum resolution, constitutional screening, evidence-lead reconciliation, abuse rules, individual linkage, and scoring. Every artifact it produces is pending; it publishes nothing.",
		routes: ["/pipeline/run", "/pipeline/status", "/pipeline/unresolved-officials"],
	},
	{
		name: "JIT LASM",
		crate: "vi-lasm",
		summary:
			"Just-in-time evidence package for a single case: flags, Brady leads, Monell pattern, trial penalty, constitutional screen, ledger provenance.",
		routes: ["/lasm/package/:case_id"],
	},
	{
		name: "Monell atlas",
		crate: "vi-monell-atlas",
		summary:
			"Pattern-and-practice fingerprint for an office, with a § 1983 municipal-liability scaffold.",
		routes: ["/atlas/offices/fingerprint", "/atlas/offices/monell-report"],
	},
	{
		name: "Brady recon",
		crate: "vi-brady-recon",
		summary:
			"Reconciles evidence that should exist against evidence actually disclosed. Gaps are investigative leads, not findings.",
		routes: ["/brady/derive/:case_id", "/brady/reconcile/:case_id", "/brady/lead-report/:case_id"],
	},
	{
		name: "Trial penalty",
		crate: "vi-trial-penalty",
		summary:
			"Plea-versus-trial sentence distributions, disparity odds ratios, and a draft motion template.",
		routes: ["/trial-penalty/offices", "/trial-penalty/disparity", "/trial-penalty/motion"],
	},
	{
		name: "Constitution / Bill of Rights",
		crate: "vi-constitution",
		summary:
			"Native corpus of Articles I–VII and Amendments 1–27, 50-state jurisdiction resolver, stare-decisis chain, advisory screens.",
		routes: ["/constitution", "/constitution/jurisdictions", "/constitution/screen/:case_id"],
	},
	{
		name: "Reckoning / individual accountability",
		crate: "vi-reckoning",
		summary:
			"Named-actor resolution, formula-audited abuse scores, counsel-reviewed referral, bar-complaint and § 1983 packages, and the Wall of Injustice.",
		routes: ["/reckoning/actors", "/reckoning/wall", "/reckoning/statutes", "/reckoning/tracker"],
	},
];
