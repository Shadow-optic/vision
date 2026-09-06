-- B2: Weak-Signal Fusion Resonance (vi-resonance).
-- Per-case composite of sub-threshold signals across engines (pending abuse
-- flags, constitution screen hits, Brady evidence-lead gap ratio, geo k-ring
-- disparity). Every row is machine-derived, advisory, and `pending` counsel
-- review; a resonance score is a research lead, never a finding.

CREATE TABLE case_resonance (
    case_id      UUID PRIMARY KEY REFERENCES court_cases(case_id) ON DELETE CASCADE,
    fisher_chi2  DOUBLE PRECISION NOT NULL,
    fisher_p     DOUBLE PRECISION NOT NULL,
    stouffer_z   DOUBLE PRECISION NOT NULL,
    -- Benjamini-Hochberg q-value across all cases scored in the same run.
    q_value      DOUBLE PRECISION NOT NULL,
    n_signals    INT NOT NULL,
    -- Per-signal detail: name, raw value, corpus size, one-sided p, weight.
    -- Labeled "machine_derived": true; never presented as findings.
    signals      JSONB NOT NULL,
    computed_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    status       TEXT NOT NULL DEFAULT 'pending'
                 CHECK (status IN ('pending','reviewed','dismissed'))
);
CREATE INDEX case_resonance_q_idx ON case_resonance (q_value);
